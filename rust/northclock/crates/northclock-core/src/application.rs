pub use crate::application_contract::{
    ApplicationCommand, CommandEnvelope, CommandError, CommandStatus,
};
use crate::application_support::{
    authorize_write, json_error, map_result, memory_report_with_whea, require_measurements,
    unusable_capability_error, validate_affinity_plan, ExecutionResult,
};
use crate::operation_workflow;
use crate::{
    inspect_rom, run_system_memory_test, AppSettings, BackendBundle, CapabilityReport,
    CapabilityState, Measurement, NorthclockError, SafetyPolicy, Storage,
};
use serde_json::{json, Value};
use std::time::Duration;

const MAX_VRAM_TEST_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_HARDWARE_OPERATION_TIMEOUT_MS: u64 = 10 * 60 * 1000;

#[derive(Clone, Debug)]
pub struct ApplicationService<B> {
    backend: B,
    safety: SafetyPolicy,
    storage: Option<Storage>,
}

impl<B: BackendBundle> ApplicationService<B> {
    #[must_use]
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            safety: SafetyPolicy,
            storage: None,
        }
    }

    #[must_use]
    pub fn with_storage(backend: B, storage: Storage) -> Self {
        Self {
            backend,
            safety: SafetyPolicy,
            storage: Some(storage),
        }
    }

    #[must_use]
    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn execute(&self, command: ApplicationCommand) -> CommandEnvelope {
        let name = command.name();
        let result = self.execute_inner(command);
        match result {
            Ok((capability, data)) => CommandEnvelope::success(name, capability, data),
            Err((capability, error)) => CommandEnvelope::failure(name, capability, error),
        }
    }

    fn execute_inner(&self, command: ApplicationCommand) -> ExecutionResult {
        match command {
            ApplicationCommand::Doctor => {
                let mut capabilities = self.backend.capabilities();
                capabilities.push(CapabilityReport::new(
                    "memory.system_test",
                    CapabilityState::Available,
                    "northclock-core",
                    "bounded measured system-memory workload",
                ));
                capabilities.push(CapabilityReport::new(
                    "persistence.local",
                    if self.storage.is_some() {
                        CapabilityState::Available
                    } else {
                        CapabilityState::Unsupported
                    },
                    "northclock-core TOML/JSONL/CSV",
                    if self.storage.is_some() {
                        "versioned local application-data storage configured"
                    } else {
                        "no platform application-data directory configured"
                    },
                ));
                Ok((None, json!(capabilities)))
            }
            ApplicationCommand::CpuIdentity => self.with_capability("cpu.identity", || {
                serde_json::to_value(self.backend.cpu_identity()?).map_err(json_error)
            }),
            ApplicationCommand::CpuMeasurements => self.with_capability("cpu.telemetry", || {
                let values = self.backend.cpu_measurements()?;
                require_measurements(&values)?;
                self.record_measurements(&values)?;
                serde_json::to_value(values).map_err(json_error)
            }),
            ApplicationCommand::CpuWorkload {
                duration_ms,
                threads,
            } => self.with_capability("cpu.workload", || {
                if duration_ms == 0
                    || duration_ms > MAX_HARDWARE_OPERATION_TIMEOUT_MS
                    || threads == 0
                    || threads > 512
                {
                    return Err(NorthclockError::InvalidUsage(
                        "CPU workload requires 1 to 600000 ms and 1 to 512 threads".into(),
                    ));
                }
                serde_json::to_value(
                    self.backend
                        .run_cpu_workload(Duration::from_millis(duration_ms), threads)?,
                )
                .map_err(json_error)
            }),
            ApplicationCommand::GpuDevices => self.with_capability("gpu.inventory", || {
                serde_json::to_value(self.backend.gpu_devices()?).map_err(json_error)
            }),
            ApplicationCommand::GpuMeasurements { stable_id } => {
                self.with_capability("gpu.telemetry", || {
                    let values = self.backend.gpu_measurements(stable_id.as_deref())?;
                    require_measurements(&values)?;
                    self.record_measurements(&values)?;
                    serde_json::to_value(values).map_err(json_error)
                })
            }
            ApplicationCommand::SystemMemoryTest(config) => {
                let capability = CapabilityReport::new(
                    "memory.system_test",
                    CapabilityState::Available,
                    "northclock-core",
                    "bounded measured system-memory workload",
                );
                let result = run_system_memory_test(config)
                    .and_then(|report| memory_report_with_whea(&self.backend, report));
                map_result(Some(capability), result)
            }
            ApplicationCommand::VramTest {
                adapter,
                bytes,
                timeout_ms,
            } => self.with_capability("memory.vram_test", || {
                if bytes == 0 || bytes > MAX_VRAM_TEST_BYTES {
                    return Err(NorthclockError::InvalidUsage(format!(
                        "VRAM test size must be between 1 and {MAX_VRAM_TEST_BYTES} bytes"
                    )));
                }
                if timeout_ms == 0 || timeout_ms > MAX_HARDWARE_OPERATION_TIMEOUT_MS {
                    return Err(NorthclockError::InvalidUsage(format!(
                        "VRAM timeout must be between 1 and {MAX_HARDWARE_OPERATION_TIMEOUT_MS} ms"
                    )));
                }
                serde_json::to_value(self.backend.run_vram_test(
                    adapter.as_deref(),
                    bytes,
                    Duration::from_millis(timeout_ms),
                )?)
                .map_err(json_error)
            }),
            ApplicationCommand::PowerPlans => self.with_capability("power.plans", || {
                serde_json::to_value(self.backend.power_plans()?).map_err(json_error)
            }),
            ApplicationCommand::SystemStatus => self
                .with_capability("windows.system_status", || {
                    serde_json::to_value(self.backend.system_status()?).map_err(json_error)
                }),
            ApplicationCommand::SettingsShow => {
                let result = self
                    .storage()
                    .and_then(Storage::load_settings)
                    .and_then(|settings| serde_json::to_value(settings).map_err(json_error));
                map_result(None, result)
            }
            ApplicationCommand::SettingsSet {
                measurement_interval_ms,
                selected_profile,
            } => {
                let result = (|| {
                    if !(100..=60_000).contains(&measurement_interval_ms) {
                        return Err(NorthclockError::InvalidUsage(
                            "measurement interval must be between 100 and 60000 ms".into(),
                        ));
                    }
                    let settings = AppSettings {
                        measurement_interval_ms,
                        selected_profile,
                        ..AppSettings::default()
                    };
                    self.storage()?.save_settings(&settings)?;
                    serde_json::to_value(settings).map_err(json_error)
                })();
                map_result(None, result)
            }
            ApplicationCommand::ProfilesList => {
                let result = self
                    .storage()
                    .and_then(Storage::list_profiles)
                    .and_then(|profiles| serde_json::to_value(profiles).map_err(json_error));
                map_result(None, result)
            }
            ApplicationCommand::ProfileImport { path } => {
                let result = self
                    .storage()
                    .and_then(|storage| storage.import_ini_once(&path))
                    .and_then(|profile| serde_json::to_value(profile).map_err(json_error));
                map_result(None, result)
            }
            ApplicationCommand::ProcessAffinityPreview { process_id, mask } => self
                .with_capability("process.affinity", || {
                    if process_id == 0 || mask == 0 {
                        return Err(NorthclockError::InvalidUsage(
                            "process ID and affinity mask must be non-zero".into(),
                        ));
                    }
                    let mut plan = self.backend.preview_process_affinity(process_id, mask)?;
                    validate_affinity_plan(&plan)?;
                    plan.bounds_validated = true;
                    serde_json::to_value(plan).map_err(json_error)
                }),
            ApplicationCommand::ProcessAffinityApply {
                plan,
                experimental,
                apply,
                risk_acknowledgement,
            } => self.with_capability("process.affinity", || {
                if !plan.bounds_validated {
                    return Err(NorthclockError::PermissionOrSafety(
                        "affinity apply requires a validated preview".into(),
                    ));
                }
                validate_affinity_plan(&plan)?;
                let current = self
                    .backend
                    .preview_process_affinity(plan.process_id, plan.requested_mask)?;
                if current.captured_mask != plan.captured_mask
                    || current.system_mask != plan.system_mask
                {
                    return Err(NorthclockError::PermissionOrSafety(
                        "process affinity changed after preview; create a new preview".into(),
                    ));
                }
                authorize_write(
                    &self.backend,
                    experimental,
                    apply,
                    risk_acknowledgement.as_deref(),
                )?;
                let receipt = self.backend.apply_process_affinity(&plan)?;
                if receipt.plan_id != plan.id
                    || receipt.process_id != plan.process_id
                    || receipt.captured_mask != plan.captured_mask
                    || receipt.requested_mask != plan.requested_mask
                    || !receipt.rollback_available
                    || !receipt.validation_passed
                    || receipt.readback_mask != receipt.requested_mask
                {
                    return Err(NorthclockError::HardwareOperation(
                        "process affinity receipt failed contract validation".into(),
                    ));
                }
                serde_json::to_value(receipt).map_err(json_error)
            }),
            ApplicationCommand::ProcessAffinityRollback {
                receipt,
                experimental,
                apply,
                risk_acknowledgement,
            } => self.with_capability("process.affinity", || {
                if receipt.plan_id.is_empty()
                    || !receipt.rollback_available
                    || !receipt.validation_passed
                    || receipt.readback_mask != receipt.requested_mask
                {
                    return Err(NorthclockError::PermissionOrSafety(
                        "rollback requires a validated affinity apply receipt".into(),
                    ));
                }
                let current = self
                    .backend
                    .preview_process_affinity(receipt.process_id, receipt.captured_mask)?;
                if current.captured_mask != receipt.readback_mask {
                    return Err(NorthclockError::PermissionOrSafety(
                        "process affinity changed after apply; refusing stale rollback".into(),
                    ));
                }
                authorize_write(
                    &self.backend,
                    experimental,
                    apply,
                    risk_acknowledgement.as_deref(),
                )?;
                let rollback = self.backend.rollback_process_affinity(&receipt)?;
                if rollback.plan_id != receipt.plan_id
                    || rollback.process_id != receipt.process_id
                    || !rollback.validation_passed
                    || rollback.restored_mask != receipt.captured_mask
                    || rollback.readback_mask != rollback.restored_mask
                {
                    return Err(NorthclockError::HardwareOperation(
                        "process affinity rollback readback validation failed".into(),
                    ));
                }
                serde_json::to_value(rollback).map_err(json_error)
            }),
            ApplicationCommand::WheaEvents { duration_ms } => {
                self.with_capability("events.whea", || {
                    if duration_ms == 0 {
                        return Err(NorthclockError::InvalidUsage(
                            "WHEA observation duration must be non-zero".into(),
                        ));
                    }
                    serde_json::to_value(
                        self.backend
                            .observe_whea(Duration::from_millis(duration_ms))?,
                    )
                    .map_err(json_error)
                })
            }
            ApplicationCommand::FrameCapture { duration_ms } => {
                self.with_capability("frames.capture", || {
                    if duration_ms == 0 {
                        return Err(NorthclockError::InvalidUsage(
                            "capture duration must be non-zero".into(),
                        ));
                    }
                    serde_json::to_value(
                        self.backend
                            .capture_frames(Duration::from_millis(duration_ms))?,
                    )
                    .map_err(json_error)
                })
            }
            ApplicationCommand::RomInspect { path } => self.with_capability("rom.inspect", || {
                let bytes = self.backend.read_rom(&path)?;
                serde_json::to_value(inspect_rom(&bytes)?).map_err(json_error)
            }),
            ApplicationCommand::OperationPreview(request) => {
                let capability_name = request.target.capability_name();
                let capability = self.capability(capability_name);
                if let Some(error) = unusable_capability_error(&capability) {
                    return Err((capability, error));
                }
                let result = operation_workflow::preview(&self.backend, self.safety, request);
                map_result(capability, result)
            }
            ApplicationCommand::OperationApply {
                plan,
                experimental,
                apply,
                risk_acknowledgement,
            } => {
                let capability_name = plan.target.capability_name();
                let capability = self.capability(capability_name);
                if let Some(error) = unusable_capability_error(&capability) {
                    return Err((capability, error));
                }
                let result = operation_workflow::apply(
                    &self.backend,
                    self.safety,
                    plan,
                    experimental,
                    apply,
                    risk_acknowledgement.as_deref(),
                );
                map_result(capability, result)
            }
            ApplicationCommand::OperationRollback {
                receipt,
                experimental,
                apply,
                risk_acknowledgement,
            } => {
                let capability_name = receipt.target.capability_name();
                let capability = self.capability(capability_name);
                if let Some(error) = unusable_capability_error(&capability) {
                    return Err((capability, error));
                }
                let result = operation_workflow::rollback(
                    &self.backend,
                    self.safety,
                    receipt,
                    experimental,
                    apply,
                    risk_acknowledgement.as_deref(),
                );
                map_result(capability, result)
            }
        }
    }

    fn with_capability<F>(
        &self,
        name: &str,
        operation: F,
    ) -> std::result::Result<
        (Option<CapabilityReport>, Value),
        (Option<CapabilityReport>, NorthclockError),
    >
    where
        F: FnOnce() -> crate::Result<Value>,
    {
        let capability = self.capability(name);
        if let Some(error) = unusable_capability_error(&capability) {
            return Err((capability, error));
        }
        map_result(capability, operation())
    }

    fn capability(&self, name: &str) -> Option<CapabilityReport> {
        self.backend
            .capabilities()
            .into_iter()
            .find(|capability| capability.name == name)
            .or_else(|| {
                Some(CapabilityReport::new(
                    name,
                    CapabilityState::Unsupported,
                    "none",
                    "no backend registered",
                ))
            })
    }

    fn storage(&self) -> crate::Result<&Storage> {
        self.storage.as_ref().ok_or_else(|| {
            NorthclockError::Unavailable(
                "persistent storage is unavailable because no platform data directory was configured"
                    .into(),
            )
        })
    }

    fn record_measurements(&self, values: &[Measurement<f64>]) -> crate::Result<()> {
        let Some(storage) = &self.storage else {
            return Ok(());
        };
        storage.append_measurements_csv(values)?;
        storage.append_history(values)
    }
}
