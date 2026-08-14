#[cfg(windows)]
mod native_services {
    use std::{mem::size_of, thread, time::Duration};

    use windows::{
        Win32::{
            Foundation::ERROR_SERVICE_DOES_NOT_EXIST,
            System::Services::{
                ChangeServiceConfig2W, ChangeServiceConfigW, ControlService, ENUM_SERVICE_TYPE,
                OpenSCManagerW, OpenServiceW, QueryServiceConfig2W, QueryServiceConfigW,
                QueryServiceStatusEx, SC_HANDLE, SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO,
                SERVICE_AUTO_START, SERVICE_BOOT_START, SERVICE_CHANGE_CONFIG,
                SERVICE_CONFIG_DELAYED_AUTO_START_INFO, SERVICE_CONTROL_STOP,
                SERVICE_DELAYED_AUTO_START_INFO, SERVICE_DEMAND_START, SERVICE_DISABLED,
                SERVICE_ERROR, SERVICE_NO_CHANGE, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS,
                SERVICE_RUNNING, SERVICE_START, SERVICE_START_TYPE, SERVICE_STATUS,
                SERVICE_STATUS_PROCESS, SERVICE_STOP, SERVICE_STOPPED, SERVICE_SYSTEM_START,
            },
        },
        core::{BOOL, HRESULT, PCWSTR},
    };

    use super::{Inspection, ServiceBatch, ServiceSnapshot};

    const SERVICE_ACCESS: u32 = SERVICE_QUERY_CONFIG
        | SERVICE_QUERY_STATUS
        | SERVICE_CHANGE_CONFIG
        | SERVICE_STOP
        | SERVICE_START;
    const WAIT_ATTEMPTS: usize = 50;
    const WAIT_INTERVAL: Duration = Duration::from_millis(100);

    struct ScopedServiceHandle(SC_HANDLE);

    impl Drop for ScopedServiceHandle {
        fn drop(&mut self) {
            // `SC_HANDLE` is Copy and does not automatically close through Drop.
            unsafe {
                let _ = windows::Win32::System::Services::CloseServiceHandle(self.0);
            }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    fn start_type(value: &str) -> Result<SERVICE_START_TYPE, String> {
        match value {
            "Automatic" => Ok(SERVICE_AUTO_START),
            "Manual" => Ok(SERVICE_DEMAND_START),
            "Disabled" => Ok(SERVICE_DISABLED),
            "Boot" => Ok(SERVICE_BOOT_START),
            "System" => Ok(SERVICE_SYSTEM_START),
            _ => Err("service start type is not allowlisted".into()),
        }
    }

    fn start_type_name(value: SERVICE_START_TYPE) -> Result<&'static str, String> {
        match value {
            SERVICE_AUTO_START => Ok("Automatic"),
            SERVICE_DEMAND_START => Ok("Manual"),
            SERVICE_DISABLED => Ok("Disabled"),
            SERVICE_BOOT_START => Ok("Boot"),
            SERVICE_SYSTEM_START => Ok("System"),
            _ => Err("service has an unsupported startup type".into()),
        }
    }

    fn open_manager() -> Result<ScopedServiceHandle, String> {
        unsafe { OpenSCManagerW(None, None, SC_MANAGER_CONNECT) }
            .map(ScopedServiceHandle)
            .map_err(|error| format!("open SCM: {error}"))
    }

    fn open_present(manager: SC_HANDLE, name: &str) -> Result<Option<ScopedServiceHandle>, String> {
        let name = wide(name);
        match unsafe { OpenServiceW(manager, PCWSTR(name.as_ptr()), SERVICE_ACCESS) } {
            Ok(service) => Ok(Some(ScopedServiceHandle(service))),
            Err(error) if error.code() == HRESULT::from_win32(ERROR_SERVICE_DOES_NOT_EXIST.0) => {
                Ok(None)
            }
            Err(error) => Err(format!("open service: {error}")),
        }
    }

