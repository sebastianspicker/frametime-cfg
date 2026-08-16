#[cfg(windows)]
mod clipboard {
    use std::{ptr, slice};

    use windows::Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL},
        System::{
            DataExchange::{
                CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
            },
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
        },
    };

    const CF_UNICODETEXT: u32 = 13;

    struct ClipboardGuard;
    impl ClipboardGuard {
        fn open() -> Result<Self, String> {
            unsafe { OpenClipboard(None) }.map_err(|error| format!("open clipboard: {error}"))?;
            Ok(Self)
        }
    }
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    pub(super) fn write(text: &str) -> Result<(), String> {
        let _clipboard = ClipboardGuard::open()?;
        let utf16 = text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let bytes = utf16
            .len()
            .checked_mul(2)
            .ok_or("clipboard text is too large")?;
        let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) }
            .map_err(|error| format!("allocate clipboard text: {error}"))?;
        let address = unsafe { GlobalLock(memory) };
        if address.is_null() {
            unsafe {
                let _ = GlobalFree(Some(memory));
            }
            return Err("lock clipboard allocation failed".into());
        }
        unsafe {
            ptr::copy_nonoverlapping(utf16.as_ptr().cast::<u8>(), address.cast::<u8>(), bytes);
        }
        unsafe {
            let _ = GlobalUnlock(memory);
        }
        unsafe { EmptyClipboard() }.map_err(|error| {
            unsafe {
                let _ = GlobalFree(Some(memory));
            }
            format!("empty clipboard: {error}")
        })?;
        if let Err(error) = unsafe { SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0))) } {
            unsafe {
                let _ = GlobalFree(Some(memory));
            }
            return Err(format!("set clipboard text: {error}"));
        }
        Ok(())
    }

    pub(super) fn read() -> Result<String, String> {
        let _clipboard = ClipboardGuard::open()?;
        let handle = unsafe { GetClipboardData(CF_UNICODETEXT) }
            .map_err(|error| format!("get clipboard text: {error}"))?;
        let memory = HGLOBAL(handle.0);
        let bytes = unsafe { GlobalSize(memory) };
        if bytes == 0 || bytes % 2 != 0 {
            return Err("clipboard Unicode payload has invalid size".into());
        }
        let address = unsafe { GlobalLock(memory) };
        if address.is_null() {
            return Err("lock clipboard text failed".into());
        }
        let units = unsafe { slice::from_raw_parts(address.cast::<u16>(), bytes / 2) };
        let result = match units.iter().position(|value| *value == 0) {
            Some(end) => String::from_utf16(&units[..end]).map_err(|error| error.to_string()),
            None => Err("clipboard Unicode payload is unterminated".into()),
        };
        unsafe {
            let _ = GlobalUnlock(memory);
        }
        result
    }
}

#[cfg(windows)]
mod boot_mode {
    use super::BootMode;
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CLEANBOOT};
    pub(super) fn current() -> Result<BootMode, String> {
        Ok(if unsafe { GetSystemMetrics(SM_CLEANBOOT) } == 0 {
            BootMode::Normal
        } else {
            BootMode::SafeMode
        })
    }
}
#[cfg(windows)]
mod platform {
    use windows::{
        Wdk::System::SystemServices::RtlGetVersion,
        Win32::System::SystemInformation::OSVERSIONINFOW,
    };

    pub(super) fn build_number() -> Result<u32, String> {
        if std::env::consts::ARCH != "x86_64" {
            return Err("native Windows operations require x86_64".into());
        }
        let mut version = OSVERSIONINFOW {
            dwOSVersionInfoSize: u32::try_from(std::mem::size_of::<OSVERSIONINFOW>())
                .map_err(|_| "OSVERSIONINFOW size exceeds u32")?,
            ..OSVERSIONINFOW::default()
        };
        let status = unsafe { RtlGetVersion(&mut version) };
        if status.0 != 0 {
            return Err(format!("RtlGetVersion failed with NTSTATUS {}", status.0));
        }
        if version.dwMajorVersion != 10 {
            return Err(format!(
                "Windows major version {} is unsupported",
                version.dwMajorVersion
            ));
        }
        Ok(version.dwBuildNumber)
    }

