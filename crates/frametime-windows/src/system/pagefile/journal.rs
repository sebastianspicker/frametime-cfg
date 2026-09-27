use super::*;
use crate::*;
pub(crate) fn persist_created_pagefile_token(
    trusted: &TrustedWorkDir,
    created: &CreatedPagefileToken,
) -> Result<(), String> {
    if created.object_path.is_empty() || created.relative_path.is_empty() {
        return Err("created pagefile token is incomplete".into());
    }
    let mut backup: BackupFile = read_json_trusted(trusted, BACKUP_FILE)
        .map_err(|error| format!("read persisted P1:8 backup: {error}"))?;
    let matching = backup
        .entries
        .iter_mut()
        .filter_map(|entry| match entry {
            BackupEntry::PagefileTransaction {
                step,
                created_object_path,
                created_relative_path,
                created_initial_size,
                created_maximum_size,
                mutation_intent,
                unknown,
                ..
            } if step == "P1:8" => Some((
                created_object_path,
                created_relative_path,
                created_initial_size,
                created_maximum_size,
                mutation_intent,
                unknown,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err("persisted backup does not contain exactly one P1:8 transaction".into());
    }
    let (object_path, relative_path, initial_size, maximum_size, intent, unknown) =
        matching.into_iter().next().expect("checked length");
    if !unknown.is_empty()
        || object_path.is_some()
        || relative_path.is_some()
        || initial_size.is_some()
        || maximum_size.is_some()
        || intent.as_deref() != Some("create_pending")
    {
        return Err("persisted P1:8 create journal has unsafe provenance".into());
    }
    *object_path = Some(created.object_path.clone());
    *relative_path = Some(created.relative_path.clone());
    *initial_size = Some(u64::from(created.initial_size));
    *maximum_size = Some(u64::from(created.maximum_size));
    *intent = Some("created".into());
    write_json_atomic_trusted(trusted, BACKUP_FILE, &backup)
        .map_err(|error| format!("atomically persist exact created pagefile token: {error}"))
}

pub(crate) fn compound_pagefile_error(
    prefix: &str,
    primary: String,
    rollback: Result<(), String>,
) -> String {
    match rollback {
        Ok(()) => {
            format!("{prefix}: {primary}; target and automatic-management compensation completed")
        }
        Err(rollback) => format!("{prefix}: {primary}; compensation also failed: {rollback}"),
    }
}
