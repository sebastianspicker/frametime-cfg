use crate::*;
impl<'authority> LiveBackend<'authority> {
    /// Construct a fresh-mutation backend while borrowing the package
    /// capability that authenticated the exact config and payload.
    pub fn from_package(package: &'authority AuthenticatedPackage) -> Result<Self, String> {
        Self::from_authority(MutationAuthority::Package(package))
    }

    /// Construct a resumed-mutation backend while borrowing the selected
    /// runtime capability that retained the exact handoff executable.
    pub fn from_runtime(runtime: &'authority VerifiedSelectedRuntime) -> Result<Self, String> {
        Self::from_authority(MutationAuthority::Runtime(runtime))
    }

    fn from_authority(authority: MutationAuthority<'authority>) -> Result<Self, String> {
        let config = match &authority {
            MutationAuthority::Package(package) => package.config().clone(),
            MutationAuthority::Runtime(runtime) => runtime.config().clone(),
        };
        let trusted_work_dir = TrustedWorkDir::acquire_fixed()?;
        require_elevation()?;
        let state = load_state_at(trusted_work_dir.path())?;
        let hardware = discover_hardware()?;
        Ok(Self {
            work_dir: trusted_work_dir.path().to_path_buf(),
            _trusted_work_dir: trusted_work_dir,
            _authority: authority,
            captured_steps: BTreeSet::new(),
            captured_msi_batches: BTreeMap::new(),
            captured_nic_affinity_bindings: BTreeMap::new(),
            observed_msi_preparation: None,
            observed_nic_affinity_preparation: None,
            captured_pagefile_bindings: BTreeMap::new(),
            created_pagefile_tokens: BTreeMap::new(),
            captured_cs2_bindings: BTreeMap::new(),
            captured_cs2_config_bindings: BTreeMap::new(),
            captured_drs_backups: BTreeMap::new(),
            captured_network_stack_bindings: BTreeMap::new(),
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

    pub(crate) fn runtime_authority(&self) -> Result<&VerifiedSelectedRuntime, String> {
        match self._authority {
            MutationAuthority::Runtime(runtime) => Ok(runtime),
            MutationAuthority::Package(_) => {
                Err("resumed driver mutation requires a verified selected runtime authority".into())
            }
        }
    }

    pub(crate) fn key(operation: Operation) -> String {
        operation.step.id.progress_key()
    }

    pub(crate) fn backup_path(&self) -> PathBuf {
        self.work_dir.join(BACKUP_FILE)
    }

    pub(crate) fn require_descriptor_inputs(
        &self,
        descriptor: &ActionDescriptor,
    ) -> Result<(), String> {
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

    pub(crate) fn nvidia_preparation_is_inapplicable(
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
        let branch = self
            .configured_gpu_branch()
            .ok_or("GPU branch is unknown; select a validated branch before NVIDIA preparation")?;
        Ok(!plan_for_step(&operation.step, branch).applicable)
    }
}
