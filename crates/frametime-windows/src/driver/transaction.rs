//! Fixed-root durable NVIDIA driver transaction state.  The record retains no
//! caller paths, URLs, signer values, or executable authority.

use std::path::Path;

use frametime_domain::driver::{
    ArtifactAcquisitionAuthorization, CaptureFreshnessPolicy, DriverExecutionCapture,
    DriverTransactionV2, DriverTransactionV2Stage, DryRunDriverPlan, InstallationEvidence,
    NVIDIA_TRANSACTION_V2_SCHEMA_VERSION, NvidiaComponentSelection, RemovalExecutionEvidence,
    Sha256Digest, SignedArtifactDescriptor,
};
#[cfg(windows)]
use frametime_domain::driver::{DrsSnapshot, ProfileBackupDigestReference};
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use sha2::{Digest, Sha256};

use crate::{DrsBackup, TrustedWorkDir, read_json_trusted};
#[cfg(windows)]
use crate::{WorkLock, write_json_atomic_trusted};

const DRIVER_TRANSACTION_FILE: &str = "driver-transaction.json";
#[cfg(windows)]
const DRIVER_PROFILE_BACKUP_FILE: &str = "driver-profile-backup.json";
const DRIVER_TRANSACTION_SCHEMA: u32 = 1;
const DRIVER_TRANSACTION_SCHEMA_V2: u32 = 2;

mod validation;
use validation::validate_v2_stage_fields;
#[cfg(windows)]
pub(crate) mod archive;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverLifecycleStatus {
    pub supported_host: bool,
    pub transaction: Option<DriverTransaction>,
    pub profile_backup_verified: bool,
    pub prepared_package_present: bool,
    pub catalog_components: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverTransaction {
    pub schema_version: u32,
    pub plan: DryRunDriverPlan,
    pub artifact: SignedArtifactDescriptor,
    pub authorization: ArtifactAcquisitionAuthorization,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<DriverExecutionCapture>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removal: Option<RemovalExecutionEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation: Option<InstallationEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<DriverTransactionV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_selection: Option<NvidiaComponentSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared_package_sha256: Option<Sha256Digest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profile_reconciliation: Vec<frametime_domain::driver::DrsItemKey>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_profile_omissions: Vec<frametime_domain::driver::DrsItemKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cs2_backup: Option<DrsBackup>,
}

impl DriverTransaction {
    pub fn prepared(
        plan: DryRunDriverPlan,
        artifact: SignedArtifactDescriptor,
        authorization: ArtifactAcquisitionAuthorization,
    ) -> Result<Self, String> {
        plan.validate().map_err(|error| error.to_string())?;
        artifact
            .validate_for(&plan.target_gpu)
            .map_err(|error| error.to_string())?;
        let plan_hash_matches = authorization.plan_sha256 == plan.input_sha256;
        let target_gpu_matches = authorization.target_gpu == plan.target_gpu;
        if !plan_hash_matches || !target_gpu_matches {
            return Err("driver authorization does not bind the prepared plan".into());
        }
        Ok(Self {
            schema_version: DRIVER_TRANSACTION_SCHEMA,
            plan,
            artifact,
            authorization,
            capture: None,
            removal: None,
            installation: None,
            lifecycle: None,
            component_selection: None,
            prepared_package_sha256: None,
            profile_reconciliation: Vec::new(),
            accepted_profile_omissions: Vec::new(),
            cs2_backup: None,
        })
    }

