use northclock_core::{DeviceIdentity, GpuDevice, Measurement, NorthclockError, PowerPlan, Result};
use std::mem::{size_of, MaybeUninit};
use std::time::Duration;
use windows::core::{w, GUID};
use windows::Win32::Foundation::{
    CloseHandle, FreeLibrary, LocalFree, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, FILETIME, HLOCAL,
};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ERROR_NOT_FOUND};
use windows::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use windows::Win32::System::Power::{
    PowerEnumerate, PowerGetActiveScheme, PowerReadFriendlyName, ACCESS_SCHEME,
};
use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, GlobalMemoryStatusEx, RelationProcessorCore, MEMORYSTATUSEX,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows::Win32::System::Threading::{
    GetActiveProcessorCount, GetProcessAffinityMask, GetSystemTimes, OpenProcess,
    SetProcessAffinityMask, ALL_PROCESSOR_GROUPS, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SET_INFORMATION,
};
use windows::Win32::UI::Shell::IsUserAnAdmin;

pub(crate) fn physical_core_count() -> Result<usize> {
    let mut bytes = 0_u32;
    // The sizing call is documented to fail with ERROR_INSUFFICIENT_BUFFER.
    let _ = unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut bytes) };
    if bytes < size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>() as u32 {
        return Err(NorthclockError::Internal(
            "Windows returned an invalid processor-topology buffer size".into(),
        ));
    }
    let byte_len =
        usize::try_from(bytes).map_err(|error| NorthclockError::Internal(error.to_string()))?;
    let element_size = size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>();
    let element_count = byte_len.div_ceil(element_size);
    let mut buffer =
        vec![MaybeUninit::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>::uninit(); element_count];
    unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            Some(buffer.as_mut_ptr().cast()),
            &mut bytes,
        )
    }
    .map_err(windows_error)?;

    let returned =
        usize::try_from(bytes).map_err(|error| NorthclockError::Internal(error.to_string()))?;
    if returned > element_count * element_size {
        return Err(NorthclockError::Internal(
            "Windows wrote beyond the declared topology buffer length".into(),
        ));
    }
    let base = buffer.as_ptr().cast::<u8>();
    let mut offset = 0_usize;
    let mut cores = 0_usize;
    while offset < returned {
        if returned - offset < 8 {
            return Err(NorthclockError::Internal(
                "truncated processor-topology record".into(),
            ));
        }
        let record = unsafe {
            &*base
                .add(offset)
                .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>()
        };
        let size = usize::try_from(record.Size)
            .map_err(|error| NorthclockError::Internal(error.to_string()))?;
        if size < 8 || offset.checked_add(size).is_none_or(|end| end > returned) {
            return Err(NorthclockError::Internal(
                "invalid processor-topology record size".into(),
            ));
        }
        if record.Relationship == RelationProcessorCore {
            cores += 1;
        }
        offset += size;
    }
    if cores == 0 {
        return Err(NorthclockError::Unavailable(
            "Windows reported no physical processor cores".into(),
        ));
    }
    Ok(cores)
}

pub(crate) fn logical_processor_count() -> Result<usize> {
    let count = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) };
    if count == 0 {
        return Err(NorthclockError::Unavailable(
            "Windows reported no active logical processors".into(),
        ));
    }
    usize::try_from(count).map_err(|error| NorthclockError::Internal(error.to_string()))
}

