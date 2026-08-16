fn verify_action(
    action: &Action,
    config: Option<&Config>,
    captured_services: Option<&[String]>,
    nagle_binding: Option<&NagleBinding>,
    cs2_binding: Option<&Cs2RegistryBinding>,
) -> Result<(), String> {
    match action {
        Action::RegistryBatch(changes) => {
            for change in changes {
                if registry_read(change)?.as_ref() != Some(&change.value) {
                    return Err("registry batch postcondition was not observed".into());
                }
            }
            Ok(())
        }
        Action::VbsHvciBatch(changes) => {
            let change = vbs_hvci_change(changes)?;
            if registry_read_exact(change)?.as_ref() == Some(&change.value) {
                Ok(())
            } else {
                Err("P3:7 HVCI registry postcondition was not observed exactly".into())
            }
        }
        Action::ProcessPriority(change) => {
            require_process_priority_change(change)?;
            if registry_read_exact(change)?.as_ref() == Some(&change.value) {
                Ok(())
            } else {
                Err("P3:10 CpuPriorityClass registry postcondition was not observed exactly".into())
            }
        }
        Action::Nagle => verify_nagle(
            nagle_binding.ok_or("Nagle verification requires a captured interface binding")?,
        ),
        Action::Dns => Err("P3:9 verification requires captured DNS bindings".into()),
        Action::MsiInterrupts | Action::NicInterruptAffinity => {
            Err("interrupt-policy verification requires captured exact device bindings".into())
        }
        Action::Autostart => Err("P1:14 verification requires a captured autostart binding".into()),
        Action::PowerPlan => Err("P1:6 verification requires a captured power-plan binding".into()),
        Action::Pagefile => {
            Err("P1:8 verification requires a captured CIM pagefile binding".into())
        }
        Action::ShaderCache => {
            Err("P1:3 verification requires its captured shader-cache inventory".into())
        }
        Action::Debloat => {
            Err("P1:13 verification requires its dedicated captured capability".into())
        }
        Action::DynamicTick => verify_disabledynamictick(true),
        Action::Cs2Registry(action) => verify_cs2_registry(
            cs2_binding.ok_or("CS2 registry verification requires a captured install binding")?,
            *action,
        ),
        Action::Cs2Config => {
            Err("P1:34 verification requires a captured CS2 config binding".into())
        }
        Action::ObserveConfigState
        | Action::ObserveGpuInventory
        | Action::ObserveChipsetDriver
        | Action::ObserveMemoryTopology
        | Action::BaselineBenchmark
        | Action::FinalBenchmark
        | Action::FpsCapInfo => {
            Err("check-only observations cannot be verified after mutation".into())
        }
        Action::GpuDriverCleanPreparation
        | Action::Hags
        | Action::NvidiaDriverDownloadPreparation
        | Action::NvidiaDriverRemoval
        | Action::NvidiaDriverInstall
        | Action::NvidiaProfilePreparation
        | Action::NvidiaProfileApply
        | Action::NetworkStack
        | Action::SafeModeHandoff
        | Action::PhaseThreeHandoff
        | Action::MsiPreparation
        | Action::NicAffinityPreparation
        | Action::Cs2LaunchVideoGuide
        | Action::AmdRadeonGuide
        | Action::VramUsageGuide
        | Action::FinalChecklistGuide => Ok(()),
        Action::Tool(command) => verify_tool(command),
        Action::ServiceBatch(batch) => {
            let names = captured_service_names(*batch, config, captured_services)?;
            native_services::verify_disabled_stopped(&names, *batch)
        }
        Action::Advisory(reason) => Err(reason.to_string()),
    }
}
