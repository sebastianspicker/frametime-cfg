use std::{fs, os::windows::fs::MetadataExt, path::Path};

use frametime_domain::driver::{
    ArtifactLocator, DriverTransactionV2Stage, NvidiaComponentCatalog, NvidiaComponentSelection,
};

use crate::TrustedWorkDir;
use crate::driver::transaction::archive::archive_verified_transaction;
use crate::driver::transaction::{
    load_profile_backup_at, persist_driver_transaction, persist_profile_backup,
};
use crate::driver::{DriverTransaction, NativeNvapiDrs, load_driver_transaction_at};

use super::NvidiaPreparationRequest;
use super::package_binding::{
    verified_prepared_package_digest, verify_transaction_prepared_package,
};
use super::source_component_directories;

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const PREPARATION_DIRECTORIES: [&str; 2] = ["driver-extracted", "driver-package"];

pub(super) fn resolve_component_selection(
    request: &NvidiaPreparationRequest,
    source_directories: &[String],
) -> Result<NvidiaComponentSelection, String> {
    NvidiaComponentCatalog::canonical()
        .map_err(|error| format!("load NVIDIA component catalog: {error:?}"))?
        .resolve_selection(
            request.preset,
            source_directories,
            &request.select,
            &request.deselect,
        )
        .map_err(|error| format!("resolve NVIDIA component selection: {error:?}"))
}

pub(super) fn clear_stale_preparation_outputs(trusted: &TrustedWorkDir) -> Result<(), String> {
    ensure_replaceable_transaction(load_driver_transaction_at(trusted.path())?.as_ref())?;
    for leaf in PREPARATION_DIRECTORIES {
        remove_fixed_directory(trusted.path(), leaf)?;
    }
    Ok(())
}

fn ensure_replaceable_transaction(transaction: Option<&DriverTransaction>) -> Result<(), String> {
    match transaction {
        None => Ok(()),
        Some(transaction)
            if transaction
                .lifecycle
                .as_ref()
                .map(|lifecycle| lifecycle.stage)
                == Some(DriverTransactionV2Stage::Verified) =>
        {
            Ok(())
        }
        Some(_) => Err("prepare-nvidia refuses to replace an active driver transaction".into()),
    }
}

pub(super) fn resume_preparation(
    trusted: &TrustedWorkDir,
    locator: &ArtifactLocator,
    request: &NvidiaPreparationRequest,
) -> Result<Option<DriverTransaction>, String> {
    let Some(mut transaction) = load_driver_transaction_at(trusted.path())? else {
        return Ok(None);
    };
    let stage = transaction
        .lifecycle
        .as_ref()
        .ok_or("prepare-nvidia refuses to replace an existing v1 transaction")?
        .stage;
    if stage == DriverTransactionV2Stage::Verified {
        archive_verified_transaction(trusted.path(), &transaction)?;
        return Ok(None);
    }
    if !matches!(
        stage,
        DriverTransactionV2Stage::InstallerAuthenticated
            | DriverTransactionV2Stage::ProfileBackupPersisted
    ) {
        return Err("prepare-nvidia refuses to replace a transaction after preparation".into());
    }
    if &transaction.artifact.locator != locator {
        return Err("prepare-nvidia request does not match the existing artifact locator".into());
    }
    let selection = transaction
        .component_selection
        .as_ref()
        .ok_or("existing driver transaction lacks its component selection")?;
    let source_directories =
        source_component_directories(&trusted.path().join("driver-extracted"))?;
    let requested_selection = resolve_component_selection(request, &source_directories)?;
    if &requested_selection != selection {
        return Err(
            "prepare-nvidia request does not match the existing component selection".into(),
        );
    }
    if transaction.prepared_package_sha256.is_none() {
        transaction.prepared_package_sha256 =
            Some(verified_prepared_package_digest(trusted, selection)?);
        transaction = persist_driver_transaction(trusted.path(), &transaction)?;
    }
    verify_transaction_prepared_package(trusted, &transaction)?;
    if stage == DriverTransactionV2Stage::InstallerAuthenticated {
        finish_preparation(trusted, transaction).map(Some)
    } else {
        let profile_backup = transaction
            .lifecycle
            .as_ref()
            .and_then(|lifecycle| lifecycle.profile_backup.as_ref())
            .ok_or("existing driver transaction lacks its profile backup")?;
        let _ = load_profile_backup_at(trusted.path(), profile_backup)?;
        Ok(Some(transaction))
    }
}

fn remove_fixed_directory(root: &Path, leaf: &str) -> Result<(), String> {
    let path = root.join(leaf);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("inspect stale {leaf}: {error}")),
    };
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(format!(
            "refuse to remove stale {leaf}: fixed preparation output is not an ordinary directory"
        ));
    }
    fs::remove_dir_all(&path).map_err(|error| format!("remove stale {leaf}: {error}"))?;
    if path.exists() {
        Err(format!("stale {leaf} remains after removal"))
    } else {
        Ok(())
    }
}

pub(super) fn finish_preparation(
    trusted: &TrustedWorkDir,
    mut transaction: DriverTransaction,
) -> Result<DriverTransaction, String> {
    let lifecycle = transaction
        .lifecycle
        .as_ref()
        .ok_or("v2 driver transaction lacks lifecycle state")?;
    if lifecycle.stage != DriverTransactionV2Stage::InstallerAuthenticated {
        return Err("profile backup requires the installer-authenticated stage".into());
    }
    let mut drs = NativeNvapiDrs::load().map_err(|error| error.to_string())?;
    let snapshot = drs
        .capture_customized_profiles()
        .map_err(|error| error.to_string())?;
    let digest = persist_profile_backup(trusted.path(), &snapshot)?;
    transaction.lifecycle = Some(
        lifecycle
            .transition(
                DriverTransactionV2Stage::ProfileBackupPersisted,
                Some(digest),
            )
            .map_err(|error| format!("advance profile backup stage: {error:?}"))?,
    );
    persist_driver_transaction(trusted.path(), &transaction)
}

#[cfg(test)]
mod tests {
    use super::ensure_replaceable_transaction;
    use crate::driver::transaction::tests::transaction;
    use frametime_domain::driver::{
        DriverTransactionV2, DriverTransactionV2Stage, NVIDIA_TRANSACTION_V2_SCHEMA_VERSION,
    };

    #[test]
    fn stale_outputs_are_removed_only_without_active_transaction() {
        assert!(ensure_replaceable_transaction(None).is_ok());
        let mut transaction = transaction();
        assert!(ensure_replaceable_transaction(Some(&transaction)).is_err());
        transaction.lifecycle = Some(DriverTransactionV2 {
            schema_version: NVIDIA_TRANSACTION_V2_SCHEMA_VERSION,
            transaction_id: "completed".into(),
            stage: DriverTransactionV2Stage::Verified,
            profile_backup: None,
        });
        assert!(ensure_replaceable_transaction(Some(&transaction)).is_ok());
    }
}
