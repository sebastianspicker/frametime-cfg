#[cfg(windows)]
mod native_drs_windows {
    use sha2::{Digest, Sha256};
    use std::{
        ffi::c_void,
        fs::File,
        io::Read,
        mem::transmute,
        path::{Path, PathBuf},
    };

    use windows::{
        Win32::{
            Foundation::{FreeLibrary, HMODULE},
            System::{
                LibraryLoader::{
                    GetModuleFileNameW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32,
                    LoadLibraryExW,
                },
                SystemInformation::GetSystemDirectoryW,
            },
        },
        core::{PCSTR, PCWSTR},
    };

    use crate::native_drs_abi::{
        NVDRS_DWORD_TYPE, NvDrsApplicationV4, NvDrsProfile, NvDrsSetting, unicode_argument,
    };
    use crate::{DrsError, DrsOriginalSetting, NvapiDrs};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NativeDrsSession(*mut c_void);
    type Handle = *mut c_void;
    type Status = i32;
    type QueryInterface = unsafe extern "system" fn(u32) -> *const c_void;
    type Initialize = unsafe extern "C" fn() -> Status;
    type CreateSession = unsafe extern "C" fn(*mut Handle) -> Status;
    type SessionCall = unsafe extern "C" fn(Handle) -> Status;
    type FindProfile = unsafe extern "C" fn(Handle, *const u16, *mut Handle) -> Status;
    type ProfileCall = unsafe extern "C" fn(Handle, Handle) -> Status;
    type ProfileInfo = unsafe extern "C" fn(Handle, Handle, *mut NvDrsProfile) -> Status;
    type CreateProfile = unsafe extern "C" fn(Handle, *mut NvDrsProfile, *mut Handle) -> Status;
    type CreateApplication =
        unsafe extern "C" fn(Handle, Handle, *mut NvDrsApplicationV4) -> Status;
    type DeleteApplication = unsafe extern "C" fn(Handle, Handle, *const u16) -> Status;
    type FindApplication =
        unsafe extern "C" fn(Handle, *const u16, *mut Handle, *mut NvDrsApplicationV4) -> Status;
    type GetSetting = unsafe extern "C" fn(Handle, Handle, u32, *mut NvDrsSetting) -> Status;
    type SetSetting = unsafe extern "C" fn(Handle, Handle, *mut NvDrsSetting) -> Status;
    type DeleteSetting = unsafe extern "C" fn(Handle, Handle, u32) -> Status;

    const OK: Status = 0;
    const SETTING_NOT_FOUND: Status = -160;
    const PROFILE_NOT_FOUND: Status = -163;
    const EXECUTABLE_NOT_FOUND: Status = -166;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NativeDrsProfile(Handle);

    #[derive(Debug)]
    pub struct NativeNvapiDrs {
        module: HMODULE,
        module_sha256: String,
        initialize: Initialize,
        create_session: CreateSession,
        destroy_session: SessionCall,
        load_settings: SessionCall,
        save_settings: SessionCall,
        find_profile: FindProfile,
        get_profile_info: ProfileInfo,
        create_profile: CreateProfile,
        delete_profile: ProfileCall,
        create_application: CreateApplication,
        delete_application: DeleteApplication,
        find_application: FindApplication,
        get_setting: GetSetting,
        set_setting: SetSetting,
        delete_setting: DeleteSetting,
    }

    impl NativeNvapiDrs {
        pub fn load() -> Result<Self, DrsError> {
            let path =
                system32_nvapi_path().map_err(|reason| error("resolve nvapi64.dll", reason))?;
            let module = unsafe {
                LoadLibraryExW(
                    PCWSTR(wide_path(&path).as_ptr()),
                    None,
                    LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            }
            .map_err(|reason| error("load nvapi64.dll", reason.to_string()))?;
            let module_sha256 = match sha256_file(&path) {
                Ok(value) => value,
                Err(reason) => {
                    unsafe {
                        let _ = FreeLibrary(module);
                    }
                    return Err(error("hash nvapi64.dll", reason));
                }
            };
            if let Err(reason) = verify_loaded_module_path(module, &path) {
                unsafe {
                    let _ = FreeLibrary(module);
                }
                return Err(error("validate nvapi64.dll identity", reason));
            }
            let query =
                unsafe { GetProcAddress(module, PCSTR(c"nvapi_QueryInterface".as_ptr().cast())) }
                    .ok_or_else(|| error("resolve nvapi_QueryInterface", "export is missing"))?;
            let query: QueryInterface = unsafe { transmute(query) };
            let functions = unsafe { Functions::resolve(query) };
            match functions {
                Ok(functions) => Ok(functions.into_host(module, module_sha256)),
                Err(reason) => {
                    unsafe {
                        let _ = FreeLibrary(module);
                    }
                    Err(reason)
                }
            }
        }

        #[must_use]
        pub fn module_sha256(&self) -> &str {
            &self.module_sha256
        }

        #[must_use]
        pub const fn interface_version() -> &'static str {
            "nvapi-public-sdk-cd6918f60b3c9a0476fdfe7e89bb32330602049d"
        }
    }

