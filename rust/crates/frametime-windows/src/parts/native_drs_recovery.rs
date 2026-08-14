fn restore_drs_entry(entry: &BackupEntry) -> Result<(), String> {
    let BackupEntry::Drs {
        profile,
        profile_created,
        settings,
        application_bindings,
        unknown,
        ..
    } = entry
    else {
        return Err("DRS restore received the wrong backup type".into());
    };
    if !unknown.is_empty()
        || settings.len() != CS2_SETTINGS.len()
        || application_bindings.len() != 2
    {
        return Err("DRS backup is incomplete or has unrecognized fields".into());
    }
    let settings = settings
        .iter()
        .map(|setting| {
            let id = setting
                .id
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or("DRS setting ID is not uint32")?;
            let value = match (setting.existed, setting.previous_value.as_u64()) {
                (false, _) => None,
                (true, Some(value)) => {
                    Some(u32::try_from(value).map_err(|_| "DRS setting exceeds uint32")?)
                }
                (true, None) => {
                    return Err("DRS setting backup is not a uint32 representation".into());
                }
            };
            Ok(DrsOriginalSetting { id, value })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let applications = application_bindings
        .iter()
        .map(|binding| {
            if !binding.unknown.is_empty() {
                return Err("DRS application binding has unrecognized fields".into());
            }
            Ok(DrsApplicationOriginal {
                application: binding.application.clone(),
                profile: binding.original_profile.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let backup = DrsBackup {
        profile: profile.clone(),
        profile_created: *profile_created,
        settings,
        applications,
    };
    #[cfg(windows)]
    {
        let mut api = NativeNvapiDrs::load().map_err(|error| error.to_string())?;
        restore_cs2_profile(&mut api, &backup).map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = backup;
        Err("DRS restore requires NVIDIA NVAPI on Windows".into())
    }
}
