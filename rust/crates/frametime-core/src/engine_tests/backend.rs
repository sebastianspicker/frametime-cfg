use super::*;
use crate::audit::{
    IrreversibleAudit, ManualRecoveryAudit, ManualRecoveryAuditOutcome, ManualRecoveryTarget,
    MixedRecoveryAudit,
};

pub(super) fn irreversible_target(step: &str) -> ManualRecoveryTarget {
    match step {
        "P1:13" => crate::P1_13_MANUAL_RECOVERY_TARGET,
        "P2:2" => crate::P2_2_MANUAL_RECOVERY_TARGET,
        "P3:1" => crate::P3_1_MANUAL_RECOVERY_TARGET,
        _ => panic!("unexpected irreversible step {step}"),
    }
}

impl Backend for Mock {
    fn is_dry_run(&self) -> bool {
        self.dry
    }

    fn inspect(&mut self, _: Operation) -> Result<Inspection, String> {
        self.call("inspect")?;
        Ok(self.inspection.unwrap_or(Inspection::NeedsApply))
    }

    fn plan(&mut self, _: Operation) -> Result<Vec<String>, String> {
        self.call("plan")?;
        Ok(vec!["Would apply".into()])
    }

    fn capture_backups(&mut self, _: Operation) -> Result<Vec<BackupEntry>, String> {
        self.call("backup")?;
        Ok(vec![BackupEntry::Unknown(serde_json::json!({
            "type": "test",
            "step": "P1:3"
        }))])
    }

    fn persist_backups(&mut self, entries: &[BackupEntry]) -> Result<(), String> {
        assert_eq!(entries.len(), 1);
        self.call("persist")
    }

    fn recovery_requirement(&self, _: Operation) -> RecoveryRequirement {
        if self.rebuildable {
            RecoveryRequirement::RebuildableAudit
        } else if self.manual {
            RecoveryRequirement::ManualRecoveryAudit
        } else if self.mixed {
            RecoveryRequirement::Mixed
        } else {
            RecoveryRequirement::LosslessBackup
        }
    }

    fn capture_pending_audit(&mut self, _: Operation) -> Result<RebuildableAudit, String> {
        self.call("audit_capture")?;
        RebuildableAudit::pending(
            self.audit_step.unwrap_or("P1:3"),
            "captured",
            self.audit_targets
                .clone()
                .unwrap_or_else(|| crate::audit::P1_3_REBUILDABLE_TARGETS.to_vec()),
        )
        .map_err(str::to_owned)
    }

    fn persist_pending_audit(&mut self, audit: &RebuildableAudit) -> Result<(), String> {
        assert!(audit.is_pending());
        self.call("audit_persist")
    }

    fn finalize_audit(&mut self, audit: &RebuildableAudit) -> Result<(), String> {
        assert!(matches!(
            audit.outcome,
            crate::audit::RebuildableAuditOutcome::Verified { .. }
        ));
        self.call("audit_finalize")
    }

    fn capture_pending_irreversible_audit(
        &mut self,
        operation: Operation,
    ) -> Result<IrreversibleAudit, String> {
        self.call("irreversible_audit_capture")?;
        let step = Progress::key(operation.step.phase as u8, operation.step.number);
        let target = irreversible_target(&step);
        Ok(if self.mixed {
            IrreversibleAudit::Mixed(MixedRecoveryAudit::pending(step, "captured", target))
        } else {
            IrreversibleAudit::Manual(ManualRecoveryAudit::pending(step, "captured", target))
        })
    }

    fn persist_pending_irreversible_audit(
        &mut self,
        audit: &IrreversibleAudit,
    ) -> Result<(), String> {
        assert!(matches!(
            audit,
            IrreversibleAudit::Manual(ManualRecoveryAudit {
                outcome: ManualRecoveryAuditOutcome::Pending,
                ..
            }) | IrreversibleAudit::Mixed(MixedRecoveryAudit {
                outcome: ManualRecoveryAuditOutcome::Pending,
                ..
            })
        ));
        self.call("irreversible_audit_persist")
    }

    fn finalize_irreversible_audit(&mut self, audit: &IrreversibleAudit) -> Result<(), String> {
        assert!(matches!(
            audit,
            IrreversibleAudit::Manual(ManualRecoveryAudit {
                outcome: ManualRecoveryAuditOutcome::Verified { .. },
                ..
            }) | IrreversibleAudit::Mixed(MixedRecoveryAudit {
                outcome: ManualRecoveryAuditOutcome::Verified { .. },
                ..
            })
        ));
        self.call("irreversible_audit_finalize")
    }

    fn fail_irreversible_audit(&mut self, audit: &IrreversibleAudit) -> Result<(), String> {
        assert!(matches!(
            audit,
            IrreversibleAudit::Manual(ManualRecoveryAudit {
                outcome: ManualRecoveryAuditOutcome::Failed { .. },
                ..
            }) | IrreversibleAudit::Mixed(MixedRecoveryAudit {
                outcome: ManualRecoveryAuditOutcome::Failed { .. },
                ..
            })
        ));
        self.call("irreversible_audit_fail")
    }

    fn evidence_requirement(&self, _: Operation) -> EvidenceRequirement {
        if self.evidence {
            EvidenceRequirement::DurableReceipt
        } else {
            EvidenceRequirement::None
        }
    }

    fn capture_evidence(&mut self, operation: Operation) -> Result<ObservationReceipt, String> {
        self.call("evidence_capture")?;
        if operation.step.phase as u8 != 1 || operation.step.number != 21 {
            return Err("test evidence is bound only to P1:21".into());
        }
        ObservationReceipt::new(
            "2026-08-13T12:00:00Z",
            None,
            None,
            ObservationSubject::MsiDeviceSet {
                devices: vec![test_device_binding()],
            },
        )
        .map_err(|error| error.to_string())
    }

    fn persist_evidence(&mut self, receipt: &ObservationReceipt) -> Result<(), String> {
        assert_eq!(receipt.step, "P1:21");
        self.call("evidence_persist")
    }

    fn verify_evidence(
        &mut self,
        operation: Operation,
        receipt: &ObservationReceipt,
    ) -> Result<(), String> {
        assert_eq!(operation.step.number, 21);
        receipt
            .validate_for("P1:21")
            .map_err(|error| error.to_string())?;
        self.call("evidence_verify")
    }

    fn apply(&mut self, _: Operation) -> Result<(), String> {
        self.call("apply")?;
        if self.fail_apply {
            Err("injected apply failure".into())
        } else {
            Ok(())
        }
    }

    fn verify(&mut self, _: Operation) -> Result<(), String> {
        self.call("verify")
    }

    fn persist_progress(&mut self, _: &Progress) -> Result<(), String> {
        self.call("progress")
    }

    fn timestamp(&self) -> String {
        "verified".into()
    }
}
