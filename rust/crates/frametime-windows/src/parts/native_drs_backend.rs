impl LiveBackend {
    fn capture_drs_backup(&mut self, key: String) -> Result<Vec<BackupEntry>, String> {
        #[cfg(windows)]
        {
            let mut api = NativeNvapiDrs::load().map_err(|error| error.to_string())?;
            let backup = capture_cs2_backup(&mut api).map_err(|error| error.to_string())?;
            let entry = BackupEntry::Drs {
                step: key.clone(),
                timestamp: timestamp(),
                profile: backup.profile.clone(),
                profile_created: backup.profile_created,
                settings: backup
                    .settings
                    .iter()
                    .map(|setting| frametime_core::backup::DrsSetting {
                        id: Value::from(setting.id),
                        previous_value: setting.value.map_or(Value::Null, Value::from),
                        existed: setting.value.is_some(),
                        unknown: BTreeMap::new(),
                    })
                    .collect(),
                application_bindings: backup
                    .applications
                    .iter()
                    .map(|binding| frametime_core::DrsApplicationBinding {
                        application: binding.application.clone(),
                        original_profile: binding.profile.clone(),
                        unknown: BTreeMap::new(),
                    })
                    .collect(),
                unknown: BTreeMap::new(),
            };
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

    fn apply_drs_backup(&mut self, key: &str) -> Result<(), String> {
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

    fn verify_drs_backup(&mut self, key: &str) -> Result<(), String> {
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