    pub fn prepared_v2(
        plan: DryRunDriverPlan,
        artifact: SignedArtifactDescriptor,
        authorization: ArtifactAcquisitionAuthorization,
        component_selection: NvidiaComponentSelection,
        prepared_package_sha256: Sha256Digest,
    ) -> Result<Self, String> {
        let transaction_id = authorization.authorization_id.clone();
        let mut transaction = Self::prepared(plan, artifact, authorization)?;
        transaction.schema_version = DRIVER_TRANSACTION_SCHEMA_V2;
        transaction.lifecycle = Some(DriverTransactionV2 {
            schema_version: NVIDIA_TRANSACTION_V2_SCHEMA_VERSION,
            transaction_id,
            stage: DriverTransactionV2Stage::InstallerAuthenticated,
            profile_backup: None,
        });
        transaction.component_selection = Some(component_selection);
        transaction.prepared_package_sha256 = Some(prepared_package_sha256);
        transaction.validate_structure()?;
        Ok(transaction)
    }

    fn validate_structure(&self) -> Result<(), String> {
        match self.schema_version {
            DRIVER_TRANSACTION_SCHEMA => self.validate_v1_structure(),
            DRIVER_TRANSACTION_SCHEMA_V2 => self.validate_v2_structure(),
            _ => Err("driver transaction schema is unsupported".into()),
        }
    }

    fn validate_v1_structure(&self) -> Result<(), String> {
        let contains_v2_fields = self.lifecycle.is_some()
            || self.component_selection.is_some()
            || self.prepared_package_sha256.is_some()
            || !self.profile_reconciliation.is_empty()
            || !self.accepted_profile_omissions.is_empty()
            || self.cs2_backup.is_some();
        if contains_v2_fields {
            Err("v1 driver transaction contains v2-only fields".into())
        } else {
            Ok(())
        }
    }

    fn validate_v2_structure(&self) -> Result<(), String> {
        self.lifecycle
            .as_ref()
            .ok_or("v2 driver transaction lacks lifecycle state")?
            .validate()
            .map_err(|error| format!("invalid v2 driver lifecycle: {error:?}"))?;
        self.component_selection
            .as_ref()
            .ok_or_else(|| String::from("v2 driver transaction lacks component selection"))?;
        if let Some(backup) = &self.cs2_backup {
            super::drs::validate_backup(backup).map_err(|error| error.to_string())?;
            if !matches!(
                self.lifecycle.as_ref().map(|lifecycle| lifecycle.stage),
                Some(
                    DriverTransactionV2Stage::ProfilesRestored
                        | DriverTransactionV2Stage::BaselineApplied
                        | DriverTransactionV2Stage::Verified
                        | DriverTransactionV2Stage::RecoveryRequired
                )
            ) {
                return Err("v2 driver transaction has a premature CS2 backup".into());
            }
        }
        validate_v2_stage_fields(self)
    }

    fn validate_persisted(&self, freshness: CaptureFreshnessPolicy) -> Result<(), String> {
        self.validate_structure()?;
        let expected = Self::prepared(
            self.plan.clone(),
            self.artifact.clone(),
            self.authorization.clone(),
        )?;
        if expected.plan != self.plan || expected.artifact != self.artifact {
            return Err("driver transaction preparation is incoherent".into());
        }
        if let Some(capture) = &self.capture {
            capture
                .validate_for_plan(&self.plan, freshness)
                .map_err(|error| error.to_string())?;
        }
        if let Some(removal) = &self.removal {
            removal
                .validate_for_plan(&self.plan, freshness)
                .map_err(|error| error.to_string())?;
            if self.capture.as_ref() != Some(&removal.capture) {
                return Err("driver removal is not bound to retained capture".into());
            }
        }
        if let Some(installation) = &self.installation {
            let capture = self
                .capture
                .as_ref()
                .ok_or("driver installation lacks retained capture")?;
            let validation = if self.schema_version == DRIVER_TRANSACTION_SCHEMA_V2 {
                installation.validate_for_recovery(capture, &self.artifact)
            } else {
                installation.validate_for_capture(capture, &self.artifact)
            };
            validation.map_err(|error| error.to_string())?;
            if installation.authorization != self.authorization {
                return Err("driver installation is not bound to retained authorization".into());
            }
        }
        Ok(())
    }

