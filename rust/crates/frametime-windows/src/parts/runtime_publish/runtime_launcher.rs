use std::{mem::size_of, os::windows::ffi::OsStrExt};

use windows::{
    Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject},
        UI::Shell::{
            SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
            ShellExecuteExW,
        },
    },
    core::PCWSTR,
};

use super::{SAFE_MODE_HANDOFF_ARGUMENTS, VerifiedPublishedRuntime};

const SW_SHOWNORMAL_VALUE: i32 = 1;

pub(super) fn launch(runtime: &VerifiedPublishedRuntime) -> Result<(), String> {
    let file = runtime
        .executable_path()
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let verb = "runas".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let parameters = SAFE_MODE_HANDOFF_ARGUMENTS
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut execute = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>())
            .map_err(|_| "published runtime launch structure is too large")?,
        fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC | SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_SHOWNORMAL_VALUE,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut execute) }
        .map_err(|error| format!("launch selected runtime with elevation: {error}"))?;
    if execute.hProcess.is_invalid() {
        return Err("selected runtime launch returned no process handle".into());
    }
    let process = ProcessHandle(execute.hProcess);
    if unsafe { WaitForSingleObject(process.0, INFINITE) } != WAIT_OBJECT_0 {
        return Err(format!(
            "wait for selected runtime handoff: {}",
            windows::core::Error::from_thread()
        ));
    }
    let mut exit_code = u32::MAX;
    unsafe { GetExitCodeProcess(process.0, &mut exit_code) }
        .map_err(|error| format!("read selected runtime handoff exit code: {error}"))?;
    if exit_code != 0 {
        return Err(format!(
            "selected runtime failed to arm Safe Mode handoff (exit {exit_code})"
        ));
    }
    Ok(())
}

struct ProcessHandle(windows::Win32::Foundation::HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
