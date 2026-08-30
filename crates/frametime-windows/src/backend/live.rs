use crate::*;
/// The fail-closed native backend.  It cannot be constructed on a non-Windows
/// host, outside the fixed root, or without an elevated token.
#[derive(Debug)]
pub struct LiveBackend<'authority> {
    pub(crate) work_dir: PathBuf,
    // Retains the validated root handle through every backend transaction.
    pub(crate) _trusted_work_dir: TrustedWorkDir,
    /// Keeps the non-cloneable source of mutation authority alive for the
    /// whole engine transaction. `VerifiedConfig` is retained only as data.
    pub(crate) _authority: MutationAuthority<'authority>,
    pub(crate) captured_steps: BTreeSet<String>,
    pub(crate) captured_msi_batches: BTreeMap<String, Vec<MsiDeviceBatch>>,
    pub(crate) captured_nic_affinity_bindings: BTreeMap<String, NicAffinityBinding>,
    pub(crate) observed_msi_preparation: Option<Vec<MsiDeviceBatch>>,
    pub(crate) observed_nic_affinity_preparation: Option<NicAffinityBinding>,
    pub(crate) captured_pagefile_bindings: BTreeMap<String, PagefileBinding>,
    pub(crate) created_pagefile_tokens: BTreeMap<String, CreatedPagefileToken>,
    pub(crate) captured_cs2_bindings: BTreeMap<String, Cs2RegistryBinding>,
    pub(crate) captured_cs2_config_bindings: BTreeMap<String, Cs2ConfigBinding>,
    pub(crate) captured_drs_backups: BTreeMap<String, DrsBackup>,
    pub(crate) captured_network_stack_bindings: BTreeMap<String, NetworkAdapterBinding>,
    pub(crate) chipset_inventory: Option<Option<ChipsetInventory>>,
    pub(crate) memory_topology: Option<Option<MemoryTopology>>,
    pub(crate) transaction_lock: Option<WorkLock>,
    pub(crate) state: State,
    pub(crate) config: VerifiedConfig,
    pub(crate) hardware: HardwareInfo,
}

