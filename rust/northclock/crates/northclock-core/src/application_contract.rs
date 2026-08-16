use crate::{
    ApplyReceipt, CapabilityReport, ErrorCategory, MemoryTestConfig, NorthclockError,
    OperationPlan, OperationRequest, ProcessAffinityPlan, ProcessAffinityReceipt, SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub enum ApplicationCommand {
    Doctor,
    CpuIdentity,
    CpuMeasurements,
    CpuWorkload {
        duration_ms: u64,
        threads: usize,
    },
    GpuDevices,
    GpuMeasurements {
        stable_id: Option<String>,
    },
    SystemMemoryTest(MemoryTestConfig),
    VramTest {
        adapter: Option<String>,
        bytes: u64,
        timeout_ms: u64,
    },
    PowerPlans,
    SystemStatus,
    SettingsShow,
    SettingsSet {
        measurement_interval_ms: u64,
        selected_profile: Option<String>,
    },
    ProfilesList,
    ProfileImport {
        path: PathBuf,
    },
    ProcessAffinityPreview {
        process_id: u32,
        mask: u64,
    },
    ProcessAffinityApply {
        plan: ProcessAffinityPlan,
        experimental: bool,
        apply: bool,
        risk_acknowledgement: Option<String>,
    },
    ProcessAffinityRollback {
        receipt: ProcessAffinityReceipt,
        experimental: bool,
        apply: bool,
        risk_acknowledgement: Option<String>,
    },
    WheaEvents {
        duration_ms: u64,
    },
    FrameCapture {
        duration_ms: u64,
    },
    RomInspect {
        path: PathBuf,
    },
    OperationPreview(OperationRequest),
    OperationApply {
        plan: OperationPlan,
        experimental: bool,
        apply: bool,
        risk_acknowledgement: Option<String>,
    },
    OperationRollback {
        receipt: ApplyReceipt,
        experimental: bool,
        apply: bool,
        risk_acknowledgement: Option<String>,
    },
}

impl ApplicationCommand {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Doctor => "doctor",
            Self::CpuIdentity => "cpu.identity",
            Self::CpuMeasurements => "cpu.measure",
            Self::CpuWorkload { .. } => "cpu.workload",
            Self::GpuDevices => "gpu.list",
            Self::GpuMeasurements { .. } => "gpu.measure",
            Self::SystemMemoryTest(_) => "memory.system_test",
            Self::VramTest { .. } => "memory.vram_test",
            Self::PowerPlans => "power.list",
            Self::SystemStatus => "system.status",
            Self::SettingsShow => "settings.show",
            Self::SettingsSet { .. } => "settings.set",
            Self::ProfilesList => "profiles.list",
            Self::ProfileImport { .. } => "profiles.import_ini",
            Self::ProcessAffinityPreview { .. } => "process.affinity.preview",
            Self::ProcessAffinityApply { .. } => "process.affinity.apply",
            Self::ProcessAffinityRollback { .. } => "process.affinity.rollback",
            Self::WheaEvents { .. } => "events.whea",
            Self::FrameCapture { .. } => "frames.capture",
            Self::RomInspect { .. } => "rom.inspect",
            Self::OperationPreview(_) => "operation.preview",
            Self::OperationApply { .. } => "operation.apply",
            Self::OperationRollback { .. } => "operation.rollback",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Success,
    Failure,
    Unavailable,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandError {
    pub category: ErrorCategory,
    pub message: String,
    pub exit_code: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CommandEnvelope {
    pub schema_version: String,
    pub command: String,
    pub capability: Option<CapabilityReport>,
    pub status: CommandStatus,
    pub data: Option<Value>,
    pub error: Option<CommandError>,
}

impl CommandEnvelope {
    #[must_use]
    pub fn success(
        command: impl Into<String>,
        capability: Option<CapabilityReport>,
        data: Value,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.into(),
            command: command.into(),
            capability,
            status: CommandStatus::Success,
            data: Some(data),
            error: None,
        }
    }

    #[must_use]
    pub fn failure(
        command: impl Into<String>,
        capability: Option<CapabilityReport>,
        error: NorthclockError,
    ) -> Self {
        let status = match error.category() {
            ErrorCategory::Unavailable => CommandStatus::Unavailable,
            ErrorCategory::PermissionOrSafety => CommandStatus::Rejected,
            ErrorCategory::Internal
            | ErrorCategory::InvalidUsage
            | ErrorCategory::HardwareOperation => CommandStatus::Failure,
        };
        Self {
            schema_version: SCHEMA_VERSION.into(),
            command: command.into(),
            capability,
            status,
            data: None,
            error: Some(CommandError {
                category: error.category(),
                message: error.to_string(),
                exit_code: error.exit_code(),
            }),
        }
    }

    #[must_use]
    pub fn exit_code(&self) -> u8 {
        self.error.as_ref().map_or(0, |error| error.exit_code)
    }
}
