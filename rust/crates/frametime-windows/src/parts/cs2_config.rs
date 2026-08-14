const LEGACY_CS2_CONFIG_ASSETS: [OptionalCfgAsset; 9] = [
    OptionalCfgAsset::NetStable,
    OptionalCfgAsset::NetHighPing,
    OptionalCfgAsset::NetUnstable,
    OptionalCfgAsset::NetBad,
    OptionalCfgAsset::DebugHud,
    OptionalCfgAsset::DebugHudOff,
    OptionalCfgAsset::AudioStable,
    OptionalCfgAsset::AudioLowLatency025,
    OptionalCfgAsset::AudioLowLatency001,
];

#[derive(Debug, Clone)]
struct Cs2ConfigBinding {
    install: Cs2Install,
    request: Cs2ConfigRequest,
}

fn fixed_cs2_config_request() -> Cs2ConfigRequest {
    Cs2ConfigRequest::new(LEGACY_CS2_CONFIG_ASSETS)
}

fn inspect_cs2_config() -> Result<Inspection, String> {
    let Some(binding) = discover_cs2_config_binding()? else {
        return Ok(Inspection::Inapplicable);
    };
    let mut files = NativeCs2ConfigFs;
    let controller = Cs2ConfigController::new(binding.install)
        .map_err(|error| format!("validate P1:34 CS2 config controller: {error}"))?;
    match controller.verify(&binding.request, &mut files) {
        Ok(()) => Ok(Inspection::Satisfied),
        Err(_) => Ok(Inspection::NeedsApply),
    }
}

fn capture_cs2_config() -> Result<(Cs2ConfigBinding, BackupEntry), String> {
    let binding = discover_cs2_config_binding()?
        .ok_or("no trusted CS2 install exists under HKCU SteamPath")?;
    let mut files = NativeCs2ConfigFs;
    let entry = BackupEntry::capture_cs2_config_transaction(
        &binding.install,
        &binding.request,
        &mut files,
    )
    .map_err(|error| format!("capture P1:34 CS2 config transaction: {error}"))?;
    Ok((binding, entry))
}

fn apply_cs2_config(binding: &Cs2ConfigBinding) -> Result<(), String> {
    reobserve_cs2_config_binding(binding)?;
    let controller = Cs2ConfigController::new(binding.install.clone())
        .map_err(|error| format!("revalidate P1:34 CS2 config controller: {error}"))?;
    let mut files = NativeCs2ConfigFs;
    controller
        .apply(&binding.request, &mut files)
        .map(|_| ())
        .map_err(|error| format!("apply P1:34 CS2 config transaction: {error}"))
}

fn verify_cs2_config(binding: &Cs2ConfigBinding) -> Result<(), String> {
    reobserve_cs2_config_binding(binding)?;
    let controller = Cs2ConfigController::new(binding.install.clone())
        .map_err(|error| format!("revalidate P1:34 CS2 config controller: {error}"))?;
    let mut files = NativeCs2ConfigFs;
    controller
        .verify(&binding.request, &mut files)
        .map_err(|error| format!("verify P1:34 CS2 config transaction: {error}"))
}

fn restore_cs2_config(entry: &BackupEntry) -> Result<(), String> {
    let binding = discover_cs2_config_binding()?
        .ok_or("P1:34 restore requires the captured HKCU SteamPath install")?;
    entry
        .validate_cs2_config_transaction(&binding.install, &binding.request)
        .map_err(|error| format!("validate P1:34 CS2 restore binding: {error}"))?;
    reobserve_cs2_config_binding(&binding)?;
    let mut files = NativeCs2ConfigFs;
    entry
        .restore_cs2_config_transaction(&binding.install, &binding.request, &mut files)
        .map_err(|error| format!("restore P1:34 CS2 config transaction: {error}"))
}

fn discover_cs2_config_binding() -> Result<Option<Cs2ConfigBinding>, String> {
    let Some(install) = discover_cs2_install_from_hkcu()? else {
        return Ok(None);
    };
    Cs2ConfigController::new(install.clone())
        .map_err(|error| format!("validate P1:34 CS2 install binding: {error}"))?;
    Ok(Some(Cs2ConfigBinding {
        install,
        request: fixed_cs2_config_request(),
    }))
}

fn reobserve_cs2_config_binding(binding: &Cs2ConfigBinding) -> Result<(), String> {
    let current = discover_cs2_config_binding()?
        .ok_or("CS2 config install disappeared after capture; refusing mutation")?;
    if current.install != binding.install {
        return Err("CS2 config install binding changed after capture; refusing mutation".into());
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn p1_34_fixed_request_contains_the_complete_legacy_asset_set() {
    let request = fixed_cs2_config_request();
    assert_eq!(request.optional_assets().len(), LEGACY_CS2_CONFIG_ASSETS.len());
    assert_eq!(
        frametime_core::Cs2ConfigTarget::for_request(&request).len(),
        LEGACY_CS2_CONFIG_ASSETS.len() + 2
    );
    assert!(LEGACY_CS2_CONFIG_ASSETS
        .iter()
        .all(|asset| request.optional_assets().contains(asset)));
}
