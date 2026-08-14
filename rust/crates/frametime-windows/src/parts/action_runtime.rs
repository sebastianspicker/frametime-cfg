const WINDOWS_UPDATE_SERVICES: [&str; 3] = ["wuauserv", "UsoSvc", "WaaSMedicSvc"];
const SYSTEM_SERVICE_BASE: [&str; 3] = ["SysMain", "WSearch", "qWave"];
const XBOX_SERVICE_IDENTITIES: [&str; 4] = [
    "XblAuthManager",
    "XblGameSave",
    "XboxNetApiSvc",
    "XboxGipSvc",
];

/// Authoritative native mapping of the two legacy service groups.  The only
/// configurable names are the fixed Xbox service identities; arbitrary
/// config-provided service names are rejected before any SCM call.
fn service_power_contract_map(
    batch: ServiceBatch,
    config: Option<&Config>,
) -> Result<Vec<String>, String> {
    match batch {
        ServiceBatch::WindowsUpdate => Ok(WINDOWS_UPDATE_SERVICES
            .iter()
            .map(|name| (*name).to_owned())
            .collect()),
        ServiceBatch::SysMainSearchQwaveXbox => {
            let config = config.ok_or("P1:37 requires validated frametime.toml Xbox services")?;
            let mut names = SYSTEM_SERVICE_BASE
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>();
            for name in &config.xbox_services {
                if !XBOX_SERVICE_IDENTITIES.contains(&name.as_str()) {
                    return Err(format!(
                        "P1:37 config Xbox service is not an exact catalog identity: {name}"
                    ));
                }
                if names.iter().any(|existing| existing == name) {
                    return Err(format!("P1:37 config contains duplicate service: {name}"));
                }
                names.push(name.clone());
            }
            Ok(names)
        }
    }
}

fn service_restore_binding(step: &str, name: &str) -> bool {
    match step {
        "P1:15" => WINDOWS_UPDATE_SERVICES.contains(&name),
        "P1:37" => SYSTEM_SERVICE_BASE.contains(&name) || XBOX_SERVICE_IDENTITIES.contains(&name),
        "P1:13" => matches!(name, "DiagTrack" | "dmwappushservice"),
        _ => false,
    }
}

const FLAT_MOUSE_CURVE: [u8; 40] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xA0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x40, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00,
];
const VISUAL_EFFECTS_MASK: [u8; 8] = [0x90, 0x12, 0x03, 0x80, 0x10, 0x00, 0x00, 0x00];

