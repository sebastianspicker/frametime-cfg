#[derive(Debug, Clone, PartialEq, Eq)]
struct Cs2RegistryBinding {
    steam_root: PathBuf,
    cs2_executable: PathBuf,
}

const STEAM_REGISTRY_KEY: &str = "SOFTWARE\\Valve\\Steam";
const STEAM_PATH_VALUE: &str = "SteamPath";
const APP_COMPAT_LAYERS_KEY: &str =
    "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers";
const DIRECTX_GPU_PREFERENCES_KEY: &str = "Software\\Microsoft\\DirectX\\UserGpuPreferences";
const DISABLE_FULLSCREEN_OPTIMIZATIONS: &str = "~ DISABLEDXMAXIMIZEDWINDOWEDMODE";
const HIGH_PERFORMANCE_GPU: &str = "GpuPreference=2;";

fn cs2_registry_key(action: Cs2RegistryAction) -> &'static str {
    match action {
        Cs2RegistryAction::DisableFullscreenOptimizations => APP_COMPAT_LAYERS_KEY,
        Cs2RegistryAction::HighPerformanceGpu => DIRECTX_GPU_PREFERENCES_KEY,
    }
}

fn cs2_registry_value(action: Cs2RegistryAction) -> &'static str {
    match action {
        Cs2RegistryAction::DisableFullscreenOptimizations => DISABLE_FULLSCREEN_OPTIMIZATIONS,
        Cs2RegistryAction::HighPerformanceGpu => HIGH_PERFORMANCE_GPU,
    }
}

fn cs2_registry_changes(binding: &Cs2RegistryBinding, action: Cs2RegistryAction) -> RegistryChange {
    RegistryChange {
        hive: Hive::CurrentUser,
        key: cs2_registry_key(action),
        name: Box::leak(cs2_path_string(&binding.cs2_executable).into_boxed_str()),
        value: RegValue::String(cs2_registry_value(action)),
    }
}

fn cs2_path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn capture_cs2_registry(
    step: String,
    action: Cs2RegistryAction,
) -> Result<(Cs2RegistryBinding, Vec<BackupEntry>), String> {
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

fn inspect_cs2_registry(action: Cs2RegistryAction) -> Result<Inspection, String> {
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

fn apply_cs2_registry(
    binding: &Cs2RegistryBinding,
    action: Cs2RegistryAction,
) -> Result<(), String> {
    reobserve_cs2_registry_binding(binding)?;
    registry_write(&cs2_registry_changes(binding, action))
}

fn verify_cs2_registry(
    binding: &Cs2RegistryBinding,
    action: Cs2RegistryAction,
) -> Result<(), String> {
    reobserve_cs2_registry_binding(binding)?;
    let change = cs2_registry_changes(binding, action);
    if registry_read(&change)?.as_ref() == Some(&change.value) {
        Ok(())
    } else {
        Err("CS2 registry value readback did not match the exact requested value".into())
    }
}

fn validate_cs2_restore_binding(
    step: &str,
    key: &str,
    name: &str,
    unknown: &BTreeMap<String, Value>,
) -> Result<(), String> {
    let action = match step {
        "P1:4" => Cs2RegistryAction::DisableFullscreenOptimizations,
        "P1:30" => Cs2RegistryAction::HighPerformanceGpu,
        _ => return Err("CS2 registry restore step is not allowlisted".into()),
    };
    if key != cs2_registry_key(action) {
        return Err("CS2 registry restore key is not the exact catalog key".into());
    }
    let captured_path = unknown
        .get("cs2Executable")
        .and_then(Value::as_str)
        .ok_or("CS2 registry backup has no canonical executable identity")?;
    if name != captured_path {
        return Err("CS2 registry restore value name does not match captured executable identity".into());
    }
    let binding = discover_cs2_registry_binding()?;
    if cs2_path_string(&binding.cs2_executable) != captured_path {
        return Err("CS2 registry restore executable identity no longer matches HKCU SteamPath discovery".into());
    }
    Ok(())
}

fn discover_cs2_registry_binding() -> Result<Cs2RegistryBinding, String> {
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
fn discover_cs2_install_from_hkcu() -> Result<Option<Cs2Install>, String> {
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

fn reobserve_cs2_registry_binding(binding: &Cs2RegistryBinding) -> Result<(), String> {
    let current = discover_cs2_registry_binding()?;
    if current.steam_root != binding.steam_root || current.cs2_executable != binding.cs2_executable {
        return Err("CS2 registry binding changed after capture; refusing mutation".into());
    }
    Ok(())
}