    pub(super) fn is_supported() -> bool {
        build_number().is_ok_and(|build| build >= 14_393)
    }
}
#[cfg(not(windows))]
mod platform {
    pub(super) fn build_number() -> Result<u32, String> {
        Err("the native Windows version query is supported only on Windows".into())
    }
    pub(super) const fn is_supported() -> bool {
        false
    }
}

/// Restrict process-wide dynamic library lookup to System32. Both native
/// entry points call this before argument parsing or any other application
/// work; the PE linker separately applies DEPENDENTLOADFLAG for static imports.
#[cfg(windows)]
pub fn harden_process_dll_search() -> Result<(), String> {
    use windows::Win32::System::LibraryLoader::{
        LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories,
    };
    unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) }
        .map_err(|error| format!("restrict process DLL search to System32: {error}"))
}
#[cfg(not(windows))]
mod boot_mode {
    use super::BootMode;
    pub(super) fn current() -> Result<BootMode, String> {
        Err("the native boot-mode query is supported only on Windows".into())
    }
}
#[cfg(not(windows))]
mod clipboard {
    pub(super) fn write(_: &str) -> Result<(), String> {
        Err("the native clipboard is supported only on Windows".into())
    }
    pub(super) fn read() -> Result<String, String> {
        Err("the native clipboard is supported only on Windows".into())
    }
}

#[cfg(any(windows, test))]
const SID_HEADER_BYTES: usize = 8;
#[cfg(any(windows, test))]
const SID_REVISION: u8 = 1;
#[cfg(any(windows, test))]
const SID_MAX_SUB_AUTHORITIES: u8 = 15;
#[cfg(windows)]
const SID_MAX_BYTES: usize = SID_HEADER_BYTES + SID_MAX_SUB_AUTHORITIES as usize * 4;

#[cfg(any(windows, test))]
#[derive(Debug, PartialEq, Eq)]
struct SidExtent {
    offset: usize,
    length: usize,
}

#[cfg(any(windows, test))]
struct TokenUserLayout {
    base_address: usize,
    capacity: usize,
    used: usize,
    token_user_size: usize,
    token_user_alignment: usize,
    sid_buffer_address: usize,
    sid_address: usize,
}

