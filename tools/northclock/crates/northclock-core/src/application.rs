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
            ApplicationCommand::Doctor => self.doctor(),
            ApplicationCommand::CpuIdentity => self.cpu_identity(),
            ApplicationCommand::CpuMeasurements => self.cpu_measurements(),
            ApplicationCommand::CpuWorkload {
                duration_ms,
                threads,
            } => self.cpu_workload(duration_ms, threads),
            ApplicationCommand::GpuDevices => self.gpu_devices(),
            ApplicationCommand::GpuMeasurements { stable_id } => self.gpu_measurements(stable_id),
            ApplicationCommand::SystemMemoryTest(config) => self.system_memory_test(config),
            ApplicationCommand::VramTest {
                adapter,
                bytes,
                timeout_ms,
            } => self.vram_test(adapter, bytes, timeout_ms),
            ApplicationCommand::PowerPlans => self.power_plans(),
            ApplicationCommand::SystemStatus => self.system_status(),
            ApplicationCommand::SettingsShow => self.settings_show(),
            ApplicationCommand::SettingsSet {
                measurement_interval_ms,
                selected_profile,
            } => self.settings_set(measurement_interval_ms, selected_profile),
            ApplicationCommand::ProfilesList => self.profiles_list(),
            ApplicationCommand::ProfileImport { path } => self.profile_import(path),
            ApplicationCommand::ProcessAffinityPreview { process_id, mask } => {
                self.process_affinity_preview(process_id, mask)
            }
            ApplicationCommand::ProcessAffinityApply {
                plan,
                experimental,
                apply,
                risk_acknowledgement,
            } => self.process_affinity_apply(plan, experimental, apply, risk_acknowledgement),
            ApplicationCommand::ProcessAffinityRollback {
                receipt,
                experimental,
                apply,
                risk_acknowledgement,
            } => self.process_affinity_rollback(receipt, experimental, apply, risk_acknowledgement),
            ApplicationCommand::WheaEvents { duration_ms } => self.whea_events(duration_ms),
            ApplicationCommand::FrameCapture { duration_ms } => self.frame_capture(duration_ms),
            ApplicationCommand::RomInspect { path } => self.rom_inspect(path),
            ApplicationCommand::OperationPreview(request) => self.operation_preview(request),
            ApplicationCommand::OperationApply {
                plan,
                experimental,
                apply,
                risk_acknowledgement,
            } => self.operation_apply(plan, experimental, apply, risk_acknowledgement),
            ApplicationCommand::OperationRollback {
                receipt,
                experimental,
                apply,
                risk_acknowledgement,
            } => self.operation_rollback(receipt, experimental, apply, risk_acknowledgement),
        }
    }

    fn doctor(&self) -> ExecutionResult {
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

    fn cpu_identity(&self) -> ExecutionResult {
        self.with_capability("cpu.identity", || {
            serde_json::to_value(self.backend.cpu_identity()?).map_err(json_error)
        })
    }

    fn cpu_measurements(&self) -> ExecutionResult {
        self.with_capability("cpu.telemetry", || {
            let values = self.backend.cpu_measurements()?;
            require_measurements(&values)?;
            self.record_measurements(&values)?;
            serde_json::to_value(values).map_err(json_error)
        })
    }

    fn cpu_workload(&self, duration_ms: u64, threads: usize) -> ExecutionResult {
        self.with_capability("cpu.workload", || {
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
        })
    }

    fn gpu_devices(&self) -> ExecutionResult {
        self.with_capability("gpu.inventory", || {
            serde_json::to_value(self.backend.gpu_devices()?).map_err(json_error)
        })
    }

    fn gpu_measurements(&self, stable_id: Option<String>) -> ExecutionResult {
        self.with_capability("gpu.telemetry", || {
            let values = self.backend.gpu_measurements(stable_id.as_deref())?;
            require_measurements(&values)?;
            self.record_measurements(&values)?;
            serde_json::to_value(values).map_err(json_error)
        })
    }

    fn system_memory_test(&self, config: crate::MemoryTestConfig) -> ExecutionResult {
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

    fn vram_test(&self, adapter: Option<String>, bytes: u64, timeout_ms: u64) -> ExecutionResult {
        self.with_capability("memory.vram_test", || {
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
        })
    }

    fn power_plans(&self) -> ExecutionResult {
        self.with_capability("power.plans", || {
            serde_json::to_value(self.backend.power_plans()?).map_err(json_error)
        })
    }

    fn system_status(&self) -> ExecutionResult {
        self.with_capability("windows.system_status", || {
            serde_json::to_value(self.backend.system_status()?).map_err(json_error)
        })
    }

    fn settings_show(&self) -> ExecutionResult {
        let result = self
            .storage()
            .and_then(Storage::load_settings)
            .and_then(|settings| serde_json::to_value(settings).map_err(json_error));
        map_result(None, result)
    }

    fn settings_set(
        &self,
        measurement_interval_ms: u64,
        selected_profile: Option<String>,
    ) -> ExecutionResult {
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

    fn profiles_list(&self) -> ExecutionResult {
        let result = self
            .storage()
            .and_then(Storage::list_profiles)
            .and_then(|profiles| serde_json::to_value(profiles).map_err(json_error));
        map_result(None, result)
    }

    fn profile_import(&self, path: std::path::PathBuf) -> ExecutionResult {
        let result = self
            .storage()
            .and_then(|storage| storage.import_ini_once(&path))
            .and_then(|profile| serde_json::to_value(profile).map_err(json_error));
        map_result(None, result)
    }

    fn process_affinity_preview(&self, process_id: u32, mask: u64) -> ExecutionResult {
        self.with_capability("process.affinity", || {
            if process_id == 0 || mask == 0 {
                return Err(NorthclockError::InvalidUsage(
                    "process ID and affinity mask must be non-zero".into(),
                ));
            }
            let mut plan = self.backend.preview_process_affinity(process_id, mask)?;
            validate_affinity_plan(&plan)?;
            plan.bounds_validated = true;
            serde_json::to_value(plan).map_err(json_error)
        })
    }

    fn process_affinity_apply(
        &self,
        plan: crate::ProcessAffinityPlan,
        experimental: bool,
        apply: bool,
        risk_acknowledgement: Option<String>,
    ) -> ExecutionResult {
        self.with_capability("process.affinity", || {
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
        })
    }

    fn process_affinity_rollback(
        &self,
        receipt: crate::ProcessAffinityReceipt,
        experimental: bool,
        apply: bool,
        risk_acknowledgement: Option<String>,
    ) -> ExecutionResult {
        self.with_capability("process.affinity", || {
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
        })
    }

    fn whea_events(&self, duration_ms: u64) -> ExecutionResult {
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

    fn frame_capture(&self, duration_ms: u64) -> ExecutionResult {
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

    fn rom_inspect(&self, path: std::path::PathBuf) -> ExecutionResult {
        self.with_capability("rom.inspect", || {
            let bytes = self.backend.read_rom(&path)?;
            serde_json::to_value(inspect_rom(&bytes)?).map_err(json_error)
        })
    }

    fn operation_preview(&self, request: crate::OperationRequest) -> ExecutionResult {
        let capability = self.capability(request.target.capability_name());
        if let Some(error) = unusable_capability_error(&capability) {
            return Err((capability, error));
        }
        let result = operation_workflow::preview(&self.backend, self.safety, request);
        map_result(capability, result)
    }

    fn operation_apply(
        &self,
        plan: crate::OperationPlan,
        experimental: bool,
        apply: bool,
        risk_acknowledgement: Option<String>,
    ) -> ExecutionResult {
        let capability = self.capability(plan.target.capability_name());
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

    fn operation_rollback(
        &self,
        receipt: crate::ApplyReceipt,
        experimental: bool,
        apply: bool,
        risk_acknowledgement: Option<String>,
    ) -> ExecutionResult {
        let capability = self.capability(receipt.target.capability_name());
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
