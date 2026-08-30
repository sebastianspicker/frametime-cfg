#[cfg(all(windows, target_arch = "x86_64"))]
use northclock_core::ErrorCategory;
use northclock_core::{CapabilityReport, CapabilityState};

pub(crate) fn capabilities() -> Vec<CapabilityReport> {
    let platform = platform_support();
    let (gpu_telemetry_state, gpu_telemetry_backend, vendor_apis) = gpu_telemetry_capability();
    let mut reports = platform_capabilities(platform);
    reports.extend(vendor_capabilities(
        platform,
        gpu_telemetry_state,
        gpu_telemetry_backend,
        vendor_apis,
    ));
    reports.extend(windows_runtime_capabilities(platform));
    reports.extend(measurement_capabilities(platform));
    reports
}

#[derive(Clone, Copy)]
struct PlatformSupport {
    state: CapabilityState,
    detail: &'static str,
    is_windows: bool,
}

fn platform_support() -> PlatformSupport {
    if cfg!(windows) {
        PlatformSupport {
            state: CapabilityState::Available,
            detail: "documented Windows API backend",
            is_windows: true,
        }
    } else {
        PlatformSupport {
            state: CapabilityState::Unsupported,
            detail: "Northclock targets Windows 11 x64",
            is_windows: false,
        }
    }
}

fn platform_capabilities(platform: PlatformSupport) -> Vec<CapabilityReport> {
    vec![
        CapabilityReport::new(
            "cpu.identity",
            platform.state,
            "CPUID + Windows topology",
            platform.detail,
        ),
        CapabilityReport::new(
            "cpu.telemetry",
            platform.state,
            "GetSystemTimes",
            platform.detail,
        ),
        CapabilityReport::new(
            "cpu.ryzen_telemetry",
            CapabilityState::Unsupported,
            "none",
            "no provenance-reviewed Ryzen model performance table is registered",
        ),
        CapabilityReport::new(
            "cpu.workload",
            CapabilityState::Available,
            "northclock-platform-windows",
            "bounded multi-threaded workload with measured elapsed time",
        ),
        CapabilityReport::new("gpu.inventory", platform.state, "DXGI", platform.detail),
    ]
}

fn vendor_capabilities(
    platform: PlatformSupport,
    gpu_telemetry_state: CapabilityState,
    gpu_telemetry_backend: String,
    vendor_apis: String,
) -> Vec<CapabilityReport> {
    vec![
        CapabilityReport::new(
            "gpu.telemetry",
            gpu_telemetry_state,
            gpu_telemetry_backend,
            vendor_apis,
        ),
        CapabilityReport::new(
            "gpu.adlx_telemetry",
            if platform.is_windows {
                CapabilityState::Unverified
            } else {
                CapabilityState::Unsupported
            },
            "installed AMD ADLX",
            "the official opaque ADLX interface has no reviewed Rust-only telemetry binding registered",
        ),
        CapabilityReport::new(
            "cpu.tuning",
            CapabilityState::Unverified,
            "experimental KMDF protocol",
            "no physically validated driver backend is registered",
        ),
        CapabilityReport::new(
            "driver.kmdf_runtime",
            CapabilityState::Unverified,
            "experimental protocol validation core",
            "no loadable, packaged, signed, installed, or hardware-qualified KMDF adapter exists",
        ),
        CapabilityReport::new(
            "gpu.tuning",
            CapabilityState::Unverified,
            "ADLX/NVAPI",
            "all write surfaces remain hardware-unverified",
        ),
    ]
}