    impl Drop for NativeNvapiDrs {
        fn drop(&mut self) {
            unsafe {
                let _ = FreeLibrary(self.module);
            }
        }
    }

    impl NvapiDrs for NativeNvapiDrs {
        type Session = NativeDrsSession;
        type Profile = NativeDrsProfile;

        fn initialize(&mut self) -> Result<(), DrsError> {
            status(unsafe { (self.initialize)() }, "NvAPI_Initialize")
        }
        fn create_session(&mut self) -> Result<NativeDrsSession, DrsError> {
            let mut session = std::ptr::null_mut();
            status(
                unsafe { (self.create_session)(&mut session) },
                "DRS_CreateSession",
            )?;
            (!session.is_null())
                .then_some(NativeDrsSession(session))
                .ok_or_else(|| error("DRS_CreateSession", "returned a null handle"))
        }
        fn destroy_session(&mut self, session: NativeDrsSession) -> Result<(), DrsError> {
            status(
                unsafe { (self.destroy_session)(session.0) },
                "DRS_DestroySession",
            )
        }
        fn load_settings(&mut self, session: &NativeDrsSession) -> Result<(), DrsError> {
            status(
                unsafe { (self.load_settings)(session.0) },
                "DRS_LoadSettings",
            )
        }
        fn save_settings(&mut self, session: &NativeDrsSession) -> Result<(), DrsError> {
            status(
                unsafe { (self.save_settings)(session.0) },
                "DRS_SaveSettings",
            )
        }
        fn find_profile_by_name(
            &mut self,
            session: &NativeDrsSession,
            name: &str,
        ) -> Result<Option<NativeDrsProfile>, DrsError> {
            let name =
                unicode_argument(name).map_err(|reason| error("encode profile name", reason))?;
            let mut profile = std::ptr::null_mut();
            optional_handle(
                unsafe { (self.find_profile)(session.0, name.as_ptr(), &mut profile) },
                PROFILE_NOT_FOUND,
                profile,
                "DRS_FindProfileByName",
            )
            .map(|value| value.map(NativeDrsProfile))
        }
        fn profile_name(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
        ) -> Result<String, DrsError> {
            let mut value =
                NvDrsProfile::query().map_err(|reason| error("prepare profile query", reason))?;
            status(
                unsafe { (self.get_profile_info)(session.0, profile.0, &mut value) },
                "DRS_GetProfileInfo",
            )?;
            value
                .name()
                .map_err(|reason| error("decode profile name", reason))
        }
        fn find_application_profile(
            &mut self,
            session: &NativeDrsSession,
            application: &str,
        ) -> Result<Option<NativeDrsProfile>, DrsError> {
            let name = unicode_argument(application)
                .map_err(|reason| error("encode application name", reason))?;
            let mut app = NvDrsApplicationV4::named(application)
                .map_err(|reason| error("prepare application query", reason))?;
            let mut profile = std::ptr::null_mut();
            optional_handle(
                unsafe {
                    (self.find_application)(session.0, name.as_ptr(), &mut profile, &mut app)
                },
                EXECUTABLE_NOT_FOUND,
                profile,
                "DRS_FindApplicationByName",
            )
            .map(|value| value.map(NativeDrsProfile))
        }
        fn create_profile(
            &mut self,
            session: &NativeDrsSession,
            name: &str,
        ) -> Result<NativeDrsProfile, DrsError> {
            let mut value =
                NvDrsProfile::named(name).map_err(|reason| error("prepare profile", reason))?;
            let mut profile = std::ptr::null_mut();
            status(
                unsafe { (self.create_profile)(session.0, &mut value, &mut profile) },
                "DRS_CreateProfile",
            )?;
            (!profile.is_null())
                .then_some(NativeDrsProfile(profile))
                .ok_or_else(|| error("DRS_CreateProfile", "returned a null handle"))
        }
        fn bind_application(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
            application: &str,
        ) -> Result<(), DrsError> {
            let mut app = NvDrsApplicationV4::named(application)
                .map_err(|reason| error("prepare application", reason))?;
            status(
                unsafe { (self.create_application)(session.0, profile.0, &mut app) },
                "DRS_CreateApplication",
            )
        }
        fn delete_application(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
            application: &str,
        ) -> Result<(), DrsError> {
            let application = unicode_argument(application)
                .map_err(|reason| error("encode application name", reason))?;
            status(
                unsafe { (self.delete_application)(session.0, profile.0, application.as_ptr()) },
                "DRS_DeleteApplication",
            )
        }
        fn delete_profile(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
        ) -> Result<(), DrsError> {
            status(
                unsafe { (self.delete_profile)(session.0, profile.0) },
                "DRS_DeleteProfile",
            )
        }
        fn read_dword(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
            id: u32,
        ) -> Result<Option<u32>, DrsError> {
            let mut value =
                NvDrsSetting::query().map_err(|reason| error("prepare setting query", reason))?;
            let result = unsafe { (self.get_setting)(session.0, profile.0, id, &mut value) };
            if result == SETTING_NOT_FOUND {
                return Ok(None);
            }
            status(result, "DRS_GetSetting")?;
            if value.setting_type() != NVDRS_DWORD_TYPE {
                return Err(error("DRS_GetSetting", "setting is not DWORD"));
            }
            Ok(Some(value.current_dword()))
        }
        fn set_dword(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
            id: u32,
            value: u32,
        ) -> Result<(), DrsError> {
            let mut setting = NvDrsSetting::dword(id, value)
                .map_err(|reason| error("prepare DWORD setting", reason))?;
            status(
                unsafe { (self.set_setting)(session.0, profile.0, &mut setting) },
                "DRS_SetSetting",
            )
        }
        fn restore_dword(
            &mut self,
            session: &NativeDrsSession,
            profile: &NativeDrsProfile,
            original: DrsOriginalSetting,
        ) -> Result<(), DrsError> {
            match original.value {
                Some(value) => self.set_dword(session, profile, original.id, value),
                None => status(
                    unsafe { (self.delete_setting)(session.0, profile.0, original.id) },
                    "DRS_DeleteProfileSetting",
                ),
            }
        }
    }

