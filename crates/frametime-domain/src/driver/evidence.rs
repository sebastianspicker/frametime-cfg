use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    DryRunDriverPlan, ExactGpuIdentity, OemPublishedName, PublishedDriverPackage, SCHEMA_VERSION,
    Sha256Digest, SignedArtifactDescriptor, ValidationError,
};

mod validation;
use validation::{text, timestamp};

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

    pub fn validate_for_resume(
        &self,
        capture: &DriverExecutionCapture,
        freshness: CaptureFreshnessPolicy,
        now_utc: &str,
    ) -> Result<(), ValidationError> {
        self.validate_against_capture(capture)?;
        freshness.validate_capture_at(&self.observed_at_utc, now_utc)
    }

    fn validate_against_capture(
        &self,
        capture: &DriverExecutionCapture,
    ) -> Result<(), ValidationError> {
        self.validate()?;
        if self.target_gpu != capture.target_gpu || self.state != SafeModeState::Confirmed {
            return Err(ValidationError::SafeModeNotConfirmed);
        }
        if timestamp(&self.observed_at_utc, "resumeSafeModeObservedAtUtc")?
            < timestamp(&capture.captured_at_utc, "capturedAtUtc")?
        {
            return Err(ValidationError::StaleCapture);
        }
        Ok(())
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
    pub fn validate_for_plan(
        &self,
        plan: &DryRunDriverPlan,
        freshness: CaptureFreshnessPolicy,
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
        if safe_mode_observed > captured_at
            || captured_at - safe_mode_observed
                > i64::try_from(freshness.maximum_age_seconds).unwrap_or(i64::MAX)
        {
            return Err(ValidationError::StaleCapture);
        }
        Ok(())
    }

    pub fn validate_for_plan_at(
        &self,
        plan: &DryRunDriverPlan,
        freshness: CaptureFreshnessPolicy,
        now_utc: &str,
    ) -> Result<(), ValidationError> {
        self.validate_for_plan(plan, freshness)?;
        freshness.validate_capture_at(&self.captured_at_utc, now_utc)
    }
}

