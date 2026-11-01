#[cfg(windows)]
use crate::driver::{apply_cs2_profile, capture_cs2_backup, verify_cs2_profile};
use crate::*;

#[cfg(windows)]
pub(crate) fn drs_backup_entry(step: String, backup: &DrsBackup) -> BackupEntry {
    BackupEntry::Drs {
        step,
        timestamp: timestamp(),
        profile: backup.profile.clone(),
        profile_created: backup.profile_created,
        settings: backup
            .settings
            .iter()
            .map(|setting| frametime_domain::backup::DrsSetting {
                id: Value::from(setting.id),
                previous_value: setting.value.map_or(Value::Null, Value::from),
                existed: setting.value.is_some(),
                unknown: BTreeMap::new(),
            })
            .collect(),
        application_bindings: backup
            .applications
            .iter()
            .map(|binding| frametime_domain::backup::DrsApplicationBinding {
                application: binding.application.clone(),
                original_profile: binding.profile.clone(),
                unknown: BTreeMap::new(),
            })
            .collect(),
        unknown: BTreeMap::new(),
    }
}

/// Saves the user's CS2 profile into `backup.json` before the baseline
/// overwrites it; an existing P3:1 entry for the profile is never replaced.
#[cfg(windows)]
pub(crate) fn persist_drs_original(
    work_dir: &std::path::Path,
    backup: &DrsBackup,
) -> Result<(), String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let _lock = WorkLock::acquire(trusted.path())?;
    let mut file = load_backup_at(trusted.path())?;
    if file.created.is_empty() {
        file.created = timestamp();
    }
    if drs_original(&file, &backup.profile).is_some() {
        return Ok(());
    }
    file.entries
        .push(drs_backup_entry(DRS_ORIGINAL_STEP.into(), backup));
    write_json_atomic_trusted(&trusted, BACKUP_FILE, &file)
        .map_err(|error| format!("persist NVIDIA original profile backup: {error}"))
}

impl LiveBackend<'_> {
    pub(crate) fn capture_drs_backup(&mut self, key: String) -> Result<Vec<BackupEntry>, String> {
        #[cfg(windows)]
        {
            let mut api = NativeNvapiDrs::load().map_err(|error| error.to_string())?;
            let backup = capture_cs2_backup(&mut api).map_err(|error| error.to_string())?;
            // F-69: P3:1 saved the user's original before the baseline; keep it.
            // The engine requires a non-empty result, so return that entry again.
            let original = drs_original(&load_backup_at(&self.work_dir)?, &backup.profile).cloned();
            let entry = original.unwrap_or_else(|| drs_backup_entry(key.clone(), &backup));
            self.captured_drs_backups.insert(key.clone(), backup);
            self.captured_steps.insert(key);
            Ok(vec![entry])
        }
        #[cfg(not(windows))]
        {
            let _ = key;
            Err("P3:4 requires NVIDIA NVAPI on Windows".into())
        }
    }

    pub(crate) fn apply_drs_backup(&mut self, key: &str) -> Result<(), String> {
        require_stored_preparation(&self._trusted_work_dir, "P1:20")?;
        let backup = self
            .captured_drs_backups
            .get(key)
            .ok_or("P3:4 mutation requires a read-only captured DRS backup")?;
        #[cfg(windows)]
        {
            let mut api = NativeNvapiDrs::load().map_err(|error| error.to_string())?;
            apply_cs2_profile(&mut api, backup)
                .map(|_| ())
                .map_err(|error| error.to_string())
        }
        #[cfg(not(windows))]
        {
            let _ = backup;
            Err("P3:4 requires NVIDIA NVAPI on Windows".into())
        }
    }

    pub(crate) fn verify_drs_backup(&mut self, key: &str) -> Result<(), String> {
        let backup = self
            .captured_drs_backups
            .get(key)
            .ok_or("P3:4 verification requires a read-only captured DRS backup")?;
        #[cfg(windows)]
        {
            let mut api = NativeNvapiDrs::load().map_err(|error| error.to_string())?;
            verify_cs2_profile(&mut api, backup).map_err(|error| error.to_string())
        }
        #[cfg(not(windows))]
        {
            let _ = backup;
            Err("P3:4 requires NVIDIA NVAPI on Windows".into())
        }
    }
}
