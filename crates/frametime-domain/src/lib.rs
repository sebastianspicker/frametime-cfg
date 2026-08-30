//! Platform-neutral workflow, persistence, migration, and recovery contracts.

pub mod audit;
pub mod backup;
pub mod benchmark;
pub mod binding;
pub mod catalog;
pub mod cleanup;
pub mod config;
pub mod cs2;
pub mod cs2_config;
pub mod engine;
pub mod evidence;
pub mod fps;
pub mod handoff;
pub mod latency;
pub mod logging;
pub mod migration;
pub mod operations;
pub mod orchestration;
pub mod persistence;
pub mod policy;
pub mod runtime;
pub mod state;
pub mod steam;
pub mod verification;
pub mod video;

/// Platform-neutral, fail-closed driver evidence and dry-run planning contracts.
pub mod driver;

/// Versioned, platform-neutral contracts for native hardware diagnostics.
pub mod hardware;

pub use audit::{
    AppxRemovalSubject, AuditEntry, AuditFile, IrreversibleAudit, ManualRecoveryAudit,
    ManualRecoveryAuditOutcome, ManualRecoveryAuditRecordType, ManualRecoveryTarget,
    MixedRecoveryAudit, MixedRecoveryAuditRecordType, P1_3_REBUILDABLE_TARGETS,
    P1_13_MANUAL_RECOVERY_TARGET, P2_2_MANUAL_RECOVERY_TARGET, P3_1_MANUAL_RECOVERY_TARGET,
    RebuildableAudit, RebuildableAuditOutcome, RebuildableAuditRecordType, RebuildableTarget,
    RecoveryRequirement,
};
pub use backup::{
    BackupEntry, BackupFile, CS2_CONFIG_MAX_FILE_BYTES, CS2_CONFIG_MAX_TOTAL_BYTES,
    CS2_CONFIG_TRANSACTION_STEP, CS2_OPTIONAL_CONFIG_TRANSACTION_STEP, Cs2ConfigBackupError,
    Cs2ConfigSnapshot, Cs2InstallIdentity, DrsApplicationBinding, InterruptPolicyBackup,
    InterruptPolicyBackupError, InterruptPolicyKind, InterruptPolicyValue,
    NETWORK_STACK_TRANSACTION_STEP, NetworkStackBackupError, NetworkStackNlaBackup,
    NetworkStackPolicy, NetworkStackPolicyBackup, NetworkStackPolicySnapshot,
    NetworkStackRawRegistryValue, NetworkStackSetting, NetworkStackSettingBackup,
    NetworkStackTransaction, NetworkStackValue, PagefileTransactionSetting,
};
pub use benchmark::{
    BASELINE_BENCHMARK_LABEL, BaselineBenchmarkCommit, FINAL_BENCHMARK_LABEL,
    FINAL_BENCHMARK_SCHEMA_VERSION, FinalBenchmarkCommit, FinalBenchmarkReceipt,
    prepare_baseline_benchmark_commit, prepare_final_benchmark_commit,
    validate_persisted_baseline_benchmark, validate_persisted_final_benchmark,
};
pub use binding::{
    BindingError, BindingReceiptId, NATIVE_BINDING_SCHEMA_VERSION, NetworkAdapterBinding,
    PciDeviceBinding,
};
pub use catalog::{
    ActionIntent, Depth, GpuApplicability, OperationKind, OrchestrationRole,
    PHASE_ONE_BASELINE_BENCHMARK, PHASE_ONE_RESIZABLE_BAR_CHECK, PHASE_ONE_SAFE_MODE_HANDOFF,
    PHASE_ONE_XMP_EXPO_CHECK, PHASE_THREE_DRIVER_INSTALL, PHASE_THREE_FINAL_BENCHMARK,
    PHASE_TWO_DRIVER_CLEANUP, PHASE_TWO_PHASE_THREE_HANDOFF, PHASE_TWO_SAFE_BOOT_CLEAR, Phase,
    Risk, Step, StepId, step_by_id, step_catalog, steps_for_phase_and_role,
};
pub use cleanup::{
    CleanupAction, CleanupActionOutcome, CleanupActionResult, CleanupActionSpec, CleanupMode,
    CleanupRecovery, CleanupReport, CleanupTargetClass, DeniedCleanupTargetClass, cleanup_actions,
    denied_cleanup_targets, requires_irreversible_acknowledgement,
};
pub use config::Config;
pub use cs2_config::{
    AutoexecBackup, CfgAssetDeployment, Cs2ConfigController, Cs2ConfigError, Cs2ConfigFs,
    Cs2ConfigPreview, Cs2ConfigRequest, Cs2ConfigTarget, Cs2ConfigWriteReport, OptimizationBackup,
    OptionalCfgAsset,
};
pub use engine::{Backend, Engine, EngineError, Event, Inspection, Operation, RunReport};
pub use evidence::{
    EVIDENCE_SCHEMA_VERSION, EvidenceEntry, EvidenceError, EvidenceFile, EvidenceRequirement,
    ObservationReceipt, ObservationSubject,
};
pub use handoff::{
    DriverPackageRecord, RebootStage, RebootTransaction, RuntimeRecord, TransactionId,
};
pub use migration::{LegacyHandoff, MigrationDecision, MigrationInventory, assess_inventory};
pub use operations::{GpuBranch, PlannedAction, plan_for_step};
pub use orchestration::{
    BootEnvironment, Compensation, Evidence, FailurePoint, GuardError, HandoffEvidence, PhaseFacts,
    PhaseRequest, RecoveryPlan, RuntimeBinding, Transition, authorize, recovery_for,
    require_phase_one_handoff_ready,
};
pub use policy::{Decision, Profile};
pub use state::{AdvisoryResolution, Progress, State};
pub use steam::{Cs2Install, SteamError, app_manifest_is_cs2, library_paths_from_vdf};
pub use verification::{VerificationItem, VerificationReport, VerificationStatus};
pub use video::{
    VideoDocument, VideoDocumentRoot, VideoError, VideoGoal, VideoRow, VideoStatus,
    parse_video_document, video_guidance,
};

pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");
