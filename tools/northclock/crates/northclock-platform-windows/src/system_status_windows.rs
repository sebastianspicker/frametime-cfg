//! Read-only Windows system-status observations.

use super::system_status_catalog::{PROCESS_IDENTIFIERS, SERVICE_IDENTIFIERS};
use northclock_core::{
    ConflictKind, Observation, PotentialConflict, RegisteredTask, ScheduledTaskState,
    SystemStatusReport, TaskSchedulerStatus, VbsRuntimeState, VbsStatus,
};
use std::mem::{size_of, zeroed};
use windows::core::{w, Error, BSTR, HRESULT, PCWSTR, PWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_Status, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceRegistryPropertyW,
    CM_DEVNODE_STATUS_FLAGS, CM_PROB, CM_PROB_NORMAL_CONFLICT, CR_SUCCESS, DIGCF_ALLCLASSES,
    DIGCF_PRESENT, DN_HAS_PROBLEM, HDEVINFO, SPDRP_DEVICEDESC, SPDRP_FRIENDLYNAME, SP_DEVINFO_DATA,
};
use windows::Win32::Foundation::{CloseHandle, E_ACCESSDENIED, HANDLE, RPC_E_CHANGED_MODE};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_MULTITHREADED, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Services::{
    CloseServiceHandle, EnumServicesStatusExW, OpenSCManagerW, ENUM_SERVICE_STATUS_PROCESSW,
    SC_ENUM_PROCESS_INFO, SC_HANDLE, SC_MANAGER_ENUMERATE_SERVICE, SERVICE_DRIVER,
    SERVICE_STATE_ALL, SERVICE_WIN32,
};
use windows::Win32::System::TaskScheduler::{ITaskService, TaskScheduler, TASK_ENUM_HIDDEN};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::System::Wmi::{
    IWbemLocator, WbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY,
};

const MAX_TASKS: usize = 128;
const MAX_PROCESSES: usize = 4096;
const MAX_SERVICES: usize = 4096;
const MAX_DEVICES: usize = 8192;
const MAX_CONFLICTS: usize = 128;
const MAX_SERVICE_BYTES: usize = 4 * 1024 * 1024;
const MAX_UTF16_UNITS: usize = 32_768;
const WMI_NEXT_TIMEOUT_MS: i32 = 1_000;
const NORTHCLOCK_TASK_FOLDER: &str = r"\Northclock";

pub(super) fn observe() -> northclock_core::Result<SystemStatusReport> {
    Ok(SystemStatusReport {
        task_scheduler: task_scheduler_observation(),
        virtualization_based_security: vbs_observation(),
        potential_conflicts: conflicts_observation(),
    })
}

fn task_scheduler_observation() -> Observation<TaskSchedulerStatus> {
    match observe_tasks() {
        Ok(Some(status)) => Observation::observed("Task Scheduler 2.0 COM", status),
        Ok(None) => Observation::not_found("Task Scheduler 2.0 COM: \\Northclock"),
        Err(error) if is_permission_error(&error) => {
            Observation::permission_required("Task Scheduler 2.0 COM", error.to_string())
        }
        Err(error) => Observation::unavailable("Task Scheduler 2.0 COM", error.to_string()),
    }
}

