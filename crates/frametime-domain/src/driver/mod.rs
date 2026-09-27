//! Platform-neutral Driver Foundry domain contracts.
//!
//! This module creates and validates read-only plans only. It contains no host
//! inspection, acquisition, or mutation implementation and makes no live
//! platform claim. Future native adapters must obtain a capture receipt before
//! an apply operation can be authorized.

mod adapters;
mod evidence;
mod lifecycle;
mod model;
mod plan;

pub use adapters::{
    AcquisitionAdapter, AdapterFailure, ApplyReceipt, ArtifactInstallationAdapter, CaptureReceipt,
    ExecutionClock, InspectionAdapter, MutationAdapter, PackageExecutionAdapter,
    SafeModeInspectionAdapter, capture_driver_execution, captured_package_removal_started,
    inspect_input, remove_captured_packages, resume_captured_package_removal,
    validate_capture_binding,
};
pub use evidence::{
    ArtifactAcquisitionAuthorization, ArtifactIdentity, CanonicalPackageSet,
    CaptureFreshnessPolicy, DriverExecutionCapture, InstallationEvidence,
    InstalledArtifactObservation, PackageRemovalDisposition, PackageRemovalOutcome,
    RemovalExecutionEvidence, SafeModeObservation, SafeModeState,
};
pub use lifecycle::{
    DriverLifecycleError, DriverTransactionV2, DriverTransactionV2Stage, DrsApplicationSnapshot,
    DrsItemKey, DrsItemKind, DrsMergeResult, DrsProfileSnapshot, DrsReconciliation,
    DrsReconciliationItem, DrsReconciliationStatus, DrsSettingSnapshot, DrsSnapshot, DrsValue,
    NVIDIA_COMPONENT_CATALOG_SCHEMA_VERSION, NVIDIA_DRS_SNAPSHOT_SCHEMA_VERSION,
    NVIDIA_TRANSACTION_V2_SCHEMA_VERSION, NvidiaCleanupScope, NvidiaComponentCatalog,
    NvidiaComponentDefinition, NvidiaComponentPreset, NvidiaComponentSelection,
    ProfileBackupDigestReference, RequiredUnclassifiedComponent, SourceComponentClassification,
    V1DriverTransactionState, V1TransactionMigrationDecision, migrate_v1_transaction,
};
pub use model::{
    ArtifactLocator, AuthenticodeEvidence, AuthenticodeStatus, ExactGpuIdentity, GpuVendor,
    OemPublishedName, PublishedDriverPackage, Sha256Digest, SignedArtifactDescriptor,
    ValidationError,
};
pub use plan::{
    DriverPlanInput, DriverPlanStep, DryRunDriverPlan, DryRunDriverPlanEntry, PlannedDriverAction,
    generate_dry_run_plan,
};

/// The schema version for public plan and evidence records.
pub const SCHEMA_VERSION: u32 = 1;
