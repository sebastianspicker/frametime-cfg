use std::{collections::BTreeSet, io, path::PathBuf};

use super::safe_backup::{
    Cs2Paths, assert_existing_path_safe, backup_path, bytes_as_autoexec, create_autoexec_backup,
    create_optimization_backup, ensure_cfg_directory, ensure_safe_target, read_optional,
    validate_install,
};
use super::transaction::{verify_bytes, write_and_verify};
use super::{AUTOEXEC_BACKUP_FILE, AUTOEXEC_FILE, OPTIMIZATION_FILE};
use super::{
    AutoexecBackup, Cs2ConfigError, Cs2ConfigFs, Cs2ConfigPreview, Cs2ConfigRequest,
    Cs2ConfigTarget, Cs2ConfigWriteReport, OptionalCfgAsset,
};
use crate::{
    cs2::{ensure_autoexec_line, render_optimization_cfg_at},
    steam::Cs2Install,
};

#[derive(Debug, Clone)]
pub struct Cs2ConfigController {
    install: Cs2Install,
}

impl Cs2ConfigController {
    pub fn new(install: Cs2Install) -> Result<Self, Cs2ConfigError> {
        validate_install(&install)?;
        Ok(Self { install })
    }

    #[must_use]
    pub fn install(&self) -> &Cs2Install {
        &self.install
    }

    pub fn preview(
        &self,
        request: &Cs2ConfigRequest,
        files: &mut dyn Cs2ConfigFs,
    ) -> Result<Cs2ConfigPreview, Cs2ConfigError> {
        let paths = self.paths()?;
        ensure_safe_target(&paths.cfg_directory, &paths.optimization_path)?;
        ensure_safe_target(&paths.cfg_directory, &paths.optimization_backup_path)?;
        if request.bootstraps_autoexec() {
            ensure_safe_target(&paths.cfg_directory, &paths.autoexec_path)?;
            ensure_safe_target(&paths.cfg_directory, &paths.autoexec_backup_path)?;
        }
        for deployment in paths.optional_deployments(request) {
            ensure_safe_target(&paths.cfg_directory, &deployment.target)?;
        }
        let (autoexec_path, autoexec_backup_path, autoexec_would_change) =
            if request.bootstraps_autoexec() {
                let existing = read_optional(files, &paths.autoexec_path)?;
                let existing = existing
                    .as_deref()
                    .map(bytes_as_autoexec)
                    .transpose()?
                    .unwrap_or_default();
                (
                    Some(paths.autoexec_path.clone()),
                    Some(paths.autoexec_backup_path.clone()),
                    ensure_autoexec_line(existing) != existing,
                )
            } else {
                (None, None, false)
            };
        Ok(Cs2ConfigPreview {
            cfg_directory: paths.cfg_directory.clone(),
            optimization_path: paths.optimization_path.clone(),
            autoexec_path,
            autoexec_backup_path,
            optimization_bytes: render_optimization_cfg_at(request.generated_at()).into_bytes(),
            autoexec_would_change,
            optional_assets: paths.optional_deployments(request),
        })
    }

