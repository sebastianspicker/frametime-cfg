#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandName {
    Bcdedit,
    Powercfg,
    Pnputil,
    Fsutil,
    Defrag,
    Netsh,
}
const COMMAND_ALLOWLIST: [CommandName; 6] = [
    CommandName::Bcdedit,
    CommandName::Powercfg,
    CommandName::Pnputil,
    CommandName::Fsutil,
    CommandName::Defrag,
    CommandName::Netsh,
];
#[cfg(any(test, windows))]
impl CommandName {
    const fn program(self) -> &'static str {
        match self {
            Self::Bcdedit => "bcdedit.exe",
            Self::Powercfg => "powercfg.exe",
            Self::Pnputil => "pnputil.exe",
            Self::Fsutil => "fsutil.exe",
            Self::Defrag => "defrag.exe",
            Self::Netsh => "netsh.exe",
        }
    }

    fn from_program(program: &str) -> Result<Self, String> {
        match program {
            "bcdedit.exe" => Ok(Self::Bcdedit),
            "powercfg.exe" => Ok(Self::Powercfg),
            "pnputil.exe" => Ok(Self::Pnputil),
            "fsutil.exe" => Ok(Self::Fsutil),
            "defrag.exe" => Ok(Self::Defrag),
            "netsh.exe" => Ok(Self::Netsh),
            _ => Err("system tool is not allowlisted".into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandVector {
    command: CommandName,
    arguments: Vec<String>,
}
impl CommandVector {
    fn new(command: CommandName, arguments: &[&str]) -> Result<Self, String> {
        if !COMMAND_ALLOWLIST.contains(&command) {
            return Err("command is not allowlisted".into());
        }
        if arguments.iter().any(|value| {
            value.is_empty()
                || value.contains('\0')
                || value.contains('|')
                || value.contains(';')
                || value.contains('\n')
                || value.contains('\r')
        }) {
            return Err("unsafe external-tool argument".into());
        }
        Ok(Self {
            command,
            arguments: arguments.iter().map(|value| (*value).to_owned()).collect(),
        })
    }
    fn run(&self) -> Result<String, String> {
        execute_allowlisted(self.command, &self.arguments)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    ObserveConfigState,
    ObserveGpuInventory,
    ObserveChipsetDriver,
    ObserveMemoryTopology,
    BaselineBenchmark,
    FinalBenchmark,
    FpsCapInfo,
    Hags,
    GpuDriverCleanPreparation,
    NvidiaDriverDownloadPreparation,
    NvidiaDriverRemoval,
    NvidiaDriverInstall,
    NvidiaProfilePreparation,
    NvidiaProfileApply,
    SafeModeHandoff,
    PhaseThreeHandoff,
    MsiPreparation,
    NicAffinityPreparation,
    NetworkStack,
    Cs2LaunchVideoGuide,
    AmdRadeonGuide,
    VramUsageGuide,
    FinalChecklistGuide,
    RegistryBatch(Vec<RegistryChange>),
    VbsHvciBatch(Vec<RegistryChange>),
    ProcessPriority(RegistryChange),
    Nagle,
    Dns,
    MsiInterrupts,
    NicInterruptAffinity,
    Autostart,
    PowerPlan,
    Pagefile,
    ShaderCache,
    Debloat,
    Cs2Registry(Cs2RegistryAction),
    Cs2Config,
    DynamicTick,
    ServiceBatch(ServiceBatch),
    Tool(CommandVector),
    Advisory(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceBatch {
    WindowsUpdate,
    SysMainSearchQwaveXbox,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cs2RegistryAction {
    DisableFullscreenOptimizations,
    HighPerformanceGpu,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryChange {
    hive: Hive,
    key: &'static str,
    name: &'static str,
    value: RegValue,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hive {
    LocalMachine,
    CurrentUser,
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum RegValue {
    Dword(u32),
    String(&'static str),
    Binary(&'static [u8]),
}

fn process_priority_change() -> RegistryChange {
    RegistryChange {
        hive: Hive::LocalMachine,
        key: "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options\\cs2.exe\\PerfOptions",
        name: "CpuPriorityClass",
        value: RegValue::Dword(3),
    }
}

fn require_process_priority_change(change: &RegistryChange) -> Result<(), String> {
    if *change == process_priority_change() {
        Ok(())
    } else {
        Err("P3:10 registry transaction is not the exact CpuPriorityClass contract".into())
    }
}

fn native_action_for(phase: u8, step: u8) -> Result<Action, String> {
    match phase {
        1 => match step {
            1..=18 | 20..=22 | 24 | 35 => Ok(action_phase_one_observations(step)),
            19 | 23 | 25..=26 => Ok(action_phase_one_registry(step)),
            27..=28 => Ok(action_phase_one_multimedia(step)),
            29 => Ok(action_phase_one_mouse(step)),
            30..=34 => Ok(action_phase_one_game(step)),
            36..=38 => Ok(action_phase_one_services(step)),
            _ => Err(format!("unknown catalog step P{phase}:{step}")),
        },
        2 => action_phase_two(step),
        3 => match step {
            1..=13 => Ok(action_phase_three(step)),
            _ => Err(format!("unknown catalog step P{phase}:{step}")),
        },
        _ => Err(format!("unknown catalog step P{phase}:{step}")),
    }
}

fn action_phase_one_observations(step: u8) -> Action {
    match (1u8, step) {
        // Check-only actions either supply concrete observations or an exact
        // operator-facing safety guide. Remaining legacy rows fail closed.
        (1, 1) => Action::ObserveConfigState,
        (1, 2) => Action::Advisory(
            "XMP/EXPO observation requires authoritative SMBIOS memory-profile data",
        ),
        (1, 5) => Action::ObserveGpuInventory,
        (1, 9) => Action::Advisory("Resizable BAR observation requires PCIe capability inspection"),
        (1, 17) => Action::BaselineBenchmark,
        (1, 18) => Action::GpuDriverCleanPreparation,
        (1, 20) => Action::NvidiaProfilePreparation,
        (1, 21) => Action::MsiPreparation,
        (1, 22) => Action::NicAffinityPreparation,
        (1, 24) => Action::ObserveMemoryTopology,
        (1, 35) => Action::ObserveChipsetDriver,
        (1, 3) => Action::ShaderCache,
        (1, 4) => Action::Cs2Registry(Cs2RegistryAction::DisableFullscreenOptimizations),
        (1, 6) => Action::PowerPlan,
        (1, 7) => Action::Hags,
        (1, 8) => Action::Pagefile,
        (1, 10) => Action::DynamicTick,
        (1, 11) => registry_batch(vec![registry_change(
            Hive::LocalMachine,
            "SOFTWARE\\Microsoft\\Windows\\Dwm",
            "OverlayTestMode",
            RegValue::Dword(5),
        )]),
        (1, 12) => registry_batch(vec![
            registry_change(
                Hive::CurrentUser,
                "SOFTWARE\\Microsoft\\GameBar",
                "AllowAutoGameMode",
                RegValue::Dword(1),
            ),
            registry_change(
                Hive::CurrentUser,
                "SOFTWARE\\Microsoft\\GameBar",
                "AutoGameModeEnabled",
                RegValue::Dword(1),
            ),
        ]),
        (1, 13) => Action::Debloat,
        (1, 14) => Action::Autostart,
        (1, 15) => Action::ServiceBatch(ServiceBatch::WindowsUpdate),
        (1, 16) => Action::NetworkStack,
        _ => unreachable!(),
    }
}

fn action_phase_one_registry(step: u8) -> Action {
    match (1u8, step) {
        (1, 19) => Action::NvidiaDriverDownloadPreparation,
        (1, 23) => registry_batch(vec![registry_change(
            Hive::LocalMachine,
            "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Power",
            "HiberbootEnabled",
            RegValue::Dword(0),
        )]),
        (1, 25) => Action::Nagle,
        (1, 26) => registry_batch(vec![
            registry_change(
                Hive::CurrentUser,
                "System\\GameConfigStore",
                "GameDVR_DXGIHonorFSEWindowsCompatible",
                RegValue::Dword(1),
            ),
            registry_change(
                Hive::CurrentUser,
                "System\\GameConfigStore",
                "GameDVR_FSEBehavior",
                RegValue::Dword(2),
            ),
            registry_change(
                Hive::CurrentUser,
                "System\\GameConfigStore",
                "GameDVR_FSEBehaviorMode",
                RegValue::Dword(2),
            ),
            registry_change(
                Hive::CurrentUser,
                "System\\GameConfigStore",
                "GameDVR_HonorUserFSEBehaviorMode",
                RegValue::Dword(1),
            ),
        ]),
        _ => unreachable!(),
    }
}

fn action_phase_one_multimedia(step: u8) -> Action {
    match (1u8, step) {
        // This is intentionally one registry transaction: every value is captured before
        // any of the policy changes are written, and the batch is read back as a unit.
        // PowerThrottlingOff is omitted because this backend has no authoritative Intel
        // hybrid-CPU detector; guessing from a display adapter would be unsafe.
        (1, 27) => registry_batch(vec![
            registry_change(
                Hive::LocalMachine,
                "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile",
                "SystemResponsiveness",
                RegValue::Dword(10),
            ),
            registry_change(
                Hive::LocalMachine,
                "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile",
                "NoLazyMode",
                RegValue::Dword(1),
            ),
            registry_change(
                Hive::LocalMachine,
                "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile\\Tasks\\Games",
                "Priority",
                RegValue::Dword(6),
            ),
            registry_change(
                Hive::LocalMachine,
                "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile\\Tasks\\Games",
                "Scheduling Category",
                RegValue::String("High"),
            ),
            registry_change(
                Hive::LocalMachine,
                "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile\\Tasks\\Games",
                "GPU Priority",
                RegValue::Dword(8),
            ),
            registry_change(
                Hive::LocalMachine,
                "SYSTEM\\CurrentControlSet\\Control\\PriorityControl",
                "Win32PrioritySeparation",
                RegValue::Dword(0x2A),
            ),
            registry_change(
                Hive::LocalMachine,
                "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Memory Management",
                "DisablePagingExecutive",
                RegValue::Dword(1),
            ),
            registry_change(
                Hive::LocalMachine,
                "SOFTWARE\\Microsoft\\FTH",
                "Enabled",
                RegValue::Dword(0),
            ),
            registry_change(
                Hive::LocalMachine,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Device Installer",
                "DisableCoInstallers",
                RegValue::Dword(1),
            ),
            registry_change(
                Hive::LocalMachine,
                "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Schedule\\Maintenance",
                "MaintenanceDisabled",
                RegValue::Dword(1),
            ),
            registry_change(
                Hive::LocalMachine,
                "SYSTEM\\CurrentControlSet\\Control\\FileSystem",
                "NtfsDisableLastAccessUpdate",
                RegValue::Dword(0x8000_0001),
            ),
            registry_change(
                Hive::LocalMachine,
                "SYSTEM\\CurrentControlSet\\Control\\FileSystem",
                "NtfsDisable8dot3NameCreation",
                RegValue::Dword(1),
            ),
        ]),
        (1, 28) => registry_batch(vec![registry_change(
            Hive::LocalMachine,
            "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Kernel",
            "GlobalTimerResolutionRequests",
            RegValue::Dword(1),
        )]),
        _ => unreachable!(),
    }
}

fn action_phase_one_mouse(step: u8) -> Action {
    match (1u8, step) {
        (1, 29) => registry_batch(vec![
            registry_change(
                Hive::CurrentUser,
                "Control Panel\\Mouse",
                "MouseSpeed",
                RegValue::String("0"),
            ),
            registry_change(
                Hive::CurrentUser,
                "Control Panel\\Mouse",
                "MouseThreshold1",
                RegValue::String("0"),
            ),
            registry_change(
                Hive::CurrentUser,
                "Control Panel\\Mouse",
                "MouseThreshold2",
                RegValue::String("0"),
            ),
            registry_change(
                Hive::CurrentUser,
                "Control Panel\\Mouse",
                "SmoothMouseXCurve",
                RegValue::Binary(&FLAT_MOUSE_CURVE),
            ),
            registry_change(
                Hive::CurrentUser,
                "Control Panel\\Mouse",
                "SmoothMouseYCurve",
                RegValue::Binary(&FLAT_MOUSE_CURVE),
            ),
            registry_change(
                Hive::LocalMachine,
                "SYSTEM\\CurrentControlSet\\Services\\mouclass\\Parameters",
                "MouseDataQueueSize",
                RegValue::Dword(50),
            ),
        ]),
        _ => unreachable!(),
    }
}

fn action_phase_one_game(step: u8) -> Action {
    match (1u8, step) {
        (1, 30) => Action::Cs2Registry(Cs2RegistryAction::HighPerformanceGpu),
        (1, 31) => registry_batch(vec![
            registry_change(
                Hive::CurrentUser,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\GameDVR",
                "AppCaptureEnabled",
                RegValue::Dword(0),
            ),
            registry_change(
                Hive::CurrentUser,
                "SOFTWARE\\Microsoft\\GameBar",
                "UseNexusForGameBarEnabled",
                RegValue::Dword(0),
            ),
            registry_change(
                Hive::LocalMachine,
                "SOFTWARE\\Policies\\Microsoft\\Windows\\GameDVR",
                "AllowGameDVR",
                RegValue::Dword(0),
            ),
            registry_change(
                Hive::CurrentUser,
                "System\\GameConfigStore",
                "GameDVR_Enabled",
                RegValue::Dword(0),
            ),
        ]),
        (1, 32) => registry_batch(vec![registry_change(
            Hive::CurrentUser,
            "Software\\Valve\\Steam",
            "GameOverlayDisabled",
            RegValue::Dword(1),
        )]),
        (1, 33) => registry_batch(vec![registry_change(
            Hive::CurrentUser,
            "Software\\Microsoft\\Multimedia\\Audio",
            "UserDuckingPreference",
            RegValue::Dword(3),
        )]),
        (1, 34) => Action::Cs2Config,
        _ => unreachable!(),
    }
}

fn action_phase_one_services(step: u8) -> Action {
    match (1u8, step) {
        (1, 36) => registry_batch(vec![
            registry_change(
                Hive::CurrentUser,
                "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VisualEffects",
                "VisualFXSetting",
                RegValue::Dword(2),
            ),
            registry_change(
                Hive::CurrentUser,
                "Control Panel\\Desktop",
                "UserPreferencesMask",
                RegValue::Binary(&VISUAL_EFFECTS_MASK),
            ),
            registry_change(
                Hive::CurrentUser,
                "Control Panel\\Desktop",
                "FontSmoothing",
                RegValue::String("2"),
            ),
            registry_change(
                Hive::CurrentUser,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\VideoSettings",
                "AutoHDREnabled",
                RegValue::Dword(0),
            ),
        ]),
        (1, 37) => Action::ServiceBatch(ServiceBatch::SysMainSearchQwaveXbox),
        (1, 38) => Action::SafeModeHandoff,
        _ => unreachable!(),
    }
}

fn action_phase_two(step: u8) -> Result<Action, String> {
    let action = match (2u8, step) {
        // Phase 2 always clears and verifies SafeBoot before any driver action.
        (2, 1) => Action::Tool(CommandVector::new(
            CommandName::Bcdedit,
            &["/deletevalue", "{current}", "safeboot"],
        )?),
        (2, 2) => Action::NvidiaDriverRemoval,
        (2, 3) => Action::PhaseThreeHandoff,
        _ => return Err(format!("unknown catalog step P2:{step}")),
    };
    Ok(action)
}

fn action_phase_three(step: u8) -> Action {
    match (3u8, step) {
        (3, 1) => Action::NvidiaDriverInstall,
        (3, 2) => Action::MsiInterrupts,
        (3, 3) => Action::NicInterruptAffinity,
        (3, 4) => Action::NvidiaProfileApply,
        (3, 5) => Action::FpsCapInfo,
        // These legacy rows are operator guidance, not Steam launch-options
        // or video.txt writers. Keep the standalone VideoController separate.
        (3, 6) => Action::Cs2LaunchVideoGuide,
        (3, 8) => Action::AmdRadeonGuide,
        (3, 11) => Action::VramUsageGuide,
        (3, 12) => Action::FinalChecklistGuide,
        (3, 13) => Action::FinalBenchmark,
        (3, 7) => Action::VbsHvciBatch(vbs_hvci_batch()),
        (3, 9) => Action::Dns,
        // This is an image execution policy, not a live process mutation. It
        // intentionally has no X3D, affinity, task, or process-topology path.
        (3, 10) => Action::ProcessPriority(process_priority_change()),
        _ => unreachable!(),
    }
}