fn observe_tasks() -> windows::core::Result<Option<TaskSchedulerStatus>> {
    let _apartment = ComApartment::initialize()?;
    let service: ITaskService =
        unsafe { CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER) }?;
    let empty = VARIANT::default();
    unsafe { service.Connect(&empty, &empty, &empty, &empty) }?;
    let folder_name = BSTR::from(NORTHCLOCK_TASK_FOLDER);
    let folder = match unsafe { service.GetFolder(&folder_name) } {
        Ok(folder) => folder,
        Err(error) if is_not_found_error(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let tasks = unsafe { folder.GetTasks(TASK_ENUM_HIDDEN.0) }?;
    let count = unsafe { tasks.Count() }?;
    if !(0..=MAX_TASKS as i32).contains(&count) {
        return Err(Error::new(
            HRESULT(0x8007_0057u32 as i32),
            "Task Scheduler returned an invalid or over-limit task count",
        ));
    }

    let mut registered = Vec::with_capacity(count as usize);
    for index in 1..=count {
        let task = unsafe { tasks.get_Item(&VARIANT::from(index)) }?;
        let name = unsafe { task.Name() }?.to_string();
        let path = unsafe { task.Path() }?.to_string();
        let state_raw = unsafe { task.State() }?.0;
        let enabled = unsafe { task.Enabled() }?.as_bool();
        registered.push(RegisteredTask {
            name,
            path,
            state: ScheduledTaskState::from_raw(state_raw),
            state_raw,
            enabled,
        });
    }
    Ok(Some(TaskSchedulerStatus {
        folder_path: NORTHCLOCK_TASK_FOLDER.into(),
        tasks: registered,
    }))
}

fn vbs_observation() -> Observation<VbsStatus> {
    match observe_vbs() {
        Ok(value) => Observation::observed("Win32_DeviceGuard WMI", value),
        Err(error) if is_permission_error(&error) => {
            Observation::permission_required("Win32_DeviceGuard WMI", error.to_string())
        }
        Err(error) => Observation::unavailable("Win32_DeviceGuard WMI", error.to_string()),
    }
}

fn observe_vbs() -> windows::core::Result<VbsStatus> {
    let _apartment = ComApartment::initialize()?;
    let locator: IWbemLocator =
        unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER) }?;
    let namespace = BSTR::from(r"ROOT\Microsoft\Windows\DeviceGuard");
    let empty = BSTR::new();
    let services =
        unsafe { locator.ConnectServer(&namespace, &empty, &empty, &empty, 0, &empty, None) }?;
    unsafe {
        CoSetProxyBlanket(
            &services,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            PCWSTR::null(),
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )
    }?;
    let language = BSTR::from("WQL");
    let query = BSTR::from("SELECT VirtualizationBasedSecurityStatus FROM Win32_DeviceGuard");
    let enumerator = unsafe {
        services.ExecQuery(
            &language,
            &query,
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None,
        )
    }?;
    let mut values = [None];
    let mut returned = 0_u32;
    unsafe {
        enumerator
            .Next(WMI_NEXT_TIMEOUT_MS, &mut values, &mut returned)
            .ok()?
    };
    if returned != 1 {
        return Err(Error::new(
            HRESULT(0x8004_1002u32 as i32),
            "Win32_DeviceGuard returned no status object within the bounded query",
        ));
    }
    let object = values[0].take().ok_or_else(|| {
        Error::new(
            HRESULT(0x8000_4003u32 as i32),
            "Win32_DeviceGuard returned a null status object",
        )
    })?;
    let mut value = VARIANT::default();
    unsafe {
        object.Get(
            w!("VirtualizationBasedSecurityStatus"),
            0,
            &mut value,
            None,
            None,
        )
    }?;
    let raw = u32::try_from(&value).map_err(|_| {
        Error::new(
            HRESULT(0x8004_1005u32 as i32),
            "Win32_DeviceGuard returned a non-u32 VBS status",
        )
    })?;
    Ok(VbsStatus {
        runtime_state: VbsRuntimeState::from_raw(raw),
        runtime_state_raw: raw,
    })
}

