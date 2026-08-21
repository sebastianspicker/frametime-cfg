fn is_debloat_operation(operation: Operation) -> bool {
    operation.step.phase as u8 == 1 && operation.step.number == 13
}

#[cfg(windows)]
impl LiveBackend {
    fn inspect_debloat(&mut self) -> Result<Inspection, String> {
        let mut capability = DebloatCapability::new(NativeDebloatHost::new()?);
        capability.inspect()
    }

    fn capture_debloat_backups(&mut self, key: String) -> Result<Vec<BackupEntry>, String> {
        let mut capability = DebloatCapability::new(NativeDebloatHost::new()?);
        let snapshot = capability.capture()?;
        let entries = debloat_backup_entries(&snapshot, &key)?;
        let subjects = debloat_appx_subjects(&snapshot);
        self.debloat = Some(capability);
        self.debloat_appx_subjects = Some(subjects);
        self.captured_steps.insert(key);
        Ok(entries)
    }

    fn apply_debloat(&mut self) -> Result<(), String> {
        if self.transaction_lock.is_none() {
            return Err("P1:13 mutation requires the retained backup transaction lock".into());
        }
        self.debloat
            .as_mut()
            .ok_or("P1:13 mutation requires an exact captured inventory")?
            .apply_and_verify()
    }

    fn verify_debloat(&mut self) -> Result<(), String> {
        match self
            .debloat
            .as_mut()
            .ok_or("P1:13 verification requires the captured native capability")?
            .inspect()?
        {
            Inspection::Satisfied => Ok(()),
            _ => Err("P1:13 readback did not observe the exact desired state".into()),
        }
    }
}

#[cfg(not(windows))]
impl LiveBackend {
    fn inspect_debloat(&mut self) -> Result<Inspection, String> {
        Err("P1:13 native debloat requires Windows APIs".into())
    }
    fn capture_debloat_backups(&mut self, _: String) -> Result<Vec<BackupEntry>, String> {
        Err("P1:13 native debloat requires Windows APIs".into())
    }
    fn apply_debloat(&mut self) -> Result<(), String> {
        Err("P1:13 native debloat requires Windows APIs".into())
    }
    fn verify_debloat(&mut self) -> Result<(), String> {
        Err("P1:13 native debloat requires Windows APIs".into())
    }
}

#[cfg(windows)]
fn debloat_backup_entries(
    snapshot: &DebloatSnapshot,
    step: &str,
) -> Result<Vec<BackupEntry>, String> {
    if step != "P1:13" {
        return Err("debloat backup capture is not bound to P1:13".into());
    }
    let mut entries = Vec::new();
    for service in &snapshot.services {
        entries.push(BackupEntry::Service {
            step: step.into(),
            timestamp: timestamp(),
            name: service.identity.name().into(),
            original_start_type: service_start_name(service.start_type).into(),
            delayed_auto_start: service.delayed_auto_start,
            original_status: service_status_name(service.status).into(),
            unknown: BTreeMap::new(),
        });
    }
    for task in &snapshot.tasks {
        entries.push(BackupEntry::Scheduledtask {
            step: step.into(),
            timestamp: timestamp(),
            task_name: task.name.clone(),
            task_path: task.folder.path().into(),
            existed: true,
            was_enabled: task.enabled,
            script_path: None,
            unknown: BTreeMap::new(),
        });
    }
    for policy in &snapshot.policies {
        let (original_value, original_type, existed) = match &policy.original {
            None => (Value::Null, None, false),
            Some(value) if value.kind == 4 && value.bytes.len() == 4 => (
                Value::from(u32::from_le_bytes(
                    value
                        .bytes
                        .clone()
                        .try_into()
                        .map_err(|_| "P1:13 DWORD capture has invalid length")?,
                )),
                Some("DWord".into()),
                true,
            ),
            Some(_) => return Err("P1:13 policy capture is not an exact DWORD value".into()),
        };
        entries.push(BackupEntry::Registry {
            step: step.into(),
            timestamp: timestamp(),
            path: format!(
                "{}:\\{}",
                if policy.identity.current_user() {
                    "HKCU"
                } else {
                    "HKLM"
                },
                policy.identity.key()
            ),
            name: policy.identity.name().into(),
            original_value,
            original_type,
            existed,
            unknown: BTreeMap::new(),
        });
    }
    Ok(entries)
}

#[cfg(windows)]
fn debloat_appx_subjects(snapshot: &DebloatSnapshot) -> Vec<frametime_core::AppxRemovalSubject> {
    let mut subjects = snapshot
        .installed
        .iter()
        .map(|item| frametime_core::AppxRemovalSubject::Installed {
            full_name: item.full_name.clone(),
        })
        .chain(snapshot.provisioned.iter().map(|item| {
            frametime_core::AppxRemovalSubject::Provisioned {
                package_name: item.package_name.clone(),
            }
        }))
        .collect::<Vec<_>>();
    subjects.sort();
    subjects
}

#[cfg(windows)]
const fn service_start_name(value: ServiceStartType) -> &'static str {
    match value {
        ServiceStartType::Automatic => "Automatic",
        ServiceStartType::Manual => "Manual",
        ServiceStartType::Disabled => "Disabled",
        ServiceStartType::Boot => "Boot",
        ServiceStartType::System => "System",
    }
}
#[cfg(windows)]
const fn service_status_name(value: ServiceStatus) -> &'static str {
    match value {
        ServiceStatus::Running => "Running",
        ServiceStatus::Stopped => "Stopped",
    }
}
