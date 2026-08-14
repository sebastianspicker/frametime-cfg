use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{
    ArtifactAcquisitionAuthorization, ArtifactLocator, CanonicalPackageSet, CaptureFreshnessPolicy,
    DriverExecutionCapture, DriverPlanInput, DryRunDriverPlan, ExactGpuIdentity,
    InstalledArtifactObservation, OemPublishedName, PackageRemovalOutcome, PublishedDriverPackage,
    RemovalExecutionEvidence, SCHEMA_VERSION, SafeModeObservation, SignedArtifactDescriptor,
    ValidationError,
};

/// An adapter error designed for safe display by a CLI or GUI status surface.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("adapter {operation} failed: {reason}")]
pub struct AdapterFailure {
    pub operation: &'static str,
    pub reason: String,
}

/// Future native inspection boundary. No implementation is supplied here.
pub trait InspectionAdapter {
    fn inspect_exact_gpu(&self) -> Result<ExactGpuIdentity, AdapterFailure>;
    fn inspect_published_packages(
        &self,
        target_gpu: &ExactGpuIdentity,
    ) -> Result<Vec<PublishedDriverPackage>, AdapterFailure>;
}

/// Future artifact-acquisition boundary. Implementations must return a fully
/// validated descriptor; this crate neither fetches nor opens artifact data.
pub trait AcquisitionAdapter {
    fn acquire_descriptor(
        &self,
        locator: &ArtifactLocator,
        target_gpu: &ExactGpuIdentity,
    ) -> Result<SignedArtifactDescriptor, AdapterFailure>;
}

/// Injected wall-clock boundary used to evaluate capture freshness. Implement
/// this with a host clock or a deterministic fake; this crate never reads a
/// system clock itself.
pub trait ExecutionClock {
    fn current_utc(&self) -> Result<String, AdapterFailure>;
}

/// Injected Safe Mode observation boundary. A non-confirming observation is
/// valid evidence but is rejected by execution-evidence validation.
pub trait SafeModeInspectionAdapter {
    fn observe_safe_mode(
        &self,
        target_gpu: &ExactGpuIdentity,
    ) -> Result<SafeModeObservation, AdapterFailure>;
}

/// Injected package execution boundary. Each OEM package has its own typed
/// outcome; callers must additionally obtain a fresh package inventory.
pub trait PackageExecutionAdapter {
    fn remove_published_package(
        &self,
        target_gpu: &ExactGpuIdentity,
        published_name: &OemPublishedName,
    ) -> Result<PackageRemovalOutcome, AdapterFailure>;

    fn inspect_published_packages(
        &self,
        target_gpu: &ExactGpuIdentity,
    ) -> Result<Vec<PublishedDriverPackage>, AdapterFailure>;
}

/// Injected signed-artifact installation boundary. An observation records the
/// exact artifact identity, never a generic success boolean.
pub trait ArtifactInstallationAdapter {
    fn install_authorized_artifact(
        &self,
        authorization: &ArtifactAcquisitionAuthorization,
        artifact: &SignedArtifactDescriptor,
    ) -> Result<InstalledArtifactObservation, AdapterFailure>;
}

/// Durable evidence captured before a mutation adapter may apply a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureReceipt {
    pub schema_version: u32,
    pub plan_sha256: crate::Sha256Digest,
    pub target_gpu: ExactGpuIdentity,
    pub complete: bool,
    pub captured_at_utc: String,
    #[serde(flatten, default)]
    pub extensions: BTreeMap<String, Value>,
}

/// A future adapter's post-apply readback record. It is evidence, not proof
/// that this platform-neutral crate has performed an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReceipt {
    pub schema_version: u32,
    pub plan_sha256: crate::Sha256Digest,
    pub applied_at_utc: String,
    pub verified: bool,
    #[serde(flatten, default)]
    pub extensions: BTreeMap<String, Value>,
}

/// Future state-changing boundary. Correct implementations call
/// `capture_before_apply` before `apply_captured`. A serialized receipt remains
/// evidence only; a native implementation must re-observe its authority before
/// mutation.
pub trait MutationAdapter {
    fn capture_before_apply(
        &self,
        plan: &DryRunDriverPlan,
    ) -> Result<CaptureReceipt, AdapterFailure>;
    fn apply_captured(
        &self,
        plan: &DryRunDriverPlan,
        capture: &CaptureReceipt,
    ) -> Result<ApplyReceipt, AdapterFailure>;
}

