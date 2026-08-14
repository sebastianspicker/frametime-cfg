//! Windows and installed-vendor adapter boundary.
//!
//! Unsafe code is confined to the Windows implementation modules. Public
//! methods expose owned Rust values and validate every OS result before use.

use northclock_core::{
    ApplyReceipt, CapabilityBackend, CapabilityReport, CpuIdentity, CpuTelemetryBackend,
    CpuTuningBackend, CpuWorkloadReport, EventObservationBackend, FrameCaptureBackend, FrameSample,
    GpuDevice, GpuTelemetryBackend, GpuTuningBackend, Measurement, NorthclockError, ObservedEvent,
    OperationPlan, OperationRequest, OverlayBackend, PowerPlan, PowerPlanBackend,
    ProcessAffinityPlan, ProcessAffinityReceipt, ProcessAffinityRollbackReceipt,
    ProcessControlBackend, Result, RollbackReceipt, RomInspectionBackend, SystemStatusBackend,
    SystemStatusReport, WorkloadBackend, WorkloadReport,
};
#[cfg(all(windows, target_arch = "x86_64"))]
use raw_cpuid::CpuId;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(feature = "fuzzing")]
pub use abi_validation::{
    validate_etw_present_header, validate_nvapi_load_fields, validate_nvapi_temperature_fields,
    validate_nvapi_thermal_header, AbiValidationError, EtwPresentHeaderFields,
};

#[cfg(windows)]
use northclock_core::DeviceIdentity;

#[derive(Clone, Debug)]
pub struct WindowsPlatform {
    in_vram_worker: bool,
}

impl Default for WindowsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsPlatform {
    #[must_use]
    pub fn new() -> Self {
        Self {
            in_vram_worker: false,
        }
    }

    /// Creates the adapter used only inside the isolated VRAM worker process.
    #[must_use]
    pub fn for_vram_worker() -> Self {
        Self {
            in_vram_worker: true,
        }
    }

    pub fn local_app_data_dir() -> Result<PathBuf> {
        Self::local_app_data_dir_with(is_elevated, || {
            std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        })
    }

    /// Resolves the local persistence directory only for an unelevated process.
    ///
    /// The injectable inputs keep the decision testable without changing the
    /// host environment. Callers must use the native elevation probe in normal
    /// operation so elevated processes never inherit a user's `LOCALAPPDATA`.
    pub fn local_app_data_dir_with<E, D>(elevation_probe: E, local_app_data: D) -> Result<PathBuf>
    where
        E: FnOnce() -> Result<bool>,
        D: FnOnce() -> Option<PathBuf>,
    {
        persistence_root::resolve(elevation_probe, local_app_data)
    }
}

impl CapabilityBackend for WindowsPlatform {
    fn capabilities(&self) -> Vec<CapabilityReport> {
        capabilities::capabilities()
    }

    fn is_elevated(&self) -> Result<bool> {
        is_elevated()
    }
}

impl CpuTelemetryBackend for WindowsPlatform {
    fn cpu_identity(&self) -> Result<CpuIdentity> {
        cpu_identity()
    }

    fn cpu_measurements(&self) -> Result<Vec<Measurement<f64>>> {
        cpu_measurements()
    }
}

impl CpuTuningBackend for WindowsPlatform {
    fn preview_cpu_operation(&self, _request: &OperationRequest) -> Result<OperationPlan> {
        Err(hardware_writes_unavailable("CPU"))
    }

    fn apply_cpu_operation(&self, _plan: &OperationPlan) -> Result<ApplyReceipt> {
        Err(hardware_writes_unavailable("CPU"))
    }

    fn rollback_cpu_operation(&self, _receipt: &ApplyReceipt) -> Result<RollbackReceipt> {
        Err(hardware_writes_unavailable("CPU"))
    }
}

impl GpuTelemetryBackend for WindowsPlatform {
    fn gpu_devices(&self) -> Result<Vec<GpuDevice>> {
        gpu_devices()
    }