    pub fn apply(
        &self,
        request: &Cs2ConfigRequest,
        files: &mut dyn Cs2ConfigFs,
    ) -> Result<Cs2ConfigWriteReport, Cs2ConfigError> {
        let paths = self.paths()?;
        ensure_cfg_directory(&self.install.install_root, &paths.cfg_directory, files)?;
        ensure_safe_target(&paths.cfg_directory, &paths.optimization_path)?;
        ensure_safe_target(&paths.cfg_directory, &paths.optimization_backup_path)?;
        if request.bootstraps_autoexec() {
            ensure_safe_target(&paths.cfg_directory, &paths.autoexec_path)?;
            ensure_safe_target(&paths.cfg_directory, &paths.autoexec_backup_path)?;
        }
        let optional = paths.optional_deployments(request);
        for deployment in &optional {
            ensure_safe_target(&paths.cfg_directory, &deployment.target)?;
        }

        let original_optimization = read_optional(files, &paths.optimization_path)?;
        let autoexec_original = if request.bootstraps_autoexec() {
            read_optional(files, &paths.autoexec_path)?
        } else {
            None
        };
        let autoexec_before = autoexec_original
            .as_deref()
            .map(bytes_as_autoexec)
            .transpose()?;
        let expected_optimization = render_optimization_cfg_at(request.generated_at()).into_bytes();
        let expected_autoexec = request.bootstraps_autoexec().then(|| {
            autoexec_before
                .map(ensure_autoexec_line)
                .unwrap_or_else(|| ensure_autoexec_line(""))
                .into_bytes()
        });
        let autoexec_updated = expected_autoexec.as_ref().is_some_and(|expected| {
            autoexec_before.is_none_or(|value| value.as_bytes() != expected.as_slice())
        });

        let backup = create_optimization_backup(
            files,
            &paths.optimization_backup_path,
            original_optimization.as_deref(),
        )?;
        let recovery = backup_path(&backup);
        write_and_verify(
            files,
            &paths.optimization_path,
            &expected_optimization,
            "optimization.cfg replacement",
            recovery.clone(),
        )?;
        let autoexec_backup = if request.bootstraps_autoexec() {
            create_autoexec_backup(
                files,
                &paths.autoexec_backup_path,
                autoexec_original.as_deref(),
                autoexec_updated,
            )?
        } else {
            AutoexecBackup::NotNeeded
        };
        if let Some(expected_autoexec) = expected_autoexec.filter(|_| autoexec_updated) {
            write_and_verify(
                files,
                &paths.autoexec_path,
                &expected_autoexec,
                "autoexec.cfg replacement",
                recovery.clone(),
            )?;
        }
        let mut optional_assets_written = Vec::with_capacity(optional.len());
        for deployment in optional {
            write_and_verify(
                files,
                &deployment.target,
                &deployment.bytes,
                "optional CFG replacement",
                recovery.clone(),
            )?;
            optional_assets_written.push(deployment.target);
        }
        Ok(Cs2ConfigWriteReport {
            optimization_path: paths.optimization_path,
            autoexec_path: request.bootstraps_autoexec().then_some(paths.autoexec_path),
            optimization_backup: backup,
            autoexec_backup,
            autoexec_updated,
            optional_assets_written,
        })
    }

    /// Confirms rendered managed-file bytes and, when explicitly requested,
    /// the idempotent bootstrap line. It does not establish runtime execution
    /// or sentinel proof.
    pub fn verify(
        &self,
        request: &Cs2ConfigRequest,
        files: &mut dyn Cs2ConfigFs,
    ) -> Result<(), Cs2ConfigError> {
        let paths = self.paths()?;
        ensure_safe_target(&paths.cfg_directory, &paths.optimization_path)?;
        if request.bootstraps_autoexec() {
            ensure_safe_target(&paths.cfg_directory, &paths.autoexec_path)?;
            ensure_safe_target(&paths.cfg_directory, &paths.autoexec_backup_path)?;
        }
        let expected_optimization = render_optimization_cfg_at(request.generated_at()).into_bytes();
        verify_bytes(files, &paths.optimization_path, &expected_optimization)?;
        if request.bootstraps_autoexec() {
            let autoexec = files.read_file(&paths.autoexec_path)?;
            let autoexec = bytes_as_autoexec(&autoexec)?;
            if ensure_autoexec_line(autoexec) != autoexec {
                return Err(Cs2ConfigError::ReadbackMismatch {
                    target: paths.autoexec_path,
                    recovery: None,
                });
            }
        }
        for deployment in paths.optional_deployments(request) {
            ensure_safe_target(&paths.cfg_directory, &deployment.target)?;
            verify_bytes(files, &deployment.target, &deployment.bytes)?;
        }
        Ok(())
    }

