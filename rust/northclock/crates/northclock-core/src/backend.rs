use crate::{
    ApplyReceipt, CapabilityReport, DeviceIdentity, Measurement, OperationPlan, OperationRequest,
    ProcessAffinityPlan, ProcessAffinityReceipt, ProcessAffinityRollbackReceipt, Result,
    RollbackReceipt, SystemStatusReport,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CpuIdentity {
    pub device: DeviceIdentity,
    pub family: Option<u8>,
    pub model: Option<u8>,
    pub physical_cores: usize,
    pub logical_processors: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GpuDevice {
    pub device: DeviceIdentity,
    pub dedicated_memory_bytes: Option<u64>,
    pub driver_backend: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkloadReport {
    pub duration_ms: u128,
    pub iterations: u64,
    pub validation_errors: u64,
    pub timed_out: bool,
    pub hardware_verified: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CpuWorkloadReport {
    pub requested_duration_ms: u64,
    pub elapsed_ms: u128,
    pub threads: usize,
    pub iterations: u64,
    pub iterations_per_second: f64,
    pub validation_checks: u64,
    pub validation_errors: u64,
    pub timed_out: bool,
    pub hardware_verified: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PowerPlan {
    pub guid: String,
    pub name: String,
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FrameSample {
    pub process_id: u32,
    pub frame_time: Measurement<f64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedEvent {
    pub provider: String,
    pub event_id: u32,
    pub timestamp_unix_ms: u128,
    pub detail: String,
}

pub trait CapabilityBackend: Send + Sync {
    fn capabilities(&self) -> Vec<CapabilityReport>;
    fn is_elevated(&self) -> Result<bool>;
}

pub trait CpuTelemetryBackend: Send + Sync {
    fn cpu_identity(&self) -> Result<CpuIdentity>;
    fn cpu_measurements(&self) -> Result<Vec<Measurement<f64>>>;
}

pub trait CpuTuningBackend: Send + Sync {
    fn preview_cpu_operation(&self, request: &OperationRequest) -> Result<OperationPlan>;
    fn apply_cpu_operation(&self, plan: &OperationPlan) -> Result<ApplyReceipt>;
    fn rollback_cpu_operation(&self, receipt: &ApplyReceipt) -> Result<RollbackReceipt>;
}

pub trait GpuTelemetryBackend: Send + Sync {
    fn gpu_devices(&self) -> Result<Vec<GpuDevice>>;
    fn gpu_measurements(&self, stable_id: Option<&str>) -> Result<Vec<Measurement<f64>>>;
}

pub trait GpuTuningBackend: Send + Sync {
    fn preview_gpu_operation(&self, request: &OperationRequest) -> Result<OperationPlan>;
    fn apply_gpu_operation(&self, plan: &OperationPlan) -> Result<ApplyReceipt>;
    fn rollback_gpu_operation(&self, receipt: &ApplyReceipt) -> Result<RollbackReceipt>;
}

pub trait WorkloadBackend: Send + Sync {
    fn run_cpu_workload(&self, duration: Duration, threads: usize) -> Result<CpuWorkloadReport>;
    fn run_vram_test(
        &self,
        adapter: Option<&str>,
        bytes: u64,
        timeout: Duration,
    ) -> Result<WorkloadReport>;
}

pub trait ProcessControlBackend: Send + Sync {
    fn preview_process_affinity(&self, process_id: u32, mask: u64) -> Result<ProcessAffinityPlan>;
    fn apply_process_affinity(&self, plan: &ProcessAffinityPlan) -> Result<ProcessAffinityReceipt>;
    fn rollback_process_affinity(
        &self,
        receipt: &ProcessAffinityReceipt,
    ) -> Result<ProcessAffinityRollbackReceipt>;
}

pub trait PowerPlanBackend: Send + Sync {
    fn power_plans(&self) -> Result<Vec<PowerPlan>>;
}

pub trait SystemStatusBackend: Send + Sync {
    fn system_status(&self) -> Result<SystemStatusReport>;
}

pub trait FrameCaptureBackend: Send + Sync {
    fn capture_frames(&self, duration: Duration) -> Result<Vec<FrameSample>>;
}

pub trait OverlayBackend: Send + Sync {
    fn show_overlay(&self, measurements: &[Measurement<f64>]) -> Result<()>;
    fn hide_overlay(&self) -> Result<()>;
}

pub trait EventObservationBackend: Send + Sync {
    fn observe_whea(&self, duration: Duration) -> Result<Vec<ObservedEvent>>;
}

pub trait RomInspectionBackend: Send + Sync {
    fn read_rom(&self, path: &Path) -> Result<Vec<u8>>;
}

pub trait BackendBundle:
    CapabilityBackend
    + CpuTelemetryBackend
    + CpuTuningBackend
    + GpuTelemetryBackend
    + GpuTuningBackend
    + WorkloadBackend
    + ProcessControlBackend
    + PowerPlanBackend
    + SystemStatusBackend
    + FrameCaptureBackend
    + OverlayBackend
    + EventObservationBackend
    + RomInspectionBackend
{
}

impl<T> BackendBundle for T where
    T: CapabilityBackend
        + CpuTelemetryBackend
        + CpuTuningBackend
        + GpuTelemetryBackend
        + GpuTuningBackend
        + WorkloadBackend
        + ProcessControlBackend
        + PowerPlanBackend
        + SystemStatusBackend
        + FrameCaptureBackend
        + OverlayBackend
        + EventObservationBackend
        + RomInspectionBackend
{
}
