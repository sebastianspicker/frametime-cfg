use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    DryRunDriverPlan, ExactGpuIdentity, OemPublishedName, PublishedDriverPackage, SCHEMA_VERSION,
    Sha256Digest, SignedArtifactDescriptor, ValidationError,
};

fn timestamp(value: &str, field: &'static str) -> Result<i64, ValidationError> {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return Err(ValidationError::Invalid { field });
    }
    let number = |start: usize, end: usize| {
        bytes[start..end]
            .iter()
            .try_fold(0_i64, |value, byte| match byte {
                b'0'..=b'9' => Ok(value * 10 + i64::from(byte - b'0')),
                _ => Err(ValidationError::Invalid { field }),
            })
    };
    let year = number(0, 4)?;
    let month = number(5, 7)?;
    let day = number(8, 10)?;
    let hour = number(11, 13)?;
    let minute = number(14, 16)?;
    let second = number(17, 19)?;
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return Err(ValidationError::Invalid { field }),
    };
    if year == 0 || day == 0 || day > days_in_month || hour > 23 || minute > 59 || second > 59 {
        Err(ValidationError::Invalid { field })
    } else {
        let completed_years = year - 1;
        let days_before_year = completed_years * 365 + completed_years / 4 - completed_years / 100
            + completed_years / 400;
        let month_days = [0_i64, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
        let mut days = days_before_year + month_days[(month - 1) as usize] + day - 1;
        if month > 2 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            days += 1;
        }
        Ok(days * 86_400 + hour * 3_600 + minute * 60 + second)
    }
}

fn text(value: &str, field: &'static str) -> Result<(), ValidationError> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(ValidationError::Invalid { field })
    } else {
        Ok(())
    }
}

/// One exact GPU-bound installed package set in canonical OEM-name order.
/// Empty sets are permitted for a fresh post-removal observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalPackageSet {
    pub target_gpu: ExactGpuIdentity,
    pub packages: Vec<PublishedDriverPackage>,
}

impl CanonicalPackageSet {
    pub fn from_unsorted(
        target_gpu: ExactGpuIdentity,
        mut packages: Vec<PublishedDriverPackage>,
    ) -> Result<Self, ValidationError> {
        packages.sort_by(|left, right| left.published_name.cmp(&right.published_name));
        let set = Self {
            target_gpu,
            packages,
        };
        set.validate()?;
        Ok(set)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        self.target_gpu.validate()?;
        let mut names = BTreeSet::new();
        let mut previous = None;
        for package in &self.packages {
            package.validate_for(&self.target_gpu)?;
            if !names.insert(package.published_name.clone())
                || previous
                    .as_ref()
                    .is_some_and(|prior| prior >= &package.published_name)
            {
                return Err(ValidationError::NonCanonicalPackageSet);
            }
            previous = Some(package.published_name.clone());
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> Result<Sha256Digest, ValidationError> {
        self.validate()?;
        let encoded = serde_json::to_vec(self).map_err(|_| ValidationError::Invalid {
            field: "packageSet",
        })?;
        Sha256Digest::parse(format!("{:x}", Sha256::digest(encoded)))
    }

    fn names(&self) -> BTreeSet<OemPublishedName> {
        self.packages
            .iter()
            .map(|package| package.published_name.clone())
            .collect()
    }
}

/// Explicit runtime observation. `Confirmed` is required for removal, but
/// recording the other states is useful evidence and never implies mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SafeModeState {
    Confirmed,
    NotDetected,
    Indeterminate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeModeObservation {
    pub target_gpu: ExactGpuIdentity,
    pub state: SafeModeState,
    pub observed_at_utc: String,
    pub boot_session_id: String,
}

impl SafeModeObservation {
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.target_gpu.validate()?;
        timestamp(&self.observed_at_utc, "safeModeObservedAtUtc")?;
        text(&self.boot_session_id, "bootSessionId")
    }
}

/// Host-selected maximum age for an execution capture. The host supplies the
/// current timestamp, which keeps the portable domain free of system-clock IO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureFreshnessPolicy {
    pub maximum_age_seconds: u64,
}