#[cfg(any(windows, test))]
fn validate_token_user_header_extent(
    base_address: usize,
    capacity: usize,
    used: usize,
    token_user_size: usize,
    token_user_alignment: usize,
) -> Result<(), String> {
    if token_user_alignment == 0 || !token_user_alignment.is_power_of_two() {
        return Err("TokenUser alignment is invalid".into());
    }
    if !base_address.is_multiple_of(token_user_alignment) {
        return Err("TokenUser buffer is misaligned".into());
    }
    if used > capacity {
        return Err("TokenUser returned extent exceeds its buffer".into());
    }
    if used < token_user_size {
        return Err("TokenUser returned extent is truncated".into());
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn validate_token_user_sid_extent(
    sid_buffer: &[u8],
    layout: TokenUserLayout,
) -> Result<SidExtent, String> {
    validate_token_user_header_extent(
        layout.base_address,
        layout.capacity,
        layout.used,
        layout.token_user_size,
        layout.token_user_alignment,
    )?;

    let returned_end = layout
        .base_address
        .checked_add(layout.used)
        .ok_or("TokenUser returned extent overflows its address space")?;
    let sid_offset = layout
        .sid_address
        .checked_sub(layout.base_address)
        .ok_or("TokenUser SID pointer is outside its returned extent")?;
    if sid_offset < layout.token_user_size || layout.sid_address >= returned_end {
        return Err("TokenUser SID pointer is outside its returned extent".into());
    }
    let sid_buffer_offset = layout
        .sid_address
        .checked_sub(layout.sid_buffer_address)
        .ok_or("TokenUser SID pointer is outside its owned SID storage")?;

    let header_end = sid_offset
        .checked_add(SID_HEADER_BYTES)
        .ok_or("TokenUser SID header extent overflows")?;
    let sid_buffer_header_end = sid_buffer_offset
        .checked_add(SID_HEADER_BYTES)
        .ok_or("TokenUser SID header extent overflows")?;
    if header_end > layout.used || sid_buffer_header_end > sid_buffer.len() {
        return Err("TokenUser SID header is truncated".into());
    }
    if sid_buffer[sid_buffer_offset] != SID_REVISION {
        return Err("TokenUser SID revision is invalid".into());
    }
    let sub_authorities = sid_buffer[sid_buffer_offset + 1];
    if sub_authorities > SID_MAX_SUB_AUTHORITIES {
        return Err("TokenUser SID sub-authority count is invalid".into());
    }
    let length = usize::from(sub_authorities)
        .checked_mul(std::mem::size_of::<u32>())
        .and_then(|bytes| SID_HEADER_BYTES.checked_add(bytes))
        .ok_or("TokenUser SID length overflows")?;
    let sid_end = sid_offset
        .checked_add(length)
        .ok_or("TokenUser SID extent overflows")?;
    let sid_buffer_end = sid_buffer_offset
        .checked_add(length)
        .ok_or("TokenUser SID extent overflows")?;
    if sid_end > layout.used || sid_buffer_end > sid_buffer.len() {
        return Err("TokenUser SID extent is truncated".into());
    }

    Ok(SidExtent {
        offset: sid_offset,
        length,
    })
}

#[cfg(windows)]
#[repr(C)]
struct TokenUserBuffer {
    user: windows::Win32::Security::TOKEN_USER,
    sid: [u8; SID_MAX_BYTES],
}

#[cfg(windows)]
fn current_token_user_sid() -> Result<String, String> {
    use windows::{
        Win32::{
            Foundation::{CloseHandle, HLOCAL, LocalFree},
            Security::Authorization::ConvertSidToStringSidW,
            Security::{
                GetLengthSid, GetTokenInformation, IsValidSid, TOKEN_QUERY, TOKEN_USER, TokenUser,
            },
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        },
        core::PWSTR,
    };

    let mut token = Default::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(|error| format!("open current process token: {error}"))?;
    struct TokenGuard(windows::Win32::Foundation::HANDLE);
    impl Drop for TokenGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let token = TokenGuard(token);
    let mut buffer = TokenUserBuffer {
        user: TOKEN_USER::default(),
        sid: [0; SID_MAX_BYTES],
    };
    let capacity = std::mem::size_of::<TokenUserBuffer>();
    let mut used = 0_u32;
    unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            Some(std::ptr::from_mut(&mut buffer).cast()),
            u32::try_from(capacity).map_err(|_| "token buffer length exceeds u32")?,
            &mut used,
        )
    }
    .map_err(|error| format!("read current TokenUser: {error}"))?;
    let used = usize::try_from(used).map_err(|_| "TokenUser returned extent exceeds usize")?;
    let base_address = std::ptr::from_ref(&buffer).addr();
    let sid_buffer_address = buffer.sid.as_ptr().addr();
    validate_token_user_header_extent(
        base_address,
        capacity,
        used,
        std::mem::size_of::<TOKEN_USER>(),
        std::mem::align_of::<TOKEN_USER>(),
    )?;
    let user = buffer.user;
    let sid_extent = validate_token_user_sid_extent(
        &buffer.sid,
        TokenUserLayout {
            base_address,
            capacity,
            used,
            token_user_size: std::mem::size_of::<TOKEN_USER>(),
            token_user_alignment: std::mem::align_of::<TOKEN_USER>(),
            sid_buffer_address,
            sid_address: user.User.Sid.0.addr(),
        },
    )?;
    // SAFETY: the portable extent validation proves that the SID's header and
    // declared sub-authorities are contained in the returned TokenUser bytes.
    let sid_length = unsafe {
        if !IsValidSid(user.User.Sid).as_bool() {
            return Err("TokenUser SID is invalid".into());
        }
        usize::try_from(GetLengthSid(user.User.Sid))
            .map_err(|_| "TokenUser SID length exceeds usize")?
    };
    if sid_length != sid_extent.length {
        return Err("TokenUser SID length disagrees with its returned extent".into());
    }
    let mut sid = PWSTR::null();
    unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid) }
        .map_err(|error| format!("render current TokenUser SID: {error}"))?;
    struct SidGuard(PWSTR);
    impl Drop for SidGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0.0.cast())));
            }
        }
    }
    let sid = SidGuard(sid);
    let length = unsafe { sid.0.len() };
    let units = unsafe { std::slice::from_raw_parts(sid.0.0, length) };
    String::from_utf16(units).map_err(|error| format!("decode TokenUser SID: {error}"))
}