    fn query_config(service: SC_HANDLE) -> Result<(SERVICE_START_TYPE, bool), String> {
        let mut needed = 0;
        let _ = unsafe { QueryServiceConfigW(service, None, 0, &mut needed) };
        if needed < size_of::<windows::Win32::System::Services::QUERY_SERVICE_CONFIGW>() as u32 {
            return Err("query service configuration did not return a complete buffer size".into());
        }
        let mut bytes = vec![0_u8; needed as usize];
        unsafe {
            QueryServiceConfigW(
                service,
                Some(bytes.as_mut_ptr().cast()),
                needed,
                &mut needed,
            )
        }
        .map_err(|error| format!("query service configuration: {error}"))?;
        let configuration = unsafe {
            bytes
                .as_ptr()
                .cast::<windows::Win32::System::Services::QUERY_SERVICE_CONFIGW>()
                .read_unaligned()
        };

        let mut delayed_needed = 0;
        let _ = unsafe {
            QueryServiceConfig2W(
                service,
                SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
                None,
                &mut delayed_needed,
            )
        };
        if delayed_needed < size_of::<SERVICE_DELAYED_AUTO_START_INFO>() as u32 {
            return Err("query delayed auto-start did not return a complete buffer size".into());
        }
        let mut delayed_bytes = vec![0_u8; delayed_needed as usize];
        unsafe {
            QueryServiceConfig2W(
                service,
                SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
                Some(&mut delayed_bytes),
                &mut delayed_needed,
            )
        }
        .map_err(|error| format!("query delayed auto-start: {error}"))?;
        let delayed = unsafe {
            delayed_bytes
                .as_ptr()
                .cast::<SERVICE_DELAYED_AUTO_START_INFO>()
                .read_unaligned()
                .fDelayedAutostart
                .as_bool()
        };
        Ok((configuration.dwStartType, delayed))
    }

    fn query_status(service: SC_HANDLE) -> Result<SERVICE_STATUS_PROCESS, String> {
        let mut bytes = vec![0_u8; size_of::<SERVICE_STATUS_PROCESS>()];
        let mut needed = 0;
        unsafe {
            QueryServiceStatusEx(
                service,
                SC_STATUS_PROCESS_INFO,
                Some(&mut bytes),
                &mut needed,
            )
        }
        .map_err(|error| format!("query service status: {error}"))?;
        if needed < size_of::<SERVICE_STATUS_PROCESS>() as u32 {
            return Err("query service status did not return a complete buffer".into());
        }
        Ok(unsafe {
            bytes
                .as_ptr()
                .cast::<SERVICE_STATUS_PROCESS>()
                .read_unaligned()
        })
    }

    fn status_name(status: SERVICE_STATUS_PROCESS) -> Result<&'static str, String> {
        match status.dwCurrentState {
            SERVICE_RUNNING => Ok("Running"),
            SERVICE_STOPPED => Ok("Stopped"),
            _ => Err("service has a pending or unsupported status; refusing lossy capture".into()),
        }
    }

    fn configure_start(service: SC_HANDLE, start: SERVICE_START_TYPE) -> Result<(), String> {
        unsafe {
            ChangeServiceConfigW(
                service,
                ENUM_SERVICE_TYPE(SERVICE_NO_CHANGE),
                start,
                SERVICE_ERROR(SERVICE_NO_CHANGE),
                PCWSTR::null(),
                PCWSTR::null(),
                None,
                PCWSTR::null(),
                PCWSTR::null(),
                PCWSTR::null(),
                PCWSTR::null(),
            )
        }
        .map_err(|error| format!("change service startup type: {error}"))
    }

    fn configure_delayed_auto_start(service: SC_HANDLE, delayed: bool) -> Result<(), String> {
        let delayed_info = SERVICE_DELAYED_AUTO_START_INFO {
            fDelayedAutostart: BOOL::from(delayed),
        };
        unsafe {
            ChangeServiceConfig2W(
                service,
                SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
                Some((&delayed_info as *const SERVICE_DELAYED_AUTO_START_INFO).cast()),
            )
        }
        .map_err(|error| format!("change delayed auto-start: {error}"))
    }

