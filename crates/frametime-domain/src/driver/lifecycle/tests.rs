use std::collections::BTreeSet;

use super::*;

fn snapshot(value: u32) -> DrsSnapshot {
    DrsSnapshot {
        schema_version: NVIDIA_DRS_SNAPSHOT_SCHEMA_VERSION,
        profiles: vec![DrsProfileSnapshot {
            name: "CS2".into(),
            applications: vec![DrsApplicationSnapshot {
                executable: "cs2.exe".into(),
            }],
            settings: vec![DrsSettingSnapshot {
                setting_id: 1,
                value: DrsValue::Dword(value),
            }],
        }],
    }
}

#[test]
fn canonical_catalog_and_presets_are_deterministic() {
    let catalog = NvidiaComponentCatalog::canonical().unwrap();
    assert_eq!(catalog.components().count(), 35);
    for preset in NvidiaComponentPreset::ALL {
        let first = catalog.resolve_selection(preset, &[], &[], &[]).unwrap();
        let second = catalog.resolve_selection(preset, &[], &[], &[]).unwrap();
        assert_eq!(first, second);
        assert!(first.selected.contains("Display.Driver"));
    }
}

#[test]
fn repeated_selection_deselection_preserves_dependencies_and_required() {
    let catalog = NvidiaComponentCatalog::canonical().unwrap();
    let selection = catalog
        .resolve_selection(
            NvidiaComponentPreset::Minimal,
            &[],
            &["GFExperience".into(), "GFExperience".into()],
            &[
                "Display.Driver".into(),
                "NvContainer".into(),
                "GFExperience".into(),
            ],
        )
        .unwrap();
    assert!(selection.selected.contains("Display.Driver"));
    assert!(!selection.selected.contains("GFExperience"));
    assert!(!selection.selected.contains("NvContainer"));
    assert!(selection.protected.contains("Display.Driver"));
}

#[test]
fn selected_dependency_cannot_be_deselected() {
    let catalog = NvidiaComponentCatalog::canonical().unwrap();
    let selection = catalog
        .resolve_selection(
            NvidiaComponentPreset::Minimal,
            &[],
            &["GFExperience".into()],
            &["NvContainer".into()],
        )
        .unwrap();
    assert!(selection.selected.contains("GFExperience"));
    assert!(selection.selected.contains("NvContainer"));
    assert!(selection.protected.contains("NvContainer"));
}

#[test]
fn unknown_source_directories_are_required_unclassified() {
    let catalog = NvidiaComponentCatalog::canonical().unwrap();
    let selection = catalog
        .resolve_selection(
            NvidiaComponentPreset::Minimal,
            &["Display.Driver".into(), "Vendor.Future".into()],
            &[],
            &[],
        )
        .unwrap();
    assert_eq!(selection.required_unclassified.len(), 1);
    assert!(
        selection
            .required_unclassified
            .contains(&RequiredUnclassifiedComponent {
                directory: "Vendor.Future".into(),
            })
    );
}

#[test]
fn drs_bounds_round_trip_and_reconciliation_are_checked() {
    let original = snapshot(1);
    let encoded = original.to_json().unwrap();
    assert_eq!(DrsSnapshot::from_json(&encoded).unwrap(), original);
    let incoming = snapshot(2);
    let reconciliation = original.reconcile(&incoming);
    let setting = reconciliation
        .items
        .iter()
        .find(|item| item.key.kind == DrsItemKind::Setting)
        .unwrap()
        .key
        .clone();
    assert!(matches!(
        original.merge_accepting(&incoming, &BTreeSet::new()),
        Err(DriverLifecycleError::IncompatibleDrsItems(_))
    ));
    let merged = original
        .merge_accepting(&incoming, &[setting].into_iter().collect())
        .unwrap();
    assert_eq!(merged.merged, incoming);
}

