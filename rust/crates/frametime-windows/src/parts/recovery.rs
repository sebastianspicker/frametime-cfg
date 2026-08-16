struct RegistryRestore<'a> {
    step: &'a str,
    path: &'a str,
    name: &'a str,
    value: &'a Value,
    original_type: &'a Option<String>,
    existed: bool,
    unknown: &'a BTreeMap<String, Value>,
    config: &'a Config,
}

fn restore_entry(entry: &BackupEntry, config: &Config) -> Result<(), String> {
    match entry {
        BackupEntry::Registry { .. } => restore_registry_entry(entry, config),
        BackupEntry::Hags {
            step,
            original_value,
            target_value,
            adapter_ids,
            effective_verification_pending,
            unknown,
            ..
        } => restore_hags_entry(
            step,
            *original_value,
            *target_value,
            adapter_ids,
            *effective_verification_pending,
            unknown,
        ),
        BackupEntry::Bootconfig {
            step,
            key,
            original_value,
            existed,
            ..
        } => restore_boot(step, key, original_value, *existed),
        BackupEntry::Service { .. } => restore_service_entry(entry),
        BackupEntry::Powerplan {
            step,
            original_guid,
            suite_owned_guids,
            unknown,
            ..
        } => restore_power_plan(step, original_guid, suite_owned_guids, unknown),
        BackupEntry::PagefileTransaction { .. } => native_pagefile_restore(entry),
        BackupEntry::Scheduledtask {
            step,
            task_name,
            task_path,
            existed,
            was_enabled,
            script_path,
            unknown,
            ..
        } => restore_debloat_task(
            step,
            task_name,
            task_path,
            *existed,
            *was_enabled,
            script_path,
            unknown,
        ),
        BackupEntry::NicAdapter { .. }
        | BackupEntry::QosUro { .. }
        | BackupEntry::Defender { .. }
        | BackupEntry::Pagefile { .. } => Err(
            "backup variant has no exact current catalog capture binding and is retained".into(),
        ),
        BackupEntry::Cs2ConfigTransaction { .. } => restore_cs2_config(entry),
        BackupEntry::NetworkStackTransaction { .. } => {
            restore_network_stack(&NativeNetworkStackHost, entry)
        }
        BackupEntry::Drs { .. } => restore_drs_entry(entry),
        BackupEntry::Dns { .. } => restore_native_dns(entry),
        BackupEntry::InterruptPolicy { .. } => restore_native_interrupt_policy(entry),
        BackupEntry::Unknown(_) => Err("unknown backup entry is retained".into()),
    }
}

fn restore_registry_entry(entry: &BackupEntry, config: &Config) -> Result<(), String> {
    let BackupEntry::Registry {
        step,
        path,
        name,
        original_value,
        original_type,
        existed,
        unknown,
        ..
    } = entry
    else {
        unreachable!("registry restore helper requires a registry backup");
    };
    restore_registry(RegistryRestore {
        step,
        path,
        name,
        value: original_value,
        original_type,
        existed: *existed,
        unknown,
        config,
    })
}

