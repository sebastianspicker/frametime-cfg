use crate::*;

pub(crate) const WINDOWS_UPDATE_SERVICES: [&str; 3] = ["wuauserv", "UsoSvc", "WaaSMedicSvc"];
pub(crate) const SYSTEM_SERVICE_BASE: [&str; 3] = ["SysMain", "WSearch", "qWave"];
pub(crate) const XBOX_SERVICE_IDENTITIES: [&str; 4] = [
    "XblAuthManager",
    "XblGameSave",
    "XboxNetApiSvc",
    "XboxGipSvc",
];

pub(crate) fn service_restore_binding(step: &str, name: &str) -> bool {
    match step {
        "P1:15" => WINDOWS_UPDATE_SERVICES.contains(&name),
        "P1:37" => SYSTEM_SERVICE_BASE.contains(&name) || XBOX_SERVICE_IDENTITIES.contains(&name),
        "P1:13" => matches!(name, "DiagTrack" | "dmwappushservice"),
        _ => false,
    }
}

pub(crate) fn inspect_action(action: &Action) -> Result<Inspection, String> {
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
        | Action::FpsCapInfo => Err("typed observations require live backend context".into()),
        Action::NvidiaDriverDownloadPreparation
        | Action::SafeModeHandoff
        | Action::PhaseThreeHandoff => Ok(Inspection::Satisfied),
        Action::NvidiaDriverRemoval | Action::NvidiaDriverInstall => {
            Err("driver execution requires the persisted P1:18/P1:19 NVIDIA transaction".into())
        }
        Action::NvidiaProfileApply => {
            Err("NVIDIA DRS application requires its dedicated native transaction".into())
        }
        Action::NetworkStack => Err("P1:16 requires its native transaction binding".into()),
        Action::RegistryBatch(changes) => {
            for change in changes {
                let _ = registry_read(change)?;
            }
            Ok(Inspection::NeedsApply)
        }
        Action::MsiInterrupts | Action::NicInterruptAffinity => {
            Err("interrupt-policy inspection requires exact live device bindings".into())
        }
        Action::Pagefile => Err("P1:8 inspection requires native CIM inventory context".into()),
        Action::Cs2Registry(action) => inspect_cs2_registry(*action),
        Action::Cs2Config => {
            Err("P1:34 inspection requires its dedicated CS2 config binding".into())
        }
        Action::Tool(_) => Ok(Inspection::NeedsApply),
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

pub(crate) fn inspect_guidance_action(action: &Action) -> Option<Result<Inspection, String>> {
    guidance_message(action).map(|_| Ok(Inspection::Satisfied))
}

pub(crate) fn guidance_message(action: &Action) -> Option<&'static str> {
    match action {
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
            "P3:6: configure CS2 launch options and video settings manually. Use the Raw latency, NVIDIA VRR, AMD FreeSync, or GPU-constrained UI guidance as appropriate; this workflow does not write Steam launch options or CS2 video files."
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
    }.into()
}

pub(crate) fn capture_actions(
    action: &Action,
    step: String,
    _config: Option<&Config>,
) -> Result<Vec<BackupEntry>, String> {
    match action {
        Action::RegistryBatch(changes) => changes
            .iter()
            .map(|change| capture_registry(change, step.clone()))
            .collect(),
        _ => capture_action(action, step).map(|entry| vec![entry]),
    }
}

pub(crate) fn capture_action(action: &Action, step: String) -> Result<BackupEntry, String> {
    match action {
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
        Action::RegistryBatch(_) => {
            Err("registry batch capture is handled by capture_actions".into())
        }
        Action::Advisory(reason) => Err(reason.to_string()),
        _ => Err("check-only action does not capture a lossless backup".into()),
    }
}

pub(crate) fn apply_action(
    action: &Action,
    _config: Option<&Config>,
    _captured_services: Option<&[String]>,
    _nagle_binding: Option<&NagleBinding>,
    cs2_binding: Option<&Cs2RegistryBinding>,
) -> Result<(), String> {
    match action {
        Action::RegistryBatch(changes) => {
            for change in changes {
                registry_write(change)?;
            }
            Ok(())
        }
        Action::Cs2Registry(action) => apply_cs2_registry(
            cs2_binding.ok_or("CS2 registry mutation requires a captured install binding")?,
            *action,
        ),
        Action::Tool(command) => {
            command.run()?;
            Ok(())
        }
        Action::Advisory(reason) => Err(reason.to_string()),
        _ => Err("native action requires its dedicated captured platform binding".into()),
    }
}

pub(crate) fn disabledynamictick_from_bcd(text: &str) -> Result<Option<bool>, String> {
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

pub(crate) fn verify_disabledynamictick(expected: bool) -> Result<(), String> {
    let output = CommandVector::new(CommandName::Bcdedit, &["/enum", "{current}", "/v"])?.run()?;
    if disabledynamictick_from_bcd(&output)? == Some(expected) {
        Ok(())
    } else {
        Err("dynamic-tick BCD raw-element readback did not match".into())
    }
}

pub(crate) fn verify_tool(command: &CommandVector) -> Result<(), String> {
    match command.command {
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

mod verify;
pub(crate) use verify::*;