    /// Deploys only explicitly selected, embedded CFG assets. It does not
    /// rewrite optimization.cfg or autoexec.cfg and does not execute a CFG.
    pub fn apply_optional_assets(
        &self,
        assets: &BTreeSet<OptionalCfgAsset>,
        files: &mut dyn Cs2ConfigFs,
    ) -> Result<Vec<PathBuf>, Cs2ConfigError> {
        if assets.is_empty() {
            return Err(Cs2ConfigError::EmptyOptionalSelection);
        }
        let paths = self.paths()?;
        ensure_cfg_directory(&self.install.install_root, &paths.cfg_directory, files)?;
        let deployments = paths.deployments_for_assets(assets.iter().copied());
        let mut written = Vec::with_capacity(deployments.len());
        for deployment in deployments {
            ensure_safe_target(&paths.cfg_directory, &deployment.target)?;
            write_and_verify(
                files,
                &deployment.target,
                &deployment.bytes,
                "optional CFG replacement",
                None,
            )?;
            written.push(deployment.target);
        }
        Ok(written)
    }

    /// Verifies only the selected optional assets through exact byte readback.
    pub fn verify_optional_assets(
        &self,
        assets: &BTreeSet<OptionalCfgAsset>,
        files: &mut dyn Cs2ConfigFs,
    ) -> Result<(), Cs2ConfigError> {
        if assets.is_empty() {
            return Err(Cs2ConfigError::EmptyOptionalSelection);
        }
        let paths = self.paths()?;
        for deployment in paths.deployments_for_assets(assets.iter().copied()) {
            ensure_safe_target(&paths.cfg_directory, &deployment.target)?;
            verify_bytes(files, &deployment.target, &deployment.bytes)?;
        }
        Ok(())
    }

    pub(crate) fn apply_optional_assets_if_unchanged(
        &self,
        assets: &BTreeSet<OptionalCfgAsset>,
        snapshots: &[(Cs2ConfigTarget, Option<&[u8]>)],
        files: &mut dyn Cs2ConfigFs,
    ) -> Result<Vec<PathBuf>, Cs2ConfigError> {
        if assets.is_empty() {
            return Err(Cs2ConfigError::EmptyOptionalSelection);
        }
        let paths = self.paths()?;
        ensure_cfg_directory(&self.install.install_root, &paths.cfg_directory, files)?;
        let deployments = paths.deployments_for_assets(assets.iter().copied());
        if deployments.len() != snapshots.len()
            || deployments
                .iter()
                .map(|deployment| Cs2ConfigTarget::from_optional_asset(deployment.asset))
                .ne(snapshots.iter().map(|(target, _)| *target))
        {
            return Err(Cs2ConfigError::InvalidBinding);
        }
        let mut written = Vec::with_capacity(deployments.len());
        for (deployment, (_, original)) in deployments.into_iter().zip(snapshots) {
            ensure_safe_target(&paths.cfg_directory, &deployment.target)?;
            if read_optional(files, &deployment.target)?.as_deref() != *original {
                return Err(Cs2ConfigError::PreconditionMismatch(deployment.target));
            }
            write_and_verify(
                files,
                &deployment.target,
                &deployment.bytes,
                "optional CFG replacement",
                None,
            )?;
            written.push(deployment.target);
        }
        Ok(written)
    }

    /// Restores one complete, already validated closed target set.
    ///
    /// The caller must validate its path-free backup transaction before this
    /// method. This method revalidates all target paths and reads each result
    /// back exactly, including absence for targets captured as absent.
    pub(crate) fn restore(
        &self,
        request: &Cs2ConfigRequest,
        snapshots: &[(Cs2ConfigTarget, Option<&[u8]>)],
        files: &mut dyn Cs2ConfigFs,
    ) -> Result<(), Cs2ConfigError> {
        let targets = self.backup_targets(request)?;
        self.restore_targets(&targets, snapshots, files)
    }

