mod irreversible_audit_tests {
    use super::*;
    use frametime_core::{
        AuditEntry, AuditFile, IrreversibleAudit, ManualRecoveryAudit, ManualRecoveryTarget,
        MixedRecoveryAudit,
    };

    fn file(entries: Vec<AuditEntry>) -> AuditFile {
        AuditFile {
            entries,
            created: "captured".into(),
            unknown: BTreeMap::new(),
        }
    }

    #[test]
    fn pending_irreversible_audit_is_typed_and_exact() {
        let expected = IrreversibleAudit::Mixed(MixedRecoveryAudit::pending(
            "P1:13",
            "captured",
            ManualRecoveryTarget::AppxRemovals,
        ));
        let loaded =
            pending_irreversible_audit(&file(vec![audit_entry(expected.clone())]), "P1:13")
                .unwrap();
        assert_eq!(loaded, Some(expected));
    }

    #[test]
    fn duplicate_pending_irreversible_audits_fail_closed() {
        let pending = IrreversibleAudit::Manual(ManualRecoveryAudit::pending(
            "P2:2",
            "captured",
            ManualRecoveryTarget::ExactDriverPackageRemoval,
        ));
        let error = pending_irreversible_audit(
            &file(vec![audit_entry(pending.clone()), audit_entry(pending)]),
            "P2:2",
        )
        .unwrap_err();
        assert!(error.contains("2 pending records"));
    }

    #[test]
    fn unknown_root_fields_block_irreversible_audit_authority() {
        let mut audit = file(Vec::new());
        audit.unknown.insert("futureAuthority".into(), true.into());
        assert!(pending_irreversible_audit(&audit, "P3:1").is_err());
    }
}
