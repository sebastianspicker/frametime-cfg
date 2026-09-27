use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
};

use super::safe_backup::valid_timestamp;
use super::{Cs2ConfigError, OptionalCfgAsset};

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
        Self::from_parts(generated_at.into(), optional_assets.into_iter().collect())
    }

    pub fn at(
        generated_at: impl Into<String>,
        optional_assets: impl IntoIterator<Item = OptionalCfgAsset>,
    ) -> Result<Self, Cs2ConfigError> {
        Self::new(generated_at, optional_assets)
    }

    fn from_parts(
        generated_at: String,
        optional_assets: BTreeSet<OptionalCfgAsset>,
    ) -> Result<Self, Cs2ConfigError> {
        if !valid_timestamp(&generated_at) {
            return Err(Cs2ConfigError::InvalidTimestamp);
        }
        Ok(Self {
            generated_at,
            optional_assets,
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