    fn wait_for_state(
        service: SC_HANDLE,
        expected: windows::Win32::System::Services::SERVICE_STATUS_CURRENT_STATE,
    ) -> Result<(), String> {
        for _ in 0..WAIT_ATTEMPTS {
            if query_status(service)?.dwCurrentState == expected {
                return Ok(());
            }
            thread::sleep(WAIT_INTERVAL);
        }
        Err("service did not reach its required state before the bounded wait expired".into())
    }

    fn stop(service: SC_HANDLE) -> Result<(), String> {
        if query_status(service)?.dwCurrentState == SERVICE_STOPPED {
            return Ok(());
        }
        let mut status = SERVICE_STATUS::default();
        unsafe { ControlService(service, SERVICE_CONTROL_STOP, &mut status) }
            .map_err(|error| format!("stop service: {error}"))?;
        wait_for_state(service, SERVICE_STOPPED)
    }

    pub(super) fn capture_present(names: &[String]) -> Result<Vec<ServiceSnapshot>, String> {
        let manager = open_manager()?;
        let mut captured = Vec::new();
        for name in names {
            let Some(service) = open_present(manager.0, name)? else {
                continue;
            };
            let (start_type, delayed_auto_start) = query_config(service.0)?;
            let status = status_name(query_status(service.0)?)?.to_owned();
            captured.push(ServiceSnapshot {
                name: name.clone(),
                start_type: start_type_name(start_type)?.to_owned(),
                delayed_auto_start,
                status,
            });
        }
        Ok(captured)
    }

    pub(super) fn inspect_batch(
        names: &[String],
        batch: ServiceBatch,
    ) -> Result<Inspection, String> {
        let manager = open_manager()?;
        let mut present = 0_usize;
        let mut satisfied = true;
        for name in names {
            let Some(service) = open_present(manager.0, name)? else {
                continue;
            };
            present += 1;
            let (start_type, _) = query_config(service.0)?;
            let state = query_status(service.0)?.dwCurrentState;
            let disabled_and_stopped = match batch {
                ServiceBatch::WindowsUpdate => {
                    start_type == SERVICE_DISABLED && state != SERVICE_RUNNING
                }
                ServiceBatch::SysMainSearchQwaveXbox => {
                    start_type == SERVICE_DISABLED && state == SERVICE_STOPPED
                }
            };
            satisfied &= disabled_and_stopped;
        }
        if present == 0 {
            Ok(Inspection::Inapplicable)
        } else if satisfied {
            Ok(Inspection::Satisfied)
        } else {
            Ok(Inspection::NeedsApply)
        }
    }

    pub(super) fn disable_stop_batch(names: &[String]) -> Result<(), String> {
        let manager = open_manager()?;
        for name in names {
            let service = open_present(manager.0, name)?
                .ok_or_else(|| format!("captured service disappeared before mutation: {name}"))?;
            configure_start(service.0, SERVICE_DISABLED)?;
            let (start_type, _) = query_config(service.0)?;
            if start_type != SERVICE_DISABLED {
                return Err(format!(
                    "service startup-type readback is not Disabled: {name}"
                ));
            }
            stop(service.0)?;
        }
        Ok(())
    }

    pub(super) fn verify_disabled_stopped(
        names: &[String],
        batch: ServiceBatch,
    ) -> Result<(), String> {
        let manager = open_manager()?;
        for name in names {
            let service = open_present(manager.0, name)?.ok_or_else(|| {
                format!("captured service disappeared before verification: {name}")
            })?;
            let (start_type, _) = query_config(service.0)?;
            if start_type != SERVICE_DISABLED {
                return Err(format!("service is not Disabled: {name}"));
            }
            let state = query_status(service.0)?.dwCurrentState;
            let valid = match batch {
                ServiceBatch::WindowsUpdate => state != SERVICE_RUNNING,
                ServiceBatch::SysMainSearchQwaveXbox => state == SERVICE_STOPPED,
            };
            if !valid {
                return Err(format!("service postcondition was not observed: {name}"));
            }
        }
        Ok(())
    }