    pub(crate) fn restore_optional_assets(
        &self,
        assets: &BTreeSet<OptionalCfgAsset>,
        snapshots: &[(Cs2ConfigTarget, Option<&[u8]>)],
        files: &mut dyn Cs2ConfigFs,
    ) -> Result<(), Cs2ConfigError> {
        let targets = self.backup_optional_targets(assets)?;
        self.restore_targets(&targets, snapshots, files)
    }

    fn restore_targets(
        &self,
        targets: &[(Cs2ConfigTarget, PathBuf)],
        snapshots: &[(Cs2ConfigTarget, Option<&[u8]>)],
        files: &mut dyn Cs2ConfigFs,
    ) -> Result<(), Cs2ConfigError> {
        if targets.len() != snapshots.len()
            || targets
                .iter()
                .map(|(target, _)| *target)
                .ne(snapshots.iter().map(|(target, _)| *target))
        {
            return Err(Cs2ConfigError::InvalidBinding);
        }
        for ((_, path), (_, original)) in targets.iter().zip(snapshots) {
            let root = path
                .parent()
                .ok_or_else(|| Cs2ConfigError::UntrustedPath(path.clone()))?;
            ensure_safe_target(root, path)?;
            if let Some(bytes) = original {
                write_and_verify(files, path, bytes, "CS2 CFG restore replacement", None)?;
            } else {
                match files.remove_file(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(source) => {
                        return Err(Cs2ConfigError::Mutation {
                            stage: "CS2 CFG restore deletion",
                            recovery: None,
                            source,
                        });
                    }
                }
                if read_optional(files, path)?.is_some() {
                    return Err(Cs2ConfigError::ReadbackMismatch {
                        target: path.clone(),
                        recovery: None,
                    });
                }
            }
        }
        Ok(())
    }

    /// Resolves only the closed write set after re-validating the bound install.
    /// Paths stay inside this controller and are intentionally never serialized.
    pub(crate) fn backup_targets(
        &self,
        request: &Cs2ConfigRequest,
    ) -> Result<Vec<(Cs2ConfigTarget, PathBuf)>, Cs2ConfigError> {
        self.backup_selected_targets(Cs2ConfigTarget::for_request(request))
    }

    pub(crate) fn backup_optional_targets(
        &self,
        assets: &BTreeSet<OptionalCfgAsset>,
    ) -> Result<Vec<(Cs2ConfigTarget, PathBuf)>, Cs2ConfigError> {
        if assets.is_empty() {
            return Err(Cs2ConfigError::EmptyOptionalSelection);
        }
        self.backup_selected_targets(Cs2ConfigTarget::for_optional_assets(assets))
    }

    fn backup_selected_targets(
        &self,
        selected: Vec<Cs2ConfigTarget>,
    ) -> Result<Vec<(Cs2ConfigTarget, PathBuf)>, Cs2ConfigError> {
        let paths = self.paths()?;
        let targets = selected
            .into_iter()
            .map(|target| (target, paths.cfg_directory.join(target.file_name())))
            .collect::<Vec<_>>();
        for (_, target) in &targets {
            ensure_safe_target(&paths.cfg_directory, target)?;
        }
        Ok(targets)
    }

    fn paths(&self) -> Result<Cs2Paths, Cs2ConfigError> {
        validate_install(&self.install)?;
        let cfg_parent = self.install.install_root.join("game/csgo");
        assert_existing_path_safe(&self.install.install_root, &cfg_parent)?;
        let cfg_directory = cfg_parent.join("cfg");
        assert_existing_path_safe(&self.install.install_root, &cfg_directory)?;
        Ok(Cs2Paths {
            optimization_path: cfg_directory.join(OPTIMIZATION_FILE),
            optimization_backup_path: cfg_directory.join("optimization.cfg.bak"),
            autoexec_path: cfg_directory.join(AUTOEXEC_FILE),
            autoexec_backup_path: cfg_directory.join(AUTOEXEC_BACKUP_FILE),
            cfg_directory,
        })
    }
}