fn inspect_action(action: &Action) -> Result<Inspection, String> {
    if let Some(inspection) = inspect_guidance_action(action) {
        return inspection;
    }
    match action {
        Action::ObserveConfigState
        | Action::ObserveGpuInventory
        | Action::ObserveChipsetDriver
        | Action::ObserveMemoryTopology
        | Action::BaselineBenchmark
        | Action::FinalBenchmark
        | Action::FpsCapInfo
        | Action::Hags => Err("typed observations require live backend context".into()),
        Action::NvidiaDriverDownloadPreparation => Ok(Inspection::Satisfied),
        Action::NvidiaDriverRemoval | Action::NvidiaDriverInstall => {
            Err("driver execution requires the persisted P1:18/P1:19 NVIDIA transaction".into())
        }
        Action::NvidiaProfileApply => {
            Err("NVIDIA DRS application requires its dedicated native transaction".into())
        }
        Action::NetworkStack => Err("P1:16 requires its native transaction binding".into()),
        Action::SafeModeHandoff | Action::PhaseThreeHandoff => Ok(Inspection::Satisfied),
        Action::RegistryBatch(changes) => {
            for change in changes {
                let _ = registry_read(change)?;
            }
            Ok(Inspection::NeedsApply)
        }
        Action::VbsHvciBatch(changes) => inspect_vbs_hvci(changes),
        Action::ProcessPriority(change) => {
            require_process_priority_change(change)?;
            if registry_read_exact(change)?.as_ref() == Some(&change.value) {
                Ok(Inspection::Satisfied)
            } else {
                Ok(Inspection::NeedsApply)
            }
        }
        Action::Nagle => inspect_nagle(),
        Action::Dns => {
            Err("P3:9 inspection requires validated DNS config and native adapter discovery".into())
        }
        Action::MsiInterrupts | Action::NicInterruptAffinity => {
            Err("interrupt-policy inspection requires exact live device bindings".into())
        }
        Action::Autostart => Err("P1:14 inspection requires validated live config context".into()),
        Action::PowerPlan => {
            Err("P1:6 inspection requires a captured native power-plan binding".into())
        }
        Action::Pagefile => Err("P1:8 inspection requires native CIM inventory context".into()),
        Action::Cs2Registry(action) => inspect_cs2_registry(*action),
        Action::Cs2Config => {
            Err("P1:34 inspection requires its dedicated CS2 config binding".into())
        }
        Action::DynamicTick => Ok(Inspection::NeedsApply),
        Action::ShaderCache | Action::Debloat | Action::ServiceBatch(_) | Action::Tool(_) => {
            Ok(Inspection::NeedsApply)
        }
        Action::Advisory(reason) => Ok(Inspection::Advisory { reason }),
        Action::GpuDriverCleanPreparation
        | Action::NvidiaProfilePreparation
        | Action::MsiPreparation
        | Action::NicAffinityPreparation
        | Action::Cs2LaunchVideoGuide
        | Action::AmdRadeonGuide
        | Action::VramUsageGuide
        | Action::FinalChecklistGuide => unreachable!("guidance actions are handled above"),
    }
}

fn inspect_guidance_action(action: &Action) -> Option<Result<Inspection, String>> {
    let message = match action {
        Action::GpuDriverCleanPreparation => {
            "P1:18: confirm the exact target GPU and signed replacement driver before clean removal; Safe Mode and recovery must be prepared first. This workflow does not remove drivers, arm a handoff, or reboot."
        }
        Action::NvidiaProfilePreparation => {
            "P1:20 requires the native read-only NVIDIA DRS inspection adapter."
        }
        Action::MsiPreparation => {
            "P1:21: enable MSI only for specifically supported devices after recording the current state; reboot and verify negotiated mode, because a registry request does not prove MSI or MSI-X is active."
        }
        Action::NicAffinityPreparation => {
            "P1:22: set NIC affinity only after a reproducible NIC DPC diagnosis and authoritative logical-processor topology check; an unsuitable mask can increase latency or concentrate load."
        }
        Action::Cs2LaunchVideoGuide => {
            "P3:6: configure CS2 launch options and video settings manually; this workflow does not write Steam launch options or video.txt."
        }
        Action::AmdRadeonGuide => {
            "P3:8: review AMD Radeon settings manually; verify current AMD and game documentation, including anti-cheat compatibility, before enabling driver features. This workflow does not change firmware or AMD settings automatically."
        }
        Action::VramUsageGuide => {
            "P3:11: compare VRAM use with the same map, settings, and workload; allocation alone does not establish a leak."
        }
        Action::FinalChecklistGuide => {
            "P3:12: review the final checklist and validate results with comparable before/after captures."
        }
        _ => return None,
    };
    println!("{message}");
    Some(Ok(Inspection::Satisfied))
}

fn inspect_timer_resolution(action: &Action) -> Result<Inspection, String> {
    match platform::build_number() {
        Ok(build) => timer_resolution_inspection_for_build(Some(build), action),
        Err(_) => Ok(Inspection::Unsupported),
    }
}

fn timer_resolution_inspection_for_build(
    build: Option<u32>,
    action: &Action,
) -> Result<Inspection, String> {
    match build {
        Some(build) if build >= 19_041 => inspect_action(action),
        Some(_) => Ok(Inspection::Inapplicable),
        None => Ok(Inspection::Unsupported),
    }
}

