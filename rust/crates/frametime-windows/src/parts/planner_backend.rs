/// A read-only backend with identical catalog planning and zero persistence.
#[derive(Debug, Clone)]
pub struct PlannerBackend {
    gpu_branch: u8,
}

impl PlannerBackend {
    #[must_use]
    pub const fn new(gpu_branch: u8) -> Self {
        Self::new_with_profile(gpu_branch, Profile::Custom)
    }
    #[must_use]
    pub const fn new_with_profile(gpu_branch: u8, _profile: Profile) -> Self {
        Self { gpu_branch }
    }
    fn key(operation: Operation) -> String {
        Progress::key(operation.step.phase as u8, operation.step.number)
    }
    fn capability(&self, descriptor: &ActionDescriptor) -> Capability {
        match descriptor.capability {
            Capability::Advisory(reason) => Capability::Advisory(reason),
            Capability::Unsupported(reason) => Capability::Unsupported(reason),
            Capability::Supported => match &descriptor.action {
                Action::ShaderCache if !shader_cache_delete_qualified() => Capability::Unsupported(
                    "P1:3 deletion is build-gated until the Windows reparse, sharing, race, and disposition qualification matrix passes",
                ),
                _ => Capability::Supported,
            },
        }
    }
}

impl Backend for PlannerBackend {
    fn is_dry_run(&self) -> bool {
        true
    }
    fn inspect(&mut self, operation: Operation) -> Result<Inspection, String> {
        let branch = GpuBranch::try_from(self.gpu_branch)?;
        if !plan_for_step(&operation.step, branch).applicable {
            return Ok(Inspection::Inapplicable);
        }
        let descriptor = descriptor_for(operation.step.phase as u8, operation.step.number)?;
        match self.capability(&descriptor) {
            Capability::Advisory(reason) => Ok(Inspection::Advisory { reason }),
            Capability::Unsupported(_) => Ok(Inspection::Unsupported),
            Capability::Supported
                if matches!(
                    descriptor.action,
                    Action::FpsCapInfo
                        | Action::GpuDriverCleanPreparation
                        | Action::NvidiaProfilePreparation
                        | Action::SafeModeHandoff
                        | Action::PhaseThreeHandoff
                        | Action::MsiPreparation
                        | Action::NicAffinityPreparation
                        | Action::Cs2LaunchVideoGuide
                        | Action::AmdRadeonGuide
                        | Action::VramUsageGuide
                        | Action::FinalChecklistGuide
                ) =>
            {
                Ok(Inspection::Satisfied)
            }
            Capability::Supported
                if matches!(
                    descriptor.action,
                    Action::ObserveConfigState
                        | Action::ObserveGpuInventory
                        | Action::ObserveChipsetDriver
                ) =>
            {
                Ok(Inspection::NeedsApply)
            }
            Capability::Supported => Ok(Inspection::NeedsApply),
        }
    }
    fn plan(&mut self, operation: Operation) -> Result<Vec<String>, String> {
        let branch = GpuBranch::try_from(self.gpu_branch)?;
        let action = plan_for_step(&operation.step, branch);
        if !action.applicable {
            return Ok(vec![format!(
                "Would skip inapplicable {} ({}) for {}; no files or system settings will be changed.",
                Self::key(operation),
                operation.step.title,
                branch.label()
            )]);
        }
        let descriptor = descriptor_for(operation.step.phase as u8, operation.step.number)?;
        let native_action = &descriptor.action;
        if matches!(native_action, Action::FpsCapInfo) {
            return Ok(vec![format!(
                "Would report {} ({}) and that the final step will calculate the FPS cap; no files or system settings will be changed.",
                Self::key(operation),
                operation.step.title,
            )]);
        }
        let guide = match native_action {
            Action::GpuDriverCleanPreparation => Some(
                "Would present GPU driver-clean preparation: confirm the exact target GPU and signed replacement driver, and prepare Safe Mode and recovery first; no driver removal, handoff, reboot, files, or system settings will be changed.",
            ),
            Action::NvidiaProfilePreparation => Some(
                "Would perform a read-only NVIDIA DRS profile inspection and prepare a durable receipt; NVAPI, DRS profiles, registry settings, files, and system settings will not be changed.",
            ),
            Action::SafeModeHandoff => Some(
                "Would publish the exact compiled payload into a protected immutable generation, visibly elevate its retained frametime.exe, and let that selected process bind HKLM RunOnce and Safe Boot before P1:38 progress; dry-run performs none of those effects.",
            ),
            Action::PhaseThreeHandoff => Some(
                "Would require coherent P2:2 removal evidence, the initiating TokenUser, and the selected immutable runtime before binding the exact HKCU Phase 3 Run handoff; dry-run performs no registry or reboot effect.",
            ),
            Action::MsiPreparation => Some(
                "Would present MSI preparation: use only specifically supported devices, record current state, then reboot and verify negotiated mode because a registry request does not prove MSI or MSI-X is active; no files or system settings will be changed.",
            ),
            Action::NicAffinityPreparation => Some(
                "Would present NIC-affinity preparation only after reproducible NIC DPC diagnosis and authoritative logical-processor topology validation; an unsuitable mask can increase latency or concentrate load, and no files or system settings will be changed.",
            ),
            Action::Cs2LaunchVideoGuide => Some(
                "Would present manual CS2 launch-options and video guidance; Steam launch options and video.txt will not be written.",
            ),
            Action::AmdRadeonGuide => Some(
                "Would present AMD Radeon guidance: verify current AMD and game documentation, including anti-cheat compatibility, before enabling driver features; firmware, AMD settings, files, and system settings will not be changed.",
            ),
            Action::VramUsageGuide => Some(
                "Would present same-workload VRAM observation guidance; no telemetry is collected and no files or system settings will be changed.",
            ),
            Action::FinalChecklistGuide => Some(
                "Would present the final checklist without requiring optional hardware observations; no files or system settings will be changed.",
            ),
            _ => None,
        };
        if let Some(guide) = guide {
            return Ok(vec![format!(
                "Would report {} ({}).",
                Self::key(operation),
                guide
            )]);
        }
        if matches!(&native_action, Action::BaselineBenchmark) {
            return Ok(vec![format!(
                "Would require a complete VProf capture through baseline-benchmark for {} ({}); no files or system settings will be changed.",
                Self::key(operation),
                operation.step.title,
            )]);
        }
        if matches!(&native_action, Action::FinalBenchmark) {
            return Ok(vec![format!(
                "Would require a complete VProf capture through final-benchmark for {} ({}) and perform zero persistence; no files or system settings will be changed.",
                Self::key(operation),
                operation.step.title,
            )]);
        }
        if matches!(&native_action, Action::ObserveMemoryTopology) {
            return Ok(vec![format!(
                "Would observe {} ({}) from SMBIOS firmware channel associations only; no active channel mode is inferred and zero persistence, files, or system settings will be changed.",
                Self::key(operation),
                operation.step.title,
            )]);
        }
        match self.capability(&descriptor) {
            Capability::Advisory(reason) => Ok(vec![format!(
                "{} is advisory and unverified: {reason}; no files or system settings will be changed.",
                Self::key(operation)
            )]),
            Capability::Unsupported(reason) => Ok(vec![format!(
                "{} is unsupported: {reason}; no files or system settings will be changed.",
                Self::key(operation)
            )]),
            Capability::Supported => Ok(vec![format!(
                "Would plan {} ({}) for {}; requires {}; no files or system settings will be changed.",
                Self::key(operation),
                operation.step.title,
                branch.label(),
                required_inputs_label(descriptor.required_inputs),
            )]),
        }
    }
    fn capture_backups(&mut self, _: Operation) -> Result<Vec<BackupEntry>, String> {
        Err("planner backend does not capture or persist backups".into())
    }
    fn persist_backups(&mut self, _: &[BackupEntry]) -> Result<(), String> {
        Err("planner backend performs zero persistence".into())
    }
    fn apply(&mut self, _: Operation) -> Result<(), String> {
        Err("planner backend does not apply changes".into())
    }
    fn verify(&mut self, _: Operation) -> Result<(), String> {
        Err("planner backend does not verify live state".into())
    }
    fn persist_progress(&mut self, _: &Progress) -> Result<(), String> {
        Err("planner backend performs zero persistence".into())
    }
    fn timestamp(&self) -> String {
        timestamp()
    }
}
