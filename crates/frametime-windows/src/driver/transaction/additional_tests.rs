use frametime_domain::driver::{
    CaptureFreshnessPolicy, DriverTransactionV2, DriverTransactionV2Stage,
    NVIDIA_TRANSACTION_V2_SCHEMA_VERSION, PackageRemovalDisposition, ProfileBackupDigestReference,
    SafeModeObservation, SafeModeState, Sha256Digest,
};

use super::{
    DriverTransaction,
    tests::{capture, installation, removal, transaction},
};
use crate::{CS2_SETTINGS, DrsApplicationOriginal, DrsBackup, DrsOriginalSetting};

fn v2_transaction() -> DriverTransaction {
    let mut transaction = transaction();
    let capture = capture(&transaction, "2026-01-01T00:00:02Z");
    transaction.authorization.package_set_sha256 = capture.package_set_sha256.clone();
    transaction.installation = Some(installation(&transaction, &capture));
    transaction.removal = Some(removal(capture.clone()));
    transaction.capture = Some(capture);
    transaction.schema_version = 2;
    transaction.lifecycle = Some(DriverTransactionV2 {
        schema_version: NVIDIA_TRANSACTION_V2_SCHEMA_VERSION,
        transaction_id: "nvidia-1".into(),
        stage: DriverTransactionV2Stage::ProfilesRestored,
        profile_backup: Some(ProfileBackupDigestReference {
            sha256: Sha256Digest::parse("f".repeat(64)).expect("digest"),
        }),
    });
    transaction.component_selection = Some(
        serde_json::from_value(serde_json::json!({
            "preset": "minimal",
            "selected": [],
            "protected": [],
            "sourceComponents": [],
            "requiredUnclassified": []
        }))
        .expect("component selection"),
    );
    transaction.prepared_package_sha256 =
        Some(Sha256Digest::parse("9".repeat(64)).expect("package digest"));
    transaction
}

fn cs2_backup() -> DrsBackup {
    DrsBackup {
        profile: "Counter-strike 2".into(),
        profile_created: false,
        settings: CS2_SETTINGS
            .iter()
            .map(|setting| DrsOriginalSetting {
                id: setting.id,
                value: None,
            })
            .collect(),
        applications: ["cs2.exe", "csgos2.exe"]
            .into_iter()
            .map(|application| DrsApplicationOriginal {
                application: application.into(),
                profile: Some("Counter-strike 2".into()),
            })
            .collect(),
    }
}

#[test]
fn historical_validation_does_not_turn_capture_age_into_corruption() {
    let mut transaction = transaction();
    let capture = capture(&transaction, "2026-01-01T00:00:02Z");
    transaction.authorization.package_set_sha256 = capture.package_set_sha256.clone();
    transaction.capture = Some(capture);
    let freshness = CaptureFreshnessPolicy {
        maximum_age_seconds: 60,
    };
    assert!(transaction.validate_persisted(freshness).is_ok());
    assert_eq!(
        transaction.validate("2026-01-02T00:00:03Z", freshness),
        Err("the execution capture is stale or from the future".into())
    );
}

#[test]
fn cs2_backup_round_trips_and_invalid_recovery_data_fails_closed() {
    let mut transaction = v2_transaction();
    transaction.cs2_backup = Some(cs2_backup());
    let encoded = serde_json::to_vec(&transaction).expect("serialize transaction");
    let decoded: DriverTransaction = serde_json::from_slice(&encoded).expect("decode transaction");
    assert_eq!(decoded.cs2_backup, transaction.cs2_backup);
    assert!(
        decoded
            .validate_persisted(CaptureFreshnessPolicy {
                maximum_age_seconds: 86_400,
            })
            .is_ok()
    );

    transaction
        .cs2_backup
        .as_mut()
        .expect("backup")
        .profile
        .clear();
    assert!(
        transaction
            .validate_persisted(CaptureFreshnessPolicy {
                maximum_age_seconds: 86_400,
            })
            .unwrap_err()
            .contains("backup is incomplete")
    );
}

#[test]
fn lifecycle_stage_rejects_missing_durable_evidence() {
    let mut transaction = v2_transaction();
    transaction.lifecycle.as_mut().expect("lifecycle").stage =
        DriverTransactionV2Stage::BaselineApplied;
    assert_eq!(
        transaction.validate_persisted(CaptureFreshnessPolicy {
            maximum_age_seconds: 86_400,
        }),
        Err("v2 driver transaction fields do not match its lifecycle stage".into())
    );

    transaction.lifecycle.as_mut().expect("lifecycle").stage =
        DriverTransactionV2Stage::CleanupComplete;
    transaction.installation = None;
    transaction.removal = None;
    assert_eq!(
        transaction.validate_persisted(CaptureFreshnessPolicy {
            maximum_age_seconds: 86_400,
        }),
        Err("v2 driver transaction fields do not match its lifecycle stage".into())
    );
}

