use serde::{Deserialize, Serialize};

use crate::{Depth, Phase, Step};

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionKind {
    Setup,
    Inspect,
    Registry,
    Service,
    BootConfiguration,
    Driver,
    Network,
    Filesystem,
    ApplicationConfiguration,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedAction {
    pub phase: u8,
    pub step: u8,
    pub kind: ActionKind,
    pub title: &'static str,
    pub mutating: bool,
    pub applicable: bool,
    pub branch: GpuBranch,
}

#[must_use]
pub fn plan_for_step(step: &Step, branch: GpuBranch) -> PlannedAction {
    let kind = match step.depth {
        Depth::Setup => ActionKind::Setup,
        Depth::Check => ActionKind::Inspect,
        Depth::Registry => ActionKind::Registry,
        Depth::Service => ActionKind::Service,
        Depth::Boot => ActionKind::BootConfiguration,
        Depth::Driver => ActionKind::Driver,
        Depth::Network => ActionKind::Network,
        Depth::Filesystem => ActionKind::Filesystem,
        Depth::App => ActionKind::ApplicationConfiguration,
    };
    let nvidia_only = (step.phase == Phase::One && matches!(step.number, 5 | 19 | 20))
        || (step.phase == Phase::Three && matches!(step.number, 1 | 4));
    let amd_only = step.phase == Phase::Three && step.number == 8;
    let applicable =
        (!nvidia_only || branch.is_nvidia()) && (!amd_only || branch == GpuBranch::Amd);
    PlannedAction {
        phase: step.phase as u8,
        step: step.number,
        kind,
        title: step.title,
        mutating: !step.check_only,
        applicable,
        branch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::STEPS;

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
        let drs = &STEPS[44];
        assert!(plan_for_step(drs, GpuBranch::Nvidia).applicable);
        assert!(!plan_for_step(drs, GpuBranch::Amd).applicable);
        let amd = &STEPS[48];
        assert!(plan_for_step(amd, GpuBranch::Amd).applicable);
        assert!(!plan_for_step(amd, GpuBranch::IntelArc).applicable);
        let nvidia_driver_install = &STEPS[41];
        assert!(plan_for_step(nvidia_driver_install, GpuBranch::Nvidia).applicable);
        assert!(!plan_for_step(nvidia_driver_install, GpuBranch::Amd).applicable);
    }
}