#[cfg(test)]
mod token_user_sid_tests {
    use super::{
        SID_HEADER_BYTES, SidExtent, TokenUserLayout, validate_token_user_sid_extent,
    };

    const BASE: usize = 0x1000;
    const TOKEN_USER_SIZE: usize = 16;
    const TOKEN_USER_ALIGNMENT: usize = 8;

    fn valid_buffer(sub_authorities: u8) -> (Vec<u8>, usize) {
        let sid_length = SID_HEADER_BYTES + usize::from(sub_authorities) * 4;
        let mut buffer = vec![0_u8; TOKEN_USER_SIZE + sid_length];
        buffer[TOKEN_USER_SIZE] = 1;
        buffer[TOKEN_USER_SIZE + 1] = sub_authorities;
        (buffer, sid_length)
    }

    fn layout(base_address: usize, capacity: usize, used: usize, sid_address: usize) -> TokenUserLayout {
        TokenUserLayout {
            base_address,
            capacity,
            used,
            token_user_size: TOKEN_USER_SIZE,
            token_user_alignment: TOKEN_USER_ALIGNMENT,
            sid_buffer_address: base_address + TOKEN_USER_SIZE,
            sid_address,
        }
    }

    #[test]
    fn rejects_truncated_token_user_extent() {
        let buffer = [0_u8; TOKEN_USER_SIZE];
        let result = validate_token_user_sid_extent(
            &buffer[TOKEN_USER_SIZE..],
            layout(
                BASE,
                buffer.len(),
                TOKEN_USER_SIZE - 1,
                BASE + TOKEN_USER_SIZE,
            ),
        );
        assert_eq!(
            result.unwrap_err(),
            "TokenUser returned extent is truncated"
        );
    }

    #[test]
    fn rejects_misaligned_token_user_buffer() {
        let (buffer, _) = valid_buffer(1);
        let result = validate_token_user_sid_extent(
            &buffer[TOKEN_USER_SIZE..],
            layout(
                BASE + 1,
                buffer.len(),
                buffer.len(),
                BASE + 1 + TOKEN_USER_SIZE,
            ),
        );
        assert_eq!(result.unwrap_err(), "TokenUser buffer is misaligned");
    }

    #[test]
    fn rejects_sid_pointer_outside_returned_extent() {
        let (buffer, _) = valid_buffer(1);
        let result = validate_token_user_sid_extent(
            &buffer[TOKEN_USER_SIZE..],
            layout(BASE, buffer.len(), buffer.len(), BASE + buffer.len()),
        );
        assert_eq!(
            result.unwrap_err(),
            "TokenUser SID pointer is outside its returned extent"
        );
    }