pub(crate) fn cpu_measurements() -> Result<Vec<Measurement<f64>>> {
    let first = system_times()?;
    std::thread::sleep(Duration::from_millis(100));
    let second = system_times()?;
    let idle_delta = second.0.saturating_sub(first.0);
    let total_delta = second.1.saturating_sub(first.1);
    if total_delta == 0 {
        return Err(NorthclockError::Unavailable(
            "Windows CPU counters did not advance".into(),
        ));
    }
    let busy_delta = total_delta.saturating_sub(idle_delta);
    let utilization = (busy_delta as f64) * 100.0 / (total_delta as f64);

    let mut memory = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..MEMORYSTATUSEX::default()
    };
    unsafe { GlobalMemoryStatusEx(&mut memory) }.map_err(windows_error)?;
    if memory.ullTotalPhys == 0 || memory.ullAvailPhys > memory.ullTotalPhys {
        return Err(NorthclockError::Internal(
            "Windows returned invalid physical-memory counters".into(),
        ));
    }
    let used_memory = memory.ullTotalPhys - memory.ullAvailPhys;
    let cpu = super::cpu_identity()?.device;
    let memory_device =
        DeviceIdentity::new("system_memory", "system-memory", "System memory", None);
    Ok(vec![
        Measurement::now(utilization, "%", cpu, "GetSystemTimes")?,
        Measurement::now(
            used_memory as f64,
            "bytes",
            memory_device,
            "GlobalMemoryStatusEx",
        )?,
    ])
}

fn system_times() -> Result<(u64, u64)> {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }
        .map_err(windows_error)?;
    let idle = filetime_to_u64(idle);
    let total = filetime_to_u64(kernel).saturating_add(filetime_to_u64(user));
    Ok((idle, total))
}

fn filetime_to_u64(value: FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

pub(crate) fn gpu_devices() -> Result<Vec<GpuDevice>> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(windows_error)?;
    let mut devices = Vec::new();
    for index in 0_u32..256 {
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(windows_error(error)),
        };
        let description = unsafe { adapter.GetDesc1() }.map_err(windows_error)?;
        let display_name = utf16z(&description.Description);
        if display_name.is_empty() {
            return Err(NorthclockError::Internal(
                "DXGI adapter description was empty".into(),
            ));
        }
        let vendor = match description.VendorId {
            0x1002 => Some("AMD".to_string()),
            0x10de => Some("NVIDIA".to_string()),
            0x8086 => Some("Intel".to_string()),
            _ => None,
        };
        let stable_id = format!(
            "pci-{:04x}-{:04x}-{:08x}",
            description.VendorId, description.DeviceId, description.SubSysId
        );
        devices.push(GpuDevice {
            device: DeviceIdentity::new("gpu", stable_id, display_name, vendor),
            dedicated_memory_bytes: Some(description.DedicatedVideoMemory as u64),
            driver_backend: "DXGI".into(),
        });
    }
    if devices.is_empty() {
        return Err(NorthclockError::Unavailable(
            "DXGI reported no graphics adapters".into(),
        ));
    }
    Ok(devices)
}

fn utf16z(value: &[u16]) -> String {
    let end = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

pub(crate) fn is_elevated() -> Result<bool> {
    Ok(unsafe { IsUserAnAdmin() }.as_bool())
}

pub(crate) fn process_affinity(process_id: u32) -> Result<(u64, u64)> {
    if process_id == 0 {
        return Err(NorthclockError::InvalidUsage(
            "process id must be non-zero".into(),
        ));
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(windows_error)?;
    let mut process_mask = 0_usize;
    let mut system_mask = 0_usize;
    let operation = unsafe { GetProcessAffinityMask(process, &mut process_mask, &mut system_mask) }
        .map_err(windows_error)
        .map(|()| (process_mask as u64, system_mask as u64));
    let close = unsafe { CloseHandle(process) }.map_err(windows_error);
    combine_operation_and_close(operation, close)
}

pub(crate) fn set_process_affinity(process_id: u32, mask: u64) -> Result<()> {
    if process_id == 0 || mask == 0 {
        return Err(NorthclockError::InvalidUsage(
            "process id and affinity mask must be non-zero".into(),
        ));
    }
    let mask = usize::try_from(mask).map_err(|_| {
        NorthclockError::InvalidUsage("affinity mask does not fit this Windows target".into())
    })?;
    let access = PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION;
    let process = unsafe { OpenProcess(access, false, process_id) }.map_err(windows_error)?;
    let operation = unsafe { SetProcessAffinityMask(process, mask) }.map_err(windows_error);
    let close = unsafe { CloseHandle(process) }.map_err(windows_error);
    combine_operation_and_close(operation, close)
}

fn combine_operation_and_close<T>(operation: Result<T>, close: Result<()>) -> Result<T> {
    match (operation, close) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(operation_error), Ok(())) => Err(operation_error),
        (Ok(_), Err(close_error)) => Err(close_error),
        (Err(operation_error), Err(close_error)) => {
            Err(NorthclockError::HardwareOperation(format!(
                "{operation_error}; closing the Windows process handle also failed: {close_error}"
            )))
        }
    }
}

