use crate::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cs2RegistryBinding {
    pub(crate) steam_root: PathBuf,
    pub(crate) cs2_executable: PathBuf,
}

pub(crate) const STEAM_REGISTRY_KEY: &str = "SOFTWARE\\Valve\\Steam";
pub(crate) const STEAM_PATH_VALUE: &str = "SteamPath";
pub(crate) const APP_COMPAT_LAYERS_KEY: &str =
    "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers";
pub(crate) const DIRECTX_GPU_PREFERENCES_KEY: &str =
    "Software\\Microsoft\\DirectX\\UserGpuPreferences";
pub(crate) const HIGH_PERFORMANCE_GPU: &str = "GpuPreference=2;";

pub(crate) fn cs2_registry_key(action: Cs2RegistryAction) -> &'static str {
    match action {
        Cs2RegistryAction::HighPerformanceGpu => DIRECTX_GPU_PREFERENCES_KEY,
    }
}

pub(crate) fn cs2_registry_value(action: Cs2RegistryAction) -> &'static str {
    match action {
        Cs2RegistryAction::HighPerformanceGpu => HIGH_PERFORMANCE_GPU,
    }
}

pub(crate) fn cs2_registry_changes(
    binding: &Cs2RegistryBinding,
    action: Cs2RegistryAction,
) -> RegistryChange {
    RegistryChange {
        hive: Hive::CurrentUser,
        key: cs2_registry_key(action),
        name: Box::leak(cs2_path_string(&binding.cs2_executable).into_boxed_str()),
        value: RegValue::String(cs2_registry_value(action)),
    }
}

pub(crate) fn cs2_path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn capture_cs2_registry(
    step: String,
    action: Cs2RegistryAction,
) -> Result<(Cs2RegistryBinding, Vec<BackupEntry>), String> {
    require_hybrid_gpu_for_preference(action)?;
    let binding = discover_cs2_registry_binding()?;
    let change = cs2_registry_changes(&binding, action);
    let mut entry = capture_registry(&change, step)?;
    let BackupEntry::Registry { unknown, .. } = &mut entry else {
        return Err("CS2 registry capture did not create a registry backup".into());
    };
    unknown.insert(
        "cs2Executable".into(),
        Value::String(cs2_path_string(&binding.cs2_executable)),
    );
    Ok((binding, vec![entry]))
}

pub(crate) fn inspect_cs2_registry(action: Cs2RegistryAction) -> Result<Inspection, String> {
    if action == Cs2RegistryAction::HighPerformanceGpu && !has_supported_hybrid_gpu_topology()? {
        return Ok(Inspection::Inapplicable);
    }
    let binding = match discover_cs2_registry_binding() {
        Ok(binding) => binding,
        Err(error)
            if error == "no trusted CS2 install exists under HKCU SteamPath"
                || error == "HKCU Valve Steam SteamPath is absent or not a non-empty REG_SZ" =>
        {
            return Ok(Inspection::Inapplicable);
        }
        Err(error) => return Err(error),
    };
    let change = cs2_registry_changes(&binding, action);
    if registry_read(&change)?.as_ref() == Some(&change.value) {
        Ok(Inspection::Satisfied)
    } else {
        Ok(Inspection::NeedsApply)
    }
}

pub(crate) fn apply_cs2_registry(
    binding: &Cs2RegistryBinding,
    action: Cs2RegistryAction,
) -> Result<(), String> {
    require_hybrid_gpu_for_preference(action)?;
    reobserve_cs2_registry_binding(binding)?;
    registry_write(&cs2_registry_changes(binding, action))
}

pub(crate) fn verify_cs2_registry(
    binding: &Cs2RegistryBinding,
    action: Cs2RegistryAction,
) -> Result<(), String> {
    require_hybrid_gpu_for_preference(action)?;
    reobserve_cs2_registry_binding(binding)?;
    let change = cs2_registry_changes(binding, action);
    if registry_read(&change)?.as_ref() == Some(&change.value) {
        Ok(())
    } else {
        Err("CS2 registry value readback did not match the exact requested value".into())
    }
}

pub(crate) fn validate_cs2_restore_binding(
    step: &str,
    key: &str,
    name: &str,
    unknown: &BTreeMap<String, Value>,
) -> Result<(), String> {
    let expected_key = match step {
        "P1:4" => APP_COMPAT_LAYERS_KEY,
        "P1:30" => DIRECTX_GPU_PREFERENCES_KEY,
        _ => return Err("CS2 registry restore step is not allowlisted".into()),
    };
    if key != expected_key {
        return Err("CS2 registry restore key is not the exact catalog key".into());
    }
    let captured_path = unknown
        .get("cs2Executable")
        .and_then(Value::as_str)
        .ok_or("CS2 registry backup has no canonical executable identity")?;
    if name != captured_path {
        return Err(
            "CS2 registry restore value name does not match captured executable identity".into(),
        );
    }
    let binding = discover_cs2_registry_binding()?;
    if cs2_path_string(&binding.cs2_executable) != captured_path {
        return Err(
            "CS2 registry restore executable identity no longer matches HKCU SteamPath discovery"
                .into(),
        );
    }
    Ok(())
}