    struct Functions {
        initialize: Initialize,
        create_session: CreateSession,
        destroy_session: SessionCall,
        load_settings: SessionCall,
        save_settings: SessionCall,
        find_profile: FindProfile,
        get_profile_info: ProfileInfo,
        create_profile: CreateProfile,
        delete_profile: ProfileCall,
        create_application: CreateApplication,
        delete_application: DeleteApplication,
        find_application: FindApplication,
        get_setting: GetSetting,
        set_setting: SetSetting,
        delete_setting: DeleteSetting,
    }

    impl Functions {
        unsafe fn resolve(query: QueryInterface) -> Result<Self, DrsError> {
            macro_rules! resolve {
                ($id:expr, $name:literal, $ty:ty) => {{
                    let pointer = unsafe { query($id) };
                    if pointer.is_null() {
                        return Err(error($name, "query interface returned null"));
                    }
                    unsafe { transmute::<*const c_void, $ty>(pointer) }
                }};
            }
            Ok(Self {
                initialize: resolve!(0x0150_e828, "NvAPI_Initialize", Initialize),
                create_session: resolve!(0x0694_d52e, "DRS_CreateSession", CreateSession),
                destroy_session: resolve!(0xdad9_cff8, "DRS_DestroySession", SessionCall),
                load_settings: resolve!(0x375d_bd6b, "DRS_LoadSettings", SessionCall),
                save_settings: resolve!(0xfcbc_7e14, "DRS_SaveSettings", SessionCall),
                find_profile: resolve!(0x7e4a_9a0b, "DRS_FindProfileByName", FindProfile),
                get_profile_info: resolve!(0x61cd_6fd6, "DRS_GetProfileInfo", ProfileInfo),
                create_profile: resolve!(0xcc17_6068, "DRS_CreateProfile", CreateProfile),
                delete_profile: resolve!(0x1709_3206, "DRS_DeleteProfile", ProfileCall),
                create_application: resolve!(
                    0x4347_a9de,
                    "DRS_CreateApplication",
                    CreateApplication
                ),
                // NvAPI_DRS_DeleteApplication: public NVIDIA SDK commit
                // cd6918f60b3c9a0476fdfe7e89bb32330602049d, interface 0x2c694bc6.
                delete_application: resolve!(
                    0x2c69_4bc6,
                    "DRS_DeleteApplication",
                    DeleteApplication
                ),
                find_application: resolve!(
                    0xeee5_66b2,
                    "DRS_FindApplicationByName",
                    FindApplication
                ),
                get_setting: resolve!(0x73bf_8338, "DRS_GetSetting", GetSetting),
                set_setting: resolve!(0x577d_d202, "DRS_SetSetting", SetSetting),
                delete_setting: resolve!(0xe4a2_6362, "DRS_DeleteProfileSetting", DeleteSetting),
            })
        }
        fn into_host(self, module: HMODULE, module_sha256: String) -> NativeNvapiDrs {
            NativeNvapiDrs {
                module,
                module_sha256,
                initialize: self.initialize,
                create_session: self.create_session,
                destroy_session: self.destroy_session,
                load_settings: self.load_settings,
                save_settings: self.save_settings,
                find_profile: self.find_profile,
                get_profile_info: self.get_profile_info,
                create_profile: self.create_profile,
                delete_profile: self.delete_profile,
                create_application: self.create_application,
                delete_application: self.delete_application,
                find_application: self.find_application,
                get_setting: self.get_setting,
                set_setting: self.set_setting,
                delete_setting: self.delete_setting,
            }
        }
    }