pub(crate) fn power_plans() -> Result<Vec<PowerPlan>> {
    let active = active_power_scheme()?;
    let mut plans = Vec::new();
    for index in 0_u32..1024 {
        let mut guid = GUID::zeroed();
        let mut size = size_of::<GUID>() as u32;
        let status = unsafe {
            PowerEnumerate(
                None,
                None,
                None,
                ACCESS_SCHEME,
                index,
                Some((&raw mut guid).cast()),
                &mut size,
            )
        };
        if status == ERROR_NO_MORE_ITEMS {
            break;
        }
        if status != ERROR_SUCCESS || size as usize != size_of::<GUID>() {
            return Err(win32_error("PowerEnumerate", status.0));
        }
        plans.push(PowerPlan {
            guid: format_guid(&guid),
            name: power_scheme_name(&guid)?,
            active: guid == active,
        });
    }
    if plans.is_empty() {
        return Err(NorthclockError::Unavailable(
            "Windows reported no power schemes".into(),
        ));
    }
    Ok(plans)
}

pub(crate) fn installed_adlx() -> bool {
    loadable_system_library(w!("amdadlx64.dll"))
}

fn loadable_system_library(name: windows::core::PCWSTR) -> bool {
    let Ok(module) = (unsafe { LoadLibraryExW(name, None, LOAD_LIBRARY_SEARCH_SYSTEM32) }) else {
        return false;
    };
    unsafe { FreeLibrary(module) }.is_ok()
}

fn active_power_scheme() -> Result<GUID> {
    let mut pointer = std::ptr::null_mut::<GUID>();
    let status = unsafe { PowerGetActiveScheme(None, &raw mut pointer) };
    if status != ERROR_SUCCESS || pointer.is_null() {
        return Err(win32_error("PowerGetActiveScheme", status.0));
    }
    let guid = unsafe { *pointer };
    let remaining = unsafe { LocalFree(Some(HLOCAL(pointer.cast()))) };
    if !remaining.is_invalid() {
        return Err(NorthclockError::Internal(
            "LocalFree did not release the active power-scheme GUID".into(),
        ));
    }
    Ok(guid)
}

fn power_scheme_name(guid: &GUID) -> Result<String> {
    let mut byte_len = 0_u32;
    let first = unsafe { PowerReadFriendlyName(None, Some(guid), None, None, None, &mut byte_len) };
    if first == ERROR_SUCCESS && byte_len == 0 {
        return Ok(format!("Power scheme {}", format_guid(guid)));
    }
    if byte_len < 2 || !byte_len.is_multiple_of(2) {
        return Err(win32_error("PowerReadFriendlyName(size)", first.0));
    }
    let mut buffer = vec![0_u16; (byte_len as usize).div_ceil(2)];
    let second = unsafe {
        PowerReadFriendlyName(
            None,
            Some(guid),
            None,
            None,
            Some(buffer.as_mut_ptr().cast()),
            &mut byte_len,
        )
    };
    if second != ERROR_SUCCESS || byte_len as usize > buffer.len() * 2 {
        return Err(win32_error("PowerReadFriendlyName", second.0));
    }
    Ok(utf16z(&buffer))
}

fn format_guid(guid: &GUID) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    )
}

fn win32_error(api: &str, code: u32) -> NorthclockError {
    NorthclockError::HardwareOperation(format!("{api} failed with Win32 error {code}"))
}

fn windows_error(error: windows::core::Error) -> NorthclockError {
    NorthclockError::HardwareOperation(format!(
        "Windows API failure {}: {}",
        error.code(),
        error.message()
    ))
}