#[derive(Debug)]
pub(crate) enum MutationAuthority<'authority> {
    Package(&'authority AuthenticatedPackage),
    Runtime(&'authority VerifiedSelectedRuntime),
}

impl Backend for LiveBackend<'_> {
    fn is_dry_run(&self) -> bool {
        false
    }
    fn inspect(&mut self, operation: Operation) -> Result<Inspection, String> {
        require_elevation()?;
        let descriptor = descriptor_for(&operation.step)?;
        match descriptor.capability {
            Capability::Advisory(reason) => return Ok(Inspection::Advisory { reason }),
            Capability::Supported => {}
        }
        self.require_descriptor_inputs(&descriptor)?;
        let action = descriptor.action;
        if self.nvidia_preparation_is_inapplicable(&action, operation)? {
            return Ok(Inspection::Inapplicable);
        }
        if matches!(action, Action::NvidiaDriverDownloadPreparation) {
            return load_driver_transaction_at(&self.work_dir)?
                .map(|_| Inspection::Satisfied)
                .ok_or_else(|| {
                    "P1:19 requires durable P1:18/P1:19 NVIDIA evidence; run `frametime driver prepare-nvidia` first".into()
                });
        }
        if let Some(inspection) = self.inspect_interrupt_action(&action, operation)? {
            return Ok(inspection);
        }
        match action {
            Action::ObserveConfigState => {
                return inspect_config_state(Some(self.config.value()), &self.state);
            }
            Action::ObserveGpuInventory => return inspect_gpu_inventory(&self.hardware),
            Action::BaselineBenchmark => {
                return Ok(
                    if baseline_benchmark_is_persisted(&self.work_dir, &self._trusted_work_dir) {
                        Inspection::Satisfied
                    } else {
                        Inspection::Unsupported
                    },
                );
            }
            Action::FinalBenchmark => return Ok(Inspection::Unsupported),
            Action::ObserveChipsetDriver => {
                let captured = capture_chipset_inventory()?;
                let inspection = if captured.is_some() {
                    Inspection::Satisfied
                } else {
                    Inspection::Inapplicable
                };
                self.chipset_inventory = Some(captured);
                return Ok(inspection);
            }
            Action::ObserveMemoryTopology => {
                let captured = capture_memory_topology()?;
                let inspection = if captured.is_some() {
                    Inspection::Satisfied
                } else {
                    Inspection::Inapplicable
                };
                self.memory_topology = Some(captured);
                return Ok(inspection);
            }
            Action::FpsCapInfo => {
                let inspection = inspect_fps_cap_info(Some(self.config.value()), &self.state)?;
                if inspection == Inspection::Satisfied {
                    let _messages = fps_cap_info_messages(&self.state);
                }
                return Ok(inspection);
            }
            Action::Pagefile => return inspect_pagefile(&self.state),
            Action::Cs2Registry(action) => return inspect_cs2_registry(action),
            Action::Cs2Config => return inspect_cs2_config(),
            Action::GpuDriverCleanPreparation => {
                return inspect_driver_cleanup_preparation_action();
            }
            Action::NvidiaProfilePreparation => return inspect_nvidia_drs_preparation(),
            Action::FinalChecklistGuide => {
                require_hags_effective_before_final_checklist(&self._trusted_work_dir)?;
                return inspect_action(&action);
            }
            _ => {}
        }
        let branch = self
            .configured_gpu_branch()
            .ok_or("GPU branch is unknown; select a validated branch before live execution")?;
        if !plan_for_step(&operation.step, branch).applicable {
            return Ok(Inspection::Inapplicable);
        }
        inspect_action(&action)
    }
    fn plan(&mut self, operation: Operation) -> Result<Vec<String>, String> {
        Ok(vec![format!(
            "Live backend will capture state before attempting {} ({}).",
            Self::key(operation),
            operation.step.title
        )])
    }
    fn capture_backups(&mut self, operation: Operation) -> Result<Vec<BackupEntry>, String> {
        let key = Self::key(operation);
        let action = descriptor_for(&operation.step)?.action;
        if matches!(action, Action::FinalBenchmark) {
            return Err("P3:13 final benchmark is check-only and does not capture backups".into());
        }
        if self.transaction_lock.is_some() {
            return Err("a previous live transaction is still active".into());
        }
        self.transaction_lock = Some(WorkLock::acquire(&self.work_dir)?);
        self.capture_backups_for_action(&action, key)
    }
    fn recovery_requirement(&self, operation: Operation) -> frametime_domain::RecoveryRequirement {
        descriptor_for(&operation.step)
            .map(|descriptor| descriptor.recovery_requirement)
            .unwrap_or(frametime_domain::RecoveryRequirement::LosslessBackup)
    }
    fn evidence_requirement(&self, operation: Operation) -> EvidenceRequirement {
        backend_evidence_requirement(operation)
    }
    fn capture_evidence(&mut self, operation: Operation) -> Result<ObservationReceipt, String> {
        capture_action_evidence(
            operation,
            self.observed_msi_preparation.as_deref(),
            self.observed_nic_affinity_preparation.as_ref(),
        )
    }
    fn persist_evidence(&mut self, receipt: &ObservationReceipt) -> Result<(), String> {
        persist_observation_receipt(&self._trusted_work_dir, receipt)
    }
    fn verify_evidence(
        &mut self,
        operation: Operation,
        receipt: &ObservationReceipt,
    ) -> Result<(), String> {
        verify_persisted_observation(&self._trusted_work_dir, operation, receipt)?;
        verify_preparation_observation(operation, receipt)
    }
    fn capture_pending_irreversible_audit(
        &mut self,
        operation: Operation,
    ) -> Result<frametime_domain::IrreversibleAudit, String> {
        self.capture_irreversible_audit(operation)
    }
    fn persist_pending_irreversible_audit(
        &mut self,
        audit: &frametime_domain::IrreversibleAudit,
    ) -> Result<(), String> {
        self.persist_irreversible_audit(audit)
    }
    fn finalize_irreversible_audit(
        &mut self,
        audit: &frametime_domain::IrreversibleAudit,
    ) -> Result<(), String> {
        self.replace_irreversible_audit(audit, false)
    }
    fn fail_irreversible_audit(
        &mut self,
        audit: &frametime_domain::IrreversibleAudit,
    ) -> Result<(), String> {
        let result = self.replace_irreversible_audit(audit, true);
        self.clear_transaction_state();
        result
    }
    fn persist_backups(&mut self, entries: &[BackupEntry]) -> Result<(), String> {
        if self.transaction_lock.is_none() {
            return Err("backup persistence requires the capture transaction lock".into());
        }
        let path = self.backup_path();
        let mut backup = if path.exists() {
            read_json_trusted(&self._trusted_work_dir, BACKUP_FILE)
                .map_err(|error| format!("read backup: {error}"))?
        } else {
            BackupFile {
                entries: Vec::new(),
                created: timestamp(),
                unknown: BTreeMap::new(),
            }
        };
        for entry in entries {
            if let BackupEntry::Powerplan { step, original_guid, suite_owned_guids, .. } = entry
                && step == "P1:6"
                && let Some(BackupEntry::Powerplan { suite_owned_guids: existing_owned, unknown, .. }) = backup.entries.iter_mut().find(|existing| matches!(existing, BackupEntry::Powerplan { step, original_guid: known, .. } if step == "P1:6" && known.eq_ignore_ascii_case(original_guid)))
            {
                if !unknown.is_empty() || existing_owned.iter().any(|guid| validate_power_plan_guid(guid).is_err()) {
                    return Err("existing P1:6 backup provenance is not exact".into());
                }
                for guid in suite_owned_guids {
                    if validate_power_plan_guid(guid).is_err() {
                        return Err("captured P1:6 suite GUID is invalid".into());
                    }
                    if !existing_owned.iter().any(|known| known.eq_ignore_ascii_case(guid)) {
                        existing_owned.push(guid.clone());
                    }
                }
                continue;
            }
            backup.push_first_value(entry.clone());
        }
        let result = write_json_atomic_trusted(&self._trusted_work_dir, BACKUP_FILE, &backup)
            .map_err(|error| format!("persist backup: {error}"));
        if result.is_err() {
            self.clear_transaction_state();
        }
        result
    }
    fn apply(&mut self, operation: Operation) -> Result<(), String> {
        let key = Self::key(operation);
        let action = descriptor_for(&operation.step)?.action;
        if matches!(action, Action::FinalBenchmark) {
            return Err("P3:13 final benchmark is check-only and cannot be applied".into());
        }
        if matches!(action, Action::NvidiaDriverRemoval) {
            remove_prepared_nvidia_driver(self.runtime_authority()?)?;
            return Ok(());
        }
        if matches!(action, Action::NvidiaDriverInstall) {
            install_prepared_nvidia_driver(self.runtime_authority()?)?;
            return Ok(());
        }
        if !self.captured_steps.contains(&key) {
            return Err(format!(
                "refusing mutation for {key}: backup capture did not succeed"
            ));
        }
        if self.transaction_lock.is_none() {
            return Err("mutation requires the capture transaction lock".into());
        }
        let result = self.apply_captured_action(&action, &key);
        if result.is_err() {
            self.abandon_transaction(&key);
        }
        result
    }
    fn verify(&mut self, operation: Operation) -> Result<(), String> {
        let key = Self::key(operation);
        let action = descriptor_for(&operation.step)?.action;
        if self.verify_observation_action(&action)?.is_some() {
            return Ok(());
        }
        let result = if matches!(&action, Action::Pagefile) {
            self.verify_pagefile_action(&key)
        } else if matches!(&action, Action::Cs2Config) {
            verify_cs2_config(
                self.captured_cs2_config_bindings
                    .get(&key)
                    .ok_or("P1:34 verification requires a captured CS2 config binding")?,
            )
        } else if let Some(result) = self.verify_interrupt_action(&action, &key) {
            result
        } else if matches!(&action, Action::NvidiaProfileApply) {
            self.verify_drs_backup(&key)
        } else if matches!(&action, Action::NetworkStack) {
            verify_network_stack(
                &NativeNetworkStackHost,
                self.captured_network_stack_bindings
                    .get(&key)
                    .ok_or("P1:16 verification requires a captured network-stack binding")?,
            )
        } else {
            verify_action(&action, self.captured_cs2_bindings.get(&key))
        };
        if result.is_err() {
            self.abandon_transaction(&key);
        }
        result
    }
    fn persist_progress(&mut self, progress: &Progress) -> Result<(), String> {
        let temporary_lock = if self.transaction_lock.is_none() {
            Some(WorkLock::acquire(&self.work_dir)?)
        } else {
            None
        };
        let result = write_json_atomic_trusted(&self._trusted_work_dir, PROGRESS_FILE, progress)
            .map_err(|error| format!("persist progress: {error}"));
        if result.is_ok() {
            self.clear_transaction_state();
        }
        drop(temporary_lock);
        result
    }
    fn timestamp(&self) -> String {
        timestamp()
    }
}

