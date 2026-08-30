//! Trusted, install-bound persistence for the CS2 CFG portion of Step 34.

use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    cs2::{ensure_autoexec_line, render_optimization_cfg_at},
    steam::Cs2Install,
};

const OPTIMIZATION_FILE: &str = "optimization.cfg";
const AUTOEXEC_FILE: &str = "autoexec.cfg";
const AUTOEXEC_BACKUP_FILE: &str = "autoexec.cfg.bak";

mod targets;
mod transaction;
pub use targets::Cs2ConfigTarget;
use transaction::{verify_bytes, write_and_verify};

#[derive(Debug, Error)]
pub enum Cs2ConfigError {
    #[error("CS2 install binding is not the exact discovered Steam layout")]
    InvalidBinding,
    #[error("CS2 CFG request timestamp must be yyyy-mm-dd hh:mm")]
    InvalidTimestamp,
    #[error("at least one closed optional CS2 CFG asset must be selected")]
    EmptyOptionalSelection,
    #[error("CS2 CFG target changed after backup capture: {0}")]
    PreconditionMismatch(PathBuf),
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
    FrameBaseline400,
    FrameRawUncapped,
    AudioEqCrisp,
    AudioEqSmooth,
    AudioLegacyLrIsolation,
    Competitive2026,
}

