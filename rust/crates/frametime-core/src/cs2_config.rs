//! Trusted, install-bound persistence for the CS2 CFG portion of Step 34.

use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    cs2::{ensure_autoexec_line, render_optimization_cfg_at},
    logging::legacy_timestamp_now,
    steam::{Cs2Install, SteamError, trusted_directory, trusted_file_under},
};

const OPTIMIZATION_FILE: &str = "optimization.cfg";
const AUTOEXEC_FILE: &str = "autoexec.cfg";

mod native;
mod targets;
mod transaction;
pub use native::NativeCs2ConfigFs;
pub use targets::Cs2ConfigTarget;
use transaction::{verify_bytes, write_and_verify};

#[derive(Debug, Error)]
pub enum Cs2ConfigError {
    #[error(transparent)]
    Steam(#[from] SteamError),
    #[error("CS2 install binding is not the exact discovered Steam layout")]
    InvalidBinding,
    #[error("CS2 CFG request timestamp must be yyyy-mm-dd hh:mm")]
    InvalidTimestamp,
    #[error("CS2 CFG path is not a trusted real path: {0}")]
    UntrustedPath(PathBuf),
    #[error("existing autoexec.cfg is not valid UTF-8; refusing to rewrite user content")]
    AutoexecNotUtf8,
    #[error("CS2 CFG I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("{stage} failed; recovery evidence retained at {recovery:?}: {source}")]
    Mutation {
        stage: &'static str,
        recovery: Option<PathBuf>,
        #[source]
        source: io::Error,
    },
    #[error("exact readback failed for {target}; recovery evidence retained at {recovery:?}")]
    ReadbackMismatch {
        target: PathBuf,
        recovery: Option<PathBuf>,
    },
}

/// Fixed assets only: no arbitrary path or byte input can cross this boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OptionalCfgAsset {
    NetStable,
    NetHighPing,
    NetUnstable,
    NetBad,
    DebugHud,
    DebugHudOff,
    AudioStable,
    AudioLowLatency025,
    AudioLowLatency001,
}