    pub fn validate(&self, now_utc: &str, freshness: CaptureFreshnessPolicy) -> Result<(), String> {
        self.validate_persisted(freshness)?;
        if let Some(capture) = &self.capture {
            freshness
                .validate_capture_at(&capture.captured_at_utc, now_utc)
                .map_err(|error| error.to_string())?;
        }
        if let (Some(installation), Some(capture)) = (&self.installation, &self.capture) {
            installation
                .validate_for_capture_at(capture, &self.artifact, now_utc)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    #[must_use]
    pub fn removal_complete(&self) -> bool {
        self.removal.is_some()
    }
}

#[cfg(windows)]
pub(crate) fn persist_profile_backup(
    work_dir: &Path,
    snapshot: &DrsSnapshot,
) -> Result<ProfileBackupDigestReference, String> {
    let bytes = snapshot
        .to_json()
        .map_err(|error| format!("validate NVIDIA DRS profile backup: {error:?}"))?;
    let digest = Sha256Digest::parse(format!("{:x}", Sha256::digest(&bytes)))
        .map_err(|error| error.to_string())?;
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let _lock = WorkLock::acquire(trusted.path())?;
    write_json_atomic_trusted(&trusted, DRIVER_PROFILE_BACKUP_FILE, snapshot)
        .map_err(|error| format!("persist NVIDIA profile backup: {error}"))?;
    let persisted: DrsSnapshot = read_json_trusted(&trusted, DRIVER_PROFILE_BACKUP_FILE)
        .map_err(|error| format!("read back NVIDIA profile backup: {error}"))?;
    let persisted_bytes = persisted
        .to_json()
        .map_err(|error| format!("validate persisted NVIDIA profile backup: {error:?}"))?;
    let persisted_digest = Sha256Digest::parse(format!("{:x}", Sha256::digest(&persisted_bytes)))
        .map_err(|error| error.to_string())?;
    if persisted != *snapshot || persisted_digest != digest {
        return Err("NVIDIA profile backup readback or digest verification failed".into());
    }
    Ok(ProfileBackupDigestReference { sha256: digest })
}

#[cfg(windows)]
pub(crate) fn load_profile_backup_at(
    work_dir: &Path,
    expected: &ProfileBackupDigestReference,
) -> Result<DrsSnapshot, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let snapshot: DrsSnapshot = read_json_trusted(&trusted, DRIVER_PROFILE_BACKUP_FILE)
        .map_err(|error| format!("read NVIDIA profile backup: {error}"))?;
    let bytes = snapshot
        .to_json()
        .map_err(|error| format!("validate NVIDIA profile backup: {error:?}"))?;
    let actual = Sha256Digest::parse(format!("{:x}", Sha256::digest(&bytes)))
        .map_err(|error| error.to_string())?;
    if actual != expected.sha256 {
        return Err("NVIDIA profile backup digest does not match the transaction".into());
    }
    Ok(snapshot)
}

#[cfg(windows)]
pub(crate) fn persist_driver_transaction(
    work_dir: &Path,
    transaction: &DriverTransaction,
) -> Result<DriverTransaction, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let _lock = WorkLock::acquire(trusted.path())?;
    transaction.validate_persisted(CaptureFreshnessPolicy {
        maximum_age_seconds: 86_400,
    })?;
    write_json_atomic_trusted(&trusted, DRIVER_TRANSACTION_FILE, transaction)
        .map_err(|error| format!("persist driver transaction: {error}"))?;
    let persisted: DriverTransaction = read_json_trusted(&trusted, DRIVER_TRANSACTION_FILE)
        .map_err(|error| format!("read back driver transaction: {error}"))?;
    persisted.validate_persisted(CaptureFreshnessPolicy {
        maximum_age_seconds: 86_400,
    })?;
    if &persisted != transaction {
        return Err("driver transaction readback verification failed".into());
    }
    Ok(persisted)
}

pub(crate) fn load_driver_transaction_at(
    work_dir: &Path,
) -> Result<Option<DriverTransaction>, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    if !trusted.path().join(DRIVER_TRANSACTION_FILE).exists() {
        return Ok(None);
    }
    let transaction: DriverTransaction = read_json_trusted(&trusted, DRIVER_TRANSACTION_FILE)
        .map_err(|error| format!("read driver transaction: {error}"))?;
    transaction.validate_persisted(CaptureFreshnessPolicy {
        maximum_age_seconds: 86_400,
    })?;
    Ok(Some(transaction))
}

