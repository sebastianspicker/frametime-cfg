#[cfg(windows)]
use crate::driver::restore_cs2_profile;
use crate::*;
pub(crate) fn restore_drs_entry(entry: &BackupEntry) -> Result<(), String> {
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

/// Step whose captured DRS entry holds the user's profile from before the baseline.
pub(crate) const DRS_ORIGINAL_STEP: &str = "P3:1";

pub(crate) fn drs_original<'a>(file: &'a BackupFile, profile: &str) -> Option<&'a BackupEntry> {
    file.entries.iter().find(|entry| {
        matches!(entry, BackupEntry::Drs { step, profile: known, .. }
            if step == DRS_ORIGINAL_STEP && known == profile)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drs(step: &str, profile: &str) -> BackupEntry {
        BackupEntry::Drs {
            step: step.into(),
            timestamp: String::new(),
            profile: profile.into(),
            profile_created: false,
            settings: Vec::new(),
            application_bindings: Vec::new(),
            unknown: BTreeMap::new(),
        }
    }

    #[test]
    fn finds_only_the_p3_1_original_for_the_same_profile() {
        let mut file = BackupFile {
            entries: vec![drs("P3:4", "CS2")],
            created: String::new(),
            unknown: BTreeMap::new(),
        };
        assert!(drs_original(&file, "CS2").is_none());
        file.entries.push(drs("P3:1", "Other"));
        assert!(drs_original(&file, "CS2").is_none());
        file.entries.push(drs("P3:1", "CS2"));
        assert!(drs_original(&file, "CS2").is_some());
    }
}