fn conflicts_observation() -> Observation<Vec<PotentialConflict>> {
    let mut findings = Vec::new();
    let mut failures = Vec::new();
    let mut successful_sources = 0_u8;

    match observe_processes() {
        Ok(mut values) => {
            successful_sources += 1;
            findings.append(&mut values);
        }
        Err(error) => failures.push((format!("Tool Help: {error}"), is_permission_error(&error))),
    }
    match observe_services() {
        Ok(mut values) => {
            successful_sources += 1;
            findings.append(&mut values);
        }
        Err(error) => failures.push((format!("SCM: {error}"), is_permission_error(&error))),
    }
    match observe_present_pci_devices() {
        Ok(mut values) => {
            successful_sources += 1;
            findings.append(&mut values);
        }
        Err(error) => failures.push((format!("SetupAPI: {error}"), is_permission_error(&error))),
    }
    findings.truncate(MAX_CONFLICTS);
    let failure_message = || {
        failures
            .iter()
            .map(|(message, _)| message.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    };
    if failures.is_empty() {
        Observation::observed("Tool Help + SCM + SetupAPI", findings)
    } else if successful_sources > 0 {
        Observation::partial("Tool Help + SCM + SetupAPI", findings, failure_message())
    } else if failures.iter().all(|(_, permission)| *permission) {
        Observation::permission_required("Tool Help + SCM + SetupAPI", failure_message())
    } else {
        Observation::unavailable("Tool Help + SCM + SetupAPI", failure_message())
    }
}

fn observe_processes() -> windows::core::Result<Vec<PotentialConflict>> {
    let snapshot = SnapshotHandle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }?);
    let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    unsafe { Process32FirstW(snapshot.0, &mut entry) }?;
    let mut results = Vec::new();
    for _ in 0..MAX_PROCESSES {
        let executable = utf16_array(&entry.szExeFile)?;
        if let Some(identifier) = classified_identifier(&executable, PROCESS_IDENTIFIERS) {
            results.push(PotentialConflict {
                kind: ConflictKind::Process,
                identifier: identifier.to_owned(),
                display_name: executable,
                process_id: Some(entry.th32ProcessID),
                active: true,
                reason: "known hardware-control executable; potential overlap only".into(),
            });
        }
        entry = unsafe { zeroed() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        match unsafe { Process32NextW(snapshot.0, &mut entry) } {
            Ok(()) => {}
            Err(error) if is_no_more_items(&error) => return Ok(results),
            Err(error) => return Err(error),
        }
    }
    Err(Error::new(
        HRESULT(0x8007_0057u32 as i32),
        "Tool Help process enumeration exceeded its 4096-entry bound",
    ))
}

fn observe_services() -> windows::core::Result<Vec<PotentialConflict>> {
    let manager =
        ServiceHandle(unsafe { OpenSCManagerW(None, None, SC_MANAGER_ENUMERATE_SERVICE) }?);
    let mut results = enum_service_group(manager.0, SERVICE_WIN32, ConflictKind::Service)?;
    results.extend(enum_service_group(
        manager.0,
        SERVICE_DRIVER,
        ConflictKind::Driver,
    )?);
    Ok(results)
}

fn enum_service_group(
    manager: SC_HANDLE,
    kind: windows::Win32::System::Services::ENUM_SERVICE_TYPE,
    conflict_kind: ConflictKind,
) -> windows::core::Result<Vec<PotentialConflict>> {
    let mut needed = 0_u32;
    let mut returned = 0_u32;
    let first = unsafe {
        EnumServicesStatusExW(
            manager,
            SC_ENUM_PROCESS_INFO,
            kind,
            SERVICE_STATE_ALL,
            None,
            &mut needed,
            &mut returned,
            None,
            None,
        )
    };
    let needed = needed as usize;
    match first {
        Ok(()) if needed == 0 && returned == 0 => return Ok(Vec::new()),
        Err(error) if needed == 0 => return Err(error),
        _ => {}
    }
    if needed == 0 || needed > MAX_SERVICE_BYTES {
        return Err(Error::new(
            HRESULT(0x8007_0057u32 as i32),
            "SCM returned an invalid or over-limit service buffer size",
        ));
    }
    let mut buffer = vec![0_u8; needed];
    let mut buffer_size = needed as u32;
    returned = 0;
    unsafe {
        EnumServicesStatusExW(
            manager,
            SC_ENUM_PROCESS_INFO,
            kind,
            SERVICE_STATE_ALL,
            Some(&mut buffer),
            &mut buffer_size,
            &mut returned,
            None,
            None,
        )
    }?;
    if buffer_size as usize > buffer.len() {
        return Err(Error::new(
            HRESULT(0x8007_0057u32 as i32),
            "SCM reported bytes beyond the supplied service buffer",
        ));
    }
    let count = returned as usize;
    if count > MAX_SERVICES
        || count.saturating_mul(size_of::<ENUM_SERVICE_STATUS_PROCESSW>()) > buffer.len()
    {
        return Err(Error::new(
            HRESULT(0x8007_0057u32 as i32),
            "SCM returned an invalid or over-limit service count",
        ));
    }
    let mut results = Vec::new();
    for index in 0..count {
        let offset = index * size_of::<ENUM_SERVICE_STATUS_PROCESSW>();
        let entry = unsafe {
            (buffer.as_ptr().add(offset) as *const ENUM_SERVICE_STATUS_PROCESSW).read_unaligned()
        };
        let name = utf16_pointer_in_buffer(entry.lpServiceName, &buffer)?;
        if let Some(identifier) = classified_identifier(&name, SERVICE_IDENTIFIERS) {
            let display_name = utf16_pointer_in_buffer(entry.lpDisplayName, &buffer)?;
            results.push(PotentialConflict {
                kind: conflict_kind,
                identifier: identifier.to_owned(),
                display_name,
                process_id: None,
                active: entry.ServiceStatusProcess.dwCurrentState.0 != 1,
                reason: "known hardware-control service or driver; potential overlap only".into(),
            });
        }
    }
    Ok(results)
}