pub fn inspect_driver_status() -> Result<DriverLifecycleStatus, String> {
    let catalog_components = frametime_domain::driver::NvidiaComponentCatalog::canonical()
        .map_err(|error| format!("load NVIDIA component catalog: {error:?}"))?
        .components()
        .count();
    #[cfg(windows)]
    {
        if !Path::new(crate::WINDOWS_WORK_DIR).exists() {
            return Ok(DriverLifecycleStatus {
                supported_host: true,
                transaction: None,
                profile_backup_verified: false,
                prepared_package_present: false,
                catalog_components,
            });
        }
        let trusted = TrustedWorkDir::acquire_fixed()?;
        let transaction = load_driver_transaction_at(trusted.path())?;
        let profile_backup_verified = transaction
            .as_ref()
            .and_then(|value| value.lifecycle.as_ref())
            .and_then(|value| value.profile_backup.as_ref())
            .is_some_and(|reference| load_profile_backup_at(trusted.path(), reference).is_ok());
        Ok(DriverLifecycleStatus {
            supported_host: true,
            transaction,
            profile_backup_verified,
            prepared_package_present: trusted.path().join("driver-package").is_dir(),
            catalog_components,
        })
    }
    #[cfg(not(windows))]
    Ok(DriverLifecycleStatus {
        supported_host: false,
        transaction: None,
        profile_backup_verified: false,
        prepared_package_present: false,
        catalog_components,
    })
}

#[cfg(test)]
pub(super) mod tests {
    use frametime_domain::driver::{
        ArtifactIdentity, CanonicalPackageSet, DriverExecutionCapture, InstallationEvidence,
        InstalledArtifactObservation, PackageRemovalDisposition, PackageRemovalOutcome,
        PlannedDriverAction, RemovalExecutionEvidence, SafeModeObservation, SafeModeState,
        Sha256Digest,
    };

    use super::{CaptureFreshnessPolicy, DriverTransaction};

    const NOW: &str = "2026-01-01T00:00:10Z";
    const CAPTURED: &str = "2026-01-01T00:00:02Z";