impl CaptureFreshnessPolicy {
    pub fn validate_capture_at(
        self,
        captured_at_utc: &str,
        now_utc: &str,
    ) -> Result<(), ValidationError> {
        if self.maximum_age_seconds == 0 {
            return Err(ValidationError::Invalid {
                field: "maximumAgeSeconds",
            });
        }
        let captured = timestamp(captured_at_utc, "capturedAtUtc")?;
        let now = timestamp(now_utc, "nowUtc")?;
        let age = now - captured;
        if age < 0 || age > i64::try_from(self.maximum_age_seconds).unwrap_or(i64::MAX) {
            Err(ValidationError::StaleCapture)
        } else {
            Ok(())
        }
    }
}

/// Durable, pre-mutation evidence bound to the exact plan and package set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverExecutionCapture {
    pub schema_version: u32,
    pub plan_sha256: Sha256Digest,
    pub target_gpu: ExactGpuIdentity,
    pub safe_mode: SafeModeObservation,
    pub installed_packages: CanonicalPackageSet,
    pub package_set_sha256: Sha256Digest,
    pub captured_at_utc: String,
}

impl DriverExecutionCapture {
    pub fn validate_for_plan_at(
        &self,
        plan: &DryRunDriverPlan,
        freshness: CaptureFreshnessPolicy,
        now_utc: &str,
    ) -> Result<(), ValidationError> {
        plan.validate()?;
        if self.schema_version != SCHEMA_VERSION
            || self.plan_sha256 != plan.input_sha256
            || self.target_gpu != plan.target_gpu
        {
            return Err(ValidationError::CapturePlanMismatch);
        }
        self.installed_packages.validate()?;
        if self.installed_packages.target_gpu != self.target_gpu
            || self.package_set_sha256 != self.installed_packages.fingerprint()?
            || self.installed_packages.names() != planned_names(plan)?
        {
            return Err(ValidationError::CapturePlanMismatch);
        }
        self.safe_mode.validate()?;
        if self.safe_mode.target_gpu != self.target_gpu
            || self.safe_mode.state != SafeModeState::Confirmed
        {
            return Err(ValidationError::SafeModeNotConfirmed);
        }
        let safe_mode_observed =
            timestamp(&self.safe_mode.observed_at_utc, "safeModeObservedAtUtc")?;
        let captured_at = timestamp(&self.captured_at_utc, "capturedAtUtc")?;
        if safe_mode_observed > captured_at {
            return Err(ValidationError::CapturePlanMismatch);
        }
        if captured_at - safe_mode_observed
            > i64::try_from(freshness.maximum_age_seconds).unwrap_or(i64::MAX)
        {
            return Err(ValidationError::StaleCapture);
        }
        freshness.validate_capture_at(&self.captured_at_utc, now_utc)
    }
}

fn planned_names(plan: &DryRunDriverPlan) -> Result<BTreeSet<OemPublishedName>, ValidationError> {
    match plan.entries.get(1).map(|entry| &entry.action) {
        Some(crate::PlannedDriverAction::RecordExactPackages { packages }) => Ok(packages
            .iter()
            .map(|package| package.published_name.clone())
            .collect()),
        _ => Err(ValidationError::CapturePlanMismatch),
    }
}

/// Stable identity of the exact signed artifact approved for installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactIdentity {
    pub artifact_id: String,
    pub artifact_file_name: String,
    pub payload_sha256: Sha256Digest,
    pub signer_thumbprint_sha256: Sha256Digest,
}

impl ArtifactIdentity {
    pub fn from_descriptor(descriptor: &SignedArtifactDescriptor) -> Result<Self, ValidationError> {
        descriptor.locator.validate()?;
        descriptor.authenticode.validate()?;
        Ok(Self {
            artifact_id: descriptor.locator.artifact_id.clone(),
            artifact_file_name: descriptor.locator.artifact_file_name.clone(),
            payload_sha256: descriptor.payload_sha256.clone(),
            signer_thumbprint_sha256: descriptor.authenticode.signer_thumbprint_sha256.clone(),
        })
    }

