#[cfg(windows)]
pub(crate) use self::native_platform::build_number;
pub(crate) use self::native_platform::is_supported;
use crate::*;

#[cfg(windows)]
pub(crate) mod boot_mode {
    use super::BootMode;
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CLEANBOOT};
    pub(crate) fn current() -> Result<BootMode, String> {
        Ok(if unsafe { GetSystemMetrics(SM_CLEANBOOT) } == 0 {
            BootMode::Normal
        } else {
            BootMode::SafeMode
        })
    }
}
#[cfg(windows)]
pub(crate) mod native_platform {
    use windows::{
        Wdk::System::SystemServices::RtlGetVersion,
        Win32::System::SystemInformation::OSVERSIONINFOW,
    };

    pub(crate) fn build_number() -> Result<u32, String> {
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

    pub(crate) fn is_supported() -> bool {
        build_number().is_ok_and(|build| build >= 14_393)
    }
}
#[cfg(not(windows))]
pub(crate) mod native_platform {
    pub(crate) const fn is_supported() -> bool {
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
pub fn harden_process_dll_search() -> Result<(), String> {
    Err("process DLL search hardening is supported only on Windows".into())
}

#[cfg(not(windows))]
pub(crate) mod boot_mode {
    use super::BootMode;
    pub(crate) fn current() -> Result<BootMode, String> {
        Err("the native boot-mode query is supported only on Windows".into())
    }
}