#[test]
fn installation_accepts_a_derived_executable_only_with_bound_id_and_signer() {
    let mut transaction = transaction();
    let capture = capture(&transaction, "2026-01-01T00:00:02Z");
    transaction.authorization.package_set_sha256 = capture.package_set_sha256.clone();
    let mut installation = installation(&transaction, &capture);
    installation.installed_artifact.artifact.artifact_file_name = "prepared-setup.exe".into();
    installation.installed_artifact.artifact.payload_sha256 =
        Sha256Digest::parse("e".repeat(64)).expect("derived digest");
    transaction.capture = Some(capture);
    transaction.installation = Some(installation);
    assert!(
        transaction
            .validate(
                "2026-01-01T00:00:10Z",
                CaptureFreshnessPolicy {
                    maximum_age_seconds: 60,
                },
            )
            .is_ok()
    );

    transaction
        .installation
        .as_mut()
        .expect("installation")
        .installed_artifact
        .artifact
        .artifact_id = "different-artifact".into();
    assert!(
        transaction
            .validate(
                "2026-01-01T00:00:10Z",
                CaptureFreshnessPolicy {
                    maximum_age_seconds: 60,
                },
            )
            .is_err()
    );

    let installation = transaction.installation.as_mut().expect("installation");
    installation.installed_artifact.artifact.artifact_id = "nvidia-1".into();
    installation
        .installed_artifact
        .artifact
        .signer_thumbprint_sha256 = Sha256Digest::parse("f".repeat(64)).expect("different signer");
    assert!(
        transaction
            .validate(
                "2026-01-01T00:00:10Z",
                CaptureFreshnessPolicy {
                    maximum_age_seconds: 60,
                },
            )
            .is_err()
    );
}

#[test]
fn removal_retry_accepts_already_absent_only_with_absent_readback() {
    let mut transaction = transaction();
    let capture = capture(&transaction, "2026-01-01T00:00:02Z");
    let mut removal = removal(capture.clone());
    removal.outcomes[0].disposition = PackageRemovalDisposition::AlreadyAbsent;
    transaction.capture = Some(capture.clone());
    transaction.removal = Some(removal.clone());
    assert!(
        transaction
            .validate_persisted(CaptureFreshnessPolicy {
                maximum_age_seconds: 86_400,
            })
            .is_ok()
    );

    removal.post_removal_packages = capture.installed_packages;
    transaction.removal = Some(removal);
    assert!(
        transaction
            .validate_persisted(CaptureFreshnessPolicy {
                maximum_age_seconds: 86_400,
            })
            .is_err()
    );
}

#[test]
fn completed_legacy_v2_record_without_new_recovery_fields_remains_readable() {
    let mut transaction = v2_transaction();
    transaction.lifecycle.as_mut().expect("lifecycle").stage = DriverTransactionV2Stage::Verified;
    transaction.prepared_package_sha256 = None;
    transaction.cs2_backup = None;
    let mut encoded = serde_json::to_value(&transaction).expect("encode legacy transaction");
    let object = encoded.as_object_mut().expect("transaction object");
    object.remove("preparedPackageSha256");
    object.remove("cs2Backup");
    let decoded: DriverTransaction =
        serde_json::from_value(encoded).expect("decode legacy transaction");

    assert!(
        decoded
            .validate_persisted(CaptureFreshnessPolicy {
                maximum_age_seconds: 86_400,
            })
            .is_ok()
    );
}

#[test]
fn armed_removal_uses_fresh_safe_mode_after_original_capture_expires() {
    let transaction = transaction();
    let capture = capture(&transaction, "2026-01-01T00:00:02Z");
    let mut ordinary = removal(capture.clone());
    assert!(
        ordinary
            .validate_for_plan_at(
                &transaction.plan,
                CaptureFreshnessPolicy {
                    maximum_age_seconds: 900,
                },
                "2026-01-01T00:20:01Z",
            )
            .is_err()
    );

    let resumed_at = "2026-01-01T00:20:00Z";
    ordinary.resume_safe_mode = Some(SafeModeObservation {
        target_gpu: capture.target_gpu,
        state: SafeModeState::Confirmed,
        observed_at_utc: resumed_at.into(),
        boot_session_id: "resumed-safe-mode-boot".into(),
    });
    ordinary.observed_at_utc = resumed_at.into();
    for outcome in &mut ordinary.outcomes {
        outcome.observed_at_utc = resumed_at.into();
    }
    assert!(
        ordinary
            .validate_for_plan_at(
                &transaction.plan,
                CaptureFreshnessPolicy {
                    maximum_age_seconds: 900,
                },
                "2026-01-01T00:20:01Z",
            )
            .is_ok()
    );
}

#[test]
fn v2_recovery_install_can_complete_after_original_authorization_expires() {
    let mut transaction = v2_transaction();
    let capture = transaction.capture.clone().expect("capture");
    let artifact = transaction.artifact.clone();
    let installation = transaction.installation.as_mut().expect("installation");
    installation.fresh_authenticode.observed_at_utc = "2026-01-02T00:00:01Z".into();
    installation.installed_artifact.observed_at_utc = "2026-01-02T00:00:02Z".into();
    installation.observed_at_utc = "2026-01-02T00:00:03Z".into();
    assert!(
        installation
            .validate_for_capture(&capture, &artifact)
            .is_err()
    );
    assert!(
        transaction
            .validate_persisted(CaptureFreshnessPolicy {
                maximum_age_seconds: 86_400,
            })
            .is_ok()
    );
}