    pub fn validate_matches(
        &self,
        descriptor: &SignedArtifactDescriptor,
    ) -> Result<(), ValidationError> {
        if self == &Self::from_descriptor(descriptor)? {
            Ok(())
        } else {
            Err(ValidationError::ArtifactIdentityMismatch)
        }
    }
}

/// Authorization to acquire and install one exact signed artifact. It is an
/// evidence contract, not a cryptographic verifier or an installer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactAcquisitionAuthorization {
    pub schema_version: u32,
    pub authorization_id: String,
    pub plan_sha256: Sha256Digest,
    pub target_gpu: ExactGpuIdentity,
    pub package_set_sha256: Sha256Digest,
    pub artifact: ArtifactIdentity,
    pub authorized_at_utc: String,
    pub expires_at_utc: String,
}

impl ArtifactAcquisitionAuthorization {
    pub fn validate_for_capture_at(
        &self,
        capture: &DriverExecutionCapture,
        artifact: &SignedArtifactDescriptor,
        now_utc: &str,
    ) -> Result<(), ValidationError> {
        if self.schema_version != SCHEMA_VERSION
            || self.plan_sha256 != capture.plan_sha256
            || self.target_gpu != capture.target_gpu
            || self.package_set_sha256 != capture.package_set_sha256
        {
            return Err(ValidationError::AuthorizationMismatch);
        }
        text(&self.authorization_id, "authorizationId")?;
        self.target_gpu.validate()?;
        self.artifact.validate_matches(artifact)?;
        let authorized = timestamp(&self.authorized_at_utc, "authorizedAtUtc")?;
        let expires = timestamp(&self.expires_at_utc, "expiresAtUtc")?;
        let now = timestamp(now_utc, "nowUtc")?;
        if authorized > now || expires < now || authorized >= expires {
            Err(ValidationError::AuthorizationExpired)
        } else {
            Ok(())
        }
    }
}

/// A per-OEM outcome deliberately richer than a boolean receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PackageRemovalDisposition {
    Removed,
    AlreadyAbsent,
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRemovalOutcome {
    pub published_name: OemPublishedName,
    pub disposition: PackageRemovalDisposition,
    pub observed_at_utc: String,
}

/// Complete per-package result plus an independently re-observed inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalExecutionEvidence {
    pub capture: DriverExecutionCapture,
    pub outcomes: Vec<PackageRemovalOutcome>,
    pub post_removal_packages: CanonicalPackageSet,
    pub observed_at_utc: String,
}

impl RemovalExecutionEvidence {
    pub fn validate_for_plan_at(
        &self,
        plan: &DryRunDriverPlan,
        freshness: CaptureFreshnessPolicy,
        now_utc: &str,
    ) -> Result<(), ValidationError> {
        self.capture
            .validate_for_plan_at(plan, freshness, now_utc)?;
        self.post_removal_packages.validate()?;
        // `post_removal_packages` is structurally the readback after the
        // ordered per-package outcomes. Windows timestamps are only precise
        // to a second here, so equality is valid; requiring an artificial
        // sleep would not strengthen the mutation binding.
        if self.post_removal_packages.target_gpu != self.capture.target_gpu
            || timestamp(&self.observed_at_utc, "postRemovalObservedAtUtc")?
                < timestamp(&self.capture.captured_at_utc, "capturedAtUtc")?
        {
            return Err(ValidationError::InvalidRemovalEvidence);
        }
        let expected = self.capture.installed_packages.names();
        let outcome_names = self
            .outcomes
            .iter()
            .map(|outcome| outcome.published_name.clone())
            .collect::<BTreeSet<_>>();
        if self.outcomes.len() != expected.len() || outcome_names != expected {
            return Err(ValidationError::InvalidRemovalEvidence);
        }
        for outcome in &self.outcomes {
            if outcome.disposition != PackageRemovalDisposition::Removed
                || timestamp(&outcome.observed_at_utc, "removalObservedAtUtc")?
                    < timestamp(&self.capture.captured_at_utc, "capturedAtUtc")?
            {
                return Err(ValidationError::InvalidRemovalEvidence);
            }
        }
        if !expected.is_disjoint(&self.post_removal_packages.names()) {
            Err(ValidationError::InvalidRemovalEvidence)
        } else {
            Ok(())
        }
    }
}

