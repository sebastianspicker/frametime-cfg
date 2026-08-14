#[cfg(windows)]
mod trusted_work_dir {
    use std::{
        ffi::c_void,
        mem::size_of,
        path::{Path, PathBuf},
    };

    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL},
            Security::Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
                SE_FILE_OBJECT,
            },
            Security::{
                CreateWellKnownSid, EqualSid, GetAce, GetSecurityDescriptorControl,
                WinBuiltinAdministratorsSid, WinLocalSystemSid, ACCESS_ALLOWED_ACE,
                CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, OBJECT_INHERIT_ACE,
                OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
                PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, SE_DACL_PROTECTED,
            },
            Storage::FileSystem::{
                CreateDirectoryW, CreateFileW, FileAttributeTagInfo, GetFileInformationByHandleEx,
                GetFinalPathNameByHandleW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
                FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
                READ_CONTROL,
            },
        },
    };

    use super::WINDOWS_WORK_DIR;

    const SUITE_DACL_SDDL: &str = super::TRUSTED_WORK_DIR_SDDL;
    const FILE_ALL_ACCESS: u32 = 0x001F_01FF;
    const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
    const OI_CI: u8 = (OBJECT_INHERIT_ACE.0 | CONTAINER_INHERIT_ACE.0) as u8;

    #[derive(Debug)]
    pub(super) struct RootHandle(HANDLE);

    impl Drop for RootHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    impl RootHandle {
        pub(super) fn raw(&self) -> HANDLE {
            self.0
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    pub(super) fn open_or_create(requested: &Path) -> Result<(PathBuf, RootHandle), String> {
        let requested_text = requested.to_string_lossy();
        let created = match open_directory(&requested_text) {
            Ok(handle) => return validate_open_root(handle),
            Err(_) => create_protected_root(&requested_text)?,
        };
        if !created {
            return Err("suite root creation did not report a new directory".into());
        }
        let handle = open_directory(&requested_text)?;
        validate_open_root(handle)
    }

    pub(super) fn validate_child_handle(handle: HANDLE, leaf: &str) -> Result<(), String> {
        if !super::trusted_json_common::is_allowed_child(leaf) {
            return Err("suite child identity is not allowlisted".into());
        }
        validate_descendant_handle(handle, leaf, false)
    }

    pub(super) fn validate_temporary_handle(handle: HANDLE, leaf: &str) -> Result<(), String> {
        if !super::trusted_json_common::is_temporary_leaf(leaf) {
            return Err("suite temporary child identity is not allowlisted".into());
        }
        validate_descendant_handle(handle, leaf, false)
    }

    /// Validate a known descendant selected through an already-retained suite
    /// root. Callers must pass a fixed, individually validated relative path;
    /// this helper never accepts a path rooted outside the suite directory.
    pub(super) fn validate_descendant_handle(
        handle: HANDLE,
        relative: &str,
        directory: bool,
    ) -> Result<(), String> {
        if !safe_descendant_path(relative) {
            return Err("suite descendant identity is not a safe relative path".into());
        }
        reject_reparse_point(handle)?;
        let final_path = final_dos_path(handle)?;
        let expected = format!("{WINDOWS_WORK_DIR}\\{relative}");
        if !final_path.eq_ignore_ascii_case(&expected) {
            return Err("suite child final path escapes the trusted root".into());
        }
        let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
        unsafe {
            GetFileInformationByHandleEx(
                handle,
                FileAttributeTagInfo,
                (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast::<c_void>(),
                u32::try_from(size_of::<FILE_ATTRIBUTE_TAG_INFO>())
                    .map_err(|_| "FILE_ATTRIBUTE_TAG_INFO size exceeds u32")?,
            )
        }
        .map_err(|error| format!("query suite descendant attributes: {error}"))?;
        let is_directory = info.FileAttributes
            & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY.0
            != 0;
        if is_directory != directory {
            return Err("suite descendant type does not match its fixed identity".into());
        }
        validate_exact_security(handle)
    }

    fn safe_descendant_path(relative: &str) -> bool {
        !relative.is_empty()
            && !relative.starts_with('\\')
            && relative.split('\\').all(|part| {
                !part.is_empty()
                    && part != "."
                    && part != ".."
                    && !part.contains('/')
                    && !part.contains(':')
            })
    }

    pub(super) fn harden_created_child(handle: HANDLE) -> Result<(), String> {
        apply_exact_security(handle)?;
        validate_exact_security(handle)
    }

    pub(super) fn open_driver_artifact_directory(root: HANDLE) -> Result<RootHandle, String> {
        const RELATIVE: &str = "driver-artifacts";
        validate_exact_security(root)?;
        let path = format!("{WINDOWS_WORK_DIR}\\{RELATIVE}");
        match create_protected_root(&path)? {
            true | false => {}
        }
        let directory = open_directory(&path)?;
        validate_descendant_handle(directory.raw(), RELATIVE, true)?;
        Ok(directory)
    }

    fn open_directory(path: &str) -> Result<RootHandle, String> {
        let path = wide(path);
        let handle = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
        }
        .map_err(|error| format!("open suite root handle: {error}"))?;
        Ok(RootHandle(handle))
    }

    fn create_protected_root(path: &str) -> Result<bool, String> {
        let descriptor = security_descriptor_from_sddl()?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
                .map_err(|_| "SECURITY_ATTRIBUTES size exceeds u32")?,
            lpSecurityDescriptor: descriptor.0 .0,
            bInheritHandle: false.into(),
        };
        let path = wide(path);
        match unsafe { CreateDirectoryW(PCWSTR(path.as_ptr()), Some(&attributes)) } {
            Ok(()) => Ok(true),
            Err(error) if error.code().0 == 183 => Ok(false),
            Err(error) => Err(format!("create protected suite root: {error}")),
        }
    }

    struct LocalSecurityDescriptor(PSECURITY_DESCRIPTOR);
    impl Drop for LocalSecurityDescriptor {
        fn drop(&mut self) {
            if !self.0 .0.is_null() {
                unsafe {
                    let _ = LocalFree(Some(HLOCAL(self.0 .0)));
                }
            }
        }
    }

    fn security_descriptor_from_sddl() -> Result<LocalSecurityDescriptor, String> {
        let sddl = wide(SUITE_DACL_SDDL);
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                1,
                &mut descriptor,
                None,
            )
        }
        .map_err(|error| format!("parse suite DACL SDDL: {error}"))?;
        Ok(LocalSecurityDescriptor(descriptor))
    }

    pub(super) struct ProtectedSecurityAttributes {
        descriptor: LocalSecurityDescriptor,
        attributes: SECURITY_ATTRIBUTES,
    }

    impl ProtectedSecurityAttributes {
        pub(super) fn new() -> Result<Self, String> {
            let descriptor = security_descriptor_from_sddl()?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
                    .map_err(|_| "SECURITY_ATTRIBUTES size exceeds u32")?,
                lpSecurityDescriptor: descriptor.0 .0,
                bInheritHandle: false.into(),
            };
            Ok(Self {
                descriptor,
                attributes,
            })
        }

        pub(super) fn as_ptr(&self) -> *const SECURITY_ATTRIBUTES {
            let _ = &self.descriptor;
            &self.attributes
        }
    }

    fn validate_open_root(handle: RootHandle) -> Result<(PathBuf, RootHandle), String> {
        reject_reparse_point(handle.0)?;
        reject_intermediate_reparse_points()?;
        let final_path = final_dos_path(handle.0)?;
        if !final_path.eq_ignore_ascii_case(WINDOWS_WORK_DIR) {
            return Err(format!(
                "suite root final path is not {WINDOWS_WORK_DIR}: {final_path}"
            ));
        }
        validate_exact_security(handle.0)?;
        Ok((PathBuf::from(WINDOWS_WORK_DIR), handle))
    }

    fn reject_intermediate_reparse_points() -> Result<(), String> {
        // The only components of the fixed DOS root are the drive root and the
        // suite directory.  Open the drive handle independently so a junction
        // cannot be traversed before the final handle check.
        let drive = open_directory("C:\\")?;
        reject_reparse_point(drive.0)
    }

    fn reject_reparse_point(handle: HANDLE) -> Result<(), String> {
        let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
        unsafe {
            GetFileInformationByHandleEx(
                handle,
                FileAttributeTagInfo,
                (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast::<c_void>(),
                u32::try_from(size_of::<FILE_ATTRIBUTE_TAG_INFO>())
                    .map_err(|_| "FILE_ATTRIBUTE_TAG_INFO size exceeds u32")?,
            )
        }
        .map_err(|error| format!("query suite root reparse metadata: {error}"))?;
        if info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            Err("suite root or an intermediate component is a reparse point".into())
        } else {
            Ok(())
        }
    }

    fn final_dos_path(handle: HANDLE) -> Result<String, String> {
        let mut units = vec![0_u16; 512];
        let required = unsafe { GetFinalPathNameByHandleW(handle, &mut units, Default::default()) };
        if required == 0 {
            return Err("resolve suite root final path failed".into());
        }
        let needed = usize::try_from(required).map_err(|_| "final path length overflows usize")?;
        if needed >= units.len() {
            units.resize(needed.saturating_add(1), 0);
            let retry =
                unsafe { GetFinalPathNameByHandleW(handle, &mut units, Default::default()) };
            if retry == 0 || usize::try_from(retry).ok() != Some(needed) {
                return Err("resolve suite root final path retry failed".into());
            }
        }
        let path = String::from_utf16(&units[..needed]).map_err(|error| error.to_string())?;
        Ok(path.strip_prefix(r"\\?\").unwrap_or(&path).to_owned())
    }

    /// Validate the security properties that prevent the elevated caller's
    /// user SID from retaining authority to rewrite the trusted DACL.
    pub(super) fn validate_exact_security(handle: HANDLE) -> Result<(), String> {
        let mut owner = PSID::default();
        let mut dacl = std::ptr::null_mut();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        let status = unsafe {
            GetSecurityInfo(
                handle,
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                Some(&mut owner),
                None,
                Some(&mut dacl),
                None,
                Some(&mut descriptor),
            )
        };
        if status.0 != 0 {
            return Err(format!(
                "read suite root owner and DACL: Win32 error {}",
                status.0
            ));
        }
        let descriptor = LocalSecurityDescriptor(descriptor);
        if owner.0.is_null() {
            return Err("suite root has no owner".into());
        }
        if dacl.is_null() {
            return Err("suite root has no DACL".into());
        }
        let mut control = 0_u16;
        let mut revision = 0_u32;
        unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) }
            .map_err(|error| format!("read suite root DACL control: {error}"))?;
        if control & SE_DACL_PROTECTED.0 == 0 {
            return Err("suite root DACL is not protected".into());
        }
        let acl = unsafe { &*dacl };
        if acl.AceCount != 2 {
            return Err(
                "suite root DACL must contain exactly SYSTEM and Administrators ACEs".into(),
            );
        }
        let system = well_known_sid(WinLocalSystemSid)?;
        let administrators = well_known_sid(WinBuiltinAdministratorsSid)?;
        if unsafe { EqualSid(owner, system.as_psid()) }.is_err()
            && unsafe { EqualSid(owner, administrators.as_psid()) }.is_err()
        {
            return Err("suite root owner is not SYSTEM or Administrators".into());
        }
        let mut found_system = false;
        let mut found_administrators = false;
        for index in 0..u32::from(acl.AceCount) {
            let mut ace = std::ptr::null_mut();
            unsafe { GetAce(dacl, index, &mut ace) }
                .map_err(|error| format!("read suite root DACL ACE: {error}"))?;
            let ace = unsafe { &*(ace.cast::<ACCESS_ALLOWED_ACE>()) };
            if ace.Header.AceType != ACCESS_ALLOWED_ACE_TYPE
                || ace.Header.AceFlags != OI_CI
                || ace.Mask != FILE_ALL_ACCESS
            {
                return Err(
                    "suite root DACL ACE is not an explicit OI|CI full-access allow ACE".into(),
                );
            }
            let sid = PSID(std::ptr::addr_of!(ace.SidStart).cast_mut().cast::<c_void>());
            if unsafe { EqualSid(sid, system.as_psid()) }.is_ok() {
                if found_system {
                    return Err("suite root DACL duplicates SYSTEM ACE".into());
                }
                found_system = true;
            } else if unsafe { EqualSid(sid, administrators.as_psid()) }.is_ok() {
                if found_administrators {
                    return Err("suite root DACL duplicates Administrators ACE".into());
                }
                found_administrators = true;
            } else {
                return Err("suite root DACL contains an unexpected SID".into());
            }
        }
        if found_system && found_administrators {
            Ok(())
        } else {
            Err("suite root DACL is missing SYSTEM or Administrators".into())
        }
    }

    #[cfg(test)]
    pub(super) fn apply_exact_dacl(handle: HANDLE) -> Result<(), String> {
        use windows::Win32::{
            Security::Authorization::SetSecurityInfo, Security::GetSecurityDescriptorDacl,
        };
        let descriptor = security_descriptor_from_sddl()?;
        let mut present = false.into();
        let mut dacl = std::ptr::null_mut();
        let mut defaulted = false.into();
        unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted) }
            .map_err(|error| format!("read exact suite DACL: {error}"))?;
        if !present.as_bool() || dacl.is_null() {
            return Err("exact suite DACL is absent from SDDL descriptor".into());
        }
        let status = unsafe {
            SetSecurityInfo(
                handle,
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(dacl.cast_const()),
                None,
            )
        };
        if status.0 == 0 {
            Ok(())
        } else {
            Err(format!(
                "apply protected suite child DACL: Win32 error {}",
                status.0
            ))
        }
    }

    fn apply_exact_security(handle: HANDLE) -> Result<(), String> {
        use windows::Win32::{
            Security::Authorization::SetSecurityInfo, Security::GetSecurityDescriptorDacl,
        };

        let descriptor = security_descriptor_from_sddl()?;
        let mut present = false.into();
        let mut dacl = std::ptr::null_mut();
        let mut defaulted = false.into();
        unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted) }
            .map_err(|error| format!("read exact suite DACL: {error}"))?;
        if !present.as_bool() || dacl.is_null() {
            return Err("exact suite DACL is absent from SDDL descriptor".into());
        }
        let administrators = well_known_sid(WinBuiltinAdministratorsSid)?;
        let status = unsafe {
            SetSecurityInfo(
                handle,
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION
                    | DACL_SECURITY_INFORMATION
                    | PROTECTED_DACL_SECURITY_INFORMATION,
                Some(administrators.as_psid()),
                None,
                Some(dacl.cast_const()),
                None,
            )
        };
        if status.0 == 0 {
            Ok(())
        } else {
            Err(format!(
                "apply protected suite child owner and DACL: Win32 error {}",
                status.0
            ))
        }
    }

    struct WellKnownSid(Vec<u8>);
    impl WellKnownSid {
        fn as_psid(&self) -> PSID {
            PSID(self.0.as_ptr().cast_mut().cast::<c_void>())
        }
    }
    fn well_known_sid(
        kind: windows::Win32::Security::WELL_KNOWN_SID_TYPE,
    ) -> Result<WellKnownSid, String> {
        let mut bytes = vec![0_u8; 68];
        let mut len = u32::try_from(bytes.len()).map_err(|_| "SID buffer length exceeds u32")?;
        unsafe {
            CreateWellKnownSid(
                kind,
                None,
                Some(PSID(bytes.as_mut_ptr().cast::<c_void>())),
                &mut len,
            )
        }
        .map_err(|error| format!("construct well-known SID: {error}"))?;
        bytes.truncate(usize::try_from(len).map_err(|_| "SID length overflows usize")?);
        Ok(WellKnownSid(bytes))
    }
}
