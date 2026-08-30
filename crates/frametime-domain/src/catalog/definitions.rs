use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    One = 1,
    Two = 2,
    Three = 3,
}

impl Phase {
    #[must_use]
    pub const fn number(self) -> u8 {
        self as u8
    }
}

/// Stable identity for one catalog action.
///
/// Its textual form remains `P<phase>:<number>` so persisted progress files
/// stay compatible with earlier releases.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StepId {
    pub phase: Phase,
    pub number: u8,
}

impl StepId {
    #[must_use]
    pub const fn new(phase: Phase, number: u8) -> Self {
        Self { phase, number }
    }

    #[must_use]
    pub fn progress_key(self) -> String {
        format!("P{}:{}", self.phase.number(), self.number)
    }

    #[must_use]
    pub fn from_progress_key(value: &str) -> Option<Self> {
        let (phase, number) = value.strip_prefix('P')?.split_once(':')?;
        let phase = match phase {
            "1" => Phase::One,
            "2" => Phase::Two,
            "3" => Phase::Three,
            _ => return None,
        };
        let number = number.parse().ok().filter(|number: &u8| *number > 0)?;
        Some(Self::new(phase, number))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ActionIntent {
    Configuration,
    XmpExpoCheck,
    ShaderCacheHealth,
    FullscreenOptimizationsCheck,
    NvidiaDriverInventory,
    PowerPolicyGuidance,
    HagsCheck,
    Pagefile,
    ResizableBarCheck,
    WindowsTimerDefaults,
    MpoHealthCheck,
    GameMode,
    BackgroundAppGuidance,
    AutostartInventory,
    WindowsUpdateStatus,
    RssBaseline,
    BaselineBenchmark,
    GpuDriverCleanPreparation,
    NvidiaDriverDownloadPreparation,
    NvidiaProfilePreparation,
    MsiPreparation,
    NicAffinityPreparation,
    FastStartupGuidance,
    MemoryTopology,
    TcpDefaultsGuidance,
    FullscreenPresentationGuidance,
    MultimediaDefaultsGuidance,
    TimerRequestGuidance,
    PointerPreferenceGuidance,
    Cs2HighPerformanceGpu,
    GameDvrGuidance,
    OverlayGuidance,
    AudioStackGuidance,
    Cs2Configuration,
    ChipsetDriverInventory,
    VisualEffectsGuidance,
    WindowsServicesGuidance,
    SafeModeHandoff,
    ClearSafeBoot,
    NvidiaDriverRemoval,
    PhaseThreeHandoff,
    NvidiaDriverInstall,
    MsiInterrupts,
    NicInterruptAffinity,
    NvidiaProfileApply,
    FpsCapInfo,
    Cs2LaunchVideoGuidance,
    VbsHvciGuidance,
    AmdRadeonGuidance,
    DnsGuidance,
    ProcessPriorityGuidance,
    VramUsageGuidance,
    FinalChecklistGuidance,
    FinalBenchmark,
}

impl ActionIntent {
    #[must_use]
    pub const fn is_mutating(self) -> bool {
        matches!(
            self,
            Self::Configuration
                | Self::Pagefile
                | Self::GameMode
                | Self::RssBaseline
                | Self::Cs2HighPerformanceGpu
                | Self::Cs2Configuration
                | Self::SafeModeHandoff
                | Self::ClearSafeBoot
                | Self::NvidiaDriverRemoval
                | Self::PhaseThreeHandoff
                | Self::NvidiaDriverInstall
                | Self::NvidiaProfileApply
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum OperationKind {
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GpuApplicability {
    Any,
    NvidiaOnly,
    AmdOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrchestrationRole {
    Engine,
    ArmSafeModeHandoff,
    ClearSafeBoot,
    ArmPhaseThreeHandoff,
    PersistFinalBenchmark,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "UPPERCASE")]
pub enum Risk {
    Safe,
    Moderate,
    Aggressive,
    Critical,
}

impl Risk {
    pub(crate) const fn rank(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Depth {
    Setup,
    Check,
    Registry,
    Service,
    Boot,
    Driver,
    Network,
    Filesystem,
    App,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Step {
    /// The authoritative schedule identity used by domain and application code.
    pub id: StepId,
    pub category: &'static str,
    pub title: &'static str,
    pub tier: u8,
    pub risk: Risk,
    pub depth: Depth,
    pub check_only: bool,
    pub reboot: bool,
    pub intent: ActionIntent,
    pub operation: OperationKind,
    pub gpu_applicability: GpuApplicability,
    pub orchestration_role: OrchestrationRole,
}

impl Step {
    #[must_use]
    pub const fn is_compatible_with_gpu(self, branch: crate::operations::GpuBranch) -> bool {
        match self.gpu_applicability {
            GpuApplicability::Any => true,
            GpuApplicability::NvidiaOnly => branch.is_nvidia(),
            GpuApplicability::AmdOnly => matches!(branch, crate::operations::GpuBranch::Amd),
        }
    }
}