fn observe_present_pci_devices() -> windows::core::Result<Vec<PotentialConflict>> {
    let set = DeviceInfoSet(unsafe {
        SetupDiGetClassDevsW(None, w!("PCI"), None, DIGCF_PRESENT | DIGCF_ALLCLASSES)
    }?);
    let mut results = Vec::new();
    for index in 0..MAX_DEVICES {
        let mut data: SP_DEVINFO_DATA = unsafe { zeroed() };
        data.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
        match unsafe { SetupDiEnumDeviceInfo(set.0, index as u32, &mut data) } {
            Ok(()) => {}
            Err(error) if is_no_more_items(&error) => return Ok(results),
            Err(error) => return Err(error),
        }
        let mut status = CM_DEVNODE_STATUS_FLAGS::default();
        let mut problem = CM_PROB::default();
        let result = unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, data.DevInst, 0) };
        if result != CR_SUCCESS {
            return Err(Error::new(
                HRESULT(0x8000_4005u32 as i32),
                format!("CM_Get_DevNode_Status failed with CONFIGRET {}", result.0),
            ));
        }
        if status.contains(DN_HAS_PROBLEM) && problem == CM_PROB_NORMAL_CONFLICT {
            let identifier = device_instance_id(set.0, &data)?;
            let display_name = device_registry_string(set.0, &data, SPDRP_FRIENDLYNAME)
                .or_else(|_| device_registry_string(set.0, &data, SPDRP_DEVICEDESC))
                .unwrap_or_else(|_| identifier.clone());
            results.push(PotentialConflict {
                kind: ConflictKind::Device,
                identifier,
                display_name,
                process_id: None,
                active: true,
                reason: "Windows PnP reports Code 12 (CM_PROB_NORMAL_CONFLICT); the competing device is not identified".into(),
            });
        }
    }
    Err(Error::new(
        HRESULT(0x8007_0057u32 as i32),
        "SetupAPI PCI device enumeration exceeded its 8192-entry bound",
    ))
}

fn device_instance_id(set: HDEVINFO, data: &SP_DEVINFO_DATA) -> windows::core::Result<String> {
    let mut required = 0_u32;
    let first = unsafe { SetupDiGetDeviceInstanceIdW(set, data, None, Some(&mut required)) };
    if first.is_err() && required == 0 {
        return first.map(|_| String::new());
    }
    let length =
        usize::try_from(required).map_err(|_| invalid_data("device ID length overflow"))?;
    if length == 0 || length > MAX_UTF16_UNITS {
        return Err(invalid_data("invalid or over-limit device ID length"));
    }
    let mut buffer = vec![0_u16; length];
    unsafe { SetupDiGetDeviceInstanceIdW(set, data, Some(&mut buffer), Some(&mut required)) }?;
    utf16_array(&buffer)
}