fn require_timer_resolution_build() -> Result<(), String> {
    let build = platform::build_number()?;
    if build >= 19_041 {
        Ok(())
    } else {
        Err(format!(
            "GlobalTimerResolutionRequests requires Windows build 19041 or later; observed {build}"
        ))
    }
}

fn capture_actions(
    action: &Action,
    step: String,
    config: Option<&Config>,
) -> Result<Vec<BackupEntry>, String> {
    if let Action::RegistryBatch(changes) | Action::VbsHvciBatch(changes) = action {
        return changes
            .iter()
            .map(|change| capture_registry(change, step.clone()))
            .collect();
    }
    if let Action::ServiceBatch(batch) = action {
        return capture_service_batch(*batch, step, config);
    }
    capture_action(action, step).map(|entry| vec![entry])
}

fn capture_service_batch(
    batch: ServiceBatch,
    step: String,
    config: Option<&Config>,
) -> Result<Vec<BackupEntry>, String> {
    let names = service_power_contract_map(batch, config)?;
    let snapshots = native_services::capture_present(&names)?;
    if snapshots.is_empty() {
        return Err("no exact contract service is present to capture".into());
    }
    snapshots
        .into_iter()
        .map(|snapshot| {
            if !service_restore_binding(&step, &snapshot.name) {
                return Err("service capture is not an exact catalog binding".into());
            }
            Ok(BackupEntry::Service {
                step: step.clone(),
                timestamp: timestamp(),
                name: snapshot.name,
                original_start_type: snapshot.start_type,
                delayed_auto_start: snapshot.delayed_auto_start,
                original_status: snapshot.status,
                unknown: BTreeMap::new(),
            })
        })
        .collect()
}

fn capture_action(action: &Action, step: String) -> Result<BackupEntry, String> {
    match action {
        Action::ProcessPriority(change) => capture_process_priority(change, step),
        Action::RegistryBatch(_) | Action::VbsHvciBatch(_) => {
            Err("registry batch capture is handled by capture_actions".into())
        }
        Action::ServiceBatch(_)
        | Action::Debloat
        | Action::Nagle
        | Action::Dns
        | Action::MsiInterrupts
        | Action::NicInterruptAffinity
        | Action::Autostart
        | Action::PowerPlan
        | Action::Pagefile
        | Action::Cs2Registry(_)
        | Action::Cs2Config
        | Action::ShaderCache => Err("service batch capture is handled by capture_actions".into()),
        Action::Tool(command) if command.command == CommandName::Bcdedit => {
            let current =
                CommandVector::new(CommandName::Bcdedit, &["/enum", "{current}"])?.run()?;
            let safe_boot = current.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("safeboot")
                    .map(|value| value.trim().to_owned())
            });
            Ok(BackupEntry::Bootconfig {
                step,
                timestamp: timestamp(),
                key: "safeboot".into(),
                original_value: safe_boot.clone().map(Value::String).unwrap_or(Value::Null),
                existed: safe_boot.is_some(),
                unknown: BTreeMap::new(),
            })
        }
        Action::DynamicTick => {
            let output =
                CommandVector::new(CommandName::Bcdedit, &["/enum", "{current}", "/v"])?.run()?;
            let original = disabledynamictick_from_bcd(&output)?;
            Ok(BackupEntry::Bootconfig {
                step,
                timestamp: timestamp(),
                key: "disabledynamictick".into(),
                original_value: original.map(Value::Bool).unwrap_or(Value::Null),
                existed: original.is_some(),
                unknown: BTreeMap::new(),
            })
        }
        Action::Tool(command) if command.command == CommandName::Powercfg => {
            let active = CommandVector::new(CommandName::Powercfg, &["/getactivescheme"])?.run()?;
            let original_guid = active
                .split_whitespace()
                .find(|word| validate_power_plan_guid(word).is_ok())
                .ok_or("could not parse active power-plan GUID")?
                .to_owned();
            Ok(BackupEntry::Powerplan {
                step,
                timestamp: timestamp(),
                original_guid,
                original_name: Some("active power plan".into()),
                suite_owned_guids: Vec::new(),
                unknown: BTreeMap::new(),
            })
        }
        Action::Tool(_) => Err("mutable action has no lossless recovery entry".into()),
        Action::ObserveConfigState
        | Action::ObserveGpuInventory
        | Action::ObserveChipsetDriver
        | Action::ObserveMemoryTopology
        | Action::BaselineBenchmark
        | Action::FinalBenchmark
        | Action::FpsCapInfo
        | Action::Hags
        | Action::GpuDriverCleanPreparation
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
        | Action::FinalChecklistGuide => {
            Err("check-only observations do not capture backups".into())
        }
        Action::Advisory(reason) => Err(reason.to_string()),
    }
}