    fn gpu_measurements(&self, stable_id: Option<&str>) -> Result<Vec<Measurement<f64>>> {
        gpu_measurements(stable_id)
    }
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn gpu_measurements(stable_id: Option<&str>) -> Result<Vec<Measurement<f64>>> {
    nvapi::gpu_measurements(stable_id)
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
fn gpu_measurements(_stable_id: Option<&str>) -> Result<Vec<Measurement<f64>>> {
    Err(NorthclockError::Unavailable(
        "installed vendor GPU telemetry requires Windows 11 x64".into(),
    ))
}

impl GpuTuningBackend for WindowsPlatform {
    fn preview_gpu_operation(&self, _request: &OperationRequest) -> Result<OperationPlan> {
        Err(hardware_writes_unavailable("GPU"))
    }

    fn apply_gpu_operation(&self, _plan: &OperationPlan) -> Result<ApplyReceipt> {
        Err(hardware_writes_unavailable("GPU"))
    }

    fn rollback_gpu_operation(&self, _receipt: &ApplyReceipt) -> Result<RollbackReceipt> {
        Err(hardware_writes_unavailable("GPU"))
    }
}

impl WorkloadBackend for WindowsPlatform {
    fn run_cpu_workload(&self, duration: Duration, threads: usize) -> Result<CpuWorkloadReport> {
        if duration.is_zero() || threads == 0 || threads > 512 {
            return Err(NorthclockError::InvalidUsage(
                "CPU workload requires a non-zero duration and 1 to 512 threads".into(),
            ));
        }
        let started = Instant::now();
        let deadline = started + duration;
        let mut handles = Vec::with_capacity(threads);
        for thread_index in 0..threads {
            handles.push(std::thread::spawn(move || {
                let mut value = (thread_index as u64) ^ 0x9E37_79B9_7F4A_7C15;
                let mut iterations = 0_u64;
                let mut validation_checks = 0_u64;
                let mut validation_errors = 0_u64;
                while Instant::now() < deadline {
                    let previous = value;
                    value =
                        previous.rotate_left(17).wrapping_add(0xD6E8_FEB8_6659_FD93) ^ iterations;
                    if iterations & 0x3ff == 0 {
                        let observed = std::hint::black_box(value);
                        let restored = (observed ^ iterations)
                            .wrapping_sub(0xD6E8_FEB8_6659_FD93)
                            .rotate_right(17);
                        validation_checks = validation_checks.saturating_add(1);
                        validation_errors =
                            validation_errors.saturating_add(u64::from(restored != previous));
                    }
                    iterations = iterations.saturating_add(1);
                }
                std::hint::black_box(value);
                ThreadWork {
                    iterations,
                    validation_checks,
                    validation_errors,
                }
            }));
        }
        let mut iterations = 0_u64;
        let mut validation_checks = 0_u64;
        let mut validation_errors = 0_u64;
        for handle in handles {
            let work = handle.join().map_err(|_| {
                NorthclockError::HardwareOperation("CPU workload thread panicked".into())
            })?;
            iterations = iterations.saturating_add(work.iterations);
            validation_checks = validation_checks.saturating_add(work.validation_checks);
            validation_errors = validation_errors.saturating_add(work.validation_errors);
        }
        let elapsed = started.elapsed();
        let elapsed_seconds = elapsed.as_secs_f64();
        if elapsed_seconds <= 0.0 {
            return Err(NorthclockError::HardwareOperation(
                "CPU workload timer did not advance".into(),
            ));
        }
        Ok(CpuWorkloadReport {
            requested_duration_ms: duration.as_millis().min(u128::from(u64::MAX)) as u64,
            elapsed_ms: elapsed.as_millis(),
            threads,
            iterations,
            iterations_per_second: iterations as f64 / elapsed_seconds,
            validation_checks,
            validation_errors,
            timed_out: false,
            hardware_verified: false,
        })
    }

    fn run_vram_test(
        &self,
        adapter: Option<&str>,
        bytes: u64,
        timeout: Duration,
    ) -> Result<WorkloadReport> {
        if self.in_vram_worker {
            run_vram_test_in_process(adapter, bytes, timeout)
        } else {
            run_vram_test_isolated(adapter, bytes, timeout)
        }
    }
}

struct ThreadWork {
    iterations: u64,
    validation_checks: u64,
    validation_errors: u64,
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn run_vram_test_in_process(
    adapter: Option<&str>,
    bytes: u64,
    timeout: Duration,
) -> Result<WorkloadReport> {
    d3d12_vram::run_vram_test(adapter, bytes, timeout)
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
fn run_vram_test_in_process(
    _adapter: Option<&str>,
    _bytes: u64,
    _timeout: Duration,
) -> Result<WorkloadReport> {
    Err(NorthclockError::Unavailable(
        "D3D12 VRAM validation requires Windows 11 x64".into(),
    ))
}

#[cfg(windows)]
fn run_vram_test_isolated(
    adapter: Option<&str>,
    bytes: u64,
    timeout: Duration,
) -> Result<WorkloadReport> {
    vram_process::run_isolated(adapter, bytes, timeout)
}

#[cfg(not(windows))]
fn run_vram_test_isolated(
    _adapter: Option<&str>,
    _bytes: u64,
    _timeout: Duration,
) -> Result<WorkloadReport> {
    Err(NorthclockError::Unavailable(
        "isolated D3D12 VRAM validation requires Windows 11 x64".into(),
    ))
}

impl ProcessControlBackend for WindowsPlatform {
    fn preview_process_affinity(&self, process_id: u32, mask: u64) -> Result<ProcessAffinityPlan> {
        preview_process_affinity(process_id, mask)
    }

    fn apply_process_affinity(&self, plan: &ProcessAffinityPlan) -> Result<ProcessAffinityReceipt> {
        apply_process_affinity(plan)
    }

    fn rollback_process_affinity(
        &self,
        receipt: &ProcessAffinityReceipt,
    ) -> Result<ProcessAffinityRollbackReceipt> {
        rollback_process_affinity(receipt)
    }
}

impl PowerPlanBackend for WindowsPlatform {
    fn power_plans(&self) -> Result<Vec<PowerPlan>> {
        power_plans()
    }
}

impl SystemStatusBackend for WindowsPlatform {
    fn system_status(&self) -> Result<SystemStatusReport> {
        system_status()
    }
}

#[cfg(windows)]
fn system_status() -> Result<SystemStatusReport> {
    system_status_windows::observe()
}

#[cfg(not(windows))]
fn system_status() -> Result<SystemStatusReport> {
    Err(NorthclockError::Unavailable(
        "Windows system-status observation requires Windows 11".into(),
    ))
}

impl FrameCaptureBackend for WindowsPlatform {
    fn capture_frames(&self, duration: Duration) -> Result<Vec<FrameSample>> {
        capture_frames(duration)
    }
}

#[cfg(not(windows))]
fn capture_frames(_duration: Duration) -> Result<Vec<FrameSample>> {
    Err(NorthclockError::Unavailable(
        "ETW frame capture requires Windows 11".into(),
    ))
}

#[cfg(windows)]
fn capture_frames(duration: Duration) -> Result<Vec<FrameSample>> {
    etw_frames::capture_frames(duration)
}

impl OverlayBackend for WindowsPlatform {
    fn show_overlay(&self, _measurements: &[Measurement<f64>]) -> Result<()> {
        Err(NorthclockError::Unavailable(
            "overlay rendering requires a measured frame source".into(),
        ))
    }

    fn hide_overlay(&self) -> Result<()> {
        Ok(())
    }
}

impl EventObservationBackend for WindowsPlatform {
    fn observe_whea(&self, duration: Duration) -> Result<Vec<ObservedEvent>> {
        observe_whea(duration)
    }
}

#[cfg(not(windows))]
fn observe_whea(_duration: Duration) -> Result<Vec<ObservedEvent>> {
    Err(NorthclockError::Unavailable(
        "native WHEA Event Log observation requires Windows 11".into(),
    ))
}

#[cfg(windows)]
fn observe_whea(duration: Duration) -> Result<Vec<ObservedEvent>> {
    event_log::observe_whea(duration)
}

impl RomInspectionBackend for WindowsPlatform {
    fn read_rom(&self, path: &Path) -> Result<Vec<u8>> {
        std::fs::read(path).map_err(|error| {
            NorthclockError::HardwareOperation(format!(
                "could not read ROM image {}: {error}",
                path.display()
            ))
        })
    }
}

fn hardware_writes_unavailable(area: &str) -> NorthclockError {
    NorthclockError::Unavailable(format!(
        "{area} writes have no physically validated backend; compile-time opt-in does not create one"
    ))
}

#[cfg(not(windows))]
fn cpu_identity() -> Result<CpuIdentity> {
    Err(NorthclockError::Unavailable(
        "CPU identity backend requires Windows 11 x64".into(),
    ))
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn cpu_identity() -> Result<CpuIdentity> {
    let cpuid = CpuId::new();
    let vendor = cpuid
        .get_vendor_info()
        .map(|value| value.as_str().to_owned());
    let feature = cpuid.get_feature_info();
    let display_name = cpuid.get_processor_brand_string().map_or_else(
        || "x86_64 processor".into(),
        |value| value.as_str().trim().to_owned(),
    );
    let logical_processors = windows_api::logical_processor_count()?;
    let physical_cores = windows_api::physical_core_count()?;
    let device = DeviceIdentity::new("cpu", "cpu-0", display_name, vendor);
    Ok(CpuIdentity {
        device,
        family: feature.as_ref().map(raw_cpuid::FeatureInfo::family_id),
        model: feature.as_ref().map(raw_cpuid::FeatureInfo::model_id),
        physical_cores,
        logical_processors,
    })
}

#[cfg(all(windows, not(target_arch = "x86_64")))]
fn cpu_identity() -> Result<CpuIdentity> {
    Err(NorthclockError::Unavailable(
        "Northclock supports x86_64 Windows processors".into(),
    ))
}

#[cfg(not(windows))]
fn cpu_measurements() -> Result<Vec<Measurement<f64>>> {
    Err(NorthclockError::Unavailable(
        "CPU telemetry backend requires Windows 11 x64".into(),
    ))
}

#[cfg(windows)]
fn cpu_measurements() -> Result<Vec<Measurement<f64>>> {
    windows_api::cpu_measurements()
}

#[cfg(not(windows))]
fn gpu_devices() -> Result<Vec<GpuDevice>> {
    Err(NorthclockError::Unavailable(
        "DXGI inventory requires Windows 11".into(),
    ))
}

#[cfg(windows)]
fn gpu_devices() -> Result<Vec<GpuDevice>> {
    windows_api::gpu_devices()
}

#[cfg(not(windows))]
fn is_elevated() -> Result<bool> {
    Ok(false)
}

#[cfg(windows)]
fn is_elevated() -> Result<bool> {
    windows_api::is_elevated()
}

#[cfg(not(windows))]
fn preview_process_affinity(_process_id: u32, _mask: u64) -> Result<ProcessAffinityPlan> {
    Err(NorthclockError::Unavailable(
        "process affinity requires Windows".into(),
    ))
}

#[cfg(windows)]
fn preview_process_affinity(process_id: u32, mask: u64) -> Result<ProcessAffinityPlan> {
    let (captured_mask, system_mask) = windows_api::process_affinity(process_id)?;
    let mut plan = ProcessAffinityPlan::new(process_id, mask, captured_mask, system_mask);
    plan.bounds_validated = mask != 0 && mask & !system_mask == 0;
    Ok(plan)
}

#[cfg(not(windows))]
fn apply_process_affinity(_plan: &ProcessAffinityPlan) -> Result<ProcessAffinityReceipt> {
    Err(NorthclockError::Unavailable(
        "process affinity requires Windows".into(),
    ))
}

#[cfg(windows)]
fn apply_process_affinity(plan: &ProcessAffinityPlan) -> Result<ProcessAffinityReceipt> {
    windows_api::set_process_affinity(plan.process_id, plan.requested_mask)?;
    let (readback_mask, _) = windows_api::process_affinity(plan.process_id)?;
    Ok(ProcessAffinityReceipt {
        plan_id: plan.id.clone(),
        process_id: plan.process_id,
        captured_mask: plan.captured_mask,
        requested_mask: plan.requested_mask,
        readback_mask,
        validation_passed: readback_mask == plan.requested_mask,
        rollback_available: true,
    })
}

#[cfg(not(windows))]
fn rollback_process_affinity(
    _receipt: &ProcessAffinityReceipt,
) -> Result<ProcessAffinityRollbackReceipt> {
    Err(NorthclockError::Unavailable(
        "process affinity requires Windows".into(),
    ))
}

#[cfg(windows)]
fn rollback_process_affinity(
    receipt: &ProcessAffinityReceipt,
) -> Result<ProcessAffinityRollbackReceipt> {
    windows_api::set_process_affinity(receipt.process_id, receipt.captured_mask)?;
    let (readback_mask, _) = windows_api::process_affinity(receipt.process_id)?;
    Ok(ProcessAffinityRollbackReceipt {
        plan_id: receipt.plan_id.clone(),
        process_id: receipt.process_id,
        restored_mask: receipt.captured_mask,
        readback_mask,
        validation_passed: readback_mask == receipt.captured_mask,
    })
}

#[cfg(not(windows))]
fn power_plans() -> Result<Vec<PowerPlan>> {
    Err(NorthclockError::Unavailable(
        "power-plan enumeration requires Windows".into(),
    ))
}

#[cfg(windows)]
fn power_plans() -> Result<Vec<PowerPlan>> {
    windows_api::power_plans()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_workload_measures_and_validates_work() {
        let report = WindowsPlatform::new()
            .run_cpu_workload(Duration::from_millis(5), 2)
            .unwrap_or_else(|error| panic!("CPU workload failed: {error}"));
        assert_eq!(report.requested_duration_ms, 5);
        assert_eq!(report.threads, 2);
        assert!(report.iterations > 0);
        assert!(report.validation_checks > 0);
        assert_eq!(report.validation_errors, 0);
        assert!(report.iterations_per_second.is_finite());
        assert!(report.iterations_per_second > 0.0);
    }
}

#[cfg(any(windows, feature = "fuzzing", test))]
mod abi_validation;
mod capabilities;
#[cfg(all(windows, target_arch = "x86_64"))]
mod d3d12_vram;
#[cfg(windows)]
mod etw_frames;
#[cfg(windows)]
mod event_log;
#[cfg(all(windows, target_arch = "x86_64"))]
mod nvapi;
mod persistence_root;
#[cfg(windows)]
mod system_status_catalog;
#[cfg(windows)]
mod system_status_windows;
#[cfg(windows)]
mod vram_process;
#[cfg(windows)]
mod windows_api;
