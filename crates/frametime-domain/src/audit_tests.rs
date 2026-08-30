use super::*;

#[test]
fn audit_round_trip_retains_unknown_document_and_record_fields() {
    let raw = serde_json::json!({
        "created": "now",
        "futureDocument": {"retained": true},
        "entries": [{
            "type": "rebuildable_mutation",
            "step": "P1:3",
            "capturedAt": "then",
            "targets": ["cs2_shader_cache"],
            "state": "pending",
            "futureRecord": [1, 2]
        }, {
            "type": "manual_recovery_mutation",
            "step": "P1:13",
            "capturedAt": "then",
            "target": "appx_removals",
            "state": "failed",
            "failedAt": "later",
            "futureManual": true
        }, {
            "type": "mixed_recovery_mutation",
            "step": "P3:1",
            "capturedAt": "then",
            "target": "driver_installation",
            "state": "pending",
            "futureMixed": {"retained": true}
        }, {"type": "future", "payload": true}]
    });
    let file: AuditFile = serde_json::from_value(raw.clone()).expect("audit");
    assert_eq!(file.unknown["futureDocument"]["retained"], true);
    let AuditEntry::Rebuildable(record) = &file.entries[0] else {
        panic!("expected rebuildable audit");
    };
    assert!(record.is_pending());
    assert_eq!(record.unknown["futureRecord"], serde_json::json!([1, 2]));
    let AuditEntry::Manual(record) = &file.entries[1] else {
        panic!("expected manual audit");
    };
    assert_eq!(record.unknown["futureManual"], true);
    let AuditEntry::Mixed(record) = &file.entries[2] else {
        panic!("expected mixed audit");
    };
    assert_eq!(record.unknown["futureMixed"]["retained"], true);
    assert!(matches!(file.entries[3], AuditEntry::Unknown(_)));
    assert_eq!(serde_json::to_value(file).expect("encoded"), raw);
}

#[test]
fn finalized_record_keeps_the_fixed_target_and_changes_only_outcome() {
    let pending = RebuildableAudit::pending("P1:3", "captured", P1_3_REBUILDABLE_TARGETS)
        .expect("pending audit");
    let finalized = pending.finalized("verified");
    assert_eq!(finalized.targets, P1_3_REBUILDABLE_TARGETS);
    assert!(matches!(
        finalized.outcome,
        RebuildableAuditOutcome::Verified { ref finalized_at } if finalized_at == "verified"
    ));
}

#[test]
fn p1_3_requires_the_complete_fixed_target_set_without_duplicates() {
    let complete = RebuildableAudit::pending(
        "P1:3",
        "captured",
        [
            RebuildableTarget::NvidiaGlCache,
            RebuildableTarget::Cs2ShaderCache,
            RebuildableTarget::NvidiaDxCache,
            RebuildableTarget::DirectxD3dCache,
            RebuildableTarget::NvidiaDxCache,
        ],
    )
    .expect("pending audit");
    assert_eq!(complete.targets, P1_3_REBUILDABLE_TARGETS);
    assert!(complete.is_valid_pending_for("P1:3"));

    let incomplete =
        RebuildableAudit::pending("P1:3", "captured", [RebuildableTarget::Cs2ShaderCache])
            .expect("typed but incomplete audit");
    assert!(!incomplete.is_valid_pending_for("P1:3"));
    assert!(RebuildableAudit::pending("P1:3", "captured", []).is_err());
}

#[test]
fn manual_and_mixed_audits_bind_only_to_fixed_catalog_subjects() {
    for (step, target) in [
        ("P1:13", P1_13_MANUAL_RECOVERY_TARGET),
        ("P2:2", P2_2_MANUAL_RECOVERY_TARGET),
        ("P3:1", P3_1_MANUAL_RECOVERY_TARGET),
    ] {
        let manual =
            IrreversibleAudit::Manual(ManualRecoveryAudit::pending(step, "captured", target));
        assert!(manual.is_valid_pending_for(RecoveryRequirement::ManualRecoveryAudit, step));
        assert!(!manual.is_valid_pending_for(RecoveryRequirement::Mixed, step));
        let mixed = IrreversibleAudit::Mixed(MixedRecoveryAudit::pending(step, "captured", target));
        assert!(mixed.is_valid_pending_for(RecoveryRequirement::Mixed, step));
        assert!(!mixed.is_valid_pending_for(RecoveryRequirement::ManualRecoveryAudit, step));
    }

    let wrong = IrreversibleAudit::Manual(ManualRecoveryAudit::pending(
        "P1:13",
        "captured",
        P3_1_MANUAL_RECOVERY_TARGET,
    ));
    assert!(!wrong.is_valid_pending_for(RecoveryRequirement::ManualRecoveryAudit, "P1:13"));
}

#[test]
fn irreversible_audits_retain_pending_verified_and_failed_states() {
    let pending = IrreversibleAudit::Mixed(MixedRecoveryAudit::pending(
        "P2:2",
        "captured",
        P2_2_MANUAL_RECOVERY_TARGET,
    ));
    let verified = pending.finalized("verified");
    let failed = pending.failed("failed");
    assert!(matches!(
        verified,
        IrreversibleAudit::Mixed(MixedRecoveryAudit {
            outcome: ManualRecoveryAuditOutcome::Verified { .. },
            ..
        })
    ));
    assert!(matches!(
        failed,
        IrreversibleAudit::Mixed(MixedRecoveryAudit {
            outcome: ManualRecoveryAuditOutcome::Failed { .. },
            ..
        })
    ));
}

#[test]
fn p1_13_appx_subjects_are_typed_canonical_and_duplicate_free() {
    let audit = MixedRecoveryAudit::pending_with_appx_subjects(
        "P1:13",
        "captured",
        [
            AppxRemovalSubject::Provisioned {
                package_name: "Microsoft.BingNews_1.0_neutral_~_8wekyb3d8bbwe".into(),
            },
            AppxRemovalSubject::Installed {
                full_name: "Microsoft.BingNews_1.0_x64__8wekyb3d8bbwe".into(),
            },
        ],
    )
    .expect("exact subjects");
    assert_eq!(audit.manual_recovery_subjects.len(), 2);
    assert_eq!(
        serde_json::to_value(audit).expect("serialize")["manualRecoverySubjects"]
            .as_array()
            .expect("subjects")
            .len(),
        2
    );
    assert!(
        MixedRecoveryAudit::pending_with_appx_subjects(
            "P1:13",
            "captured",
            [
                AppxRemovalSubject::Installed {
                    full_name: "same".into()
                },
                AppxRemovalSubject::Installed {
                    full_name: "same".into()
                },
            ],
        )
        .is_err()
    );
    assert!(MixedRecoveryAudit::pending_with_appx_subjects("P2:2", "captured", [],).is_err());
}
