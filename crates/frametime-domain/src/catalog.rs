mod definitions;

pub use definitions::{
    ActionIntent, Depth, GpuApplicability, OperationKind, OrchestrationRole, Phase, Risk, Step,
    StepId,
};

pub const PHASE_ONE_BASELINE_BENCHMARK: StepId = StepId::new(Phase::One, 17);
pub const PHASE_ONE_XMP_EXPO_CHECK: StepId = StepId::new(Phase::One, 2);
pub const PHASE_ONE_RESIZABLE_BAR_CHECK: StepId = StepId::new(Phase::One, 9);
pub const PHASE_ONE_SAFE_MODE_HANDOFF: StepId = StepId::new(Phase::One, 38);
pub const PHASE_TWO_SAFE_BOOT_CLEAR: StepId = StepId::new(Phase::Two, 1);
pub const PHASE_TWO_DRIVER_CLEANUP: StepId = StepId::new(Phase::Two, 2);
pub const PHASE_TWO_PHASE_THREE_HANDOFF: StepId = StepId::new(Phase::Two, 3);
pub const PHASE_THREE_DRIVER_INSTALL: StepId = StepId::new(Phase::Three, 1);
pub const PHASE_THREE_FINAL_BENCHMARK: StepId = StepId::new(Phase::Three, 13);

macro_rules! s {
    ($p:ident,$n:literal,$c:literal,$t:literal,$tier:literal,$r:ident,$d:ident,$intent:path,$check:literal,$reboot:literal) => {
        Step {
            id: StepId::new(Phase::$p, $n),
            category: $c,
            title: $t,
            tier: $tier,
            risk: Risk::$r,
            depth: Depth::$d,
            check_only: $check,
            reboot: $reboot,
            intent: $intent,
            operation: operation_kind(Depth::$d),
            gpu_applicability: gpu_applicability(StepId::new(Phase::$p, $n)),
            orchestration_role: orchestration_role(StepId::new(Phase::$p, $n)),
        }
    };
}