    #[test]
    fn rejects_invalid_sid_structure_and_length() {
        let (mut invalid_revision, _) = valid_buffer(1);
        invalid_revision[TOKEN_USER_SIZE] = 2;
        assert_eq!(
            validate_token_user_sid_extent(
                &invalid_revision[TOKEN_USER_SIZE..],
                layout(
                    BASE,
                    invalid_revision.len(),
                    invalid_revision.len(),
                    BASE + TOKEN_USER_SIZE,
                ),
            )
            .unwrap_err(),
            "TokenUser SID revision is invalid"
        );

        let (mut truncated, _) = valid_buffer(1);
        truncated[TOKEN_USER_SIZE + 1] = 2;
        assert_eq!(
            validate_token_user_sid_extent(
                &truncated[TOKEN_USER_SIZE..],
                layout(
                    BASE,
                    truncated.len(),
                    truncated.len(),
                    BASE + TOKEN_USER_SIZE,
                ),
            )
            .unwrap_err(),
            "TokenUser SID extent is truncated"
        );
    }

    #[test]
    fn accepts_sid_ending_at_exact_returned_boundary() {
        let (buffer, sid_length) = valid_buffer(2);
        assert_eq!(
            validate_token_user_sid_extent(
                &buffer[TOKEN_USER_SIZE..],
                layout(
                    BASE,
                    buffer.len(),
                    buffer.len(),
                    BASE + TOKEN_USER_SIZE,
                ),
            )
            .unwrap(),
            SidExtent {
                offset: TOKEN_USER_SIZE,
                length: sid_length,
            }
        );
    }
}

#[cfg(not(windows))]
fn current_token_user_sid() -> Result<String, String> {
    Err("current TokenUser SID is supported only on Windows".into())
}

#[cfg(windows)]
#[derive(Debug)]
struct LockHandle(windows::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Drop for LockHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[derive(Debug)]
struct WorkLock {
    #[cfg(windows)]
    _handle: LockHandle,
    #[cfg(not(windows))]
    path: PathBuf,
}
impl WorkLock {
    #[cfg(windows)]
    fn acquire(work_dir: &Path) -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::Storage::FileSystem::{
                CREATE_NEW, CreateFileW, DELETE, FILE_FLAG_DELETE_ON_CLOSE,
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH, FILE_READ_ATTRIBUTES,
                FILE_SHARE_MODE, FILE_WRITE_DATA, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
            },
            core::PCWSTR,
        };

        let path = work_dir
            .join(LOCK_FILE)
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let handle = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                DELETE.0
                    | FILE_WRITE_DATA.0
                    | FILE_READ_ATTRIBUTES.0
                    | READ_CONTROL.0
                    | WRITE_DAC.0
                    | WRITE_OWNER.0,
                FILE_SHARE_MODE(0),
                None,
                CREATE_NEW,
                FILE_FLAG_DELETE_ON_CLOSE | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH,
                None,
            )
        }
        .map_err(|error| format!("live transaction lock unavailable: {error}"))?;
        let lock = LockHandle(handle);
        trusted_work_dir::harden_created_child(lock.0)
            .map_err(|error| format!("harden live transaction lock: {error}"))?;
        Ok(Self { _handle: lock })
    }

    #[cfg(not(windows))]
    fn acquire(work_dir: &Path) -> Result<Self, String> {
        let path = work_dir.join(LOCK_FILE);
        fs::create_dir_all(work_dir).map_err(|error| error.to_string())?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("live transaction lock unavailable: {error}"))?;
        Ok(Self { path })
    }
}
#[cfg(not(windows))]
impl Drop for WorkLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(any(test, windows))]
fn resolve_system_tool_path(system_directory: &Path, program: &str) -> Result<PathBuf, String> {
    let command = CommandName::from_program(program)?;
    Ok(system_directory.join(command.program()))
}

