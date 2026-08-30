#![forbid(unsafe_code)]

//! Portable Northclock contracts and application workflows.

pub mod application;
mod application_contract;
mod application_support;
pub mod backend;
pub mod capability;
pub mod error;
pub mod measurement;
pub mod memory;
pub mod operation;
mod operation_workflow;
pub mod persistence;
pub mod rom;
pub mod system_status;

pub use application::{ApplicationCommand, ApplicationService, CommandEnvelope, CommandStatus};
pub use backend::*;
pub use capability::{CapabilityReport, CapabilityState};
pub use error::{ErrorCategory, NorthclockError, Result};
pub use measurement::{DeviceIdentity, Measurement};
pub use memory::{run_system_memory_test, MemoryTestConfig, MemoryTestReport};
pub use operation::{
    ApplyReceipt, OperationPlan, OperationRequest, OperationTarget, ProcessAffinityPlan,
    ProcessAffinityReceipt, ProcessAffinityRollbackReceipt, RollbackReceipt, SafetyPolicy,
    WriteAuthorization, RISK_ACKNOWLEDGEMENT,
};
pub use persistence::{AppSettings, Profile, Storage, SETTINGS_SCHEMA_VERSION};
pub use rom::{inspect_rom, RomInspection};
pub use system_status::{
    ConflictKind, Observation, ObservationState, PotentialConflict, RegisteredTask,
    ScheduledTaskState, SystemStatusReport, TaskSchedulerStatus, VbsRuntimeState, VbsStatus,
};

pub const SCHEMA_VERSION: &str = "1.0";