fn capture_process_priority(change: &RegistryChange, step: String) -> Result<BackupEntry, String> {
    if step != "P3:10" {
        return Err("P3:10 capture step is not exact".into());
    }
    require_process_priority_change(change)?;
    let original = registry_read_exact(change)?;
    let (original_value, original_type, existed) = match original {
        Some(RegValue::Dword(value)) => (Value::from(value), Some("DWord".into()), true),
        Some(RegValue::String(value)) => (Value::String(value.into()), Some("String".into()), true),
        Some(RegValue::Binary(value)) => (
            Value::Array(value.iter().copied().map(Value::from).collect()),
            Some("Binary".into()),
            true,
        ),
        None => (Value::Null, None, false),
    };
    Ok(BackupEntry::Registry {
        step,
        timestamp: timestamp(),
        path: format!("HKLM:\\{}", change.key),
        name: change.name.into(),
        original_value,
        original_type,
        existed,
        unknown: BTreeMap::new(),
    })
}

fn apply_action(
    action: &Action,
    config: Option<&Config>,
    captured_services: Option<&[String]>,
    nagle_binding: Option<&NagleBinding>,
    cs2_binding: Option<&Cs2RegistryBinding>,
) -> Result<(), String> {
    match action {
        Action::ObserveConfigState
        | Action::ObserveGpuInventory
        | Action::ObserveChipsetDriver
        | Action::ObserveMemoryTopology
        | Action::BaselineBenchmark
        | Action::FinalBenchmark
        | Action::FpsCapInfo
        | Action::Hags
        | Action::GpuDriverCleanPreparation
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
        | Action::FinalChecklistGuide => Err("check-only observations cannot be applied".into()),
        Action::ProcessPriority(change) => {
            require_process_priority_change(change)?;
            registry_write(change)
        }
        Action::RegistryBatch(changes) => {
            for change in changes {
                registry_write(change)?;
            }
            Ok(())
        }
        Action::VbsHvciBatch(changes) => {
            let change = vbs_hvci_change(changes)?;
            registry_write(change)
        }
        Action::Nagle => apply_nagle(
            nagle_binding.ok_or("Nagle mutation requires a captured interface binding")?,
        ),
        Action::Dns => Err("P3:9 mutation requires captured DNS bindings".into()),
        Action::MsiInterrupts | Action::NicInterruptAffinity => {
            Err("interrupt-policy mutation requires captured exact device bindings".into())
        }
        Action::Autostart => Err("P1:14 mutation requires a captured autostart binding".into()),
        Action::PowerPlan => Err("P1:6 mutation requires a captured power-plan binding".into()),
        Action::Pagefile => Err("P1:8 mutation requires a captured CIM pagefile binding".into()),
        Action::ShaderCache => {
            Err("P1:3 mutation requires its captured shader-cache inventory".into())
        }
        Action::Debloat => Err("P1:13 mutation requires its dedicated captured capability".into()),
        Action::DynamicTick => CommandVector::new(
            CommandName::Bcdedit,
            &["/set", "{current}", "disabledynamictick", "yes"],
        )?
        .run()
        .map(|_| ()),
        Action::Cs2Registry(action) => apply_cs2_registry(
            cs2_binding.ok_or("CS2 registry mutation requires a captured install binding")?,
            *action,
        ),
        Action::Cs2Config => Err("P1:34 mutation requires a captured CS2 config binding".into()),
        Action::ServiceBatch(batch) => {
            let names = captured_service_names(*batch, config, captured_services)?;
            native_services::disable_stop_batch(&names)
        }
        Action::Tool(command) => {
            command.run()?;
            Ok(())
        }
        Action::Advisory(reason) => Err(reason.to_string()),
    }
}
include!("action_runtime_verify.rs");

