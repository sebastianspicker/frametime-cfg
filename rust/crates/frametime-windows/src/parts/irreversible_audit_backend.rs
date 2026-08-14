use frametime_core::{
    AuditEntry as CoreAuditEntry, IrreversibleAudit, ManualRecoveryAudit,
    ManualRecoveryAuditOutcome, ManualRecoveryTarget, MixedRecoveryAudit, RecoveryRequirement,
};

impl LiveBackend {
    fn capture_irreversible_audit(
        &mut self,
        operation: Operation,
    ) -> Result<IrreversibleAudit, String> {
        let step = Self::key(operation);
        let requirement = self.recovery_requirement(operation);
        let target = irreversible_target(&step)?;
        if requirement == RecoveryRequirement::ManualRecoveryAudit {
            if self.transaction_lock.is_some() {
                return Err(format!(
                    "{step} manual audit capture found an unrelated active transaction"
                ));
            }
            self.transaction_lock = Some(WorkLock::acquire(&self.work_dir)?);
        } else if requirement != RecoveryRequirement::Mixed {
            return Err(format!("{step} does not declare irreversible recovery"));
        } else if self.transaction_lock.is_none() {
            return Err(format!(
                "{step} mixed audit capture requires the retained backup transaction lock"
            ));
        }

        let existing = load_audit_file(&self._trusted_work_dir)
            .and_then(|file| pending_irreversible_audit(&file, &step))
            .inspect_err(|_| {
                if requirement == RecoveryRequirement::ManualRecoveryAudit {
                    self.transaction_lock = None;
                }
            })?;
        if let Some(audit) = existing {
            if step == "P1:13" {
                let expected = self
                    .debloat_appx_subjects
                    .as_ref()
                    .ok_or("P1:13 retry requires exact captured AppX subjects")?;
                let IrreversibleAudit::Mixed(record) = &audit else {
                    return Err("P1:13 retry audit has the wrong recovery kind".into());
                };
                if record.manual_recovery_subjects != *expected {
                    return Err(
                        "P1:13 AppX inventory changed since the pending audit; refusing retry"
                            .into(),
                    );
                }
            }
            return Ok(audit);
        }
        match requirement {
            RecoveryRequirement::ManualRecoveryAudit => Ok(IrreversibleAudit::Manual(
                ManualRecoveryAudit::pending(step, timestamp(), target),
            )),
            RecoveryRequirement::Mixed if step == "P1:13" => {
                let subjects = self
                    .debloat_appx_subjects
                    .clone()
                    .ok_or("P1:13 audit requires exact captured AppX subjects")?;
                MixedRecoveryAudit::pending_with_appx_subjects(step, timestamp(), subjects)
                    .map(IrreversibleAudit::Mixed)
                    .map_err(str::to_owned)
            }
            RecoveryRequirement::Mixed => Ok(IrreversibleAudit::Mixed(
                MixedRecoveryAudit::pending(step, timestamp(), target),
            )),
            _ => Err("irreversible audit recovery requirement changed during capture".into()),
        }
    }

    fn persist_irreversible_audit(&self, audit: &IrreversibleAudit) -> Result<(), String> {
        if self.transaction_lock.is_none() {
            return Err("irreversible audit persistence requires the retained work lock".into());
        }
        let (step, requirement) = pending_audit_identity(audit)?;
        if !audit.is_valid_pending_for(requirement, step) {
            return Err(format!(
                "{step} irreversible audit is not exact and pending"
            ));
        }
        let mut file = load_audit_file(&self._trusted_work_dir)?;
        match pending_irreversible_audit(&file, step)? {
            Some(existing) if existing != *audit => {
                return Err(format!(
                    "{step} pending irreversible audit changed during retry"
                ));
            }
            Some(_) => {}
            None => file.entries.push(audit_entry(audit.clone())),
        }
        persist_audit_file(&self._trusted_work_dir, &file)
    }

