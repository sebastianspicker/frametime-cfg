use frametime_hardware::{CpuIdentity, DiagnosticError, GpuAdapter, GpuInventory, SystemStatus};
use raw_cpuid::CpuId;
use std::mem::size_of;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND, IDXGIFactory1,
};
use windows::Win32::System::SystemInformation::{
    GetNativeSystemInfo, GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO,
};

pub(crate) fn cpu_identity() -> Result<CpuIdentity, DiagnosticError> {
    let cpuid = CpuId::new();
    let feature = cpuid.get_feature_info();
    let vendor = cpuid.get_vendor_info().map(|item| item.as_str().to_owned());
    let display_name = cpuid.get_processor_brand_string().map_or_else(
        || "x86 processor".into(),
        |item| item.as_str().trim().to_owned(),
    );
    let system_info = system_info();
    Ok(CpuIdentity {
        display_name,
        vendor,
        family: feature.as_ref().map(raw_cpuid::FeatureInfo::family_id),
        model: feature.as_ref().map(raw_cpuid::FeatureInfo::model_id),
        logical_processors: system_info.dwNumberOfProcessors,
        physical_cores: None,
        source: "CPUID + GetNativeSystemInfo".into(),
    })
}

pub(crate) fn gpu_inventory() -> Result<GpuInventory, DiagnosticError> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(windows_error)?;
    let mut adapters = Vec::new();
    for index in 0_u32..256 {
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(windows_error(error)),
        };
        let description = unsafe { adapter.GetDesc1() }.map_err(windows_error)?;
        let display_name = utf16z(&description.Description);
        if display_name.is_empty() {
            return Err(DiagnosticError::system(
                "DXGI returned an empty adapter description",
            ));
        }
        let vendor = match description.VendorId {
            0x1002 => Some("AMD".into()),
            0x10de => Some("NVIDIA".into()),
            0x8086 => Some("Intel".into()),
            _ => None,
        };
        adapters.push(GpuAdapter {
            stable_id: format!(
                "pci-{:04x}-{:04x}-{:08x}",
                description.VendorId, description.DeviceId, description.SubSysId
            ),
            display_name,
            vendor,
            vendor_id: description.VendorId,
            device_id: description.DeviceId,
            subsystem_id: description.SubSysId,
            revision: description.Revision,
            is_software: (description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0,
            source: "DXGI".into(),
        });
    }
    if adapters.is_empty() {
        return Err(DiagnosticError::unavailable(
            "DXGI reported no graphics adapters",
        ));
    }
    Ok(GpuInventory { adapters })
}

pub(crate) fn system_status() -> Result<SystemStatus, DiagnosticError> {
    let info = system_info();
    let mut memory = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..MEMORYSTATUSEX::default()
    };
    unsafe { GlobalMemoryStatusEx(&mut memory) }.map_err(windows_error)?;
    if memory.ullAvailPhys > memory.ullTotalPhys {
        return Err(DiagnosticError::system(
            "Windows returned invalid physical-memory counters",
        ));
    }
    Ok(SystemStatus {
        architecture: architecture_name(unsafe {
            info.Anonymous.Anonymous.wProcessorArchitecture.0
        }),
        logical_processors: info.dwNumberOfProcessors,
        total_physical_memory_bytes: Some(memory.ullTotalPhys),
        available_physical_memory_bytes: Some(memory.ullAvailPhys),
        uptime_ms: unsafe { GetTickCount64() },
        source: "GetNativeSystemInfo + GlobalMemoryStatusEx + GetTickCount64".into(),
    })
}

fn system_info() -> SYSTEM_INFO {
    let mut info = SYSTEM_INFO::default();
    unsafe { GetNativeSystemInfo(&mut info) };
    info
}

fn architecture_name(value: u16) -> String {
    match value {
        0 => "x86".into(),
        9 => "x86_64".into(),
        12 => "arm64".into(),
        other => format!("windows_processor_architecture_{other}"),
    }
}

fn utf16z(value: &[u16]) -> String {
    let end = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

fn windows_error(error: windows::core::Error) -> DiagnosticError {
    DiagnosticError::system(error.to_string())
}