fn disabledynamictick_from_bcd(text: &str) -> Result<Option<bool>, String> {
    let mut observed = None;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        if !matches!(fields.next(), Some("0x26000060")) {
            continue;
        }
        let value = fields
            .next()
            .ok_or("dynamic-tick BCD element has no raw value")?
            .to_ascii_lowercase();
        if fields.next().is_some() {
            return Err("dynamic-tick BCD element has an ambiguous raw value".into());
        }
        let enabled = match value.as_str() {
            "yes" | "true" | "1" => true,
            "no" | "false" | "0" => false,
            _ => return Err("dynamic-tick BCD element has a non-boolean raw value".into()),
        };
        if observed.replace(enabled).is_some() {
            return Err("dynamic-tick BCD element appears more than once".into());
        }
    }
    Ok(observed)
}

fn verify_disabledynamictick(expected: bool) -> Result<(), String> {
    let output = CommandVector::new(CommandName::Bcdedit, &["/enum", "{current}", "/v"])?.run()?;
    if disabledynamictick_from_bcd(&output)? == Some(expected) {
        Ok(())
    } else {
        Err("dynamic-tick BCD raw-element readback did not match".into())
    }
}

fn captured_service_names(
    batch: ServiceBatch,
    config: Option<&Config>,
    captured: Option<&[String]>,
) -> Result<Vec<String>, String> {
    let contract = service_power_contract_map(batch, config)?;
    let captured = captured.ok_or("service mutation requires a captured service batch")?;
    if captured.is_empty() {
        return Err("service mutation requires at least one captured present service".into());
    }
    if captured
        .iter()
        .any(|name| !contract.iter().any(|expected| expected == name))
    {
        return Err("captured service batch is outside its exact contract".into());
    }
    if captured.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("captured service batch contains a duplicate identity".into());
    }
    Ok(captured.to_vec())
}

fn verify_tool(command: &CommandVector) -> Result<(), String> {
    match command.command {
        CommandName::Powercfg if command.arguments.as_slice() == ["/setactive", "SCHEME_MIN"] => {
            let active = CommandVector::new(CommandName::Powercfg, &["/getactivescheme"])?.run()?;
            if active.to_ascii_lowercase().contains("high performance") {
                Ok(())
            } else {
                Err("power-plan postcondition was not observed".into())
            }
        }
        CommandName::Bcdedit
            if command.arguments.as_slice() == ["/set", "disabledynamictick", "yes"] =>
        {
            let current =
                CommandVector::new(CommandName::Bcdedit, &["/enum", "{current}"])?.run()?;
            if current.to_ascii_lowercase().contains("disabledynamictick") {
                Ok(())
            } else {
                Err("dynamic-tick postcondition was not observed".into())
            }
        }
        CommandName::Bcdedit
            if command.arguments.as_slice() == ["/deletevalue", "{current}", "safeboot"] =>
        {
            let current =
                CommandVector::new(CommandName::Bcdedit, &["/enum", "{current}"])?.run()?;
            if !current.to_ascii_lowercase().contains("safeboot") {
                Ok(())
            } else {
                Err("Safe Mode remains armed after deletion".into())
            }
        }
        _ => Err("external-tool command lacks an exact read-only verifier".into()),
    }
}