pub static STEPS: [Step; 54] = [
    s!(
        One,
        1,
        "System",
        "Configuration",
        1,
        Safe,
        Setup,
        ActionIntent::Configuration,
        false,
        false
    ),
    s!(
        One,
        2,
        "Hardware",
        "XMP/EXPO Check",
        1,
        Safe,
        Check,
        ActionIntent::XmpExpoCheck,
        true,
        false
    ),
    s!(
        One,
        3,
        "GPU",
        "Shader Cache Health",
        1,
        Safe,
        Check,
        ActionIntent::ShaderCacheHealth,
        true,
        false
    ),
    s!(
        One,
        4,
        "Display",
        "Fullscreen Optimizations Check",
        1,
        Safe,
        Check,
        ActionIntent::FullscreenOptimizationsCheck,
        true,
        false
    ),
    s!(
        One,
        5,
        "GPU",
        "NVIDIA Driver Version Inventory",
        1,
        Safe,
        Check,
        ActionIntent::NvidiaDriverInventory,
        true,
        false
    ),
    s!(
        One,
        6,
        "System",
        "Power Policy Guidance",
        1,
        Safe,
        Check,
        ActionIntent::PowerPolicyGuidance,
        true,
        false
    ),
    s!(
        One,
        7,
        "GPU",
        "HAGS Check",
        2,
        Safe,
        Check,
        ActionIntent::HagsCheck,
        true,
        false
    ),
    s!(
        One,
        8,
        "System",
        "Pagefile",
        2,
        Moderate,
        Registry,
        ActionIntent::Pagefile,
        false,
        true
    ),
    s!(
        One,
        9,
        "GPU",
        "Resizable BAR",
        2,
        Safe,
        Check,
        ActionIntent::ResizableBarCheck,
        true,
        true
    ),
    s!(
        One,
        10,
        "System",
        "Windows Timer Defaults",
        3,
        Safe,
        Check,
        ActionIntent::WindowsTimerDefaults,
        true,
        false
    ),
    s!(
        One,
        11,
        "Display",
        "MPO Health Check",
        3,
        Safe,
        Check,
        ActionIntent::MpoHealthCheck,
        true,
        false
    ),
    s!(
        One,
        12,
        "System",
        "Game Mode",
        1,
        Safe,
        Registry,
        ActionIntent::GameMode,
        false,
        false
    ),
    s!(
        One,
        13,
        "System",
        "Background App Guidance",
        2,
        Safe,
        Check,
        ActionIntent::BackgroundAppGuidance,
        true,
        false
    ),
    s!(
        One,
        14,
        "System",
        "Autostart Inventory",
        2,
        Safe,
        Check,
        ActionIntent::AutostartInventory,
        true,
        false
    ),
    s!(
        One,
        15,
        "System",
        "Windows Update Status",
        3,
        Safe,
        Check,
        ActionIntent::WindowsUpdateStatus,
        true,
        false
    ),
    s!(
        One,
        16,
        "Network",
        "RSS Baseline",
        1,
        Safe,
        Network,
        ActionIntent::RssBaseline,
        false,
        true
    ),
    s!(
        One,
        17,
        "Benchmark",
        "Baseline Benchmark",
        1,
        Safe,
        Check,
        ActionIntent::BaselineBenchmark,
        true,
        false
    ),
    s!(
        One,
        18,
        "GPU",
        "GPU Driver Clean (prep)",
        1,
        Safe,
        Check,
        ActionIntent::GpuDriverCleanPreparation,
        true,
        false
    ),
    s!(
        One,
        19,
        "GPU",
        "NVIDIA Driver Download",
        1,
        Safe,
        Check,
        ActionIntent::NvidiaDriverDownloadPreparation,
        true,
        false
    ),
    s!(
        One,
        20,
        "GPU",
        "NVIDIA Profile (prep)",
        3,
        Safe,
        Check,
        ActionIntent::NvidiaProfilePreparation,
        true,
        false
    ),
    s!(
        One,
        21,
        "Hardware",
        "MSI Interrupts (prep)",
        2,
        Safe,
        Check,
        ActionIntent::MsiPreparation,
        true,
        false
    ),
    s!(
        One,
        22,
        "Network",
        "NIC Interrupt Affinity (prep)",
        3,
        Safe,
        Check,
        ActionIntent::NicAffinityPreparation,
        true,
        false
    ),
    s!(
        One,
        23,
        "System",
        "Fast Startup Check",
        2,
        Safe,
        Check,
        ActionIntent::FastStartupGuidance,
        true,
        false
    ),
    s!(
        One,
        24,
        "Hardware",
        "Dual-Channel RAM",
        1,
        Safe,
        Check,
        ActionIntent::MemoryTopology,
        true,
        false
    ),
    s!(
        One,
        25,
        "Network",
        "TCP Defaults Check",
        2,
        Safe,
        Check,
        ActionIntent::TcpDefaultsGuidance,
        true,
        false
    ),
    s!(
        One,
        26,
        "Display",
        "Fullscreen Presentation Defaults",
        2,
        Safe,
        Check,
        ActionIntent::FullscreenPresentationGuidance,
        true,
        false
    ),
    s!(
        One,
        27,
        "System",
        "Scheduler Defaults",
        2,
        Safe,
        Check,
        ActionIntent::MultimediaDefaultsGuidance,
        true,
        false
    ),
    s!(
        One,
        28,
        "System",
        "Timer Defaults",
        2,
        Safe,
        Check,
        ActionIntent::TimerRequestGuidance,
        true,
        false
    ),
    s!(
        One,
        29,
        "Input",
        "Pointer Preference Check",
        2,
        Safe,
        Check,
        ActionIntent::PointerPreferenceGuidance,
        true,
        false
    ),
    s!(
        One,
        30,
        "GPU",
        "CS2 GPU Preference",
        2,
        Moderate,
        Registry,
        ActionIntent::Cs2HighPerformanceGpu,
        false,
        false
    ),
    s!(
        One,
        31,
        "System",
        "Capture Feature Check",
        2,
        Safe,
        Check,
        ActionIntent::GameDvrGuidance,
        true,
        false
    ),
    s!(
        One,
        32,
        "System",
        "Overlay Conflict Check",
        2,
        Safe,
        Check,
        ActionIntent::OverlayGuidance,
        true,
        false
    ),
    s!(
        One,
        33,
        "Audio",
        "Audio Path Check",
        2,
        Safe,
        Check,
        ActionIntent::AudioStackGuidance,
        true,
        false
    ),
    s!(
        One,
        34,
        "CS2",
        "optimization.cfg (fps_max only)",
        2,
        Moderate,
        App,
        ActionIntent::Cs2Configuration,
        false,
        false
    ),
    s!(
        One,
        35,
        "System",
        "Chipset Driver Check",
        2,
        Safe,
        Check,
        ActionIntent::ChipsetDriverInventory,
        true,
        false
    ),
    s!(
        One,
        36,
        "Display",
        "Visual Effects + Auto HDR Check",
        3,
        Safe,
        Check,
        ActionIntent::VisualEffectsGuidance,
        true,
        false
    ),
    s!(
        One,
        37,
        "System",
        "Service Status Check",
        3,
        Safe,
        Check,
        ActionIntent::WindowsServicesGuidance,
        true,
        false
    ),
    s!(
        One,
        38,
        "System",
        "Driver Repair: Activate Safe Mode",
        3,
        Critical,
        Boot,
        ActionIntent::SafeModeHandoff,
        false,
        true
    ),
    s!(
        Two,
        1,
        "Boot",
        "Disable Safe Mode",
        1,
        Moderate,
        Boot,
        ActionIntent::ClearSafeBoot,
        false,
        true
    ),
    s!(
        Two,
        2,
        "GPU",
        "GPU Driver Clean Removal",
        1,
        Critical,
        Driver,
        ActionIntent::NvidiaDriverRemoval,
        false,
        true
    ),
    s!(
        Two,
        3,
        "System",
        "Register Phase 3 for next boot",
        1,
        Moderate,
        Registry,
        ActionIntent::PhaseThreeHandoff,
        false,
        true
    ),
    s!(
        Three,
        1,
        "GPU",
        "Install NVIDIA Driver",
        1,
        Moderate,
        Driver,
        ActionIntent::NvidiaDriverInstall,
        false,
        true
    ),
    s!(
        Three,
        2,
        "GPU",
        "MSI Interrupt Guidance",
        2,
        Safe,
        Check,
        ActionIntent::MsiInterrupts,
        true,
        false
    ),
    s!(
        Three,
        3,
        "Network",
        "NIC Interrupt-Affinity Guidance",
        3,
        Safe,
        Check,
        ActionIntent::NicInterruptAffinity,
        true,
        false
    ),
    s!(
        Three,
        4,
        "GPU",
        "NVIDIA DRS Profile",
        3,
        Safe,
        Driver,
        ActionIntent::NvidiaProfileApply,
        false,
        false
    ),
    s!(
        Three,
        5,
        "CS2",
        "FPS Cap Info",
        1,
        Safe,
        Check,
        ActionIntent::FpsCapInfo,
        true,
        false
    ),
    s!(
        Three,
        6,
        "CS2",
        "Launch Options + Video Guidance",
        2,
        Safe,
        App,
        ActionIntent::Cs2LaunchVideoGuidance,
        true,
        false
    ),
    s!(
        Three,
        7,
        "Security",
        "VBS / Core Isolation Check",
        2,
        Safe,
        Check,
        ActionIntent::VbsHvciGuidance,
        true,
        false
    ),
    s!(
        Three,
        8,
        "GPU",
        "AMD GPU Settings",
        2,
        Safe,
        Check,
        ActionIntent::AmdRadeonGuidance,
        true,
        false
    ),
    s!(
        Three,
        9,
        "Network",
        "DNS Resolver Check",
        3,
        Safe,
        Check,
        ActionIntent::DnsGuidance,
        true,
        false
    ),
    s!(
        Three,
        10,
        "CPU",
        "Process Priority Defaults",
        3,
        Safe,
        Check,
        ActionIntent::ProcessPriorityGuidance,
        true,
        false
    ),
    s!(
        Three,
        11,
        "System",
        "VRAM Usage Review",
        2,
        Safe,
        Check,
        ActionIntent::VramUsageGuidance,
        true,
        false
    ),
    s!(
        Three,
        12,
        "System",
        "Final Checklist",
        1,
        Safe,
        Check,
        ActionIntent::FinalChecklistGuidance,
        true,
        false
    ),
    s!(
        Three,
        13,
        "Benchmark",
        "Final Benchmark + FPS Strategy",
        1,
        Safe,
        Check,
        ActionIntent::FinalBenchmark,
        true,
        false
    ),
];

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

#[cfg(test)]
mod tests;