    fn status(value: Status, operation: &'static str) -> Result<(), DrsError> {
        if value == OK {
            Ok(())
        } else {
            Err(error(operation, format!("status {value}")))
        }
    }
    fn optional_handle(
        value: Status,
        missing: Status,
        handle: Handle,
        operation: &'static str,
    ) -> Result<Option<Handle>, DrsError> {
        if value == missing {
            return Ok(None);
        }
        status(value, operation)?;
        (!handle.is_null())
            .then_some(Some(handle))
            .ok_or_else(|| error(operation, "returned a null handle"))
    }
    fn error(operation: &'static str, reason: impl Into<String>) -> DrsError {
        DrsError::new(operation, reason)
    }

    fn system32_nvapi_path() -> Result<PathBuf, String> {
        let mut buffer = vec![0_u16; 32_768];
        let copied = unsafe { GetSystemDirectoryW(Some(&mut buffer)) };
        let copied = usize::try_from(copied).map_err(|_| "System32 path length overflows")?;
        if copied == 0 || copied >= buffer.len() {
            return Err("GetSystemDirectoryW failed or truncated".into());
        }
        buffer.truncate(copied);
        let root = String::from_utf16(&buffer).map_err(|_| "System32 path is invalid UTF-16")?;
        Ok(PathBuf::from(root).join("nvapi64.dll"))
    }

    fn verify_loaded_module_path(module: HMODULE, expected: &Path) -> Result<(), String> {
        let mut buffer = vec![0_u16; 32_768];
        let copied = unsafe { GetModuleFileNameW(Some(module), &mut buffer) };
        let copied = usize::try_from(copied).map_err(|_| "module path length overflows")?;
        if copied == 0 || copied >= buffer.len() {
            return Err("GetModuleFileNameW failed or truncated".into());
        }
        buffer.truncate(copied);
        let actual = PathBuf::from(
            String::from_utf16(&buffer).map_err(|_| "module path is invalid UTF-16")?,
        );
        if actual
            .to_string_lossy()
            .eq_ignore_ascii_case(&expected.to_string_lossy())
        {
            Ok(())
        } else {
            Err("loaded NVAPI module is not the absolute System32 nvapi64.dll".into())
        }
    }

    fn wide_path(path: &std::path::Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    fn sha256_file(path: &Path) -> Result<String, String> {
        let mut file = File::open(path).map_err(|error| error.to_string())?;
        let mut hasher = Sha256::new();
        let mut bytes = [0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut bytes).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            hasher.update(&bytes[..read]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }
}

#[cfg(windows)]
pub use native_drs_windows::NativeNvapiDrs;