#[test]
fn drs_property_style_string_and_binary_limits_hold() {
    for units in [0, 1, MAX_DRS_UTF16_UNITS, MAX_DRS_UTF16_UNITS + 1] {
        let mut value = snapshot(1);
        value.profiles[0].settings[0].value = DrsValue::String("x".repeat(units));
        assert_eq!(
            value.validate().is_ok(),
            units > 0 && units <= MAX_DRS_UTF16_UNITS
        );
    }
    for bytes in [0, MAX_DRS_BINARY_BYTES, MAX_DRS_BINARY_BYTES + 1] {
        let mut value = snapshot(1);
        value.profiles[0].settings[0].value = DrsValue::Binary(vec![0; bytes]);
        assert_eq!(value.validate().is_ok(), bytes <= MAX_DRS_BINARY_BYTES);
    }
}

#[test]
fn drs_collection_bounds_are_enforced() {
    let too_many_profiles = DrsSnapshot {
        schema_version: NVIDIA_DRS_SNAPSHOT_SCHEMA_VERSION,
        profiles: (0..=MAX_DRS_PROFILES)
            .map(|index| DrsProfileSnapshot {
                name: format!("p{index}"),
                applications: Vec::new(),
                settings: Vec::new(),
            })
            .collect(),
    };
    assert!(too_many_profiles.validate().is_err());

    let mut too_many_settings = snapshot(1);
    too_many_settings.profiles[0].settings = (0..=MAX_DRS_ITEMS_PER_PROFILE)
        .map(|setting_id| DrsSettingSnapshot {
            setting_id: u32::try_from(setting_id).expect("bounded test setting id"),
            value: DrsValue::Dword(0),
        })
        .collect();
    assert!(too_many_settings.validate().is_err());

    let too_many_records = DrsSnapshot {
        schema_version: NVIDIA_DRS_SNAPSHOT_SCHEMA_VERSION,
        profiles: (0..16)
            .map(|profile_index| DrsProfileSnapshot {
                name: format!("p{profile_index}"),
                applications: (0..MAX_DRS_ITEMS_PER_PROFILE)
                    .map(|application_index| DrsApplicationSnapshot {
                        executable: format!("app-{profile_index}-{application_index}"),
                    })
                    .collect(),
                settings: Vec::new(),
            })
            .collect(),
    };
    assert!(too_many_records.validate().is_err());
}

#[test]
fn transaction_requires_a_backup_before_capture_or_cleanup() {
    let prepared = DriverTransactionV2 {
        schema_version: NVIDIA_TRANSACTION_V2_SCHEMA_VERSION,
        transaction_id: "transaction-1".into(),
        stage: DriverTransactionV2Stage::InstallerAuthenticated,
        profile_backup: None,
    };
    assert!(
        prepared
            .transition(DriverTransactionV2Stage::ProfileBackupPersisted, None)
            .is_err()
    );
    let digest = ProfileBackupDigestReference {
        sha256: Sha256Digest::parse("a".repeat(64)).unwrap(),
    };
    let backed_up = prepared
        .transition(
            DriverTransactionV2Stage::ProfileBackupPersisted,
            Some(digest),
        )
        .unwrap();
    let captured = backed_up
        .transition(DriverTransactionV2Stage::StateCapturePersisted, None)
        .unwrap();
    assert!(
        captured
            .transition(DriverTransactionV2Stage::SafeModeHandoffArmed, None)
            .is_ok()
    );
}

#[test]
fn transaction_v1_migration_fails_closed_at_resume_boundary() {
    assert_eq!(
        migrate_v1_transaction(V1DriverTransactionState {
            capture_present: false,
            cleanup_present: false,
            installation_present: false,
            unknown_resume_state: false,
        }),
        V1TransactionMigrationDecision::StartAt(DriverTransactionV2Stage::InstallerAuthenticated)
    );
    assert_eq!(
        migrate_v1_transaction(V1DriverTransactionState {
            capture_present: true,
            cleanup_present: false,
            installation_present: false,
            unknown_resume_state: false,
        }),
        V1TransactionMigrationDecision::RefuseAmbiguousResume
    );
    assert_eq!(
        migrate_v1_transaction(V1DriverTransactionState {
            capture_present: true,
            cleanup_present: true,
            installation_present: false,
            unknown_resume_state: false,
        }),
        V1TransactionMigrationDecision::RefuseAmbiguousResume
    );
}
