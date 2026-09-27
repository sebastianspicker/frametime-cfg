use crate::*;
mod power_plan;

use power_plan::restore_power_plan;
pub(crate) use power_plan::validate_power_plan_guid;

pub(crate) struct RegistryRestore<'a> {
    pub(crate) step: &'a str,
    pub(crate) path: &'a str,
    pub(crate) name: &'a str,
    pub(crate) value: &'a Value,
    pub(crate) original_type: &'a Option<String>,
    pub(crate) existed: bool,
    pub(crate) unknown: &'a BTreeMap<String, Value>,
    pub(crate) config: &'a Config,
}

pub(crate) fn restore_entry(entry: &BackupEntry, config: &Config) -> Result<(), String> {
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
        BackupEntry::Cs2ConfigTransaction { step, .. }
            if step == CS2_OPTIONAL_CONFIG_TRANSACTION_STEP =>
        {
            restore_optional_cs2_config(entry)
        }
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

pub(crate) fn restore_registry_entry(entry: &BackupEntry, config: &Config) -> Result<(), String> {
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

pub(crate) fn restore_service_entry(entry: &BackupEntry) -> Result<(), String> {
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
pub(crate) fn restore_registry(restore: RegistryRestore<'_>) -> Result<(), String> {
    let (hive, key) = parse_registry_path(restore.path)?;
    let identity = RegistryIdentity {
        hive,
        key,
        name: restore.name,
    };
    let autostart_restore = validate_registry_restore_request(&restore, identity)?;
    if !restore.existed {
        return delete_registry_restore_value(identity);
    }
    let change = registry_change(identity, registry_value_from_backup(&restore)?);
    registry_write(&change)?;
    verify_registry_restore_readback(&change, restore.step, autostart_restore)
}

#[derive(Clone, Copy)]
struct RegistryIdentity<'a> {
    hive: Hive,
    key: &'a str,
    name: &'a str,
}

fn validate_registry_restore_request(
    restore: &RegistryRestore<'_>,
    identity: RegistryIdentity<'_>,
) -> Result<bool, String> {
    let autostart_restore = restore.step == "P1:14";
    if restore.step == "P1:25" {
        if identity.hive != Hive::LocalMachine {
            return Err("Nagle restore hive is not allowlisted".into());
        }
        validate_nagle_restore_binding(identity.key, identity.name, restore.unknown)?;
    } else if matches!(restore.step, "P1:4" | "P1:30") {
        if identity.hive != Hive::CurrentUser {
            return Err("CS2 registry restore hive is not allowlisted".into());
        }
        validate_cs2_restore_binding(restore.step, identity.key, identity.name, restore.unknown)?;
    } else if restore.step == "P1:14" {
        if !restore.existed || !restore.unknown.is_empty() {
            return Err("P1:14 backup is not an exact captured Run value".into());
        }
        validate_autostart_restore_binding(
            restore.config,
            identity.hive,
            identity.key,
            identity.name,
        )?;
    } else if restore.step == "P1:13" {
        if !restore.unknown.is_empty() {
            return Err("P1:13 registry backup has unrecognized fields".into());
        }
        validate_debloat_policy_restore(identity.hive, identity.key, identity.name)?;
    } else if restore.step == "P3:10" {
        if !restore.unknown.is_empty() {
            return Err("P3:10 backup has unrecognized fields".into());
        }
        validate_process_priority_restore_binding(identity.hive, identity.key, identity.name)?;
    } else {
        validate_registry_restore_binding(
            restore.step,
            identity.hive,
            identity.key,
            identity.name,
        )?;
    }
    Ok(autostart_restore)
}

fn delete_registry_restore_value(identity: RegistryIdentity<'_>) -> Result<(), String> {
    registry_delete(identity.hive, identity.key, identity.name)?;
    if registry_read_exact(&registry_change(identity, RegValue::Dword(0)))?.is_none() {
        Ok(())
    } else {
        Err("registry value remains after restore deletion".into())
    }
}

fn registry_value_from_backup(restore: &RegistryRestore<'_>) -> Result<RegValue, String> {
    Ok(
        match restore
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
        },
    )
}

fn registry_change(identity: RegistryIdentity<'_>, value: RegValue) -> RegistryChange {
    RegistryChange {
        hive: identity.hive,
        key: Box::leak(identity.key.to_owned().into_boxed_str()),
        name: Box::leak(identity.name.to_owned().into_boxed_str()),
        value,
    }
}

fn verify_registry_restore_readback(
    change: &RegistryChange,
    step: &str,
    autostart_restore: bool,
) -> Result<(), String> {
    if requires_registry_restore_readback(step, autostart_restore)
        && registry_read_exact(change)?.as_ref() != Some(&change.value)
    {
        return Err("registry restore readback did not match the captured value".into());
    }
    Ok(())
}

fn requires_registry_restore_readback(step: &str, autostart_restore: bool) -> bool {
    autostart_restore || matches!(step, "P1:13" | "P3:7" | "P3:10")
}
pub(crate) fn restore_boot(
    step: &str,
    key: &str,
    value: &Value,
    existed: bool,
) -> Result<(), String> {
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

pub(crate) fn dynamic_tick_restore_binding(step: &str, key: &str) -> Result<(), String> {
    if step == "P1:10" && key == "disabledynamictick" {
        Ok(())
    } else {
        Err("dynamic-tick boot restore binding is not allowlisted".into())
    }
}
pub(crate) fn parse_registry_path(path: &str) -> Result<(Hive, &str), String> {
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
pub(crate) fn validate_registry_restore_binding(
    step: &str,
    hive: Hive,
    key: &str,
    name: &str,
) -> Result<(), String> {
    let expected = frametime_domain::catalog::step_catalog().iter().any(|catalog_step| {
        let catalog_key = catalog_step.id.progress_key();
        catalog_key == step
            && matches!(
                action_for(catalog_step),
                Ok(Action::RegistryBatch(changes)) if changes.iter().any(|change| change.hive == hive && change.key == key && change.name == name)
            )
    });
    if expected {
        Ok(())
    } else {
        Err("registry restore step and identity are not an exact catalog binding".into())
    }
}
pub(crate) fn validate_process_priority_restore_binding(
    hive: Hive,
    key: &str,
    name: &str,
) -> Result<(), String> {
    if hive == Hive::LocalMachine
        && key
            == "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options\\cs2.exe\\PerfOptions"
        && name == "CpuPriorityClass"
    {
        Ok(())
    } else {
        Err("P3:10 restore is not the exact CpuPriorityClass binding".into())
    }
}
pub(crate) fn restore_service(
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
pub(crate) fn validate_service(step: &str, name: &str) -> Result<(), String> {
    if service_restore_binding(step, name) {
        Ok(())
    } else {
        Err("service restore step and identity are not allowlisted".into())
    }
}
pub(crate) fn validate_debloat_policy_restore(
    hive: Hive,
    key: &str,
    name: &str,
) -> Result<(), String> {
    match (hive, key, name) {
        (
            Hive::LocalMachine,
            r"SOFTWARE\Policies\Microsoft\Windows\CloudContent",
            "DisableWindowsConsumerFeatures",
        )
        | (
            Hive::LocalMachine,
            r"SOFTWARE\Policies\Microsoft\Windows\CloudContent",
            "DisableSoftLanding",
        )
        | (
            Hive::CurrentUser,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo",
            "Enabled",
        ) => {}
        _ => return Err("P1:13 registry restore is not an exact consumer-policy identity".into()),
    }
    Ok(())
}

const AUTOSTART_RUN_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";

fn valid_autostart_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, ' ' | '_' | '-' | '.')
        })
}

