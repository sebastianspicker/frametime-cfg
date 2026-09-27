#[cfg(any(test, windows))]
use super::*;
#[cfg(any(test, windows))]
use crate::*;
#[cfg(any(test, windows))]
pub(crate) fn restore_pagefile_transaction<S: PagefileStore>(
    store: &mut S,
    entry: &BackupEntry,
) -> Result<(), String> {
    let (before, created, target_path, target_existed) = parse_pagefile_restore_backup(entry)?;
    restore_pagefile_settings(
        store,
        &before,
        created.as_ref(),
        &target_path,
        target_existed,
    )?;
    verify_pagefile_restoration(
        store,
        &before,
        created.as_ref(),
        &target_path,
        target_existed,
    )
}
#[cfg(any(test, windows))]
fn parse_pagefile_restore_backup(
    entry: &BackupEntry,
) -> Result<
    (
        PagefileInventory,
        Option<CreatedPagefileToken>,
        String,
        bool,
    ),
    String,
> {
    let BackupEntry::PagefileTransaction {
        step,
        automatic_managed,
        target_path,
        target_existed,
        settings,
        unknown,
        computer_object_path,
        computer_relative_path,
        created_object_path,
        created_relative_path,
        created_initial_size,
        created_maximum_size,
        mutation_intent,
        ..
    } = entry
    else {
        return Err("pagefile transaction restore received the wrong entry type".into());
    };
    if step != "P1:8"
        || !unknown.is_empty()
        || computer_object_path
            .as_deref()
            .filter(|v| !v.is_empty())
            .is_none()
        || computer_relative_path
            .as_deref()
            .filter(|v| !v.is_empty())
            .is_none()
    {
        return Err("P1:8 backup has incomplete or untrusted transaction provenance".into());
    }
    let created = created_pagefile_token(
        target_path,
        *target_existed,
        created_object_path,
        created_relative_path,
        created_initial_size,
        created_maximum_size,
        mutation_intent.as_deref(),
    )?;
    let before = PagefileInventory {
        automatic_managed: *automatic_managed,
        computer_object_path: computer_object_path.clone().expect("checked"),
        computer_relative_path: computer_relative_path.clone().expect("checked"),
        system_drive: target_path
            .get(0..2)
            .ok_or("P1:8 target path is malformed")?
            .to_owned(),
        physical_ram_mb: 1,
        free_space_mb: u64::MAX,
        settings: settings
            .iter()
            .map(|setting| {
                let initial_size = u32::try_from(setting.initial_size)
                    .map_err(|_| "P1:8 initial size exceeds uint32")?;
                let maximum_size = u32::try_from(setting.maximum_size)
                    .map_err(|_| "P1:8 maximum size exceeds uint32")?;
                let item = PagefileSetting {
                    path: setting.path.clone(),
                    initial_size,
                    maximum_size,
                    object_path: setting
                        .object_path
                        .clone()
                        .ok_or("P1:8 setting lacks exact object token")?,
                    relative_path: setting
                        .relative_path
                        .clone()
                        .ok_or("P1:8 setting lacks exact relative token")?,
                };
                validate_pagefile_identity(&item)?;
                Ok(item)
            })
            .collect::<Result<Vec<_>, String>>()?,
    };
    Ok((before, created, target_path.clone(), *target_existed))
}

#[cfg(any(test, windows))]
fn created_pagefile_token(
    target_path: &str,
    target_existed: bool,
    object_path: &Option<String>,
    relative_path: &Option<String>,
    initial_size: &Option<u64>,
    maximum_size: &Option<u64>,
    mutation_intent: Option<&str>,
) -> Result<Option<CreatedPagefileToken>, String> {
    match (object_path, relative_path, mutation_intent) {
        (None, None, Some("update_pending")) if target_existed => Ok(None),
        (Some(object_path), Some(relative_path), Some("created"))
            if !target_existed && !object_path.is_empty() && !relative_path.is_empty() =>
        {
            let initial_size = u32::try_from(
                initial_size.ok_or("P1:8 created token lacks its expected initial size")?,
            )
            .map_err(|_| "P1:8 created initial size exceeds uint32")?;
            let maximum_size = u32::try_from(
                maximum_size.ok_or("P1:8 created token lacks its expected maximum size")?,
            )
            .map_err(|_| "P1:8 created maximum size exceeds uint32")?;
            if initial_size > maximum_size {
                return Err("P1:8 created token has invalid expected sizes".into());
            }
            Ok(Some(CreatedPagefileToken {
                object_path: object_path.clone(),
                relative_path: relative_path.clone(),
                path: target_path.into(),
                initial_size,
                maximum_size,
            }))
        }
        _ => Err("P1:8 backup has no exact deletion authority; manual recovery is required".into()),
    }
}

#[cfg(any(test, windows))]
fn restore_pagefile_settings<S: PagefileStore>(
    store: &mut S,
    before: &PagefileInventory,
    created: Option<&CreatedPagefileToken>,
    target_path: &str,
    target_existed: bool,
) -> Result<(), String> {
    validate_inventory(before)?;
    let live = store.inventory()?;
    validate_inventory(&live)?;
    if live.computer_object_path != before.computer_object_path
        || live.computer_relative_path != before.computer_relative_path
    {
        return Err(
            "P1:8 computer-system token no longer matches; manual recovery is required".into(),
        );
    }
    if let Some(created) = created {
        let matches = live
            .settings
            .iter()
            .filter(|setting| {
                setting.object_path == created.object_path
                    && setting.relative_path == created.relative_path
                    && setting.path.eq_ignore_ascii_case(target_path)
            })
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err("P1:8 created token is absent or ambiguous; refusing deletion".into());
        }
        if matches[0].initial_size != created.initial_size
            || matches[0].maximum_size != created.maximum_size
        {
            return Err("P1:8 created token sizes changed; refusing deletion".into());
        }
        store.delete(created)?;
    }
    if target_existed {
        let target = before
            .settings
            .iter()
            .find(|setting| setting.path.eq_ignore_ascii_case(target_path))
            .ok_or("P1:8 target token is absent from the complete original inventory")?;
        store.update(target, target.initial_size, target.maximum_size)?;
    }
    Ok(())
}
#[cfg(any(test, windows))]
fn verify_pagefile_restoration<S: PagefileStore>(
    store: &mut S,
    before: &PagefileInventory,
    created: Option<&CreatedPagefileToken>,
    target_path: &str,
    target_existed: bool,
) -> Result<(), String> {
    let restored = store.inventory()?;
    for setting in &before.settings {
        let current = restored
            .settings
            .iter()
            .find(|current| current.object_path == setting.object_path);
        if setting.path.eq_ignore_ascii_case(target_path) && target_existed {
            if current != Some(setting) {
                return Err("P1:8 target setting did not restore exactly".into());
            }
        } else if current != Some(setting) {
            return Err("P1:8 foreign pagefile setting changed during restore".into());
        }
    }
    if let Some(created) = created
        && restored.settings.iter().any(|setting| {
            setting.object_path == created.object_path
                || setting.relative_path == created.relative_path
        })
    {
        return Err("P1:8 exact created setting remains after deletion".into());
    }
    store.set_automatic(
        &before.computer_object_path,
        &before.computer_relative_path,
        None,
        before.automatic_managed,
    )?;
    if store.inventory()?.automatic_managed != before.automatic_managed {
        return Err("P1:8 AutomaticManagedPagefile restore did not read back exactly".into());
    }
    Ok(())
}