impl LiveBackend<'_> {
    pub(crate) fn capture_backups_for_action(
        &mut self,
        action: &Action,
        key: String,
    ) -> Result<Vec<BackupEntry>, String> {
        match action {
            Action::Cs2Registry(action) => return self.capture_cs2_registry_backup(key, *action),
            Action::Cs2Config => return self.capture_cs2_config_backup(key),
            Action::NvidiaProfileApply => return self.capture_drs_backup(key),
            Action::NetworkStack => return self.capture_network_stack_backup(key),
            Action::Pagefile => return self.capture_pagefile_backup(key),
            _ => {}
        }
        if let Some(entries) = self.capture_interrupt_action(action, &key)? {
            return Ok(entries);
        }
        self.capture_standard_backups(action, key)
    }

    pub(crate) fn capture_standard_backups(
        &mut self,
        action: &Action,
        key: String,
    ) -> Result<Vec<BackupEntry>, String> {
        let entries = capture_actions(action, key.clone(), Some(self.config.value()))
            .inspect_err(|_| self.transaction_lock = None)?;
        self.captured_steps.insert(key);
        Ok(entries)
    }

    pub(crate) fn apply_captured_action(
        &mut self,
        action: &Action,
        key: &str,
    ) -> Result<(), String> {
        match action {
            Action::Pagefile => self.apply_pagefile_action(key),
            Action::Cs2Config => apply_cs2_config(
                self.captured_cs2_config_bindings
                    .get(key)
                    .ok_or("P1:34 mutation requires a captured CS2 config binding")?,
            ),
            Action::MsiInterrupts => apply_native_msi_batches(
                self.captured_msi_batches
                    .get(key)
                    .ok_or("P3:2 mutation requires captured MSI device bindings")?,
            ),
            Action::NicInterruptAffinity => apply_native_nic_affinity(
                self.captured_nic_affinity_bindings
                    .get(key)
                    .ok_or("P3:3 mutation requires a captured NIC affinity binding")?,
            ),
            Action::NvidiaProfileApply => self.apply_drs_backup(key),
            Action::NetworkStack => apply_network_stack(
                &NativeNetworkStackHost,
                self.captured_network_stack_bindings
                    .get(key)
                    .ok_or("P1:16 mutation requires a captured network-stack binding")?,
            ),
            _ => apply_action(
                action,
                Some(self.config.value()),
                None,
                None,
                self.captured_cs2_bindings.get(key),
            ),
        }
    }
}