#[cfg(windows)]
fn system_directory() -> Result<PathBuf, String> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows::Win32::System::SystemInformation::GetSystemDirectoryW;

    let mut buffer = vec![0_u16; 260];
    loop {
        let copied = unsafe { GetSystemDirectoryW(Some(&mut buffer)) };
        if copied == 0 {
            return Err(format!(
                "read Windows System32 directory: {}",
                windows::core::Error::from_thread()
            ));
        }
        let copied = usize::try_from(copied).map_err(|_| "Windows System32 path is too large")?;
        if copied < buffer.len() {
            return Ok(PathBuf::from(OsString::from_wide(&buffer[..copied])));
        }
        let required = copied
            .checked_add(1)
            .ok_or("Windows System32 path is too large")?;
        buffer.resize(required, 0);
    }
}

#[cfg(windows)]
fn execute_allowlisted(command: CommandName, arguments: &[String]) -> Result<String, String> {
    use std::process::Command;
    let system_directory = system_directory()?;
    let program = resolve_system_tool_path(&system_directory, command.program())?;
    let output = Command::new(&program)
        .args(arguments)
        // Keep an allowlisted inbox tool's current-directory DLL search away
        // from the portable package or caller-controlled working directory.
        .current_dir(&system_directory)
        .output()
        .map_err(|error| format!("{} execution failed: {error}", command.program()))?;
    if !output.status.success() {
        return Err(format!(
            "{} exited with {}: {}",
            command.program(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
#[cfg(not(windows))]
fn execute_allowlisted(_: CommandName, _: &[String]) -> Result<String, String> {
    Err("the live backend is supported only on Windows".into())
}

#[cfg(windows)]
fn require_elevation() -> Result<(), String> {
    use windows::Win32::UI::Shell::IsUserAnAdmin;
    if unsafe { IsUserAnAdmin() }.as_bool() {
        Ok(())
    } else {
        Err("live commands require an elevated administrator token".into())
    }
}
#[cfg(not(windows))]
fn require_elevation() -> Result<(), String> {
    Err("the live backend is supported only on Windows".into())
}

#[cfg(windows)]
fn discover_hardware() -> Result<HardwareInfo, String> {
    let output = CommandVector::new(
        CommandName::Pnputil,
        &["/enum-devices", "/class", "Display"],
    )?
    .run()?;
    let display_adapters = output
        .lines()
        .filter_map(|line| {
            line.strip_prefix("Device Description:")
                .map(|value| value.trim().to_owned())
        })
        .collect::<Vec<_>>();
    let joined = display_adapters.join(" ").to_ascii_lowercase();
    let gpu_branch = if joined.contains("nvidia") {
        Some(GpuBranch::Nvidia)
    } else if joined.contains("amd") || joined.contains("radeon") {
        Some(GpuBranch::Amd)
    } else if joined.contains("intel") || joined.contains("arc") {
        Some(GpuBranch::IntelArc)
    } else {
        None
    };
    Ok(HardwareInfo {
        display_adapters,
        gpu_branch,
    })
}
#[cfg(not(windows))]
fn discover_hardware() -> Result<HardwareInfo, String> {
    Err("the live backend is supported only on Windows".into())
}

#[cfg(test)]
mod system_tool_tests {
    use std::path::Path;

    use super::{CommandName, resolve_system_tool_path};

    #[test]
    fn system_tools_resolve_beneath_the_trusted_directory() {
        let system_directory = Path::new("/trusted/System32");
        for command in [
            CommandName::Bcdedit,
            CommandName::Powercfg,
            CommandName::Pnputil,
            CommandName::Fsutil,
            CommandName::Defrag,
        ] {
            assert_eq!(
                resolve_system_tool_path(system_directory, command.program()),
                Ok(system_directory.join(command.program()))
            );
        }
    }

    #[test]
    fn system_tool_resolution_rejects_unexpected_program_names() {
        let system_directory = Path::new("/trusted/System32");
        for program in ["cmd.exe", "bcdedit", "bcdedit.exe.bak", r"..\bcdedit.exe"] {
            assert_eq!(
                resolve_system_tool_path(system_directory, program),
                Err("system tool is not allowlisted".into())
            );
        }
    }
}
