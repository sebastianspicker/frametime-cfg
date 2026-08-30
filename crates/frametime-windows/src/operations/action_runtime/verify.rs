use crate::*;

pub(crate) fn verify_action(
    action: &Action,
    cs2_binding: Option<&Cs2RegistryBinding>,
) -> Result<(), String> {
    match action {
        Action::RegistryBatch(changes) => {
            for change in changes {
                if registry_read_exact(change)?.as_ref() != Some(&change.value) {
                    return Err("registry postcondition was not observed".into());
                }
            }
            Ok(())
        }
        Action::Cs2Registry(action) => verify_cs2_registry(
            cs2_binding.ok_or("CS2 registry verification requires a captured install binding")?,
            *action,
        ),
        Action::Tool(command) => verify_tool(command),
        Action::Pagefile
        | Action::Cs2Config
        | Action::MsiInterrupts
        | Action::NicInterruptAffinity
        | Action::NvidiaProfileApply
        | Action::NetworkStack => {
            Err("native action verification requires its captured platform binding".into())
        }
        Action::Advisory(reason) => Err(reason.to_string()),
        Action::ObserveConfigState
        | Action::ObserveGpuInventory
        | Action::ObserveChipsetDriver
        | Action::ObserveMemoryTopology
        | Action::BaselineBenchmark
        | Action::FinalBenchmark
        | Action::FpsCapInfo
        | Action::GpuDriverCleanPreparation
        | Action::NvidiaDriverDownloadPreparation
        | Action::NvidiaDriverRemoval
        | Action::NvidiaDriverInstall
        | Action::NvidiaProfilePreparation
        | Action::SafeModeHandoff
        | Action::PhaseThreeHandoff
        | Action::MsiPreparation
        | Action::NicAffinityPreparation
        | Action::Cs2LaunchVideoGuide
        | Action::AmdRadeonGuide
        | Action::VramUsageGuide
        | Action::FinalChecklistGuide => Ok(()),
    }
}
