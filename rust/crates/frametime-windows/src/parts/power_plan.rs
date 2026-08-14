const HIGH_PERFORMANCE_SCHEME: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const BALANCED_SCHEME: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
const SUITE_POWER_PLAN_NAME: &str = "frametime.cfg";
const SUITE_POWER_PLAN_DESCRIPTION: &str =
    "Tiered plan: T1 baseline, T2 vendor-aware CPU/disk/USB, topology-safe T3";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CpuVendor {
    Intel,
    Amd,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PowerSetting {
    subgroup: &'static str,
    setting: &'static str,
    value: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActivePowerPlan {
    guid: String,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PowerPlanBinding {
    original: ActivePowerPlan,
    suite_guid: String,
    settings: Vec<PowerSetting>,
}

fn power_settings(profile: Profile, vendor: CpuVendor) -> Vec<PowerSetting> {
    let processor = "54533251-82be-4824-96c1-47b60b740d00";
    let disk = "0012ee47-9041-4b5d-9b77-535fba8b1442";
    let usb = "2a737441-1930-4402-8d77-b2bebba308a3";
    let sleep = "238c9fa8-0aad-41ed-83f4-97be242c8f20";
    let cooling = "5fb4938d-1ee8-4b0f-9a3c-5036b0ab995c";
    let pcie = "501a4d13-42af-4429-9fd1-a8218c268e20";
    let network = "f905f51b-3de9-4be5-9ef8-2b7b6e31cbdb";
    let gpu = "48672f38-7a9a-4bb2-8bf8-3d85be19de4e";
    let mut settings = vec![
        (processor, "bc5038f7-23e0-4960-96da-33abaf5935ec", 100),
        (processor, "ea062031-0e34-4ff1-9b6d-eb1059334028", 100),
        (usb, "48e6b7a6-50f5-4782-a5d4-53bb8f07e226", 1),
        (disk, "6738e2c4-e8a5-4a42-b16a-e040e769756e", 0),
        (sleep, "29f6c1db-86da-48c5-9fdb-f2b67b1f44da", 0),
        (sleep, "9d7815a6-7ee4-497e-8888-515a05f02364", 0),
        (cooling, "dd848b2a-8a5d-4451-9ae2-39cd41658f6c", 1),
        (pcie, "ee12f906-d277-404b-b6da-e5fa1a576df5", 0),
    ];
    let tier_two = !matches!(profile, Profile::Safe);
    if tier_two {
        settings.extend([
            (
                processor,
                "893dee8e-2bef-41e0-89c6-b55d0929964c",
                if vendor == CpuVendor::Intel { 100 } else { 0 },
            ),
            (processor, "4e4450b3-6179-4e91-b8f1-5bb9938f81a1", 0),
            (processor, "2ddd5a84-5a71-437e-912a-db0b8c788732", 0),
            (processor, "b000397d-9b0b-483d-98c9-692a6060cfbf", 254),
            (processor, "be337238-0d82-4146-a960-4f3749d470c7", 255),
            (processor, "9943e905-9a30-4ec1-9b99-44dd3b76f7a2", 2),
            (processor, "0cc5b647-c1df-4637-891a-dec35c318583", 100),
            (disk, "0b2d69d7-a2a1-449c-9680-f91c70521c60", 0),
            (disk, "dab60367-53fe-4fbc-825e-521d069d2456", 0),
            (disk, "d3d55efd-c1ff-424e-9dc3-441be7833010", 0),
            (disk, "dbc9e238-6de9-49d9-a138-611ececd40d0", 0),
            (usb, "25dfa149-5dd1-4736-b5ab-e8a37b5b8187", 1),
            (usb, "0853a681-27c8-4100-a2fd-82013e970683", 1),
            (network, "12bbebe6-58d6-4636-95bb-3217ef867c1a", 0),
            (gpu, "2bfc24f9-5ea2-4801-8213-3dbae01aa39d", 4),
        ]);
        if vendor == CpuVendor::Intel {
            settings.push((processor, "4d2b0152-7d5c-498b-88e2-34345392a2c5", 100));
        }
    } else {
        settings.push((disk, "0b2d69d7-a2a1-449c-9680-f91c70521c60", 1));
    }
    if includes_tier_three(profile, vendor) {
        settings.extend([
            (processor, "4009efa7-e72d-4cba-9edf-91084ea8cbc3", 1),
            (processor, "4e4d2049-be1a-4064-b872-bcc8dccebce4", 0),
            (processor, "7d24baa7-0b84-480f-840c-1b0743c00f5f", 1),
            (processor, "984cf492-3bed-4488-a8f9-4286c97bf5aa", 0),
            (processor, "d8edeb9b-95cf-4f95-a73c-b061973693c8", 100),
        ]);
    }
    settings
        .into_iter()
        .map(|(subgroup, setting, value)| PowerSetting {
            subgroup,
            setting,
            value,
        })
        .collect()
}

fn parse_power_plan_line(line: &str) -> Option<ActivePowerPlan> {
    let raw_guid = line
        .split(|character: char| !character.is_ascii_hexdigit() && character != '-')
        .find(|candidate| validate_power_plan_guid(candidate).is_ok())?;
    let guid = raw_guid.to_ascii_lowercase();
    let tail = line
        .get(line.find(raw_guid)? + raw_guid.len()..)?
        .trim()
        .trim_end_matches('*')
        .trim();
    let name = tail.strip_prefix('(')?.strip_suffix(')')?.trim();
    (!name.is_empty()).then(|| ActivePowerPlan {
        guid,
        name: name.into(),
    })
}

fn find_power_guid(text: &str) -> Option<String> {
    text.split(|character: char| !character.is_ascii_hexdigit() && character != '-')
        .find(|candidate| validate_power_plan_guid(candidate).is_ok())
        .map(|value| value.to_ascii_lowercase())
}

fn parse_active_power_plan(text: &str) -> Result<ActivePowerPlan, String> {
    let plans = text
        .lines()
        .filter_map(parse_power_plan_line)
        .collect::<Vec<_>>();
    match plans.as_slice() {
        [plan] => Ok(plan.clone()),
        [] => Err("powercfg active-plan output has no exact GUID/name pair".into()),
        _ => Err("powercfg active-plan output has ambiguous GUID/name pairs".into()),
    }
}

fn capture_power_plan(
    step: String,
    profile: Profile,
) -> Result<(PowerPlanBinding, BackupEntry), String> {
    let vendor = detected_cpu_vendor()?;
    let active = powercfg(&["/getactivescheme"])?;
    let original = parse_active_power_plan(&active)?;
    let suite_guid = fresh_power_plan_guid()?;
    let listed = powercfg(&["/list"])?;
    if listed
        .lines()
        .filter_map(find_power_guid)
        .any(|guid| guid == suite_guid)
    {
        return Err("fresh suite power-plan GUID already exists".into());
    }
    let binding = PowerPlanBinding {
        original: original.clone(),
        suite_guid: suite_guid.clone(),
        settings: power_settings(profile, vendor),
    };
    let entry = BackupEntry::Powerplan {
        step,
        timestamp: timestamp(),
        original_guid: original.guid,
        original_name: Some(original.name),
        suite_owned_guids: vec![suite_guid],
        unknown: BTreeMap::new(),
    };
    Ok((binding, entry))
}

fn inspect_power_plan(profile: Profile) -> Result<Inspection, String> {
    let vendor = detected_cpu_vendor()?;
    let _ = power_settings(profile, vendor);
    parse_active_power_plan(&powercfg(&["/getactivescheme"])?).map(|_| Inspection::NeedsApply)
}

const fn includes_tier_three(profile: Profile, vendor: CpuVendor) -> bool {
    matches!(
        profile,
        Profile::Competitive | Profile::Custom | Profile::Yolo
    ) && !matches!(vendor, CpuVendor::Amd)
}

fn apply_power_plan(binding: &PowerPlanBinding) -> Result<(), String> {
    let result = (|| {
        if powercfg(&[
            "/duplicatescheme",
            HIGH_PERFORMANCE_SCHEME,
            &binding.suite_guid,
        ])
        .is_err()
        {
            powercfg(&["/duplicatescheme", BALANCED_SCHEME, &binding.suite_guid])?;
        }
        powercfg(&[
            "/changename",
            &binding.suite_guid,
            SUITE_POWER_PLAN_NAME,
            SUITE_POWER_PLAN_DESCRIPTION,
        ])?;
        for setting in &binding.settings {
            powercfg(&[
                "/setacvalueindex",
                &binding.suite_guid,
                setting.subgroup,
                setting.setting,
                &setting.value.to_string(),
            ])?;
            verify_power_setting(&binding.suite_guid, *setting)?;
        }
        powercfg(&["/setactive", &binding.suite_guid])?;
        verify_power_plan(binding)
    })();
    if result.is_err() {
        let _ = powercfg(&["/setactive", &binding.original.guid]);
        let _ = powercfg(&["/delete", &binding.suite_guid]);
    }
    result
}

fn verify_power_plan(binding: &PowerPlanBinding) -> Result<(), String> {
    let active = parse_active_power_plan(&powercfg(&["/getactivescheme"])?)?;
    if active.guid != binding.suite_guid || active.name != SUITE_POWER_PLAN_NAME {
        return Err("power-plan active readback did not identify the suite plan".into());
    }
    for setting in &binding.settings {
        verify_power_setting(&binding.suite_guid, *setting)?;
    }
    Ok(())
}

fn verify_power_setting(plan: &str, setting: PowerSetting) -> Result<(), String> {
    let output = powercfg(&["/q", plan, setting.subgroup, setting.setting])?;
    if parse_current_ac_value(&output) == Some(setting.value) {
        Ok(())
    } else {
        Err("powercfg AC-setting readback did not match the requested value".into())
    }
}

fn parse_current_ac_value(text: &str) -> Option<u32> {
    text.lines()
        .find(|line| {
            line.to_ascii_lowercase()
                .contains("current ac power setting index")
        })
        .and_then(find_hex_value)
}

fn find_hex_value(line: &str) -> Option<u32> {
    line.split_whitespace()
        .find_map(|token| {
            token
                .strip_prefix("0x")
                .or_else(|| token.strip_prefix("0X"))
        })
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
}

fn restore_power_plan(
    step: &str,
    original_guid: &str,
    suite_owned_guids: &[String],
    unknown: &BTreeMap<String, Value>,
) -> Result<(), String> {
    if step != "P1:6" || !unknown.is_empty() || validate_power_plan_guid(original_guid).is_err() {
        return Err("power-plan restore binding is not exact".into());
    }
    if suite_owned_guids.is_empty()
        || suite_owned_guids.iter().any(|guid| {
            validate_power_plan_guid(guid).is_err() || guid.eq_ignore_ascii_case(original_guid)
        })
        || suite_owned_guids.iter().enumerate().any(|(index, guid)| {
            suite_owned_guids[..index]
                .iter()
                .any(|previous| previous.eq_ignore_ascii_case(guid))
        })
    {
        return Err("power-plan recovery identities are not exact".into());
    }
    powercfg(&["/setactive", original_guid])?;
    if parse_active_power_plan(&powercfg(&["/getactivescheme"])?)?.guid != original_guid {
        return Err("original power-plan readback did not match".into());
    }
    for suite_guid in suite_owned_guids {
        let plans = powercfg(&["/list"])?;
        let known = plans
            .lines()
            .filter_map(parse_power_plan_line)
            .find(|plan| plan.guid == *suite_guid);
        let Some(plan) = known else {
            continue;
        };
        if plan.name != SUITE_POWER_PLAN_NAME {
            return Err("suite power-plan provenance is no longer exact".into());
        }
        powercfg(&["/delete", suite_guid])?;
        if powercfg(&["/list"])?
            .lines()
            .filter_map(find_power_guid)
            .any(|guid| guid == *suite_guid)
        {
            return Err("suite power-plan remains after deletion".into());
        }
    }
    Ok(())
}

#[cfg(windows)]
fn powercfg(arguments: &[&str]) -> Result<String, String> {
    CommandVector::new(CommandName::Powercfg, arguments)?.run()
}
#[cfg(not(windows))]
fn powercfg(_: &[&str]) -> Result<String, String> {
    Err("the live backend is supported only on Windows".into())
}

#[cfg(windows)]
fn fresh_power_plan_guid() -> Result<String, String> {
    let guid = windows::core::GUID::new().map_err(|error| error.to_string())?;
    Ok(format!("{guid:?}").to_ascii_lowercase())
}
#[cfg(not(windows))]
fn fresh_power_plan_guid() -> Result<String, String> {
    Err("the live backend is supported only on Windows".into())
}

fn detected_cpu_vendor() -> Result<CpuVendor, String> {
    let value = registry_read_exact(&RegistryChange {
        hive: Hive::LocalMachine,
        key: "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0",
        name: "VendorIdentifier",
        value: RegValue::Dword(0),
    })?;
    Ok(match value {
        Some(RegValue::String("GenuineIntel")) => CpuVendor::Intel,
        Some(RegValue::String("AuthenticAMD")) => CpuVendor::Amd,
        Some(RegValue::String(_)) | Some(RegValue::Dword(_)) | Some(RegValue::Binary(_)) | None => {
            CpuVendor::Unknown
        }
    })
}