impl OptionalCfgAsset {
    pub const ALL: [Self; 15] = [
        Self::NetStable,
        Self::NetHighPing,
        Self::NetUnstable,
        Self::NetBad,
        Self::DebugHud,
        Self::DebugHudOff,
        Self::AudioStable,
        Self::AudioLowLatency025,
        Self::AudioLowLatency001,
        Self::FrameBaseline400,
        Self::FrameRawUncapped,
        Self::AudioEqCrisp,
        Self::AudioEqSmooth,
        Self::AudioLegacyLrIsolation,
        Self::Competitive2026,
    ];

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
            Self::FrameBaseline400 => "frame_baseline_400.cfg",
            Self::FrameRawUncapped => "frame_raw_uncapped.cfg",
            Self::AudioEqCrisp => "audio_eq_crisp.cfg",
            Self::AudioEqSmooth => "audio_eq_smooth.cfg",
            Self::AudioLegacyLrIsolation => "audio_legacy_lr_isolation.cfg",
            Self::Competitive2026 => "competitive_2026.cfg",
        }
    }

    #[must_use]
    pub const fn cli_token(self) -> &'static str {
        match self {
            Self::NetStable => "net-stable",
            Self::NetHighPing => "net-highping",
            Self::NetUnstable => "net-unstable",
            Self::NetBad => "net-bad",
            Self::DebugHud => "debug-hud",
            Self::DebugHudOff => "debug-hud-off",
            Self::AudioStable => "audio-stable",
            Self::AudioLowLatency025 => "audio-lowlatency-025",
            Self::AudioLowLatency001 => "audio-lowlatency-001",
            Self::FrameBaseline400 => "frame-baseline-400",
            Self::FrameRawUncapped => "frame-raw-uncapped",
            Self::AudioEqCrisp => "audio-eq-crisp",
            Self::AudioEqSmooth => "audio-eq-smooth",
            Self::AudioLegacyLrIsolation => "audio-legacy-lr-isolation",
            Self::Competitive2026 => "competitive-2026",
        }
    }

    #[must_use]
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::NetStable => "Network stable",
            Self::NetHighPing => "Network high ping",
            Self::NetUnstable => "Network unstable",
            Self::NetBad => "Network loss diagnostics",
            Self::DebugHud => "Debug HUD on",
            Self::DebugHudOff => "Debug HUD off",
            Self::AudioStable => "Audio stable",
            Self::AudioLowLatency025 => "Audio low latency 0.025",
            Self::AudioLowLatency001 => "Audio auto latency / 0.001 alias",
            Self::FrameBaseline400 => "Frame baseline 400",
            Self::FrameRawUncapped => "Frame raw uncapped",
            Self::AudioEqCrisp => "Audio EQ crisp",
            Self::AudioEqSmooth => "Audio EQ smooth",
            Self::AudioLegacyLrIsolation => "Audio legacy L/R isolation",
            Self::Competitive2026 => "Competitive 2026",
        }
    }

    #[must_use]
    pub fn from_cli_token(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|asset| asset.cli_token() == value)
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
            Self::FrameBaseline400 => include_bytes!("../../../assets/cfgs/frame_baseline_400.cfg"),
            Self::FrameRawUncapped => include_bytes!("../../../assets/cfgs/frame_raw_uncapped.cfg"),
            Self::AudioEqCrisp => include_bytes!("../../../assets/cfgs/audio_eq_crisp.cfg"),
            Self::AudioEqSmooth => include_bytes!("../../../assets/cfgs/audio_eq_smooth.cfg"),
            Self::AudioLegacyLrIsolation => {
                include_bytes!("../../../assets/cfgs/audio_legacy_lr_isolation.cfg")
            }
            Self::Competitive2026 => include_bytes!("../../../assets/cfgs/competitive_2026.cfg"),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cs2ConfigRequest {
    generated_at: String,
    optional_assets: BTreeSet<OptionalCfgAsset>,
    bootstrap_autoexec: bool,
}
impl Cs2ConfigRequest {
    pub fn new(
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
            bootstrap_autoexec: false,
        })
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
            bootstrap_autoexec: false,
        })
    }

    /// Explicitly authorizes a conditional `exec optimization.cfg` bootstrap.
    /// The default request only writes the generated optimization CFG.
    #[must_use]
    pub const fn with_autoexec_bootstrap(mut self) -> Self {
        self.bootstrap_autoexec = true;
        self
    }

    #[must_use]
    pub fn generated_at(&self) -> &str {
        &self.generated_at
    }

    #[must_use]
    pub fn optional_assets(&self) -> &BTreeSet<OptionalCfgAsset> {
        &self.optional_assets
    }

    #[must_use]
    pub const fn bootstraps_autoexec(&self) -> bool {
        self.bootstrap_autoexec
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
    pub autoexec_path: Option<PathBuf>,
    pub autoexec_backup_path: Option<PathBuf>,
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

/// A one-time sidecar backup of pre-existing user autoexec content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoexecBackup {
    Created(PathBuf),
    Retained(PathBuf),
    NotNeeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cs2ConfigWriteReport {
    pub optimization_path: PathBuf,
    pub autoexec_path: Option<PathBuf>,
    pub optimization_backup: OptimizationBackup,
    pub autoexec_backup: AutoexecBackup,
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

    pub(crate) fn backup_optional_targets(
        &self,
        assets: &BTreeSet<OptionalCfgAsset>,
    ) -> Result<Vec<(Cs2ConfigTarget, PathBuf)>, Cs2ConfigError> {
        if assets.is_empty() {
            return Err(Cs2ConfigError::EmptyOptionalSelection);
        }
        let paths = self.paths()?;
        let targets = Cs2ConfigTarget::for_optional_assets(assets)
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
#[derive(Debug)]
struct Cs2Paths {
    cfg_directory: PathBuf,
    optimization_path: PathBuf,
    optimization_backup_path: PathBuf,
    autoexec_path: PathBuf,
    autoexec_backup_path: PathBuf,
}

impl Cs2Paths {
    fn optional_deployments(&self, request: &Cs2ConfigRequest) -> Vec<CfgAssetDeployment> {
        self.deployments_for_assets(request.optional_assets().iter().copied())
    }

    fn deployments_for_assets(
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

fn ensure_cfg_directory(
    root: &Path,
    path: &Path,
    files: &mut dyn Cs2ConfigFs,
) -> Result<(), Cs2ConfigError> {
    files.create_directory(path)?;
    assert_existing_path_safe(root, path)?;
    Ok(())
}

fn ensure_safe_target(root: &Path, target: &Path) -> Result<(), Cs2ConfigError> {
    assert_existing_path_safe(root, target)?;
    Ok(())
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

fn create_autoexec_backup(
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[derive(Default)]
    struct FakeFs {
        files: BTreeMap<PathBuf, Vec<u8>>,
    }

    impl Cs2ConfigFs for FakeFs {
        fn create_directory(&mut self, _: &Path) -> io::Result<()> {
            Ok(())
        }
        fn read_file(&mut self, path: &Path) -> io::Result<Vec<u8>> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        }
        fn create_file_new(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
            if self.files.contains_key(path) {
                return Err(io::Error::from(io::ErrorKind::AlreadyExists));
            }
            self.files.insert(path.to_path_buf(), bytes.to_vec());
            Ok(())
        }
        fn atomic_replace(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
            self.files.insert(path.to_path_buf(), bytes.to_vec());
            Ok(())
        }
        fn remove_file(&mut self, path: &Path) -> io::Result<()> {
            self.files
                .remove(path)
                .map(|_| ())
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        }
    }

    fn controller() -> Cs2ConfigController {
        Cs2ConfigController::new(Cs2Install {
            steam_root: PathBuf::from("/steam"),
            library_root: PathBuf::from("/steam"),
            install_root: PathBuf::from("/steam/steamapps/common/Counter-Strike Global Offensive"),
        })
        .expect("lexically bound install")
    }

    #[test]
    fn preview_and_apply_use_only_the_injected_port() {
        let request =
            Cs2ConfigRequest::at("2026-08-23 12:34", [OptionalCfgAsset::NetStable]).unwrap();
        let controller = controller();
        let mut files = FakeFs::default();
        let preview = controller.preview(&request, &mut files).unwrap();
        assert!(
            String::from_utf8(preview.optimization_bytes.clone())
                .unwrap()
                .contains("Generated: 2026-08-23 12:34")
        );
        let report = controller.apply(&request, &mut files).unwrap();
        assert_eq!(
            files.files.get(&report.optimization_path).unwrap(),
            &preview.optimization_bytes
        );
        assert_eq!(report.optional_assets_written.len(), 1);
    }

    #[test]
    fn request_timestamp_is_supplied_not_observed() {
        assert!(Cs2ConfigRequest::at("bad", []).is_err());
        assert!(Cs2ConfigRequest::new("2026-08-23 12:34", []).is_ok());
    }
}
