use super::*;
use crate::*;
use frametime_domain::backup::PagefileTransactionSetting;
pub(crate) fn capture_pagefile_binding(
    step: String,
    state: &State,
    inventory: PagefileInventory,
) -> Result<PagefileBinding, String> {
    validate_inventory(&inventory)?;
    let target_path = pagefile_target(&inventory.system_drive)?;
    let initial_size = pagefile_size_mb(state, inventory.physical_ram_mb)?;
    if u64::from(initial_size) > MAX_PAGEFILE_MB
        || inventory.free_space_mb < u64::from(initial_size)
    {
        return Err("pagefile target size exceeds validated free space or safe bounds".into());
    }
    let target = inventory
        .settings
        .iter()
        .filter(|setting| setting.path.eq_ignore_ascii_case(&target_path))
        .cloned()
        .collect::<Vec<_>>();
    if target.len() > 1 {
        return Err("CIM inventory contains duplicate target pagefile instances".into());
    }
    Ok(PagefileBinding {
        step,
        target_path,
        initial_size,
        maximum_size: initial_size,
        before: inventory,
        target: target.into_iter().next(),
    })
}

pub(crate) fn pagefile_backup_entry(binding: &PagefileBinding) -> BackupEntry {
    BackupEntry::PagefileTransaction {
        step: binding.step.clone(),
        timestamp: timestamp(),
        automatic_managed: binding.before.automatic_managed,
        target_path: binding.target_path.clone(),
        target_existed: binding.target.is_some(),
        computer_object_path: Some(binding.before.computer_object_path.clone()),
        computer_relative_path: Some(binding.before.computer_relative_path.clone()),
        created_object_path: None,
        created_relative_path: None,
        created_initial_size: None,
        created_maximum_size: None,
        mutation_intent: Some(
            if binding.target.is_some() {
                "update_pending"
            } else {
                "create_pending"
            }
            .into(),
        ),
        settings: binding
            .before
            .settings
            .iter()
            .map(pagefile_setting_backup)
            .collect(),
        unknown: BTreeMap::new(),
    }
}

pub(crate) fn pagefile_setting_backup(setting: &PagefileSetting) -> PagefileTransactionSetting {
    PagefileTransactionSetting {
        path: setting.path.clone(),
        initial_size: u64::from(setting.initial_size),
        maximum_size: u64::from(setting.maximum_size),
        object_path: Some(setting.object_path.clone()),
        relative_path: Some(setting.relative_path.clone()),
        unknown: BTreeMap::new(),
    }
}

pub(crate) fn inspect_pagefile(state: &State) -> Result<Inspection, String> {
    let inventory = match native_pagefile_inventory() {
        Ok(inventory) => inventory,
        Err(_) => return Ok(Inspection::Unsupported),
    };
    if state.pagefile_mb == 0 {
        return Ok(if inventory.automatic_managed {
            Inspection::Satisfied
        } else {
            Inspection::Advisory {
                reason: "Windows system-managed pagefile is the baseline. Enable it manually or provide an explicit pagefile_mb experiment override.",
            }
        });
    }
    if inventory.physical_ram_mb == 0 {
        return Ok(Inspection::Unsupported);
    }
    capture_pagefile_binding("P1:8".into(), state, inventory)?;
    Ok(Inspection::NeedsApply)
}