fn restore_service_entry(entry: &BackupEntry) -> Result<(), String> {
    let BackupEntry::Service {
        step,
        name,
        original_start_type,
        delayed_auto_start,
        original_status,
        unknown,
        ..
    } = entry
    else {
        unreachable!("service restore helper requires a service backup");
    };
    if step == "P1:13" && !unknown.is_empty() {
        return Err("P1:13 service backup has unrecognized fields".into());
    }
    restore_service(
        step,
        name,
        original_start_type,
        *delayed_auto_start,
        original_status,
    )
}
fn restore_registry(restore: RegistryRestore<'_>) -> Result<(), String> {
    let (hive, key) = parse_registry_path(restore.path)?;
    let autostart_restore = restore.step == "P1:14";
    if restore.step == "P1:25" {
        if hive != Hive::LocalMachine {
            return Err("Nagle restore hive is not allowlisted".into());
        }
        validate_nagle_restore_binding(key, restore.name, restore.unknown)?;
    } else if matches!(restore.step, "P1:4" | "P1:30") {
        if hive != Hive::CurrentUser {
            return Err("CS2 registry restore hive is not allowlisted".into());
        }
        validate_cs2_restore_binding(restore.step, key, restore.name, restore.unknown)?;
    } else if restore.step == "P1:14" {
        if !restore.existed || !restore.unknown.is_empty() {
            return Err("P1:14 backup is not an exact captured Run value".into());
        }
        validate_autostart_restore_binding(restore.config, hive, key, restore.name)?;
    } else if restore.step == "P1:13" {
        if !restore.unknown.is_empty() {
            return Err("P1:13 registry backup has unrecognized fields".into());
        }
        validate_debloat_policy_restore(hive, key, restore.name)?;
    } else if restore.step == "P3:10" {
        if !restore.unknown.is_empty() {
            return Err("P3:10 backup has unrecognized fields".into());
        }
        validate_process_priority_restore_binding(hive, key, restore.name)?;
    } else {
        validate_registry_restore_binding(restore.step, hive, key, restore.name)?;
    }
    if !restore.existed {
        registry_delete(hive, key, restore.name)?;
        let probe = RegistryChange {
            hive,
            key: Box::leak(key.to_owned().into_boxed_str()),
            name: Box::leak(restore.name.to_owned().into_boxed_str()),
            value: RegValue::Dword(0),
        };
        return if registry_read_exact(&probe)?.is_none() {
            Ok(())
        } else {
            Err("registry value remains after restore deletion".into())
        };
    }
    let value = match restore
        .original_type
        .as_deref()
        .ok_or("existing registry backup has no value type")?
    {
        "DWord" | "DWORD" => RegValue::Dword(
            u32::try_from(
                restore
                    .value
                    .as_u64()
                    .ok_or("invalid registry DWORD backup")?,
            )
            .map_err(|_| "registry DWORD exceeds u32")?,
        ),
        "String" | "REG_SZ" => RegValue::String(Box::leak(
            restore
                .value
                .as_str()
                .ok_or("invalid registry string backup")?
                .to_owned()
                .into_boxed_str(),
        )),
        "Binary" | "REG_BINARY" => RegValue::Binary(Box::leak(
            restore
                .value
                .as_array()
                .ok_or("invalid registry binary backup")?
                .iter()
                .map(|item| {
                    u8::try_from(item.as_u64().ok_or("invalid registry binary byte")?)
                        .map_err(|_| "registry binary byte exceeds u8")
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_boxed_slice(),
        )),
        _ => return Err("unsupported registry value type retained".into()),
    };
    let change = RegistryChange {
        hive,
        key: Box::leak(key.to_owned().into_boxed_str()),
        name: Box::leak(restore.name.to_owned().into_boxed_str()),
        value,
    };
    registry_write(&change)?;
    if (autostart_restore || matches!(restore.step, "P1:13" | "P3:7" | "P3:10"))
        && registry_read_exact(&change)?.as_ref() != Some(&change.value)
    {
        return Err("registry restore readback did not match the captured value".into());
    }
    Ok(())
}
fn restore_boot(step: &str, key: &str, value: &Value, existed: bool) -> Result<(), String> {
    match (step, key) {
        ("P2:1", "safeboot") if existed => {
            let safe_boot = value.as_str().ok_or("invalid safeboot backup")?;
            if !matches!(safe_boot, "minimal" | "network" | "dsrepair") {
                return Err("safeboot backup is not allowlisted".into());
            }
            CommandVector::new(
                CommandName::Bcdedit,
                &["/set", "{current}", "safeboot", safe_boot],
            )?
            .run()
            .map(|_| ())
        }
        ("P2:1", "safeboot") => CommandVector::new(
            CommandName::Bcdedit,
            &["/deletevalue", "{current}", "safeboot"],
        )?
        .run()
        .map(|_| ()),
        (step, key) if dynamic_tick_restore_binding(step, key).is_ok() && existed => {
            let enabled = value
                .as_bool()
                .ok_or("invalid dynamic-tick boolean backup")?;
            let setting = if enabled { "yes" } else { "no" };
            CommandVector::new(
                CommandName::Bcdedit,
                &["/set", "{current}", "disabledynamictick", setting],
            )?
            .run()?;
            verify_disabledynamictick(enabled)
        }
        (step, key) if dynamic_tick_restore_binding(step, key).is_ok() => {
            CommandVector::new(
                CommandName::Bcdedit,
                &["/deletevalue", "{current}", "disabledynamictick"],
            )?
            .run()?;
            let output =
                CommandVector::new(CommandName::Bcdedit, &["/enum", "{current}", "/v"])?.run()?;
            if disabledynamictick_from_bcd(&output)?.is_none() {
                Ok(())
            } else {
                Err("dynamic-tick BCD element remains after restore deletion".into())
            }
        }
        _ => Err("boot restore binding is not allowlisted".into()),
    }
}

fn dynamic_tick_restore_binding(step: &str, key: &str) -> Result<(), String> {
    if step == "P1:10" && key == "disabledynamictick" {
        Ok(())
    } else {
        Err("dynamic-tick boot restore binding is not allowlisted".into())
    }
}
fn parse_registry_path(path: &str) -> Result<(Hive, &str), String> {
    path.strip_prefix("HKLM:\\")
        .or_else(|| path.strip_prefix("HKLM\\"))
        .map(|key| (Hive::LocalMachine, key))
        .or_else(|| {
            path.strip_prefix("HKCU:\\")
                .or_else(|| path.strip_prefix("HKCU\\"))
                .map(|key| (Hive::CurrentUser, key))
        })
        .ok_or_else(|| "registry hive is not allowlisted".into())
}
fn validate_registry_restore_binding(
    step: &str,
    hive: Hive,
    key: &str,
    name: &str,
) -> Result<(), String> {
    let expected = frametime_core::step_catalog().iter().any(|catalog_step| {
        let catalog_key = Progress::key(catalog_step.phase as u8, catalog_step.number);
        catalog_key == step
            && (matches!(
                action_for(catalog_step.phase as u8, catalog_step.number),
                Ok(Action::ProcessPriority(change)) if change.hive == hive && change.key == key && change.name == name
            ) || matches!(
                action_for(catalog_step.phase as u8, catalog_step.number),
                Ok(Action::RegistryBatch(changes)) if changes.iter().any(|change| change.hive == hive && change.key == key && change.name == name)
            ) || matches!(
                action_for(catalog_step.phase as u8, catalog_step.number),
                Ok(Action::VbsHvciBatch(changes)) if changes.iter().any(|change| change.hive == hive && change.key == key && change.name == name)
            ))
    });
    if expected {
        Ok(())
    } else {
        Err("registry restore step and identity are not an exact catalog binding".into())
    }
}
fn validate_process_priority_restore_binding(
    hive: Hive,
    key: &str,
    name: &str,
) -> Result<(), String> {
    let expected = process_priority_change();
    if expected.hive == hive && expected.key == key && expected.name == name {
        Ok(())
    } else {
        Err("P3:10 restore is not the exact CpuPriorityClass binding".into())
    }
}
fn restore_service(
    step: &str,
    name: &str,
    original_start: &str,
    delayed: bool,
    original_status: &str,
) -> Result<(), String> {
    validate_service(step, name)?;
    if !matches!(
        original_start,
        "Automatic" | "Manual" | "Disabled" | "Boot" | "System"
    ) {
        return Err("service backup has an unsupported original startup type".into());
    }
    if !matches!(original_status, "Running" | "Stopped") {
        return Err("service backup has an unsupported original status".into());
    }
    native_services::restore(name, original_start, delayed, original_status)
}
fn validate_service(step: &str, name: &str) -> Result<(), String> {
    if service_restore_binding(step, name) {
        Ok(())
    } else {
        Err("service restore step and identity are not allowlisted".into())
    }
}
fn validate_debloat_policy_restore(hive: Hive, key: &str, name: &str) -> Result<(), String> {
    let policy = match (hive, key, name) {
        (
            Hive::LocalMachine,
            r"SOFTWARE\Policies\Microsoft\Windows\CloudContent",
            "DisableWindowsConsumerFeatures",
        ) => PolicyIdentity::DisableWindowsConsumerFeatures,
        (
            Hive::LocalMachine,
            r"SOFTWARE\Policies\Microsoft\Windows\CloudContent",
            "DisableSoftLanding",
        ) => PolicyIdentity::DisableSoftLanding,
        (
            Hive::CurrentUser,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo",
            "Enabled",
        ) => PolicyIdentity::DisableAdvertisingId,
        _ => return Err("P1:13 registry restore is not an exact consumer-policy identity".into()),
    };
    if policy.key() == key
        && policy.name() == name
        && policy.current_user() == matches!(hive, Hive::CurrentUser)
    {
        Ok(())
    } else {
        Err("P1:13 registry restore identity is inconsistent".into())
    }
}
fn restore_debloat_task(
    step: &str,
    name: &str,
    path: &str,
    existed: bool,
    enabled: bool,
    script_path: &Option<String>,
    unknown: &BTreeMap<String, Value>,
) -> Result<(), String> {
    if step != "P1:13"
        || !unknown.is_empty()
        || script_path.is_some()
        || !TASK_FOLDERS.iter().any(|folder| folder.path() == path)
        || name.is_empty()
        || name.contains(['\\', '/', '\0'])
    {
        return Err("P1:13 scheduled-task restore is not an exact captured identity".into());
    }
    native_task_scheduler::restore(name, path, existed, enabled)
}
fn validate_power_plan_guid(value: &str) -> Result<(), String> {
    if value.len() == 36
        && value.chars().enumerate().all(|(index, character)| {
            matches!(index, 8 | 13 | 18 | 23) && character == '-' || character.is_ascii_hexdigit()
        })
    {
        Ok(())
    } else {
        Err("power-plan GUID is not allowlisted".into())
    }
}
// Each legacy type has a dedicated native boundary.  Unsupported platform
// capabilities are reported with type-specific evidence and remain retained;
// no adapter falls back to a shell, PowerShell, or broad registry wildcard.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ServiceSnapshot {
    name: String,
    start_type: String,
    delayed_auto_start: bool,
    status: String,
}