fn planned_names(plan: &DryRunDriverPlan) -> Result<BTreeSet<OemPublishedName>, ValidationError> {
    match plan.entries.get(1).map(|entry| &entry.action) {
        Some(super::PlannedDriverAction::RecordExactPackages { packages }) => Ok(packages
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
    pub fn validate(&self) -> Result<(), ValidationError> {
        super::model::validate_token(&self.artifact_id, "artifactId")?;
        super::model::validate_leaf(&self.artifact_file_name, "artifactFileName")
    }

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
    pub fn validate_for_capture(
        &self,
        capture: &DriverExecutionCapture,
        artifact: &SignedArtifactDescriptor,
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
        if authorized >= expires {
            return Err(ValidationError::AuthorizationExpired);
        }
        Ok(())
    }

    pub fn validate_for_capture_at(
        &self,
        capture: &DriverExecutionCapture,
        artifact: &SignedArtifactDescriptor,
        now_utc: &str,
    ) -> Result<(), ValidationError> {
        self.validate_for_capture(capture, artifact)?;
        let authorized = timestamp(&self.authorized_at_utc, "authorizedAtUtc")?;
        let expires = timestamp(&self.expires_at_utc, "expiresAtUtc")?;
        let now = timestamp(now_utc, "nowUtc")?;
        if authorized > now || expires < now {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_safe_mode: Option<SafeModeObservation>,
    pub outcomes: Vec<PackageRemovalOutcome>,
    pub post_removal_packages: CanonicalPackageSet,
    pub observed_at_utc: String,
}

impl RemovalExecutionEvidence {
    pub fn validate_for_plan(
        &self,
        plan: &DryRunDriverPlan,
        freshness: CaptureFreshnessPolicy,
    ) -> Result<(), ValidationError> {
        self.capture.validate_for_plan(plan, freshness)?;
        if let Some(resume) = &self.resume_safe_mode {
            resume.validate_against_capture(&self.capture)?;
        }
        self.post_removal_packages.validate()?;
        // `post_removal_packages` is structurally the readback after the
        // ordered per-package outcomes. Windows timestamps are only precise
        // to a second here, so equality is valid; requiring an artificial
        // sleep would not strengthen the mutation binding.
        let authorized_at = self
            .resume_safe_mode
            .as_ref()
            .map_or(&self.capture.captured_at_utc, |resume| {
                &resume.observed_at_utc
            });
        if self.post_removal_packages.target_gpu != self.capture.target_gpu
            || timestamp(&self.observed_at_utc, "postRemovalObservedAtUtc")?
                < timestamp(authorized_at, "removalAuthorizedAtUtc")?
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
            if !matches!(
                &outcome.disposition,
                PackageRemovalDisposition::Removed | PackageRemovalDisposition::AlreadyAbsent
            ) || timestamp(&outcome.observed_at_utc, "removalObservedAtUtc")?
                < timestamp(authorized_at, "removalAuthorizedAtUtc")?
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

    pub fn validate_for_plan_at(
        &self,
        plan: &DryRunDriverPlan,
        freshness: CaptureFreshnessPolicy,
        now_utc: &str,
    ) -> Result<(), ValidationError> {
        self.validate_for_plan(plan, freshness)?;
        let authorized_at = self
            .resume_safe_mode
            .as_ref()
            .map_or(&self.capture.captured_at_utc, |resume| {
                &resume.observed_at_utc
            });
        freshness.validate_capture_at(authorized_at, now_utc)
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
    pub fresh_authenticode: super::AuthenticodeEvidence,
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
        self.validate_for_capture(capture, artifact)?;
        self.authorization
            .validate_for_capture_at(capture, artifact, now_utc)
    }

    pub fn validate_for_capture(
        &self,
        capture: &DriverExecutionCapture,
        artifact: &SignedArtifactDescriptor,
    ) -> Result<(), ValidationError> {
        self.validate_bound_observations(capture, artifact, true)
    }

    /// Validate a post-removal recovery install. The original bounded
    /// authorization remains structurally bound, while its wall-clock expiry
    /// does not strand a machine whose captured packages are already absent.
    pub fn validate_for_recovery(
        &self,
        capture: &DriverExecutionCapture,
        artifact: &SignedArtifactDescriptor,
    ) -> Result<(), ValidationError> {
        self.validate_bound_observations(capture, artifact, false)
    }

    fn validate_bound_observations(
        &self,
        capture: &DriverExecutionCapture,
        artifact: &SignedArtifactDescriptor,
        enforce_authorization_expiry: bool,
    ) -> Result<(), ValidationError> {
        self.authorization.validate_for_capture(capture, artifact)?;
        self.fresh_authenticode.validate()?;
        self.installed_artifact.artifact.validate()?;
        self.post_install_packages.validate()?;
        if !self.matches_capture_and_artifact(capture, artifact)
            || !self.observations_are_ordered(capture, enforce_authorization_expiry)?
        {
            return Err(ValidationError::InvalidInstallationEvidence);
        }
        Ok(())
    }

    fn matches_capture_and_artifact(
        &self,
        capture: &DriverExecutionCapture,
        artifact: &SignedArtifactDescriptor,
    ) -> bool {
        if self.post_install_packages.target_gpu != capture.target_gpu
            || self.post_install_packages.packages.is_empty()
            || self.installed_artifact.artifact.artifact_id
                != self.authorization.artifact.artifact_id
            || self.installed_artifact.artifact.signer_thumbprint_sha256
                != self.fresh_authenticode.signer_thumbprint_sha256
        {
            return false;
        }
        self.matches_artifact_signer(artifact)
    }

    fn matches_artifact_signer(&self, artifact: &SignedArtifactDescriptor) -> bool {
        if self.fresh_authenticode.signer_subject != artifact.authenticode.signer_subject {
            return false;
        }
        self.fresh_authenticode.signer_thumbprint_sha256
            == artifact.authenticode.signer_thumbprint_sha256
    }

    fn observations_are_ordered(
        &self,
        capture: &DriverExecutionCapture,
        enforce_authorization_expiry: bool,
    ) -> Result<bool, ValidationError> {
        let authorization = timestamp(&self.authorization.authorized_at_utc, "authorizedAtUtc")?;
        let expires = timestamp(&self.authorization.expires_at_utc, "expiresAtUtc")?;
        let fresh = timestamp(
            &self.fresh_authenticode.observed_at_utc,
            "freshAuthenticodeObservedAtUtc",
        )?;
        let installed = timestamp(
            &self.installed_artifact.observed_at_utc,
            "installObservedAtUtc",
        )?;
        let observed = timestamp(&self.observed_at_utc, "postInstallObservedAtUtc")?;
        let captured = timestamp(&capture.captured_at_utc, "capturedAtUtc")?;
        Ok(fresh >= authorization
            && (!enforce_authorization_expiry || fresh <= expires)
            && fresh <= installed
            && installed >= authorization
            && (!enforce_authorization_expiry || installed <= expires)
            && observed >= authorization
            && installed <= observed
            && observed >= captured)
    }
}

#[cfg(test)]
mod timestamp_tests {
    use super::*;

    #[test]
    fn timestamp_accepts_leap_day_and_preserves_second_resolution() {
        let first = timestamp("2024-02-29T23:59:58Z", "capturedAtUtc").expect("leap day");
        let second = timestamp("2024-02-29T23:59:59Z", "capturedAtUtc").expect("leap day");
        assert_eq!(second - first, 1);
    }

    #[test]
    fn timestamp_rejects_invalid_calendar_bound_with_its_input_field() {
        assert_eq!(
            timestamp("2023-02-29T00:00:00Z", "authorizedAtUtc"),
            Err(ValidationError::Invalid {
                field: "authorizedAtUtc"
            })
        );
    }
}