    pub(super) fn restore(
        name: &str,
        original_start: &str,
        delayed: bool,
        original_status: &str,
    ) -> Result<(), String> {
        let manager = open_manager()?;
        let service = open_present(manager.0, name)?
            .ok_or_else(|| format!("captured service no longer exists: {name}"))?;
        let start = start_type(original_start)?;
        configure_start(service.0, start)?;
        configure_delayed_auto_start(service.0, delayed)?;
        match original_status {
            "Running" => {
                if query_status(service.0)?.dwCurrentState != SERVICE_RUNNING {
                    unsafe { windows::Win32::System::Services::StartServiceW(service.0, None) }
                        .map_err(|error| format!("start service during restore: {error}"))?;
                }
                wait_for_state(service.0, SERVICE_RUNNING)?;
            }
            "Stopped" => stop(service.0)?,
            _ => return Err("service backup has an unsupported original status".into()),
        }
        let (read_start, read_delayed) = query_config(service.0)?;
        if read_start != start || read_delayed != delayed {
            return Err("service restoration configuration readback mismatch".into());
        }
        if status_name(query_status(service.0)?)? != original_status {
            return Err("service restoration status readback mismatch".into());
        }
        Ok(())
    }
}
#[cfg(not(windows))]
mod native_services {
    use super::{Inspection, ServiceBatch, ServiceSnapshot};

    pub(super) fn inspect_batch(_: &[String], _: ServiceBatch) -> Result<Inspection, String> {
        Err("service inspection requires Windows SCM".into())
    }
    pub(super) fn capture_present(_: &[String]) -> Result<Vec<ServiceSnapshot>, String> {
        Err("service capture requires Windows SCM".into())
    }
    pub(super) fn disable_stop_batch(_: &[String]) -> Result<(), String> {
        Err("service mutation requires Windows SCM".into())
    }
    pub(super) fn verify_disabled_stopped(_: &[String], _: ServiceBatch) -> Result<(), String> {
        Err("service verification requires Windows SCM".into())
    }
    pub(super) fn restore(_: &str, _: &str, _: bool, _: &str) -> Result<(), String> {
        Err("service restore requires Windows SCM".into())
    }
}
#[cfg(windows)]
mod native_task_scheduler {
    use windows::{
        Win32::{
            Foundation::VARIANT_BOOL,
            System::{
                Com::{
                    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance,
                    CoInitializeEx,
                },
                TaskScheduler::{ITaskService, TaskScheduler},
                Variant::VARIANT,
            },
        },
        core::{BSTR, IUnknown},
    };

    pub(super) fn restore(
        name: &str,
        path: &str,
        existed: bool,
        enabled: bool,
    ) -> Result<(), String> {
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if initialized.is_err() && initialized.0 != 1 {
            return Err(format!("initialize Task Scheduler COM: {initialized:?}"));
        }
        let service: ITaskService =
            unsafe { CoCreateInstance(&TaskScheduler, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }
                .map_err(|error| format!("create Task Scheduler service: {error}"))?;
        let empty = VARIANT::default();
        unsafe { service.Connect(&empty, &empty, &empty, &empty) }
            .map_err(|error| format!("connect Task Scheduler: {error}"))?;
        let folder = unsafe { service.GetFolder(&BSTR::from(path)) }
            .map_err(|error| format!("open task folder: {error}"))?;
        let task_name = BSTR::from(name);
        if !existed {
            unsafe { folder.DeleteTask(&task_name, 0) }
                .map_err(|error| format!("delete suite-created task: {error}"))?;
            return Ok(());
        }
        let task = unsafe { folder.GetTask(&task_name) }
            .map_err(|error| format!("open observed task: {error}"))?;
        unsafe { task.SetEnabled(VARIANT_BOOL::from(enabled)) }
            .map_err(|error| format!("restore task enabled state: {error}"))?;
        let observed = unsafe { task.Enabled() }
            .map_err(|error| format!("verify task enabled state: {error}"))?;
        if observed.as_bool() == enabled {
            Ok(())
        } else {
            Err("Task Scheduler enabled-state verification failed".into())
        }
    }
}
#[cfg(not(windows))]
mod native_task_scheduler {
    pub(super) fn restore(_: &str, _: &str, _: bool, _: bool) -> Result<(), String> {
        Err("scheduled-task restore requires Windows Task Scheduler COM".into())
    }
}
