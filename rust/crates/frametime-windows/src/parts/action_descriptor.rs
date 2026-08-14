/// Native capability verdict for one catalog action. Advisory means that a
/// check-only step has a stable explanation but no authoritative proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Capability {
    Supported,
    Advisory(&'static str),
    Unsupported(&'static str),
}

/// Inputs the native action expects before it can be executed live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequiredInput {
    ValidatedConfig,
    GpuBranch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActionDescriptor {
    action: Action,
    capability: Capability,
    required_inputs: &'static [RequiredInput],
    recovery_requirement: frametime_core::RecoveryRequirement,
    evidence_requirement: EvidenceRequirement,
}

impl ActionDescriptor {
    fn new(action: Action) -> Self {
        let capability = match &action {
            Action::Advisory(reason) => Capability::Advisory(reason),
            _ => Capability::Supported,
        };
        let required_inputs: &'static [RequiredInput] = match &action {
            Action::ObserveConfigState
            | Action::Autostart
            | Action::Dns
            | Action::ServiceBatch(ServiceBatch::SysMainSearchQwaveXbox) => {
                &[RequiredInput::ValidatedConfig]
            }
            Action::PowerPlan
            | Action::Cs2Registry(_)
            | Action::Cs2Config
            | Action::ShaderCache
            | Action::NvidiaProfilePreparation
            | Action::NvidiaProfileApply => &[RequiredInput::GpuBranch],
            _ => &[],
        };
        let recovery_requirement = if matches!(&action, Action::ShaderCache) {
            frametime_core::RecoveryRequirement::RebuildableAudit
        } else if matches!(&action, Action::Debloat) {
            frametime_core::RecoveryRequirement::Mixed
        } else if matches!(
            &action,
            Action::NvidiaDriverRemoval | Action::NvidiaDriverInstall
        ) {
            frametime_core::RecoveryRequirement::ManualRecoveryAudit
        } else {
            frametime_core::RecoveryRequirement::LosslessBackup
        };
        let evidence_requirement = if matches!(
            &action,
            Action::GpuDriverCleanPreparation
                | Action::NvidiaProfilePreparation
                | Action::MsiPreparation
                | Action::NicAffinityPreparation
        ) {
            EvidenceRequirement::DurableReceipt
        } else {
            EvidenceRequirement::None
        };
        Self {
            action,
            capability,
            required_inputs,
            recovery_requirement,
            evidence_requirement,
        }
    }
}

fn descriptor_for(phase: u8, step: u8) -> Result<ActionDescriptor, String> {
    native_action_for(phase, step).map(ActionDescriptor::new)
}

/// Compatibility accessor for code that only needs the typed native action.
/// Planner and live execution use `descriptor_for` for the full contract.
fn action_for(phase: u8, step: u8) -> Result<Action, String> {
    descriptor_for(phase, step).map(|descriptor| descriptor.action)
}

fn required_inputs_label(inputs: &[RequiredInput]) -> &'static str {
    match inputs {
        [] => "no additional native inputs",
        [RequiredInput::ValidatedConfig] => "a validated frametime.toml configuration",
        [RequiredInput::GpuBranch] => "a validated GPU branch",
        _ => "validated native inputs",
    }
}