impl OptionalCfgAsset {
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::NetStable => "net_stable.cfg",
            Self::NetHighPing => "net_highping.cfg",
            Self::NetUnstable => "net_unstable.cfg",
            Self::NetBad => "net_bad.cfg",
            Self::DebugHud => "debug_hud.cfg",
            Self::DebugHudOff => "debug_hud_off.cfg",
            Self::AudioStable => "audio_stable.cfg",
            Self::AudioLowLatency025 => "audio_lowlatency_025.cfg",
            Self::AudioLowLatency001 => "audio_lowlatency_001.cfg",
        }
    }
    #[must_use]
    pub const fn bytes(self) -> &'static [u8] {
        match self {
            Self::NetStable => include_bytes!("../../../assets/cfgs/net_stable.cfg"),
            Self::NetHighPing => include_bytes!("../../../assets/cfgs/net_highping.cfg"),
            Self::NetUnstable => include_bytes!("../../../assets/cfgs/net_unstable.cfg"),
            Self::NetBad => include_bytes!("../../../assets/cfgs/net_bad.cfg"),
            Self::DebugHud => include_bytes!("../../../assets/cfgs/debug_hud.cfg"),
            Self::DebugHudOff => include_bytes!("../../../assets/cfgs/debug_hud_off.cfg"),
            Self::AudioStable => include_bytes!("../../../assets/cfgs/audio_stable.cfg"),
            Self::AudioLowLatency025 => {
                include_bytes!("../../../assets/cfgs/audio_lowlatency_025.cfg")
            }
            Self::AudioLowLatency001 => {
                include_bytes!("../../../assets/cfgs/audio_lowlatency_001.cfg")
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cs2ConfigRequest {
    generated_at: String,
    optional_assets: BTreeSet<OptionalCfgAsset>,
}
impl Cs2ConfigRequest {
    #[must_use]
    pub fn new(optional_assets: impl IntoIterator<Item = OptionalCfgAsset>) -> Self {
        let timestamp = legacy_timestamp_now();
        Self {
            generated_at: timestamp[..16].to_owned(),
            optional_assets: optional_assets.into_iter().collect(),
        }
    }

    pub fn at(
        generated_at: impl Into<String>,
        optional_assets: impl IntoIterator<Item = OptionalCfgAsset>,
    ) -> Result<Self, Cs2ConfigError> {
        let generated_at = generated_at.into();
        if !valid_timestamp(&generated_at) {
            return Err(Cs2ConfigError::InvalidTimestamp);
        }
        Ok(Self {
            generated_at,
            optional_assets: optional_assets.into_iter().collect(),
        })
    }

    #[must_use]
    pub fn generated_at(&self) -> &str {
        &self.generated_at
    }

    #[must_use]
    pub fn optional_assets(&self) -> &BTreeSet<OptionalCfgAsset> {
        &self.optional_assets
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgAssetDeployment {
    pub asset: OptionalCfgAsset,
    pub source_name: &'static str,
    pub target: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cs2ConfigPreview {
    pub cfg_directory: PathBuf,
    pub optimization_path: PathBuf,
    pub autoexec_path: PathBuf,
    pub optimization_bytes: Vec<u8>,
    pub autoexec_would_change: bool,
    pub optional_assets: Vec<CfgAssetDeployment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizationBackup {
    Created(PathBuf),
    Retained(PathBuf),
    NotNeeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cs2ConfigWriteReport {
    pub optimization_path: PathBuf,
    pub autoexec_path: PathBuf,
    pub optimization_backup: OptimizationBackup,
    pub autoexec_updated: bool,
    pub optional_assets_written: Vec<PathBuf>,
}

/// Narrow mutation seam; production trust checks remain outside this trait.
pub trait Cs2ConfigFs {
    fn create_directory(&mut self, path: &Path) -> io::Result<()>;
    fn read_file(&mut self, path: &Path) -> io::Result<Vec<u8>>;
    fn create_file_new(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    fn atomic_replace(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    fn remove_file(&mut self, path: &Path) -> io::Result<()>;
}

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

    pub fn preview(&self, request: &Cs2ConfigRequest) -> Result<Cs2ConfigPreview, Cs2ConfigError> {
        let paths = self.paths()?;
        ensure_safe_target(&paths.cfg_directory, &paths.optimization_path)?;
        ensure_safe_target(&paths.cfg_directory, &paths.optimization_backup_path)?;
        ensure_safe_target(&paths.cfg_directory, &paths.autoexec_path)?;
        for deployment in paths.optional_deployments(request) {
            ensure_safe_target(&paths.cfg_directory, &deployment.target)?;
        }
        let existing = read_optional(&mut NativeCs2ConfigFs, &paths.autoexec_path)?;
        let existing = existing
            .as_deref()
            .map(bytes_as_autoexec)
            .transpose()?
            .unwrap_or_default();
        Ok(Cs2ConfigPreview {
            cfg_directory: paths.cfg_directory.clone(),
            optimization_path: paths.optimization_path.clone(),
            autoexec_path: paths.autoexec_path.clone(),
            optimization_bytes: render_optimization_cfg_at(request.generated_at()).into_bytes(),
            autoexec_would_change: ensure_autoexec_line(existing) != existing,
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
        ensure_safe_target(&paths.cfg_directory, &paths.autoexec_path)?;
        let optional = paths.optional_deployments(request);
        for deployment in &optional {
            ensure_safe_target(&paths.cfg_directory, &deployment.target)?;
        }

        let original_optimization = read_optional(files, &paths.optimization_path)?;
        let autoexec_before = read_optional(files, &paths.autoexec_path)?;
        let autoexec_before = autoexec_before
            .as_deref()
            .map(bytes_as_autoexec)
            .transpose()?;
        let expected_optimization = render_optimization_cfg_at(request.generated_at()).into_bytes();
        let expected_autoexec = autoexec_before
            .map(ensure_autoexec_line)
            .unwrap_or_else(|| ensure_autoexec_line(""))
            .into_bytes();
        let autoexec_updated =
            autoexec_before.is_none_or(|value| value.as_bytes() != expected_autoexec.as_slice());

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
        if autoexec_updated {
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
            autoexec_path: paths.autoexec_path,
            optimization_backup: backup,
            autoexec_updated,
            optional_assets_written,
        })
    }

    /// Confirms every requested managed file without changing the install.
    pub fn verify(
        &self,
        request: &Cs2ConfigRequest,
        files: &mut dyn Cs2ConfigFs,
    ) -> Result<(), Cs2ConfigError> {
        let paths = self.paths()?;
        ensure_safe_target(&paths.cfg_directory, &paths.optimization_path)?;
        ensure_safe_target(&paths.cfg_directory, &paths.autoexec_path)?;
        let expected_optimization = render_optimization_cfg_at(request.generated_at()).into_bytes();
        verify_bytes(files, &paths.optimization_path, &expected_optimization)?;
        let autoexec = files.read_file(&paths.autoexec_path)?;
        let autoexec = bytes_as_autoexec(&autoexec)?;
        if ensure_autoexec_line(autoexec) != autoexec {
            return Err(Cs2ConfigError::ReadbackMismatch {
                target: paths.autoexec_path,
                recovery: None,
            });
        }
        for deployment in paths.optional_deployments(request) {
            ensure_safe_target(&paths.cfg_directory, &deployment.target)?;
            verify_bytes(files, &deployment.target, &deployment.bytes)?;
        }
        Ok(())
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
        let paths = self.paths()?;
        let targets = Cs2ConfigTarget::for_request(request)
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
        trusted_directory(&cfg_parent)?;
        let cfg_directory = cfg_parent.join("cfg");
        assert_existing_path_safe(&self.install.install_root, &cfg_directory)?;
        Ok(Cs2Paths {
            optimization_path: cfg_directory.join(OPTIMIZATION_FILE),
            optimization_backup_path: cfg_directory.join("optimization.cfg.bak"),
            autoexec_path: cfg_directory.join(AUTOEXEC_FILE),
            cfg_directory,
        })
    }
}
#[derive(Debug)]
struct Cs2Paths {
    cfg_directory: PathBuf,
    optimization_path: PathBuf,
    optimization_backup_path: PathBuf,
    autoexec_path: PathBuf,
}

impl Cs2Paths {
    fn optional_deployments(&self, request: &Cs2ConfigRequest) -> Vec<CfgAssetDeployment> {
        request
            .optional_assets()
            .iter()
            .map(|asset| CfgAssetDeployment {
                asset: *asset,
                source_name: asset.file_name(),
                target: self.cfg_directory.join(asset.file_name()),
                bytes: asset.bytes().to_vec(),
            })
            .collect()
    }
}

fn valid_timestamp(value: &str) -> bool {
    value.len() == 16
        && value.bytes().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 if byte == b'-')
                || matches!(index, 10 if byte == b' ')
                || matches!(index, 13 if byte == b':')
                || (byte.is_ascii_digit() && !matches!(index, 4 | 7 | 10 | 13))
        })
}

fn validate_install(install: &Cs2Install) -> Result<(), Cs2ConfigError> {
    trusted_directory(&install.steam_root)?;
    trusted_directory(&install.library_root)?;
    let expected = install
        .library_root
        .join("steamapps")
        .join("common")
        .join(crate::steam::CS2_DIRECTORY);
    if install.install_root != expected {
        return Err(Cs2ConfigError::InvalidBinding);
    }
    trusted_directory(&install.install_root)?;
    trusted_file_under(
        &install.library_root,
        &install.install_root.join("game/bin/win64/cs2.exe"),
    )?;
    Ok(())
}

fn ensure_cfg_directory(
    root: &Path,
    path: &Path,
    files: &mut dyn Cs2ConfigFs,
) -> Result<(), Cs2ConfigError> {
    files.create_directory(path)?;
    assert_existing_path_safe(root, path)?;
    trusted_directory(path)?;
    Ok(())
}

fn ensure_safe_target(root: &Path, target: &Path) -> Result<(), Cs2ConfigError> {
    assert_existing_path_safe(root, target)?;
    match fs::symlink_metadata(target) {
        Ok(metadata) if is_reparse(&metadata) || !metadata.is_file() => {
            Err(Cs2ConfigError::UntrustedPath(target.to_path_buf()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Cs2ConfigError::Io(error)),
    }
}

fn assert_existing_path_safe(root: &Path, candidate: &Path) -> Result<(), Cs2ConfigError> {
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
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if is_reparse(&metadata) => {
                return Err(Cs2ConfigError::UntrustedPath(current));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(Cs2ConfigError::Io(error)),
        }
    }
    Ok(())
}

fn read_optional(
    files: &mut dyn Cs2ConfigFs,
    path: &Path,
) -> Result<Option<Vec<u8>>, Cs2ConfigError> {
    match files.read_file(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Cs2ConfigError::Io(error)),
    }
}

fn bytes_as_autoexec(bytes: &[u8]) -> Result<&str, Cs2ConfigError> {
    std::str::from_utf8(bytes).map_err(|_| Cs2ConfigError::AutoexecNotUtf8)
}

fn create_optimization_backup(
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

fn backup_path(backup: &OptimizationBackup) -> Option<PathBuf> {
    match backup {
        OptimizationBackup::Created(path) | OptimizationBackup::Retained(path) => {
            Some(path.clone())
        }
        OptimizationBackup::NotNeeded => None,
    }
}

fn is_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x0400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}