fn validated_autostart_names(config: &Config) -> Result<Vec<String>, String> {
    config
        .validate()
        .map_err(|error| format!("invalid config: {error}"))?;
    let mut names = Vec::with_capacity(config.autostart_remove.len());
    for candidate in &config.autostart_remove {
        if !valid_autostart_name(candidate)
            || names
                .iter()
                .any(|known: &String| known.eq_ignore_ascii_case(candidate))
        {
            return Err("P1:14 restore configuration has an unsafe or duplicate Run name".into());
        }
        names.push(candidate.clone());
    }
    Ok(names)
}

fn validate_autostart_restore_binding(
    config: &Config,
    hive: Hive,
    key: &str,
    name: &str,
) -> Result<(), String> {
    if !matches!(hive, Hive::CurrentUser | Hive::LocalMachine) || key != AUTOSTART_RUN_KEY {
        return Err("P1:14 restore key is not an exact Run binding".into());
    }
    if !valid_autostart_name(name) {
        return Err("P1:14 restore name is unsafe".into());
    }
    if validated_autostart_names(config)?
        .iter()
        .any(|configured| configured == name)
    {
        Ok(())
    } else {
        Err("P1:14 restore name is not present in validated config".into())
    }
}
pub(crate) fn restore_debloat_task(
    step: &str,
    name: &str,
    path: &str,
    existed: bool,
    enabled: bool,
    script_path: &Option<String>,
    unknown: &BTreeMap<String, Value>,
) -> Result<(), String> {
    let restore = DebloatTaskRestore {
        step,
        name,
        path,
        existed,
        enabled,
        script_path,
        unknown,
    };
    validate_debloat_task_restore(&restore)?;
    native_task_scheduler::restore(restore.name, restore.path, restore.existed, restore.enabled)
}

struct DebloatTaskRestore<'a> {
    step: &'a str,
    name: &'a str,
    path: &'a str,
    existed: bool,
    enabled: bool,
    script_path: &'a Option<String>,
    unknown: &'a BTreeMap<String, Value>,
}

fn validate_debloat_task_restore(restore: &DebloatTaskRestore<'_>) -> Result<(), String> {
    if restore.step != "P1:13"
        || !restore.unknown.is_empty()
        || restore.script_path.is_some()
        || !is_debloat_task_path(restore.path)
        || !is_debloat_task_name(restore.name)
    {
        return Err("P1:13 scheduled-task restore is not an exact captured identity".into());
    }
    Ok(())
}

fn is_debloat_task_path(path: &str) -> bool {
    matches!(
        path,
        r"\Microsoft\Windows\Application Experience\"
            | r"\Microsoft\Windows\Customer Experience Improvement Program\"
    )
}

fn is_debloat_task_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['\\', '/', '\0'])
}
