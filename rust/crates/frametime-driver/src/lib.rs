#![forbid(unsafe_code)]

//! Platform-neutral Driver Foundry domain contracts.
//!
//! This crate creates and validates read-only plans only. It contains no host
//! inspection, acquisition, or mutation implementation and makes no live
//! platform claim. Future native adapters must obtain a capture receipt before
//! an apply operation can be authorized.

mod adapters;
mod evidence;
mod model;
mod plan;

pub use adapters::{
    AcquisitionAdapter, AdapterFailure, ApplyReceipt, ArtifactInstallationAdapter, CaptureReceipt,
    ExecutionClock, InspectionAdapter, MutationAdapter, PackageExecutionAdapter,
    SafeModeInspectionAdapter, capture_driver_execution, inspect_input, remove_captured_packages,
    validate_capture_binding,
};
pub use evidence::{
    ArtifactAcquisitionAuthorization, ArtifactIdentity, CanonicalPackageSet,
    CaptureFreshnessPolicy, DriverExecutionCapture, InstallationEvidence,
    InstalledArtifactObservation, PackageRemovalDisposition, PackageRemovalOutcome,
    RemovalExecutionEvidence, SafeModeObservation, SafeModeState,
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