    fn replace_irreversible_audit(
        &self,
        audit: &IrreversibleAudit,
        failed: bool,
    ) -> Result<(), String> {
        if self.transaction_lock.is_none() {
            return Err("irreversible audit update requires the retained work lock".into());
        }
        let (step, captured_at, outcome) = audit_update_identity(audit);
        let outcome_ok = matches!(outcome, ManualRecoveryAuditOutcome::Failed { .. }) == failed
            && !matches!(outcome, ManualRecoveryAuditOutcome::Pending);
        if !outcome_ok || captured_at.is_empty() {
            return Err(format!("{step} irreversible audit outcome is invalid"));
        }
        let mut file = load_audit_file(&self._trusted_work_dir)?;
        let mut matches = 0usize;
        for entry in &mut file.entries {
            if pending_entry_identity(entry)
                .is_some_and(|identity| identity.0 == step && identity.1 == captured_at)
            {
                *entry = audit_entry(audit.clone());
                matches += 1;
            }
        }
        if matches != 1 {
            return Err(format!(
                "{step} expected one pending irreversible audit, found {matches}"
            ));
        }
        persist_audit_file(&self._trusted_work_dir, &file)
    }
}

fn irreversible_target(step: &str) -> Result<ManualRecoveryTarget, String> {
    match step {
        "P1:13" => Ok(ManualRecoveryTarget::AppxRemovals),
        "P2:2" => Ok(ManualRecoveryTarget::ExactDriverPackageRemoval),
        "P3:1" => Ok(ManualRecoveryTarget::DriverInstallation),
        _ => Err(format!("{step} has no fixed irreversible recovery target")),
    }
}

fn pending_audit_identity(
    audit: &IrreversibleAudit,
) -> Result<(&str, RecoveryRequirement), String> {
    match audit {
        IrreversibleAudit::Manual(record) => {
            Ok((&record.step, RecoveryRequirement::ManualRecoveryAudit))
        }
        IrreversibleAudit::Mixed(record) => Ok((&record.step, RecoveryRequirement::Mixed)),
    }
}

fn audit_update_identity(audit: &IrreversibleAudit) -> (&str, &str, &ManualRecoveryAuditOutcome) {
    match audit {
        IrreversibleAudit::Manual(record) => (&record.step, &record.captured_at, &record.outcome),
        IrreversibleAudit::Mixed(record) => (&record.step, &record.captured_at, &record.outcome),
    }
}

fn audit_entry(audit: IrreversibleAudit) -> CoreAuditEntry {
    match audit {
        IrreversibleAudit::Manual(record) => CoreAuditEntry::Manual(record),
        IrreversibleAudit::Mixed(record) => CoreAuditEntry::Mixed(record),
    }
}

fn entry_irreversible_audit(entry: &CoreAuditEntry) -> Option<IrreversibleAudit> {
    match entry {
        CoreAuditEntry::Manual(record) => Some(IrreversibleAudit::Manual(record.clone())),
        CoreAuditEntry::Mixed(record) => Some(IrreversibleAudit::Mixed(record.clone())),
        CoreAuditEntry::Rebuildable(_) | CoreAuditEntry::Unknown(_) => None,
    }
}

fn pending_entry_identity(entry: &CoreAuditEntry) -> Option<(&str, &str)> {
    match entry {
        CoreAuditEntry::Manual(record)
            if matches!(record.outcome, ManualRecoveryAuditOutcome::Pending) =>
        {
            Some((&record.step, &record.captured_at))
        }
        CoreAuditEntry::Mixed(record)
            if matches!(record.outcome, ManualRecoveryAuditOutcome::Pending) =>
        {
            Some((&record.step, &record.captured_at))
        }
        _ => None,
    }
}

fn pending_irreversible_audit(
    file: &frametime_core::AuditFile,
    step: &str,
) -> Result<Option<IrreversibleAudit>, String> {
    if !file.unknown.is_empty() {
        return Err("audit document has unrecognized root fields".into());
    }
    let mut pending = file
        .entries
        .iter()
        .filter_map(entry_irreversible_audit)
        .filter(|audit| {
            let (candidate_step, _, outcome) = audit_update_identity(audit);
            candidate_step == step && matches!(outcome, ManualRecoveryAuditOutcome::Pending)
        })
        .collect::<Vec<_>>();
    match pending.len() {
        0 => Ok(None),
        1 => Ok(pending.pop()),
        count => Err(format!(
            "{step} audit contains {count} pending records; retry is ambiguous"
        )),
    }
}
