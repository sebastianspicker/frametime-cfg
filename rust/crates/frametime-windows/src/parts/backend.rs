/// The fail-closed native backend.  It cannot be constructed on a non-Windows
/// host, outside the fixed root, or without an elevated token.
#[derive(Debug)]
pub struct LiveBackend {
    work_dir: PathBuf,
    // Retains the validated root handle through every backend transaction.
    _trusted_work_dir: TrustedWorkDir,
    captured_steps: BTreeSet<String>,
    captured_service_batches: BTreeMap<String, Vec<String>>,
    captured_nagle_bindings: BTreeMap<String, NagleBinding>,
    captured_dns_bindings: BTreeMap<String, Vec<DnsBinding>>,
    captured_msi_batches: BTreeMap<String, Vec<MsiDeviceBatch>>,
    captured_nic_affinity_bindings: BTreeMap<String, NicAffinityBinding>,
    observed_msi_preparation: Option<Vec<MsiDeviceBatch>>,
    observed_nic_affinity_preparation: Option<NicAffinityBinding>,
    captured_autostart_bindings: BTreeMap<String, AutostartBinding>,
    captured_power_plan_bindings: BTreeMap<String, PowerPlanBinding>,
    captured_pagefile_bindings: BTreeMap<String, PagefileBinding>,
    created_pagefile_tokens: BTreeMap<String, CreatedPagefileToken>,
    captured_cs2_bindings: BTreeMap<String, Cs2RegistryBinding>,
    captured_cs2_config_bindings: BTreeMap<String, Cs2ConfigBinding>,
    captured_drs_backups: BTreeMap<String, DrsBackup>,
    captured_hags: BTreeMap<String, HagsRegistryCompatibility>,
    captured_network_stack_bindings: BTreeMap<String, NetworkAdapterBinding>,
    shader_cache_inventory: Option<ShaderCacheInventory>,
    #[cfg(windows)]
    debloat: Option<DebloatCapability<NativeDebloatHost>>,
    debloat_appx_subjects: Option<Vec<frametime_core::AppxRemovalSubject>>,
    chipset_inventory: Option<Option<ChipsetInventory>>,
    memory_topology: Option<Option<MemoryTopology>>,
    transaction_lock: Option<WorkLock>,
    state: State,
    config: Option<Config>,
    hardware: HardwareInfo,
}

#[cfg(windows)]
pub type WindowsBackend = LiveBackend;

