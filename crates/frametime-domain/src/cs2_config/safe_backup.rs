use std::{
    io,
    path::{Path, PathBuf},
};

use super::{
    AutoexecBackup, CfgAssetDeployment, Cs2ConfigError, Cs2ConfigFs, Cs2ConfigRequest,
    OptimizationBackup, OptionalCfgAsset,
};
use crate::steam::Cs2Install;

#[derive(Debug)]
pub(super) struct Cs2Paths {
    pub(super) cfg_directory: PathBuf,
    pub(super) optimization_path: PathBuf,
    pub(super) optimization_backup_path: PathBuf,
    pub(super) autoexec_path: PathBuf,
    pub(super) autoexec_backup_path: PathBuf,
}

impl Cs2Paths {
    pub(super) fn optional_deployments(
        &self,
        request: &Cs2ConfigRequest,
    ) -> Vec<CfgAssetDeployment> {
        self.deployments_for_assets(request.optional_assets().iter().copied())
    }

    pub(super) fn deployments_for_assets(
        &self,
        assets: impl IntoIterator<Item = OptionalCfgAsset>,
    ) -> Vec<CfgAssetDeployment> {
        assets
            .into_iter()
            .map(|asset| CfgAssetDeployment {
                asset,
                source_name: asset.file_name(),
                target: self.cfg_directory.join(asset.file_name()),
                bytes: asset.bytes().to_vec(),
            })
            .collect()
    }
}

pub(super) fn valid_timestamp(value: &str) -> bool {
    value.len() == 16
        && value.bytes().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 if byte == b'-')
                || matches!(index, 10 if byte == b' ')
                || matches!(index, 13 if byte == b':')
                || (byte.is_ascii_digit() && !matches!(index, 4 | 7 | 10 | 13))
        })
}

pub(super) fn validate_install(install: &Cs2Install) -> Result<(), Cs2ConfigError> {
    let expected = install
        .library_root
        .join("steamapps")
        .join("common")
        .join(crate::steam::CS2_DIRECTORY);
    if install.install_root != expected {
        return Err(Cs2ConfigError::InvalidBinding);
    }
    Ok(())
}

pub(super) fn ensure_cfg_directory(
    root: &Path,
    path: &Path,
    files: &mut dyn Cs2ConfigFs,
) -> Result<(), Cs2ConfigError> {
    files.create_directory(path)?;
    assert_existing_path_safe(root, path)?;
    Ok(())
}

pub(super) fn ensure_safe_target(root: &Path, target: &Path) -> Result<(), Cs2ConfigError> {
    assert_existing_path_safe(root, target)?;
    Ok(())
}

pub(super) fn assert_existing_path_safe(
    root: &Path,
    candidate: &Path,
) -> Result<(), Cs2ConfigError> {
    let relative = candidate
        .strip_prefix(root)
        .map_err(|_| Cs2ConfigError::UntrustedPath(candidate.to_path_buf()))?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(Cs2ConfigError::UntrustedPath(candidate.to_path_buf()));
    }
    Ok(())
}

pub(super) fn read_optional(
    files: &mut dyn Cs2ConfigFs,
    path: &Path,
) -> Result<Option<Vec<u8>>, Cs2ConfigError> {
    match files.read_file(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Cs2ConfigError::Io(error)),
    }
}

pub(super) fn bytes_as_autoexec(bytes: &[u8]) -> Result<&str, Cs2ConfigError> {
    std::str::from_utf8(bytes).map_err(|_| Cs2ConfigError::AutoexecNotUtf8)
}

pub(super) fn create_optimization_backup(
    files: &mut dyn Cs2ConfigFs,
    path: &Path,
    original: Option<&[u8]>,
) -> Result<OptimizationBackup, Cs2ConfigError> {
    let Some(original) = original else {
        return Ok(OptimizationBackup::NotNeeded);
    };
    match files.create_file_new(path, original) {
        Ok(()) => match files.read_file(path) {
            Ok(readback) if readback == original => {
                Ok(OptimizationBackup::Created(path.to_path_buf()))
            }
            Ok(_) => Err(Cs2ConfigError::ReadbackMismatch {
                target: path.to_path_buf(),
                recovery: None,
            }),
            Err(error) => Err(Cs2ConfigError::Mutation {
                stage: "optimization.cfg backup readback",
                recovery: None,
                source: error,
            }),
        },
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Ok(OptimizationBackup::Retained(path.to_path_buf()))
        }
        Err(error) => Err(Cs2ConfigError::Mutation {
            stage: "optimization.cfg backup",
            recovery: None,
            source: error,
        }),
    }
}

pub(super) fn backup_path(backup: &OptimizationBackup) -> Option<PathBuf> {
    match backup {
        OptimizationBackup::Created(path) | OptimizationBackup::Retained(path) => {
            Some(path.clone())
        }
        OptimizationBackup::NotNeeded => None,
    }
}

pub(super) fn create_autoexec_backup(
    files: &mut dyn Cs2ConfigFs,
    path: &Path,
    original: Option<&[u8]>,
    autoexec_updated: bool,
) -> Result<AutoexecBackup, Cs2ConfigError> {
    if !autoexec_updated {
        return Ok(AutoexecBackup::NotNeeded);
    }
    let Some(original) = original else {
        return Ok(AutoexecBackup::NotNeeded);
    };
    match files.create_file_new(path, original) {
        Ok(()) => match files.read_file(path) {
            Ok(readback) if readback == original => Ok(AutoexecBackup::Created(path.to_path_buf())),
            Ok(_) => Err(Cs2ConfigError::ReadbackMismatch {
                target: path.to_path_buf(),
                recovery: None,
            }),
            Err(source) => Err(Cs2ConfigError::Mutation {
                stage: "autoexec.cfg backup readback",
                recovery: None,
                source,
            }),
        },
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Ok(AutoexecBackup::Retained(path.to_path_buf()))
        }
        Err(source) => Err(Cs2ConfigError::Mutation {
            stage: "autoexec.cfg backup",
            recovery: None,
            source,
        }),
    }
}
