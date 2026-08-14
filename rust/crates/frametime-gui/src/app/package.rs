use super::*;

pub(super) fn require_authenticated_package(window: HWND) -> bool {
    let unavailable = with_state(window, |app| {
        (!app.package.has_capability()).then(|| app.package.unavailable_detail())
    })
    .flatten();
    match unavailable {
        Some(detail) => {
            update_status(window, StatusKind::Warning, &detail);
            false
        }
        None => true,
    }
}

pub(super) fn launch_terminal(window: HWND, arguments: &[&str]) -> Result<RetainedChild, String> {
    let cli = with_state(window, |app| {
        app.package
            .package()
            .map(|package| package.cli().path().to_path_buf())
    })
    .flatten()
    .ok_or_else(|| model::EXTERNAL_EXECUTION_UNAVAILABLE.to_owned())?;
    let child = Command::new(cli)
        .args(arguments)
        .creation_flags((CREATE_NEW_CONSOLE | CREATE_NEW_PROCESS_GROUP).0)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(RetainedChild::new(child))
}

pub(super) struct ElevationWatchdog {
    elevated_process: windows::Win32::Foundation::HANDLE,
}

impl ElevationWatchdog {
    fn new(elevated_process: windows::Win32::Foundation::HANDLE) -> Result<Self, String> {
        if elevated_process.is_invalid() {
            return Err("ShellExecuteExW did not return an elevated process handle".into());
        }
        Ok(Self { elevated_process })
    }

    pub(super) fn has_exited(&self) -> Result<bool, String> {
        use windows::Win32::{
            Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
            System::Threading::WaitForSingleObject,
        };

        match unsafe { WaitForSingleObject(self.elevated_process, 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(format!(
                "wait for elevated GUI process: {}",
                windows::core::Error::from_thread()
            )),
        }
    }
}

impl Drop for ElevationWatchdog {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.elevated_process);
        }
    }
}

pub(super) fn relaunch_elevated(window: HWND) -> Result<(), String> {
    let file = with_state(window, |app| {
        app.package.package().map(|package| {
            package
                .gui()
                .path()
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        })
    })
    .flatten()
    .ok_or_else(|| model::EXTERNAL_EXECUTION_UNAVAILABLE.to_owned())?;
    let verb = utf16("runas");
    let mut launch = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        hwnd: window,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe {
        ShellExecuteExW(&mut launch).map_err(|error| error.to_string())?;
    }
    let watchdog = ElevationWatchdog::new(launch.hProcess)?;
    with_state(window, |app| app.elevation_watchdog = Some(watchdog))
        .ok_or("native GUI state is unavailable after elevation request")?;
    unsafe {
        let _ = ShowWindow(window, SW_HIDE);
    }
    Ok(())
}