impl Backend for LiveBackend {
    fn is_dry_run(&self) -> bool {
        false
    }
    fn inspect(&mut self, operation: Operation) -> Result<Inspection, String> {
        require_elevation()?;
        let descriptor = descriptor_for(operation.step.phase as u8, operation.step.number)?;
        match descriptor.capability {
            Capability::Advisory(reason) => return Ok(Inspection::Advisory { reason }),
            Capability::Unsupported(_) => return Ok(Inspection::Unsupported),
            Capability::Supported => {}
        }
        self.require_descriptor_inputs(&descriptor)?;
        let action = descriptor.action;
        if self.nvidia_preparation_is_inapplicable(&action, operation)? {
            return Ok(Inspection::Inapplicable);
        }
        if matches!(action, Action::NvidiaDriverDownloadPreparation) {
            return load_driver_transaction(&self.work_dir)?
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
                return inspect_config_state(self.config.as_ref(), &self.state);
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
                let inspection = inspect_fps_cap_info(self.config.as_ref(), &self.state)?;
                if inspection == Inspection::Satisfied {
                    report_fps_cap_info(&self.state);
                }
                return Ok(inspection);
            }
            Action::ShaderCache => return self.inspect_shader_cache_with_audit(),
            Action::Debloat => return self.inspect_debloat(),
            Action::ServiceBatch(batch) => {
                let names = service_power_contract_map(batch, self.config.as_ref())?;
                return native_services::inspect_batch(&names, batch);
            }
            Action::Nagle => return inspect_nagle(),
            Action::Dns => return inspect_dns(self.config.as_ref()),
            Action::Autostart => return inspect_autostart(self.config.as_ref()),
            Action::PowerPlan => return inspect_power_plan(self.state.profile),
            Action::Pagefile => return inspect_pagefile(&self.state),
            Action::Cs2Registry(action) => return inspect_cs2_registry(action),
            Action::Cs2Config => return inspect_cs2_config(),
            Action::GpuDriverCleanPreparation => {
                return inspect_driver_cleanup_preparation_action();
            }
            Action::NvidiaProfilePreparation => return inspect_nvidia_drs_preparation(),
            Action::Hags => return inspect_hags(),
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
        if operation.step.phase as u8 == 1 && operation.step.number == 28 {
            return inspect_timer_resolution(&action);
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
        let action = descriptor_for(operation.step.phase as u8, operation.step.number)?.action;
        if matches!(action, Action::FinalBenchmark) {
            return Err("P3:13 final benchmark is check-only and does not capture backups".into());
        }
        if self.transaction_lock.is_some() {
            return Err("a previous live transaction is still active".into());
        }
        self.transaction_lock = Some(WorkLock::acquire(&self.work_dir)?);
        self.capture_backups_for_action(&action, key)
    }
    fn recovery_requirement(&self, operation: Operation) -> frametime_core::RecoveryRequirement {
        descriptor_for(operation.step.phase as u8, operation.step.number)
            .map(|descriptor| descriptor.recovery_requirement)
            .unwrap_or(frametime_core::RecoveryRequirement::LosslessBackup)
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
    fn capture_pending_audit(
        &mut self,
        operation: Operation,
    ) -> Result<frametime_core::RebuildableAudit, String> {
        self.capture_shader_cache_audit(operation)
    }
    fn persist_pending_audit(
        &mut self,
        audit: &frametime_core::RebuildableAudit,
    ) -> Result<(), String> {
        self.persist_shader_cache_audit(audit)
    }
    fn finalize_audit(&mut self, audit: &frametime_core::RebuildableAudit) -> Result<(), String> {
        self.finalize_shader_cache_audit(audit)
    }
    fn capture_pending_irreversible_audit(
        &mut self,
        operation: Operation,
    ) -> Result<frametime_core::IrreversibleAudit, String> {
        self.capture_irreversible_audit(operation)
    }
    fn persist_pending_irreversible_audit(
        &mut self,
        audit: &frametime_core::IrreversibleAudit,
    ) -> Result<(), String> {
        self.persist_irreversible_audit(audit)
    }
    fn finalize_irreversible_audit(
        &mut self,
        audit: &frametime_core::IrreversibleAudit,
    ) -> Result<(), String> {
        self.replace_irreversible_audit(audit, false)
    }
    fn fail_irreversible_audit(
        &mut self,
        audit: &frametime_core::IrreversibleAudit,
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
        let action = descriptor_for(operation.step.phase as u8, operation.step.number)?.action;
        if matches!(action, Action::FinalBenchmark) {
            return Err("P3:13 final benchmark is check-only and cannot be applied".into());
        }
        if is_shader_cache_operation(operation) {
            let inventory = self
                .shader_cache_inventory
                .as_ref()
                .ok_or("P1:3 mutation requires the captured cache inventory")?;
            let result = clear_shader_cache(inventory);
            if result.is_err() {
                self.transaction_lock = None;
            }
            return result;
        }
        if is_debloat_operation(operation) {
            return self.apply_debloat();
        }
        if matches!(action, Action::NvidiaDriverRemoval) {
            remove_prepared_nvidia_driver(&self.work_dir)?;
            return Ok(());
        }
        if matches!(action, Action::NvidiaDriverInstall) {
            install_prepared_nvidia_driver(&self.work_dir)?;
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
        if operation.step.phase as u8 == 1 && operation.step.number == 28 {
            require_timer_resolution_build()?;
        }
        let result = self.apply_captured_action(&action, &key);
        if result.is_err() {
            self.abandon_transaction(&key);
        }
        result
    }
    fn verify(&mut self, operation: Operation) -> Result<(), String> {
        let key = Self::key(operation);
        if is_shader_cache_operation(operation) {
            let result = match self.shader_cache_inventory.as_ref() {
                Some(inventory) => verify_shader_cache(inventory),
                None => match inspect_shader_cache(self.config.as_ref())? {
                    Inspection::Satisfied => Ok(()),
                    Inspection::Unsupported => {
                        Err("P1:3 cache contents appeared before verification".into())
                    }
                    Inspection::Advisory { .. }
                    | Inspection::NeedsApply
                    | Inspection::Inapplicable => {
                        Err("P1:3 cache verification produced an invalid inspection state".into())
                    }
                },
            };
            if result.is_err() {
                self.transaction_lock = None;
            }
            return result;
        }
        if is_debloat_operation(operation) {
            return self.verify_debloat();
        }
        let action = descriptor_for(operation.step.phase as u8, operation.step.number)?.action;
        if self.verify_observation_action(&action)?.is_some() {
            return Ok(());
        }
        let result = if matches!(&action, Action::PowerPlan) {
            verify_power_plan(
                self.captured_power_plan_bindings
                    .get(&key)
                    .ok_or("P1:6 verification requires a captured power-plan binding")?,
            )
        } else if matches!(&action, Action::Autostart) {
            verify_autostart(
                self.captured_autostart_bindings
                    .get(&key)
                    .ok_or("P1:14 verification requires a captured autostart binding")?,
            )
        } else if matches!(&action, Action::Pagefile) {
            self.verify_pagefile_action(&key)
        } else if matches!(&action, Action::Cs2Config) {
            verify_cs2_config(
                self.captured_cs2_config_bindings
                    .get(&key)
                    .ok_or("P1:34 verification requires a captured CS2 config binding")?,
            )
        } else if matches!(&action, Action::Dns) {
            let config = self
                .config
                .as_ref()
                .ok_or("P3:9 requires a validated frametime.toml DNS provider selection")?;
            verify_native_dns(
                self.captured_dns_bindings
                    .get(&key)
                    .ok_or("P3:9 verification requires captured DNS bindings")?,
                config,
            )
        } else if let Some(result) = self.verify_interrupt_action(&action, &key) {
            result
        } else if matches!(&action, Action::NvidiaProfileApply) {
            self.verify_drs_backup(&key)
        } else if matches!(&action, Action::Hags) {
            self.verify_hags_immediate(&key)
        } else if matches!(&action, Action::NetworkStack) {
            verify_network_stack(
                &NativeNetworkStackHost,
                self.captured_network_stack_bindings
                    .get(&key)
                    .ok_or("P1:16 verification requires a captured network-stack binding")?,
            )
        } else {
            if operation.step.phase as u8 == 1 && operation.step.number == 28 {
                require_timer_resolution_build()?;
            }
            verify_action(
                &action,
                self.config.as_ref(),
                self.captured_service_batches.get(&key).map(Vec::as_slice),
                self.captured_nagle_bindings.get(&key),
                self.captured_cs2_bindings.get(&key),
            )
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

impl LiveBackend {
    fn capture_backups_for_action(
        &mut self,
        action: &Action,
        key: String,
    ) -> Result<Vec<BackupEntry>, String> {
        match action {
            Action::Debloat => {
                return self
                    .capture_debloat_backups(key)
                    .inspect_err(|_| self.transaction_lock = None);
            }
            Action::Cs2Registry(action) => return self.capture_cs2_registry_backup(key, *action),
            Action::Cs2Config => return self.capture_cs2_config_backup(key),
            Action::NvidiaProfileApply => return self.capture_drs_backup(key),
            Action::Hags => return self.capture_hags_backup(key),
            Action::NetworkStack => return self.capture_network_stack_backup(key),
            Action::Autostart => return self.capture_autostart_backup(key),
            Action::PowerPlan => return self.capture_power_plan_backup(key),
            Action::Pagefile => return self.capture_pagefile_backup(key),
            Action::Dns => return self.capture_dns_backup(key),
            _ => {}
        }
        if let Some(entries) = self.capture_interrupt_action(action, &key)? {
            return Ok(entries);
        }
        self.capture_standard_backups(action, key)
    }

    fn capture_standard_backups(
        &mut self,
        action: &Action,
        key: String,
    ) -> Result<Vec<BackupEntry>, String> {
        let capture = if matches!(action, Action::Nagle) {
            capture_nagle_batch(key.clone()).map(|(binding, entries)| (entries, Some(binding)))
        } else {
            capture_actions(action, key.clone(), self.config.as_ref())
                .map(|entries| (entries, None))
        };
        let (entries, nagle_binding) = capture.inspect_err(|_| self.transaction_lock = None)?;
        if matches!(action, Action::ServiceBatch(_)) {
            let captured = entries
                .iter()
                .filter_map(|entry| match entry {
                    BackupEntry::Service { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .collect();
            self.captured_service_batches.insert(key.clone(), captured);
        }
        if let Some(binding) = nagle_binding {
            self.captured_nagle_bindings.insert(key.clone(), binding);
        }
        self.captured_steps.insert(key);
        Ok(entries)
    }

    fn apply_captured_action(&mut self, action: &Action, key: &str) -> Result<(), String> {
        match action {
            Action::PowerPlan => apply_power_plan(
                self.captured_power_plan_bindings
                    .get(key)
                    .ok_or("P1:6 mutation requires a captured power-plan binding")?,
            ),
            Action::Autostart => apply_autostart(
                self.captured_autostart_bindings
                    .get(key)
                    .ok_or("P1:14 mutation requires a captured autostart binding")?,
            ),
            Action::Pagefile => self.apply_pagefile_action(key),
            Action::Cs2Config => apply_cs2_config(
                self.captured_cs2_config_bindings
                    .get(key)
                    .ok_or("P1:34 mutation requires a captured CS2 config binding")?,
            ),
            Action::Dns => self.apply_dns_action(key),
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
            Action::Hags => self.apply_hags(key),
            Action::NetworkStack => apply_network_stack(
                &NativeNetworkStackHost,
                self.captured_network_stack_bindings
                    .get(key)
                    .ok_or("P1:16 mutation requires a captured network-stack binding")?,
            ),
            _ => apply_action(
                action,
                self.config.as_ref(),
                self.captured_service_batches.get(key).map(Vec::as_slice),
                self.captured_nagle_bindings.get(key),
                self.captured_cs2_bindings.get(key),
            ),
        }
    }

    fn apply_dns_action(&self, key: &str) -> Result<(), String> {
        let config = self
            .config
            .as_ref()
            .ok_or("P3:9 requires a validated frametime.toml DNS provider selection")?;
        apply_native_dns(
            self.captured_dns_bindings
                .get(key)
                .ok_or("P3:9 mutation requires captured DNS bindings")?,
            config,
        )
    }
}
