#[cfg(all(windows, target_arch = "x86_64"))]
use northclock_core::ErrorCategory;
use northclock_core::{CapabilityReport, CapabilityState};

pub(crate) fn capabilities() -> Vec<CapabilityReport> {
    let windows = cfg!(windows);
    let (gpu_telemetry_state, gpu_telemetry_backend, vendor_apis) = gpu_telemetry_capability();
    let platform_state = if windows {
        CapabilityState::Available
    } else {
        CapabilityState::Unsupported
    };
    let platform_detail = if windows {
        "documented Windows API backend"
    } else {
        "Northclock targets Windows 11 x64"
    };
    vec![
        CapabilityReport::new(
            "cpu.identity",
            platform_state,
            "CPUID + Windows topology",
            platform_detail,
        ),
        CapabilityReport::new(
            "cpu.telemetry",
            platform_state,
            "GetSystemTimes",
            platform_detail,
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
        CapabilityReport::new("gpu.inventory", platform_state, "DXGI", platform_detail),
        CapabilityReport::new(
            "gpu.telemetry",
            gpu_telemetry_state,
            gpu_telemetry_backend,
            vendor_apis,
        ),
        CapabilityReport::new(
            "gpu.adlx_telemetry",
            if windows {
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
        CapabilityReport::new(
            "memory.vram_test",
            platform_state,
            "isolated native D3D12 copy/readback",
            if windows {
                "bounded physical adapter test; implementation remains hardware-unverified by project acceptance"
            } else {
                platform_detail
            },
        ),
        CapabilityReport::new(
            "power.plans",
            platform_state,
            "Windows power API",
            platform_detail,
        ),
        CapabilityReport::new(
            "windows.task_scheduler",
            platform_state,
            "Task Scheduler 2.0 COM",
            platform_detail,
        ),
        CapabilityReport::new(
            "windows.vbs_status",
            platform_state,
            "Win32_DeviceGuard WMI",
            platform_detail,
        ),
        CapabilityReport::new(
            "windows.conflict_detection",
            platform_state,
            "Tool Help + Service Control Manager + SetupAPI",
            if windows {
                "read-only bounded observation of known overlapping hardware-control processes, services, drivers, and devices"
            } else {
                platform_detail
            },
        ),
        CapabilityReport::new(
            "windows.system_status",
            platform_state,
            "documented Windows observation APIs",
            if windows {
                "aggregated read-only Task Scheduler, VBS, and potential-conflict status"
            } else {
                platform_detail
            },
        ),
        CapabilityReport::new(
            "process.affinity",
            platform_state,
            "Windows process API",
            platform_detail,
        ),
        CapabilityReport::new(
            "events.whea",
            platform_state,
            "Windows Event Log API",
            platform_detail,
        ),
        CapabilityReport::new(
            "frames.capture",
            platform_state,
            "DxgKrnl ETW Present_Start",
            if windows {
                "bounded native ETW capture; implementation remains hardware-unverified by project acceptance"
            } else {
                platform_detail
            },
        ),
        CapabilityReport::new(
            "overlay.measurements",
            platform_state,
            "northclock-gui transparent egui viewport",
            if windows {
                "read-only overlay renders measured backend values; hardware-unverified"
            } else {
                platform_detail
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