fn windows_runtime_capabilities(platform: PlatformSupport) -> Vec<CapabilityReport> {
    vec![
        CapabilityReport::new(
            "memory.vram_test",
            platform.state,
            "isolated native D3D12 copy/readback",
            if platform.is_windows {
                "bounded physical adapter test; implementation remains hardware-unverified by project acceptance"
            } else {
                platform.detail
            },
        ),
        CapabilityReport::new(
            "power.plans",
            platform.state,
            "Windows power API",
            platform.detail,
        ),
        CapabilityReport::new(
            "windows.task_scheduler",
            platform.state,
            "Task Scheduler 2.0 COM",
            platform.detail,
        ),
        CapabilityReport::new(
            "windows.vbs_status",
            platform.state,
            "Win32_DeviceGuard WMI",
            platform.detail,
        ),
        CapabilityReport::new(
            "windows.conflict_detection",
            platform.state,
            "Tool Help + Service Control Manager + SetupAPI",
            if platform.is_windows {
                "read-only bounded observation of known overlapping hardware-control processes, services, drivers, and devices"
            } else {
                platform.detail
            },
        ),
        CapabilityReport::new(
            "windows.system_status",
            platform.state,
            "documented Windows observation APIs",
            if platform.is_windows {
                "aggregated read-only Task Scheduler, VBS, and potential-conflict status"
            } else {
                platform.detail
            },
        ),
        CapabilityReport::new(
            "process.affinity",
            platform.state,
            "Windows process API",
            platform.detail,
        ),
        CapabilityReport::new(
            "events.whea",
            platform.state,
            "Windows Event Log API",
            platform.detail,
        ),
    ]
}

fn measurement_capabilities(platform: PlatformSupport) -> Vec<CapabilityReport> {
    vec![
        CapabilityReport::new(
            "frames.capture",
            platform.state,
            "DxgKrnl ETW Present_Start",
            if platform.is_windows {
                "bounded native ETW capture; implementation remains hardware-unverified by project acceptance"
            } else {
                platform.detail
            },
        ),
        CapabilityReport::new(
            "overlay.measurements",
            platform.state,
            "northclock-gui transparent egui viewport",
            if platform.is_windows {
                "read-only overlay renders measured backend values; hardware-unverified"
            } else {
                platform.detail
            },
        ),
        CapabilityReport::new(
            "rom.inspect",
            CapabilityState::Available,
            "bounded file parser",
            "read-only inspection",
        ),
    ]
}

#[cfg(not(windows))]
fn gpu_telemetry_capability() -> (CapabilityState, String, String) {
    (
        CapabilityState::Unsupported,
        "installed vendor API".into(),
        "installed vendor APIs can only be inspected on Windows".into(),
    )
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn gpu_telemetry_capability() -> (CapabilityState, String, String) {
    let adlx = super::windows_api::installed_adlx();
    match (adlx, super::nvapi::probe()) {
        (true, Ok(())) => (
            CapabilityState::Available,
            "NVAPI Release 590 ABI".into(),
            "installed NVAPI initialized and enumerated a physical GPU; installed ADLX telemetry is not registered; hardware-unverified".into(),
        ),
        (true, Err(error)) => (
            CapabilityState::Unverified,
            "ADLX/NVAPI".into(),
            format!("installed ADLX telemetry ABI is not registered; NVAPI probe failed: {error}"),
        ),
        (false, Ok(())) => (
            CapabilityState::Available,
            "NVAPI Release 590 ABI".into(),
            "installed NVAPI initialized and enumerated a physical GPU; hardware-unverified".into(),
        ),
        (false, Err(error)) => {
            let state = if error.category() == ErrorCategory::Unavailable {
                CapabilityState::Unsupported
            } else {
                CapabilityState::Unverified
            };
            (
                state,
                "installed vendor API".into(),
                format!("ADLX was not loadable from System32; NVAPI probe failed: {error}"),
            )
        }
    }
}

#[cfg(all(windows, not(target_arch = "x86_64")))]
fn gpu_telemetry_capability() -> (CapabilityState, String, String) {
    (
        CapabilityState::Unsupported,
        "installed vendor API".into(),
        "Northclock vendor telemetry supports Windows x64 only".into(),
    )
}