    pub(crate) fn transaction() -> DriverTransaction {
        serde_json::from_str(
            r#"{
                "schemaVersion": 1,
                "plan": {
                    "schemaVersion": 1,
                    "readOnly": true,
                    "targetGpu": {
                        "vendor": "nvidia",
                        "pciVendorId": 4318,
                        "pciDeviceId": 9348,
                        "subsystemVendorId": 4318,
                        "subsystemDeviceId": 1,
                        "revisionId": 1
                    },
                    "inputSha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                    "entries": [
                        {"step": "P1:18", "action": {"kind": "inspectExactGpu", "target_gpu": {"vendor": "nvidia", "pciVendorId": 4318, "pciDeviceId": 9348, "subsystemVendorId": 4318, "subsystemDeviceId": 1, "revisionId": 1}}},
                        {"step": "P1:19", "action": {"kind": "recordExactPackages", "packages": [{"targetGpu": {"vendor": "nvidia", "pciVendorId": 4318, "pciDeviceId": 9348, "subsystemVendorId": 4318, "subsystemDeviceId": 1, "revisionId": 1}, "publishedName": "oem1.inf", "originalInfName": "nv_dispi.inf", "providerName": "NVIDIA", "driverVersion": "1.0"}]}},
                        {"step": "P2:2", "action": {"kind": "proposePackageCleanup", "published_names": ["oem1.inf"]}},
                        {"step": "P3:1", "action": {"kind": "proposeSignedArtifactInstall", "artifact": {"locator": {"artifactId": "nvidia-1", "artifactFileName": "setup.exe"}, "targetGpu": {"vendor": "nvidia", "pciVendorId": 4318, "pciDeviceId": 9348, "subsystemVendorId": 4318, "subsystemDeviceId": 1, "revisionId": 1}, "payloadSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "authenticode": {"status": "valid", "signerSubject": "NVIDIA", "signerThumbprintSha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "observedAtUtc": "2026-01-01T00:00:00Z"}}}}
                    ]
                },
                "artifact": {
                    "locator": {"artifactId": "nvidia-1", "artifactFileName": "setup.exe"},
                    "targetGpu": {"vendor": "nvidia", "pciVendorId": 4318, "pciDeviceId": 9348, "subsystemVendorId": 4318, "subsystemDeviceId": 1, "revisionId": 1},
                    "payloadSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "authenticode": {"status": "valid", "signerSubject": "NVIDIA", "signerThumbprintSha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "observedAtUtc": "2026-01-01T00:00:00Z"}
                },
                "authorization": {
                    "schemaVersion": 1,
                    "authorizationId": "nvidia-1",
                    "planSha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                    "targetGpu": {"vendor": "nvidia", "pciVendorId": 4318, "pciDeviceId": 9348, "subsystemVendorId": 4318, "subsystemDeviceId": 1, "revisionId": 1},
                    "packageSetSha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                    "artifact": {"artifactId": "nvidia-1", "artifactFileName": "setup.exe", "payloadSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "signerThumbprintSha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
                    "authorizedAtUtc": "2026-01-01T00:00:00Z",
                    "expiresAtUtc": "2026-01-01T01:00:00Z"
                }
            }"#,
        )
        .expect("valid driver transaction fixture")
    }

    pub(super) fn capture(
        transaction: &DriverTransaction,
        captured_at_utc: &str,
    ) -> DriverExecutionCapture {
        let packages = match &transaction.plan.entries[1].action {
            PlannedDriverAction::RecordExactPackages { packages } => packages.clone(),
            _ => panic!("fixture records packages"),
        };
        let installed_packages =
            CanonicalPackageSet::from_unsorted(transaction.plan.target_gpu.clone(), packages)
                .expect("canonical fixture package set");
        DriverExecutionCapture {
            schema_version: 1,
            plan_sha256: transaction.plan.input_sha256.clone(),
            target_gpu: transaction.plan.target_gpu.clone(),
            safe_mode: SafeModeObservation {
                target_gpu: transaction.plan.target_gpu.clone(),
                state: SafeModeState::Confirmed,
                observed_at_utc: "2026-01-01T00:00:01Z".into(),
                boot_session_id: "safe-mode-boot".into(),
            },
            package_set_sha256: installed_packages
                .fingerprint()
                .expect("package fingerprint"),
            installed_packages,
            captured_at_utc: captured_at_utc.into(),
        }
    }

    pub(super) fn removal(capture: DriverExecutionCapture) -> RemovalExecutionEvidence {
        let observed_at_utc = capture.captured_at_utc.clone();
        RemovalExecutionEvidence {
            resume_safe_mode: None,
            outcomes: capture
                .installed_packages
                .packages
                .iter()
                .map(|package| PackageRemovalOutcome {
                    published_name: package.published_name.clone(),
                    disposition: PackageRemovalDisposition::Removed,
                    observed_at_utc: observed_at_utc.clone(),
                })
                .collect(),
            post_removal_packages: CanonicalPackageSet::from_unsorted(
                capture.target_gpu.clone(),
                Vec::new(),
            )
            .expect("empty post-removal package set"),
            observed_at_utc,
            capture,
        }
    }

    pub(super) fn installation(
        transaction: &DriverTransaction,
        capture: &DriverExecutionCapture,
    ) -> InstallationEvidence {
        InstallationEvidence {
            authorization: transaction.authorization.clone(),
            fresh_authenticode: transaction.artifact.authenticode.clone(),
            installed_artifact: InstalledArtifactObservation {
                artifact: ArtifactIdentity::from_descriptor(&transaction.artifact)
                    .expect("fixture artifact identity"),
                observed_at_utc: "2026-01-01T00:00:04Z".into(),
            },
            post_install_packages: capture.installed_packages.clone(),
            observed_at_utc: "2026-01-01T00:00:05Z".into(),
        }
    }

    fn validate(transaction: &DriverTransaction) -> Result<(), String> {
        transaction.validate(
            NOW,
            CaptureFreshnessPolicy {
                maximum_age_seconds: 60,
            },
        )
    }

    #[test]
    fn v1_transaction_rejects_v2_only_fields() {
        let mut transaction = transaction();
        transaction.component_selection = Some(
            serde_json::from_value(serde_json::json!({
                "preset": "minimal",
                "selected": [],
                "protected": [],
                "sourceComponents": [],
                "requiredUnclassified": []
            }))
            .expect("fixture component selection"),
        );

        assert_eq!(
            validate(&transaction),
            Err("v1 driver transaction contains v2-only fields".into())
        );
    }

    #[test]
    fn v2_transaction_requires_lifecycle_and_component_selection() {
        let mut transaction = transaction();
        transaction.schema_version = 2;
        assert_eq!(
            validate(&transaction),
            Err("v2 driver transaction lacks lifecycle state".into())
        );

        transaction.lifecycle = Some(
            serde_json::from_value(serde_json::json!({
                "schemaVersion": 2,
                "transactionId": "nvidia-1",
                "stage": "installerAuthenticated"
            }))
            .expect("fixture lifecycle"),
        );
        assert_eq!(
            validate(&transaction),
            Err("v2 driver transaction lacks component selection".into())
        );
    }

    #[test]
    fn prepared_transaction_rejects_plan_and_gpu_authorization_mismatches() {
        let transaction = transaction();
        let mut authorization = transaction.authorization.clone();
        authorization.plan_sha256 = Sha256Digest::parse("e".repeat(64)).expect("digest");
        assert_eq!(
            DriverTransaction::prepared(
                transaction.plan.clone(),
                transaction.artifact.clone(),
                authorization,
            ),
            Err("driver authorization does not bind the prepared plan".into())
        );

        let mut authorization = transaction.authorization.clone();
        authorization.target_gpu.pci_device_id = 0x2685;
        assert_eq!(
            DriverTransaction::prepared(transaction.plan, transaction.artifact, authorization),
            Err("driver authorization does not bind the prepared plan".into())
        );
    }

    #[test]
    fn removal_requires_the_retained_capture() {
        let mut transaction = transaction();
        transaction.capture = Some(capture(&transaction, CAPTURED));
        transaction.removal = Some(removal(capture(&transaction, "2026-01-01T00:00:03Z")));

        assert_eq!(
            validate(&transaction),
            Err("driver removal is not bound to retained capture".into())
        );
    }

    #[test]
    fn installation_requires_capture_and_retained_authorization() {
        let mut transaction = transaction();
        let capture = capture(&transaction, CAPTURED);
        transaction.authorization.package_set_sha256 = capture.package_set_sha256.clone();
        transaction.installation = Some(installation(&transaction, &capture));
        assert_eq!(
            validate(&transaction),
            Err("driver installation lacks retained capture".into())
        );

        transaction.capture = Some(capture);
        transaction
            .installation
            .as_mut()
            .expect("fixture installation")
            .authorization
            .authorization_id = "different-authorization".into();
        assert_eq!(
            validate(&transaction),
            Err("driver installation is not bound to retained authorization".into())
        );
    }
}

#[cfg(test)]
#[path = "transaction/additional_tests.rs"]
mod additional_tests;
