use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandName {
    Bcdedit,
    Powercfg,
    Pnputil,
    Fsutil,
    Defrag,
    Netsh,
}

pub(crate) const COMMAND_ALLOWLIST: [CommandName; 6] = [
    CommandName::Bcdedit,
    CommandName::Powercfg,
    CommandName::Pnputil,
    CommandName::Fsutil,
    CommandName::Defrag,
    CommandName::Netsh,
];

#[cfg(any(test, windows))]
impl CommandName {
    pub(crate) const fn program(self) -> &'static str {
        match self {
            Self::Bcdedit => "bcdedit.exe",
            Self::Powercfg => "powercfg.exe",
            Self::Pnputil => "pnputil.exe",
            Self::Fsutil => "fsutil.exe",
            Self::Defrag => "defrag.exe",
            Self::Netsh => "netsh.exe",
        }
    }
    pub(crate) fn from_program(program: &str) -> Result<Self, String> {
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
pub(crate) struct CommandVector {
    pub(crate) command: CommandName,
    pub(crate) arguments: Vec<String>,
}

impl CommandVector {
    pub(crate) fn new(command: CommandName, arguments: &[&str]) -> Result<Self, String> {
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
    pub(crate) fn run(&self) -> Result<String, String> {
        execute_allowlisted(self.command, &self.arguments)
    }
}

/// Windows-only execution detail selected by a domain-owned [`StepId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    ObserveConfigState,
    ObserveGpuInventory,
    ObserveChipsetDriver,
    ObserveMemoryTopology,
    BaselineBenchmark,
    FinalBenchmark,
    FpsCapInfo,
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
    MsiInterrupts,
    NicInterruptAffinity,
    Pagefile,
    Cs2Registry(Cs2RegistryAction),
    Cs2Config,
    Tool(CommandVector),
    Advisory(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cs2RegistryAction {
    HighPerformanceGpu,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegistryChange {
    pub(crate) hive: Hive,
    pub(crate) key: &'static str,
    pub(crate) name: &'static str,
    pub(crate) value: RegValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hive {
    LocalMachine,
    CurrentUser,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RegValue {
    Dword(u32),
    String(&'static str),
    Binary(&'static [u8]),
}

const XMP_EXPO_ADVISORY: &str =
    "XMP/EXPO observation requires authoritative SMBIOS memory-profile data";
const SHADER_CACHE_HEALTH_ADVISORY: &str = "P1:3 preserves shader caches. Clear only a diagnosed corrupt cache as a repair action, then compare cold and warmed runs separately.";
const FULLSCREEN_OPTIMIZATIONS_ADVISORY: &str = "P1:4 preserves Windows Fullscreen Optimizations. Use a per-executable opt-out only after a reproducible presentation regression and retain the prior value for recovery.";
const POWER_POLICY_ADVISORY: &str = "P1:6 preserves the active OEM or Windows power policy. Use the OEM gaming mode or Best performance only when plugged in and measurements show throttling.";
const HAGS_ADVISORY: &str = "P1:7 preserves the current supported HAGS state. Compare HAGS only as an explicit, reversible A/B experiment after a driver or Windows change.";
const RESIZABLE_BAR_ADVISORY: &str =
    "Resizable BAR observation requires PCIe capability inspection";
const WINDOWS_TIMER_DEFAULTS_ADVISORY: &str = "P1:10 preserves the Windows boot timer defaults. BCDEdit timer switches, including disabledynamictick, are debugging controls and are not gaming tweaks.";
const MPO_HEALTH_ADVISORY: &str = "P1:11 preserves Multiplane Overlay. Disable MPO only as a reversible workaround for a reproduced compositor, flicker, or presentation fault.";
const BACKGROUND_APPS_ADVISORY: &str = "P1:13 preserves installed applications. Use a temporary clean boot to identify a measured offender before removing software.";
const AUTOSTART_INVENTORY_ADVISORY: &str = "P1:14 inventories startup applications without removing user-selected entries. Disable only a measured offender with explicit consent.";
const WINDOWS_UPDATE_ADVISORY: &str = "P1:15 preserves Windows Update servicing. Schedule updates outside play rather than disabling update services.";
const FAST_STARTUP_ADVISORY: &str = "P1:23 preserves Fast Startup. Change it only to diagnose a reproduced shutdown, driver, or dual-boot problem.";
const TCP_DEFAULTS_ADVISORY: &str = "P1:25 preserves interface TCP defaults. TcpNoDelay and TcpAckFrequency do not tune CS2 UDP gameplay.";
const FULLSCREEN_PRESENTATION_ADVISORY: &str = "P1:26 preserves the Windows fullscreen-presentation defaults. Test a per-game exception only after measuring a regression with the current compositor and driver.";
const MULTIMEDIA_DEFAULTS_ADVISORY: &str = "P1:27 preserves the documented Windows scheduler, MMCSS, maintenance, memory-manager, and NTFS defaults. Diagnose a specific fault before a reversible, measured exception.";
const TIMER_REQUEST_ADVISORY: &str = "P1:28 preserves Windows timer-request defaults. Do not install a global timer registry policy as a gaming preset.";
const POINTER_PREFERENCE_ADVISORY: &str = "P1:29 preserves the user's Windows pointer preference. Acceleration off is a consistency preference, not a frametime optimization.";
const GAME_DVR_ADVISORY: &str = "P1:31 preserves capture features. Disable Game DVR only for a measured conflict or controlled benchmark session.";
const OVERLAY_ADVISORY: &str = "P1:32 preserves overlay preferences. Disable only a measured conflicting overlay, and keep capture tooling mutually exclusive.";
const AUDIO_STACK_ADVISORY: &str = "P1:33 preserves the current audio stack. Device, EQ, spatialization, and buffer choices require device-specific tests.";
const VISUAL_EFFECTS_ADVISORY: &str = "P1:36 preserves visual-effects and Auto HDR preferences. Select them for display consistency or measured cost, not as a universal latency preset.";
const WINDOWS_SERVICES_ADVISORY: &str = "P1:37 preserves Windows service defaults. Use a temporary clean boot or targeted diagnosis for a measured background offender rather than a service-disable preset.";
const VBS_HVCI_ADVISORY: &str = "P3:7 preserves VBS, HVCI, and Memory Integrity. Disabling kernel protections is not an optimization baseline.";
const DNS_ADVISORY: &str = "P3:9 preserves the configured resolver. DNS affects name resolution, not an established CS2 match flow.";
const PROCESS_PRIORITY_ADVISORY: &str = "P3:10 preserves Normal process priority. Do not force IFEO priority or affinity as a general CS2 preset; test an explicit, reversible exception with system-wide measurements.";

/// The adapter is keyed exclusively by the domain's typed action intent.
/// `StepId` remains the domain's progress and evidence identity.
pub(crate) fn native_action_for(intent: ActionIntent) -> Result<Action, String> {
    match intent {
        ActionIntent::Configuration => Ok(Action::ObserveConfigState),
        ActionIntent::XmpExpoCheck => Ok(Action::Advisory(XMP_EXPO_ADVISORY)),
        ActionIntent::ShaderCacheHealth => Ok(Action::Advisory(SHADER_CACHE_HEALTH_ADVISORY)),
        ActionIntent::FullscreenOptimizationsCheck => {
            Ok(Action::Advisory(FULLSCREEN_OPTIMIZATIONS_ADVISORY))
        }
        ActionIntent::NvidiaDriverInventory => Ok(Action::ObserveGpuInventory),
        ActionIntent::PowerPolicyGuidance => Ok(Action::Advisory(POWER_POLICY_ADVISORY)),
        ActionIntent::HagsCheck => Ok(Action::Advisory(HAGS_ADVISORY)),
        ActionIntent::Pagefile => Ok(Action::Pagefile),
        ActionIntent::ResizableBarCheck => Ok(Action::Advisory(RESIZABLE_BAR_ADVISORY)),
        ActionIntent::WindowsTimerDefaults => Ok(Action::Advisory(WINDOWS_TIMER_DEFAULTS_ADVISORY)),
        ActionIntent::MpoHealthCheck => Ok(Action::Advisory(MPO_HEALTH_ADVISORY)),
        ActionIntent::GameMode => Ok(registry_batch(vec![
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
        ])),
        ActionIntent::BackgroundAppGuidance => Ok(Action::Advisory(BACKGROUND_APPS_ADVISORY)),
        ActionIntent::AutostartInventory => Ok(Action::Advisory(AUTOSTART_INVENTORY_ADVISORY)),
        ActionIntent::WindowsUpdateStatus => Ok(Action::Advisory(WINDOWS_UPDATE_ADVISORY)),
        ActionIntent::RssBaseline => Ok(Action::NetworkStack),
        ActionIntent::BaselineBenchmark => Ok(Action::BaselineBenchmark),
        ActionIntent::GpuDriverCleanPreparation => Ok(Action::GpuDriverCleanPreparation),
        ActionIntent::NvidiaDriverDownloadPreparation => {
            Ok(Action::NvidiaDriverDownloadPreparation)
        }
        ActionIntent::NvidiaProfilePreparation => Ok(Action::NvidiaProfilePreparation),
        ActionIntent::MsiPreparation => Ok(Action::MsiPreparation),
        ActionIntent::NicAffinityPreparation => Ok(Action::NicAffinityPreparation),
        ActionIntent::FastStartupGuidance => Ok(Action::Advisory(FAST_STARTUP_ADVISORY)),
        ActionIntent::MemoryTopology => Ok(Action::ObserveMemoryTopology),
        ActionIntent::TcpDefaultsGuidance => Ok(Action::Advisory(TCP_DEFAULTS_ADVISORY)),
        ActionIntent::FullscreenPresentationGuidance => {
            Ok(Action::Advisory(FULLSCREEN_PRESENTATION_ADVISORY))
        }
        ActionIntent::MultimediaDefaultsGuidance => {
            Ok(Action::Advisory(MULTIMEDIA_DEFAULTS_ADVISORY))
        }
        ActionIntent::TimerRequestGuidance => Ok(Action::Advisory(TIMER_REQUEST_ADVISORY)),
        ActionIntent::PointerPreferenceGuidance => {
            Ok(Action::Advisory(POINTER_PREFERENCE_ADVISORY))
        }
        ActionIntent::Cs2HighPerformanceGpu => {
            Ok(Action::Cs2Registry(Cs2RegistryAction::HighPerformanceGpu))
        }
        ActionIntent::GameDvrGuidance => Ok(Action::Advisory(GAME_DVR_ADVISORY)),
        ActionIntent::OverlayGuidance => Ok(Action::Advisory(OVERLAY_ADVISORY)),
        ActionIntent::AudioStackGuidance => Ok(Action::Advisory(AUDIO_STACK_ADVISORY)),
        ActionIntent::Cs2Configuration => Ok(Action::Cs2Config),
        ActionIntent::ChipsetDriverInventory => Ok(Action::ObserveChipsetDriver),
        ActionIntent::VisualEffectsGuidance => Ok(Action::Advisory(VISUAL_EFFECTS_ADVISORY)),
        ActionIntent::WindowsServicesGuidance => Ok(Action::Advisory(WINDOWS_SERVICES_ADVISORY)),
        ActionIntent::SafeModeHandoff => Ok(Action::SafeModeHandoff),
        ActionIntent::ClearSafeBoot => Ok(Action::Tool(CommandVector::new(
            CommandName::Bcdedit,
            &["/deletevalue", "{current}", "safeboot"],
        )?)),
        ActionIntent::NvidiaDriverRemoval => Ok(Action::NvidiaDriverRemoval),
        ActionIntent::PhaseThreeHandoff => Ok(Action::PhaseThreeHandoff),
        ActionIntent::NvidiaDriverInstall => Ok(Action::NvidiaDriverInstall),
        ActionIntent::MsiInterrupts => Ok(Action::MsiInterrupts),
        ActionIntent::NicInterruptAffinity => Ok(Action::NicInterruptAffinity),
        ActionIntent::NvidiaProfileApply => Ok(Action::NvidiaProfileApply),
        ActionIntent::FpsCapInfo => Ok(Action::FpsCapInfo),
        ActionIntent::Cs2LaunchVideoGuidance => Ok(Action::Cs2LaunchVideoGuide),
        ActionIntent::VbsHvciGuidance => Ok(Action::Advisory(VBS_HVCI_ADVISORY)),
        ActionIntent::AmdRadeonGuidance => Ok(Action::AmdRadeonGuide),
        ActionIntent::DnsGuidance => Ok(Action::Advisory(DNS_ADVISORY)),
        ActionIntent::ProcessPriorityGuidance => Ok(Action::Advisory(PROCESS_PRIORITY_ADVISORY)),
        ActionIntent::VramUsageGuidance => Ok(Action::VramUsageGuide),
        ActionIntent::FinalChecklistGuidance => Ok(Action::FinalChecklistGuide),
        ActionIntent::FinalBenchmark => Ok(Action::FinalBenchmark),
    }
}

#[cfg(test)]
mod action_catalog_tests {
    use super::*;
    #[test]
    fn every_current_domain_step_has_exactly_one_native_action() {
        let resolved = frametime_domain::step_catalog()
            .iter()
            .map(|step| native_action_for(step.intent).map(|_| step.intent))
            .collect::<Result<Vec<_>, _>>()
            .expect("every domain step must resolve");
        assert_eq!(resolved.len(), frametime_domain::step_catalog().len());
        assert_eq!(
            resolved
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            resolved.len()
        );
    }
}