/// A direct host observation of the artifact that was installed. The domain
/// does not open files or assert that the host actually ran an installer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledArtifactObservation {
    pub artifact: ArtifactIdentity,
    pub observed_at_utc: String,
}

/// Fresh post-install evidence bound to the authorizing capture and artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationEvidence {
    pub authorization: ArtifactAcquisitionAuthorization,
    /// Authenticode evidence observed immediately before launching the exact
    /// retained installer capability. It is distinct from acquisition-time
    /// evidence because the package may have been replaced meanwhile.
    pub fresh_authenticode: crate::AuthenticodeEvidence,
    pub installed_artifact: InstalledArtifactObservation,
    pub post_install_packages: CanonicalPackageSet,
    pub observed_at_utc: String,
}

impl InstallationEvidence {
    /// The fail-closed execution path: validate the plan-bound fresh capture
    /// before accepting post-install evidence.
    pub fn validate_for_plan_at(
        &self,
        plan: &DryRunDriverPlan,
        capture: &DriverExecutionCapture,
        artifact: &SignedArtifactDescriptor,
        freshness: CaptureFreshnessPolicy,
        now_utc: &str,
    ) -> Result<(), ValidationError> {
        capture.validate_for_plan_at(plan, freshness, now_utc)?;
        self.validate_for_capture_at(capture, artifact, now_utc)
    }

    pub fn validate_for_capture_at(
        &self,
        capture: &DriverExecutionCapture,
        artifact: &SignedArtifactDescriptor,
        now_utc: &str,
    ) -> Result<(), ValidationError> {
        self.authorization
            .validate_for_capture_at(capture, artifact, now_utc)?;
        self.fresh_authenticode.validate()?;
        self.post_install_packages.validate()?;
        if self.post_install_packages.target_gpu != capture.target_gpu
            || self.post_install_packages.packages.is_empty()
            || self.installed_artifact.artifact != self.authorization.artifact
            || self.fresh_authenticode.signer_subject != artifact.authenticode.signer_subject
            || self.fresh_authenticode.signer_thumbprint_sha256
                != artifact.authenticode.signer_thumbprint_sha256
            || timestamp(
                &self.fresh_authenticode.observed_at_utc,
                "freshAuthenticodeObservedAtUtc",
            )? < timestamp(&self.authorization.authorized_at_utc, "authorizedAtUtc")?
            || timestamp(
                &self.fresh_authenticode.observed_at_utc,
                "freshAuthenticodeObservedAtUtc",
            )? > timestamp(
                &self.installed_artifact.observed_at_utc,
                "installObservedAtUtc",
            )?
            || timestamp(
                &self.installed_artifact.observed_at_utc,
                "installObservedAtUtc",
            )? < timestamp(&self.authorization.authorized_at_utc, "authorizedAtUtc")?
            || timestamp(&self.observed_at_utc, "postInstallObservedAtUtc")?
                < timestamp(&self.authorization.authorized_at_utc, "authorizedAtUtc")?
            || timestamp(
                &self.installed_artifact.observed_at_utc,
                "installObservedAtUtc",
            )? > timestamp(&self.observed_at_utc, "postInstallObservedAtUtc")?
            // The record's field order is the operation sequence: retained
            // artifact launch, SetupAPI reinspection, then this readback.
            // Equal second-resolution timestamps therefore remain coherent.
            || timestamp(&self.observed_at_utc, "postInstallObservedAtUtc")?
                < timestamp(&capture.captured_at_utc, "capturedAtUtc")?
        {
            return Err(ValidationError::InvalidInstallationEvidence);
        }
        Ok(())
    }
}
