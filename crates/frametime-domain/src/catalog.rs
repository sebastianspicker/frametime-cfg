mod definitions;
#[macro_use]
mod row_macro;
mod rows;

pub use definitions::{
    ActionIntent, Depth, GpuApplicability, OperationKind, OrchestrationRole, Phase, Risk, Step,
    StepId,
};
pub use rows::STEPS;

pub const PHASE_ONE_BASELINE_BENCHMARK: StepId = StepId::new(Phase::One, 17);
pub const PHASE_ONE_XMP_EXPO_CHECK: StepId = StepId::new(Phase::One, 2);
pub const PHASE_ONE_RESIZABLE_BAR_CHECK: StepId = StepId::new(Phase::One, 9);
pub const PHASE_ONE_SAFE_MODE_HANDOFF: StepId = StepId::new(Phase::One, 38);
pub const PHASE_TWO_SAFE_BOOT_CLEAR: StepId = StepId::new(Phase::Two, 1);
pub const PHASE_TWO_DRIVER_CLEANUP: StepId = StepId::new(Phase::Two, 2);
pub const PHASE_TWO_PHASE_THREE_HANDOFF: StepId = StepId::new(Phase::Two, 3);
pub const PHASE_THREE_DRIVER_INSTALL: StepId = StepId::new(Phase::Three, 1);
pub const PHASE_THREE_FINAL_BENCHMARK: StepId = StepId::new(Phase::Three, 13);

#[must_use]
pub fn step_catalog() -> &'static [Step; 54] {
    &STEPS
}

#[must_use]
pub fn step_by_id(id: StepId) -> Option<&'static Step> {
    STEPS.iter().find(|step| step.id == id)
}

pub fn steps_for_phase_and_role(
    phase: Phase,
    role: OrchestrationRole,
) -> impl Iterator<Item = &'static Step> {
    STEPS
        .iter()
        .filter(move |step| step.id.phase == phase && step.orchestration_role == role)
}

const fn operation_kind(depth: Depth) -> OperationKind {
    match depth {
        Depth::Setup => OperationKind::Setup,
        Depth::Check => OperationKind::Inspect,
        Depth::Registry => OperationKind::Registry,
        Depth::Service => OperationKind::Service,
        Depth::Boot => OperationKind::BootConfiguration,
        Depth::Driver => OperationKind::Driver,
        Depth::Network => OperationKind::Network,
        Depth::Filesystem => OperationKind::Filesystem,
        Depth::App => OperationKind::ApplicationConfiguration,
    }
}

const fn gpu_applicability(id: StepId) -> GpuApplicability {
    match (id.phase, id.number) {
        (Phase::One, 5 | 19 | 20) | (Phase::Three, 1 | 4) => GpuApplicability::NvidiaOnly,
        (Phase::Three, 8) => GpuApplicability::AmdOnly,
        _ => GpuApplicability::Any,
    }
}

const fn orchestration_role(id: StepId) -> OrchestrationRole {
    match id {
        PHASE_ONE_SAFE_MODE_HANDOFF => OrchestrationRole::ArmSafeModeHandoff,
        PHASE_TWO_SAFE_BOOT_CLEAR => OrchestrationRole::ClearSafeBoot,
        PHASE_TWO_PHASE_THREE_HANDOFF => OrchestrationRole::ArmPhaseThreeHandoff,
        PHASE_THREE_FINAL_BENCHMARK => OrchestrationRole::PersistFinalBenchmark,
        _ => OrchestrationRole::Engine,
    }
}
