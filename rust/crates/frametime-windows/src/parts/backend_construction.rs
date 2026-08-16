impl LiveBackend {
    /// Create the backend from a retained-verifier configuration snapshot and
    /// native display-adapter discovery.
    pub fn new(work_dir: PathBuf, config: VerifiedConfig) -> Result<Self, String> {
        let trusted_work_dir = TrustedWorkDir::acquire(&work_dir)?;
        require_elevation()?;
        let state = load_state(trusted_work_dir.path())?;
        let hardware = discover_hardware()?;
        Ok(Self {
            work_dir: trusted_work_dir.path().to_path_buf(),
            _trusted_work_dir: trusted_work_dir,
            captured_steps: BTreeSet::new(),
            captured_service_batches: BTreeMap::new(),
            captured_nagle_bindings: BTreeMap::new(),
            captured_dns_bindings: BTreeMap::new(),
            captured_msi_batches: BTreeMap::new(),
            captured_nic_affinity_bindings: BTreeMap::new(),
            observed_msi_preparation: None,
            observed_nic_affinity_preparation: None,
            captured_autostart_bindings: BTreeMap::new(),
            captured_power_plan_bindings: BTreeMap::new(),
            captured_pagefile_bindings: BTreeMap::new(),
            created_pagefile_tokens: BTreeMap::new(),
            captured_cs2_bindings: BTreeMap::new(),
            captured_cs2_config_bindings: BTreeMap::new(),
            captured_drs_backups: BTreeMap::new(),
            captured_hags: BTreeMap::new(),
            captured_network_stack_bindings: BTreeMap::new(),
            shader_cache_inventory: None,
            #[cfg(windows)]
            debloat: None,
            debloat_appx_subjects: None,
            chipset_inventory: None,
            memory_topology: None,
            transaction_lock: None,
            state,
            config,
            hardware,
        })
    }

    #[must_use]
    pub fn hardware(&self) -> &HardwareInfo {
        &self.hardware
    }

    #[must_use]
    pub fn configured_gpu_branch(&self) -> Option<GpuBranch> {
        self.state
            .gpu_input
            .as_deref()
            .and_then(|value| value.parse::<u8>().ok())
            .and_then(|value| GpuBranch::try_from(value).ok())
            .or(self.hardware.gpu_branch)
    }

    fn key(operation: Operation) -> String {
        Progress::key(operation.step.phase as u8, operation.step.number)
    }

    fn backup_path(&self) -> PathBuf {
        self.work_dir.join(BACKUP_FILE)
    }

    fn require_descriptor_inputs(&self, descriptor: &ActionDescriptor) -> Result<(), String> {
        for input in descriptor.required_inputs {
            match input {
                RequiredInput::GpuBranch if self.configured_gpu_branch().is_none() => {
                    return Err("native action requires a validated GPU branch".into());
                }
                RequiredInput::ValidatedConfig | RequiredInput::GpuBranch => {}
            }
        }
        Ok(())
    }

    fn nvidia_preparation_is_inapplicable(
        &self,
        action: &Action,
        operation: Operation,
    ) -> Result<bool, String> {
        if !matches!(
            action,
            Action::NvidiaDriverDownloadPreparation | Action::NvidiaProfilePreparation
        ) {
            return Ok(false);
        }
        let branch = self.configured_gpu_branch().ok_or(
            "GPU branch is unknown; select a validated branch before NVIDIA preparation",
        )?;
        Ok(!plan_for_step(&operation.step, branch).applicable)
    }
}
