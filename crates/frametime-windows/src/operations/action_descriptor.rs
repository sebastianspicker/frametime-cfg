use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Capability {
    Supported,
    Advisory(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequiredInput {
    ValidatedConfig,
    GpuBranch,
}

/// One Windows-only execution contract attached to a domain catalog row.
/// The domain owns identity, intent, operation kind, GPU applicability, and
/// orchestration role; this adapter owns native payload and runtime contracts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActionDescriptor {
    pub(crate) id: StepId,
    pub(crate) intent: ActionIntent,
    pub(crate) operation: OperationKind,
    pub(crate) gpu_applicability: GpuApplicability,
    pub(crate) orchestration_role: OrchestrationRole,
    pub(crate) action: Action,
    pub(crate) capability: Capability,
    pub(crate) required_inputs: &'static [RequiredInput],
    pub(crate) recovery_requirement: frametime_domain::RecoveryRequirement,
    pub(crate) evidence_requirement: EvidenceRequirement,
}

impl ActionDescriptor {
    pub(crate) fn new(step: &Step, action: Action) -> Self {
        let capability = match &action {
            Action::Advisory(reason) => Capability::Advisory(reason),
            _ => Capability::Supported,
        };
        let required_inputs = match &action {
            Action::ObserveConfigState => &[RequiredInput::ValidatedConfig][..],
            Action::Pagefile
            | Action::Cs2Registry(_)
            | Action::Cs2Config
            | Action::NvidiaProfilePreparation
            | Action::NvidiaProfileApply => &[RequiredInput::GpuBranch][..],
            _ => &[],
        };
        let recovery_requirement = match &action {
            Action::NvidiaDriverRemoval | Action::NvidiaDriverInstall => {
                frametime_domain::RecoveryRequirement::ManualRecoveryAudit
            }
            _ => frametime_domain::RecoveryRequirement::LosslessBackup,
        };
        let evidence_requirement = match &action {
            Action::GpuDriverCleanPreparation
            | Action::NvidiaProfilePreparation
            | Action::MsiPreparation
            | Action::NicAffinityPreparation => EvidenceRequirement::DurableReceipt,
            _ => EvidenceRequirement::None,
        };
        Self {
            id: step.id,
            intent: step.intent,
            operation: step.operation,
            gpu_applicability: step.gpu_applicability,
            orchestration_role: step.orchestration_role,
            action,
            capability,
            required_inputs,
            recovery_requirement,
            evidence_requirement,
        }
    }
}

pub(crate) fn descriptor_for(step: &Step) -> Result<ActionDescriptor, String> {
    native_action_for(step.intent).map(|action| ActionDescriptor::new(step, action))
}

pub(crate) fn action_for(step: &Step) -> Result<Action, String> {
    descriptor_for(step).map(|descriptor| descriptor.action)
}

pub(crate) fn required_inputs_label(inputs: &[RequiredInput]) -> &'static str {
    match inputs {
        [] => "no additional native inputs",
        [RequiredInput::ValidatedConfig] => "a validated frametime.toml configuration",
        [RequiredInput::GpuBranch] => "a validated GPU branch",
        _ => "validated native inputs",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_catalog_intent_has_one_descriptor_with_its_domain_contract() {
        for step in frametime_domain::step_catalog() {
            let descriptor = descriptor_for(step).expect("current intent maps to descriptor");
            assert_eq!(descriptor.id, step.id);
            assert_eq!(descriptor.intent, step.intent);
            assert_eq!(descriptor.operation, step.operation);
            assert_eq!(descriptor.gpu_applicability, step.gpu_applicability);
            assert_eq!(descriptor.orchestration_role, step.orchestration_role);
        }
    }
}
