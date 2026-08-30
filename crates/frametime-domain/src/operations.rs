use serde::{Deserialize, Serialize};

use crate::{OperationKind, Step, StepId};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GpuBranch {
    NvidiaRtx5000 = 1,
    Nvidia = 2,
    Amd = 3,
    IntelArc = 4,
}

impl TryFrom<u8> for GpuBranch {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::NvidiaRtx5000),
            2 => Ok(Self::Nvidia),
            3 => Ok(Self::Amd),
            4 => Ok(Self::IntelArc),
            _ => Err("GPU branch must be 1, 2, 3, or 4"),
        }
    }
}

impl GpuBranch {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NvidiaRtx5000 => "NVIDIA RTX 5000",
            Self::Nvidia => "other NVIDIA",
            Self::Amd => "AMD Radeon",
            Self::IntelArc => "Intel Arc",
        }
    }

    #[must_use]
    pub const fn is_nvidia(self) -> bool {
        matches!(self, Self::NvidiaRtx5000 | Self::Nvidia)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedAction {
    pub id: StepId,
    pub kind: OperationKind,
    pub title: &'static str,
    pub mutating: bool,
    pub applicable: bool,
    pub branch: GpuBranch,
}

#[must_use]
pub fn plan_for_step(step: &Step, branch: GpuBranch) -> PlannedAction {
    PlannedAction {
        id: step.id,
        kind: step.operation,
        title: step.title,
        mutating: step.intent.is_mutating(),
        applicable: step.is_compatible_with_gpu(branch),
        branch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{PHASE_THREE_DRIVER_INSTALL, STEPS, step_by_id};

    #[test]
    fn every_step_has_a_typed_plan_in_every_branch() {
        for branch in [
            GpuBranch::NvidiaRtx5000,
            GpuBranch::Nvidia,
            GpuBranch::Amd,
            GpuBranch::IntelArc,
        ] {
            let plans = STEPS
                .iter()
                .map(|step| plan_for_step(step, branch))
                .collect::<Vec<_>>();
            assert_eq!(plans.len(), 54);
        }
    }

    #[test]
    fn mutually_exclusive_gpu_actions_are_explicit() {
        let drs = step_by_id(crate::StepId::new(crate::Phase::Three, 4)).expect("DRS step");
        assert!(plan_for_step(drs, GpuBranch::Nvidia).applicable);
        assert!(!plan_for_step(drs, GpuBranch::Amd).applicable);
        let amd = step_by_id(crate::StepId::new(crate::Phase::Three, 8)).expect("AMD step");
        assert!(plan_for_step(amd, GpuBranch::Amd).applicable);
        assert!(!plan_for_step(amd, GpuBranch::IntelArc).applicable);
        let nvidia_driver_install = step_by_id(PHASE_THREE_DRIVER_INSTALL).expect("driver step");
        assert!(plan_for_step(nvidia_driver_install, GpuBranch::Nvidia).applicable);
        assert!(!plan_for_step(nvidia_driver_install, GpuBranch::Amd).applicable);
    }
}
