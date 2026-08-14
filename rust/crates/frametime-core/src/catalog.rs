mod definitions;

pub use definitions::{Depth, Phase, Risk, Step};

macro_rules! s {
    ($p:ident,$n:literal,$c:literal,$t:literal,$tier:literal,$r:ident,$d:ident,$check:literal,$reboot:literal) => {
        Step {
            phase: Phase::$p,
            number: $n,
            category: $c,
            title: $t,
            tier: $tier,
            risk: Risk::$r,
            depth: Depth::$d,
            check_only: $check,
            reboot: $reboot,
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
        true,
        false
    ),
    s!(
        One,
        3,
        "GPU",
        "Clear Shader Cache",
        1,
        Safe,
        Filesystem,
        false,
        false
    ),
    s!(
        One,
        4,
        "Display",
        "Fullscreen Optimizations",
        1,
        Safe,
        Registry,
        false,
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
        true,
        false
    ),
    s!(
        One,
        6,
        "System",
        "frametime.cfg Power Plan",
        1,
        Moderate,
        Registry,
        false,
        false
    ),
    s!(One, 7, "GPU", "HAGS", 2, Moderate, Registry, false, true),
    s!(
        One, 8, "System", "Pagefile", 2, Moderate, Registry, false, true
    ),
    s!(One, 9, "GPU", "Resizable BAR", 2, Safe, Check, true, true),
    s!(
        One,
        10,
        "System",
        "Dynamic Tick",
        3,
        Moderate,
        Boot,
        false,
        true
    ),
    s!(
        One,
        11,
        "Display",
        "Disable MPO",
        3,
        Safe,
        Registry,
        false,
        true
    ),
    s!(
        One,
        12,
        "System",
        "Game Mode",
        3,
        Safe,
        Registry,
        false,
        false
    ),
    s!(
        One,
        13,
        "System",
        "Gaming Debloat",
        2,
        Moderate,
        App,
        false,
        false
    ),
    s!(
        One,
        14,
        "System",
        "Autostart Cleanup",
        2,
        Safe,
        Registry,
        false,
        false
    ),
    s!(
        One,
        15,
        "System",
        "Windows Update Blocker",
        3,
        Critical,
        Service,
        false,
        false
    ),
    s!(
        One,
        16,
        "Network",
        "NIC Latency Stack",
        2,
        Moderate,
        Network,
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
        Filesystem,
        false,
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
        true,
        false
    ),
    s!(
        One,
        23,
        "System",
        "Disable Fast Startup",
        2,
        Safe,
        Registry,
        false,
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
        true,
        false
    ),
    s!(
        One,
        25,
        "Network",
        "Disable Nagle",
        2,
        Safe,
        Registry,
        false,
        false
    ),
    s!(
        One,
        26,
        "Display",
        "GameConfigStore FSE",
        2,
        Safe,
        Registry,
        false,
        false
    ),
    s!(
        One,
        27,
        "System",
        "MMCSS + Gaming Priority",
        2,
        Safe,
        Registry,
        false,
        false
    ),
    s!(
        One,
        28,
        "System",
        "Timer Resolution",
        2,
        Safe,
        Registry,
        false,
        false
    ),
    s!(
        One,
        29,
        "Input",
        "Mouse Acceleration Off",
        2,
        Safe,
        Registry,
        false,
        false
    ),
    s!(
        One,
        30,
        "GPU",
        "CS2 GPU Preference",
        2,
        Safe,
        Registry,
        false,
        false
    ),
    s!(
        One,
        31,
        "System",
        "Disable Game DVR",
        2,
        Safe,
        Registry,
        false,
        false
    ),
    s!(
        One,
        32,
        "System",
        "Disable Overlays",
        2,
        Safe,
        Registry,
        false,
        false
    ),
    s!(
        One,
        33,
        "Audio",
        "Audio Optimization",
        2,
        Safe,
        Registry,
        false,
        false
    ),
    s!(
        One,
        34,
        "CS2",
        "optimization.cfg (73 CVars)",
        2,
        Safe,
        App,
        true,
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
        true,
        false
    ),
    s!(
        One,
        36,
        "Display",
        "Visual Effects + Auto HDR",
        3,
        Safe,
        Registry,
        false,
        false
    ),
    s!(
        One,
        37,
        "System",
        "Disable SysMain + Windows Search",
        3,
        Moderate,
        Service,
        false,
        false
    ),
    s!(
        One,
        38,
        "System",
        "Activate Safe Mode",
        1,
        Moderate,
        Boot,
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
        false,
        true
    ),
    s!(
        Three,
        2,
        "GPU",
        "MSI Interrupts",
        2,
        Moderate,
        Registry,
        false,
        true
    ),
    s!(
        Three,
        3,
        "Network",
        "NIC Interrupt Affinity",
        3,
        Moderate,
        Registry,
        false,
        true
    ),
    s!(
        Three,
        4,
        "GPU",
        "NVIDIA DRS Profile",
        3,
        Safe,
        Driver,
        false,
        false
    ),
    s!(Three, 5, "CS2", "FPS Cap Info", 1, Safe, Check, true, false),
    s!(
        Three,
        6,
        "CS2",
        "Launch Options + Video",
        2,
        Safe,
        App,
        true,
        false
    ),
    s!(
        Three,
        7,
        "Security",
        "VBS / Core Isolation",
        2,
        Moderate,
        Registry,
        false,
        true
    ),
    s!(
        Three,
        8,
        "GPU",
        "AMD GPU Settings",
        2,
        Safe,
        Check,
        true,
        false
    ),
    s!(
        Three,
        9,
        "Network",
        "DNS Configuration",
        3,
        Safe,
        Network,
        false,
        false
    ),
    s!(
        Three,
        10,
        "CPU",
        "Process Priority + X3D CCD",
        3,
        Safe,
        Registry,
        false,
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
        true,
        false
    ),
    s!(
        Three,
        13,
        "Benchmark",
        "Final Benchmark + FPS Cap",
        1,
        Safe,
        Check,
        true,
        false
    ),
];

#[must_use]
pub fn step_catalog() -> &'static [Step; 54] {
    &STEPS
}

#[cfg(test)]
mod tests;