/// Validate that serialized capture evidence names the same plan and GPU.
///
/// This does not authorize a mutation. Operator-supplied JSON can reproduce
/// these fields, so a native adapter must independently re-observe the device,
/// package, signature, and durable snapshot immediately before apply.
pub fn validate_capture_binding(
    plan: &DryRunDriverPlan,
    capture: &CaptureReceipt,
) -> Result<(), ValidationError> {
    plan.validate()?;
    if capture.schema_version != SCHEMA_VERSION || !capture.complete {
        return Err(ValidationError::IncompleteReceipt);
    }
    capture.target_gpu.validate()?;
    if capture.plan_sha256 != plan.input_sha256 || capture.target_gpu != plan.target_gpu {
        return Err(ValidationError::ReceiptPlanMismatch);
    }
    if capture.captured_at_utc.trim().is_empty()
        || capture.captured_at_utc.chars().any(char::is_control)
    {
        return Err(ValidationError::Invalid {
            field: "capturedAtUtc",
        });
    }
    Ok(())
}

/// Helper for a future UI/controller to obtain validated input without
/// allowing it to interpret platform records itself.
pub fn inspect_input(
    inspection: &dyn InspectionAdapter,
    acquisition: &dyn AcquisitionAdapter,
    locator: &ArtifactLocator,
) -> Result<DriverPlanInput, AdapterFailure> {
    let target_gpu = inspection.inspect_exact_gpu()?;
    let installed_packages = inspection.inspect_published_packages(&target_gpu)?;
    let artifact = acquisition.acquire_descriptor(locator, &target_gpu)?;
    Ok(DriverPlanInput {
        target_gpu,
        installed_packages,
        artifact,
        extensions: BTreeMap::new(),
    })
}

/// Capture the exact, fresh Safe Mode package state used by P2:2.  This is a
/// typed boundary: the caller never derives package identities from PnPUtil
/// output or assembles a command string.
pub fn capture_driver_execution(
    plan: &DryRunDriverPlan,
    safe_mode: &dyn SafeModeInspectionAdapter,
    packages: &dyn PackageExecutionAdapter,
    clock: &dyn ExecutionClock,
    freshness: CaptureFreshnessPolicy,
) -> Result<DriverExecutionCapture, AdapterFailure> {
    plan.validate().map_err(|error| AdapterFailure {
        operation: "capture driver execution",
        reason: error.to_string(),
    })?;
    let safe_mode = safe_mode.observe_safe_mode(&plan.target_gpu)?;
    let installed_packages = CanonicalPackageSet::from_unsorted(
        plan.target_gpu.clone(),
        packages.inspect_published_packages(&plan.target_gpu)?,
    )
    .map_err(|error| AdapterFailure {
        operation: "capture driver execution",
        reason: error.to_string(),
    })?;
    let captured_at_utc = clock.current_utc()?;
    let capture = DriverExecutionCapture {
        schema_version: SCHEMA_VERSION,
        plan_sha256: plan.input_sha256.clone(),
        target_gpu: plan.target_gpu.clone(),
        safe_mode,
        package_set_sha256: installed_packages
            .fingerprint()
            .map_err(|error| AdapterFailure {
                operation: "capture driver execution",
                reason: error.to_string(),
            })?,
        installed_packages,
        captured_at_utc: captured_at_utc.clone(),
    };
    capture
        .validate_for_plan_at(plan, freshness, &captured_at_utc)
        .map_err(|error| AdapterFailure {
            operation: "capture driver execution",
            reason: error.to_string(),
        })?;
    Ok(capture)
}

/// Execute P2:2 only from a just-captured, Safe-Mode-confirmed package set,
/// then require a complete fresh inventory readback before returning evidence.
pub fn remove_captured_packages(
    plan: &DryRunDriverPlan,
    capture: DriverExecutionCapture,
    packages: &dyn PackageExecutionAdapter,
    clock: &dyn ExecutionClock,
    freshness: CaptureFreshnessPolicy,
) -> Result<RemovalExecutionEvidence, AdapterFailure> {
    let now = clock.current_utc()?;
    capture
        .validate_for_plan_at(plan, freshness, &now)
        .map_err(|error| AdapterFailure {
            operation: "remove driver packages",
            reason: error.to_string(),
        })?;
    let mut outcomes = Vec::with_capacity(capture.installed_packages.packages.len());
    for package in &capture.installed_packages.packages {
        outcomes
            .push(packages.remove_published_package(&capture.target_gpu, &package.published_name)?);
    }
    let post_removal_packages = CanonicalPackageSet::from_unsorted(
        capture.target_gpu.clone(),
        packages.inspect_published_packages(&capture.target_gpu)?,
    )
    .map_err(|error| AdapterFailure {
        operation: "read back driver removal",
        reason: error.to_string(),
    })?;
    let evidence = RemovalExecutionEvidence {
        capture,
        outcomes,
        post_removal_packages,
        observed_at_utc: clock.current_utc()?,
    };
    let now = clock.current_utc()?;
    evidence
        .validate_for_plan_at(plan, freshness, &now)
        .map_err(|error| AdapterFailure {
            operation: "verify driver removal",
            reason: error.to_string(),
        })?;
    Ok(evidence)
}
