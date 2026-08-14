use frametime_core::audit::{AuditEntry, AuditFile, RebuildableAuditOutcome};

fn is_shader_cache_operation(operation: Operation) -> bool {
    operation.step.phase as u8 == 1 && operation.step.number == 3
}

impl LiveBackend {
    fn inspect_shader_cache_with_audit(&mut self) -> Result<Inspection, String> {
        let inventory = shader_cache_inventory(self.config.as_ref())?;
        let file = load_audit_file(&self._trusted_work_dir)?;
        let pending = p1_3_pending_audit(&file)?;
        inspection_from_shader_cache_audit(&inventory, pending.as_ref())
    }

    fn capture_shader_cache_audit(&mut self, operation: Operation) -> Result<RebuildableAudit, String> {
        if !is_shader_cache_operation(operation) {
            return Err("rebuildable audit was requested for a non-P1:3 operation".into());
        }
        if self.transaction_lock.is_some() {
            return Err("a previous live transaction is still active".into());
        }
        self.transaction_lock = Some(WorkLock::acquire(&self.work_dir)?);
        let pending = load_audit_file(&self._trusted_work_dir)
            .and_then(|file| p1_3_pending_audit(&file))
            .inspect_err(|_| self.transaction_lock = None)?;
        let inventory = shader_cache_inventory(self.config.as_ref()).inspect_err(|_| {
            self.transaction_lock = None;
        })?;
        if inventory.is_empty() && pending.is_none() {
            self.transaction_lock = None;
            return Err("P1:3 audit capture requires a non-empty inspected cache tree".into());
        }
        let audit = match pending {
            Some(audit) => audit,
            None => RebuildableAudit::pending(
                "P1:3",
                timestamp(),
                frametime_core::P1_3_REBUILDABLE_TARGETS,
            )
            .map_err(str::to_owned)?,
        };
        self.shader_cache_inventory = Some(inventory);
        Ok(audit)
    }

    fn persist_shader_cache_audit(&self, audit: &RebuildableAudit) -> Result<(), String> {
        if self.transaction_lock.is_none() {
            return Err("P1:3 audit persistence requires the retained work lock".into());
        }
        if !audit.is_valid_pending_for("P1:3") {
            return Err("P1:3 audit persistence requires the exact pending four-target record".into());
        }
        let mut file = load_audit_file(&self._trusted_work_dir)?;
        match p1_3_pending_audit(&file)? {
            Some(existing) if existing != *audit => {
                return Err("P1:3 pending audit changed during retry; refusing mutation".into());
            }
            Some(_) => {}
            None => file.entries.push(AuditEntry::Rebuildable(audit.clone())),
        }
        persist_audit_file(&self._trusted_work_dir, &file)
    }

    fn finalize_shader_cache_audit(&self, audit: &RebuildableAudit) -> Result<(), String> {
        if self.transaction_lock.is_none() {
            return Err("P1:3 audit finalization requires the retained work lock".into());
        }
        let RebuildableAuditOutcome::Verified { .. } = audit.outcome else {
            return Err("P1:3 audit finalization requires a verified outcome".into());
        };
        finalize_shader_cache_audit_file(&self._trusted_work_dir, audit)
    }
}

fn p1_3_pending_audit(file: &AuditFile) -> Result<Option<RebuildableAudit>, String> {
    let mut pending = Vec::new();
    for entry in &file.entries {
        let AuditEntry::Rebuildable(record) = entry else {
            continue;
        };
        if record.step != "P1:3" || !record.is_pending() {
            continue;
        }
        if !record.is_valid_pending_for("P1:3") || !record.unknown.is_empty() {
            return Err("P1:3 pending audit record is malformed or has unrecognized fields".into());
        }
        pending.push(record.clone());
    }
    match pending.len() {
        0 => Ok(None),
        1 => Ok(pending.pop()),
        _ => Err("P1:3 audit contains multiple pending records; refusing ambiguous retry".into()),
    }
}

fn finalize_shader_cache_audit_file(
    trusted: &TrustedWorkDir,
    audit: &RebuildableAudit,
) -> Result<(), String> {
    if !matches!(audit.outcome, RebuildableAuditOutcome::Verified { .. }) {
        return Err("P1:3 audit finalization requires a verified outcome".into());
    }
    let mut file = load_audit_file(trusted)?;
    let mut replaced = false;
    for entry in &mut file.entries {
        let AuditEntry::Rebuildable(existing) = entry else {
            continue;
        };
        if existing.step == "P1:3"
            && existing.captured_at == audit.captured_at
            && existing.targets == audit.targets
            && existing.is_pending()
        {
            *existing = audit.clone();
            replaced = true;
            break;
        }
    }
    if !replaced {
        return Err("P1:3 pending audit record is missing; refusing to finalize progress".into());
    }
    dedupe_exact_p1_3_audits(&mut file.entries);
    persist_audit_file(trusted, &file)
}

fn load_audit_file(trusted: &TrustedWorkDir) -> Result<AuditFile, String> {
    let path = trusted.path().join(AUDIT_FILE);
    if path.exists() {
        read_json_trusted(trusted, AUDIT_FILE).map_err(|error| format!("read P1:3 audit: {error}"))
    } else {
        Ok(AuditFile {
            entries: Vec::new(),
            created: timestamp(),
            unknown: BTreeMap::new(),
        })
    }
}

fn persist_audit_file(trusted: &TrustedWorkDir, file: &AuditFile) -> Result<(), String> {
    write_json_atomic_trusted(trusted, AUDIT_FILE, file)
        .map_err(|error| format!("persist P1:3 audit: {error}"))?;
    let verified: AuditFile = read_json_trusted(trusted, AUDIT_FILE)
        .map_err(|error| format!("read back P1:3 audit: {error}"))?;
    if verified != *file {
        return Err("P1:3 audit readback verification failed".into());
    }
    Ok(())
}

fn dedupe_exact_p1_3_audits(entries: &mut Vec<AuditEntry>) {
    let mut retained = Vec::with_capacity(entries.len());
    for entry in entries.drain(..) {
        let duplicate = matches!(&entry, AuditEntry::Rebuildable(record) if record.step == "P1:3")
            && retained.iter().any(|known| known == &entry);
        if !duplicate {
            retained.push(entry);
        }
    }
    *entries = retained;
}
