use super::*;
use crate::*;
pub(crate) fn pagefile_target(system_drive: &str) -> Result<String, String> {
    let bytes = system_drive.as_bytes();
    if bytes.len() != 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return Err("SystemDrive is not an exact local drive designator".into());
    }
    Ok(format!(
        "{}\\pagefile.sys",
        system_drive.to_ascii_uppercase()
    ))
}

pub(crate) fn pagefile_size_mb(state: &State, _ram_mb: u64) -> Result<u32, String> {
    if state.pagefile_mb != 0 {
        return u32::try_from(state.pagefile_mb)
            .map_err(|_| "configured pagefile size does not fit Win32 uint32".into());
    }
    Err("system-managed pagefiles are the baseline; a fixed pagefile experiment requires an explicit pagefile_mb override".into())
}

pub(crate) fn validate_inventory(inventory: &PagefileInventory) -> Result<(), String> {
    if inventory.computer_object_path.is_empty()
        || inventory.computer_relative_path.is_empty()
        || inventory.system_drive.is_empty()
    {
        return Err("CIM computer-system identity or SystemDrive is missing".into());
    }
    let mut object_paths = std::collections::BTreeSet::new();
    let mut relative_paths = std::collections::BTreeSet::new();
    let mut pagefile_paths = std::collections::BTreeSet::new();
    for setting in &inventory.settings {
        validate_pagefile_identity(setting)?;
        if !object_paths.insert(setting.object_path.to_ascii_lowercase())
            || !relative_paths.insert(setting.relative_path.to_ascii_lowercase())
            || !pagefile_paths.insert(setting.path.to_ascii_lowercase())
        {
            return Err("CIM pagefile inventory contains duplicate identities".into());
        }
    }
    Ok(())
}

pub(crate) fn validate_pagefile_identity(setting: &PagefileSetting) -> Result<(), String> {
    if setting.path.is_empty()
        || setting.object_path.is_empty()
        || setting.relative_path.is_empty()
        || [
            setting.path.as_str(),
            setting.object_path.as_str(),
            setting.relative_path.as_str(),
        ]
        .iter()
        .any(|value| value.contains('\0') || value.contains('\r') || value.contains('\n'))
    {
        return Err("CIM pagefile instance has an unsafe or incomplete identity".into());
    }
    if setting.initial_size > setting.maximum_size {
        return Err("CIM pagefile instance has invalid size ordering".into());
    }
    Ok(())
}