fn device_registry_string(
    set: HDEVINFO,
    data: &SP_DEVINFO_DATA,
    property: windows::Win32::Devices::DeviceAndDriverInstallation::SETUP_DI_REGISTRY_PROPERTY,
) -> windows::core::Result<String> {
    let mut required = 0_u32;
    let first = unsafe {
        SetupDiGetDeviceRegistryPropertyW(set, data, property, None, None, Some(&mut required))
    };
    if first.is_err() && required == 0 {
        return first.map(|_| String::new());
    }
    let length = usize::try_from(required).map_err(|_| invalid_data("property length overflow"))?;
    if length < size_of::<u16>()
        || length % size_of::<u16>() != 0
        || length > MAX_UTF16_UNITS * size_of::<u16>()
    {
        return Err(invalid_data("invalid or over-limit device property length"));
    }
    let mut buffer = vec![0_u8; length];
    unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            set,
            data,
            property,
            None,
            Some(&mut buffer),
            Some(&mut required),
        )
    }?;
    let units = buffer
        .chunks_exact(size_of::<u16>())
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    utf16_array(&units)
}

fn invalid_data(message: &str) -> Error {
    Error::new(HRESULT(0x8007_0057u32 as i32), message)
}

fn classified_identifier<'a>(candidate: &str, allowlist: &'a [&str]) -> Option<&'a str> {
    allowlist
        .iter()
        .copied()
        .find(|known| candidate.eq_ignore_ascii_case(known))
}

fn utf16_array(value: &[u16]) -> windows::core::Result<String> {
    let end = value.iter().position(|unit| *unit == 0).ok_or_else(|| {
        Error::new(
            HRESULT(0x8007_0057u32 as i32),
            "Windows returned an unterminated UTF-16 string",
        )
    })?;
    String::from_utf16(&value[..end]).map_err(|_| {
        Error::new(
            HRESULT(0x8007_0057u32 as i32),
            "Windows returned invalid UTF-16",
        )
    })
}

fn utf16_pointer_in_buffer(value: PWSTR, buffer: &[u8]) -> windows::core::Result<String> {
    if value.is_null() {
        return Err(Error::new(
            HRESULT(0x8000_4003u32 as i32),
            "Windows returned a null UTF-16 pointer",
        ));
    }
    let start = buffer.as_ptr() as usize;
    let end = start.saturating_add(buffer.len());
    let pointer = value.0 as usize;
    if pointer < start || pointer >= end || !(pointer - start).is_multiple_of(size_of::<u16>()) {
        return Err(Error::new(
            HRESULT(0x8007_0057u32 as i32),
            "SCM returned a UTF-16 pointer outside its service buffer",
        ));
    }
    let length = ((end - pointer) / size_of::<u16>()).min(MAX_UTF16_UNITS);
    if length == 0 {
        return Err(Error::new(
            HRESULT(0x8007_0057u32 as i32),
            "Windows returned an unterminated UTF-16 string",
        ));
    }
    let value = unsafe { std::slice::from_raw_parts(value.0, length) };
    utf16_array(value)
}

fn is_permission_error(error: &Error) -> bool {
    error.code() == E_ACCESSDENIED
        || error.code().0 == 0x8007_0005u32 as i32
        || error.code().0 == 0x8004_1003u32 as i32
}

fn is_not_found_error(error: &Error) -> bool {
    error.code().0 == 0x8007_0002u32 as i32 || error.code().0 == 0x8004_1002u32 as i32
}

fn is_no_more_items(error: &Error) -> bool {
    error.code().0 == 0x8007_0103u32 as i32
}

struct ComApartment(bool);

impl ComApartment {
    fn initialize() -> windows::core::Result<Self> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self(false));
        }
        result.ok()?;
        Ok(Self(true))
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

struct SnapshotHandle(HANDLE);

impl Drop for SnapshotHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

struct ServiceHandle(SC_HANDLE);

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseServiceHandle(self.0) };
    }
}

struct DeviceInfoSet(HDEVINFO);

impl Drop for DeviceInfoSet {
    fn drop(&mut self) {
        let _ = unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifier_requires_exact_case_insensitive_identifier() {
        assert_eq!(
            classified_identifier("MSIAfterburner.EXE", PROCESS_IDENTIFIERS),
            Some("msiafterburner.exe")
        );
        assert_eq!(
            classified_identifier("msiafterburner-helper.exe", PROCESS_IDENTIFIERS),
            None
        );
    }
}