pub(crate) fn discover_cs2_registry_binding() -> Result<Cs2RegistryBinding, String> {
    let install = discover_cs2_install_from_hkcu()?
        .ok_or("no trusted CS2 install exists under HKCU SteamPath")?;
    let executable = install
        .install_root
        .join("game")
        .join("bin")
        .join("win64")
        .join("cs2.exe");
    let canonical = std::fs::canonicalize(&executable)
        .map_err(|error| format!("canonicalize trusted CS2 executable: {error}"))?;
    Ok(Cs2RegistryBinding {
        steam_root: install.steam_root,
        cs2_executable: canonical,
    })
}

/// Resolves the sole live Steam authority before any CS2 action. `Ok(None)`
/// means the authoritative HKCU value is absent or has no trusted CS2 install;
/// malformed values and trust failures remain hard errors.
pub(crate) fn discover_cs2_install_from_hkcu() -> Result<Option<Cs2Install>, String> {
    let steam_path = registry_read(&RegistryChange {
        hive: Hive::CurrentUser,
        key: STEAM_REGISTRY_KEY,
        name: STEAM_PATH_VALUE,
        value: RegValue::String(""),
    })?
    .and_then(|value| match value {
        RegValue::String(path) if !path.is_empty() => Some(PathBuf::from(path)),
        _ => None,
    });
    let Some(steam_path) = steam_path else {
        return Ok(None);
    };
    discover_cs2_install(&steam_path)
        .map_err(|error| format!("validate CS2 install from HKCU SteamPath: {error}"))?
        .map_or(Ok(None), |install| Ok(Some(install)))
}

pub(crate) fn reobserve_cs2_registry_binding(binding: &Cs2RegistryBinding) -> Result<(), String> {
    let current = discover_cs2_registry_binding()?;
    if current.steam_root != binding.steam_root || current.cs2_executable != binding.cs2_executable
    {
        return Err("CS2 registry binding changed after capture; refusing mutation".into());
    }
    Ok(())
}

pub(crate) fn require_hybrid_gpu_for_preference(action: Cs2RegistryAction) -> Result<(), String> {
    if action == Cs2RegistryAction::HighPerformanceGpu && !has_supported_hybrid_gpu_topology()? {
        return Err(
            "CS2 high-performance GPU preference requires a confirmed Intel-plus-discrete hybrid topology"
                .into(),
        );
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn has_supported_hybrid_gpu_topology() -> Result<bool, String> {
    let devices = enumerate_present_status_ok_pci(&WindowsSetupApiEnumerator)
        .map_err(|error| format!("discover display adapters for hybrid GPU preference: {error}"))?;
    let vendors = devices
        .into_iter()
        .filter_map(|(class, binding)| {
            (class == PciDeviceClass::Display).then_some(binding.vendor_id)
        })
        .collect::<Vec<_>>();
    Ok(is_confirmed_hybrid_vendor_set(&vendors))
}

#[cfg(not(windows))]
pub(crate) fn has_supported_hybrid_gpu_topology() -> Result<bool, String> {
    Ok(false)
}

#[cfg(any(test, windows))]
pub(crate) fn is_confirmed_hybrid_vendor_set(vendors: &[u16]) -> bool {
    const INTEL: u16 = 0x8086;
    const AMD: u16 = 0x1002;
    const NVIDIA: u16 = 0x10de;
    vendors.contains(&INTEL) && vendors.iter().any(|vendor| matches!(*vendor, AMD | NVIDIA))
}

#[cfg(test)]
mod cs2_registry_hybrid_tests {
    use super::is_confirmed_hybrid_vendor_set;

    #[test]
    pub(crate) fn gpu_preference_requires_a_confirmed_integrated_plus_discrete_vendor_set() {
        assert!(is_confirmed_hybrid_vendor_set(&[0x8086, 0x10de]));
        assert!(is_confirmed_hybrid_vendor_set(&[0x8086, 0x1002]));
        assert!(!is_confirmed_hybrid_vendor_set(&[0x10de]));
        assert!(!is_confirmed_hybrid_vendor_set(&[0x1002, 0x10de]));
        assert!(!is_confirmed_hybrid_vendor_set(&[]));
    }
}
