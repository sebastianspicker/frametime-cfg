//! Install domain: package acquisition, package filtering, tweaks, signing, and reporting.
//!
//! The public API stays intentionally small. Implementation domains live in focused modules so
//! source handling, package transformation, and side-effecting stages can evolve independently.

use std::path::PathBuf;

use thiserror::Error;

mod archive;
pub mod catalog;
mod download;
mod fixture;
mod launch;
mod report;

mod copy;
mod copy_source;
mod options;
mod pipeline;
mod sign;
mod source;
mod tweaks;
mod uninstall;

pub use launch::looks_like_windows_pe as pe_check;
pub use options::options_from_wizard;
pub use pipeline::run_install;

use catalog::SelectionPresets;

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub work_directory: PathBuf,
    pub preset: String,
    pub package_root: Option<PathBuf>,
    /// Local archive (zip/7z/exe) to extract as package source.
    pub package_archive: Option<PathBuf>,
    /// HTTPS URL to download as package source.
    pub package_url: Option<String>,
    /// Expected SHA-256 for a directly supplied HTTPS package URL.
    ///
    /// This pin must come from the vendor or another independently trusted channel.
    pub package_sha256: Option<String>,
    /// driver-index.v1.json path for download resolution.
    pub driver_index: Option<PathBuf>,
    /// Package id within driver-index.
    pub driver_index_id: Option<String>,
    pub catalog_path: Option<PathBuf>,
    pub enable_install: bool,
    pub dry_run_install: bool,
    pub enable_run_report: bool,
    pub run_report_path: Option<PathBuf>,
    /// Explicit component select (merged onto preset).
    pub select: Vec<String>,
    pub deselect: Vec<String>,
    /// Import selection JSON path (array of ids or {selected:[]}).
    pub import_selection: Option<PathBuf>,
    /// Export prepared workspace to this directory.
    pub export_path: Option<PathBuf>,
    /// Build portable archive at this path (extension or archive_format selects zip/7z/sfx).
    pub archive_out: Option<PathBuf>,
    /// Archive format: zip | 7z | sfx (default zip; 7z/sfx need embedded helpers).
    pub archive_format: String,
    /// Run uninstall-drivers stage (pnputil / delete NVIDIA display drivers plan).
    pub uninstall_drivers: bool,
    /// Deep INF surgery: text-level strip of telemetry/GFE/Appx lines + markers.
    pub deep_inf: bool,
    /// Telemetry/strip tweak flags.
    pub disable_telemetry: bool,
    pub disable_installer_telemetry: bool,
    pub disable_nvcontainer: bool,
    pub disable_nvcamera: bool,
    pub disable_hdcp: bool,
    pub disable_mpo: bool,
    pub disable_hdaudio_sleep: bool,
    pub enable_msi: bool,
    pub clean_install: bool,
    pub unattended: bool,
    /// Apply post-install .reg markers as live registry (requires admin when true).
    pub live_registry_apply: bool,
    /// Extra setup.exe arguments.
    pub setup_args: Vec<String>,
    /// Attempt catalog rebuild / signtool probe (Not WHQL unless a real sign is proven).
    pub try_sign: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            work_directory: std::env::temp_dir()
                .join(format!("dfoundry-install-{}", std::process::id())),
            preset: SelectionPresets::CLEAN.to_string(),
            package_root: None,
            package_archive: None,
            package_url: None,
            package_sha256: None,
            driver_index: None,
            driver_index_id: None,
            catalog_path: None,
            enable_install: true,
            dry_run_install: true,
            enable_run_report: true,
            run_report_path: None,
            select: Vec::new(),
            deselect: Vec::new(),
            import_selection: None,
            export_path: None,
            archive_out: None,
            archive_format: "zip".into(),
            uninstall_drivers: false,
            deep_inf: false,
            disable_telemetry: true,
            disable_installer_telemetry: true,
            disable_nvcontainer: true,
            disable_nvcamera: true,
            disable_hdcp: false,
            disable_mpo: false,
            disable_hdaudio_sleep: false,
            enable_msi: true,
            clean_install: true,
            unattended: true,
            live_registry_apply: false,
            setup_args: Vec::new(),
            try_sign: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallResult {
    pub exit_code: i32,
    pub dry_run_install: bool,
    pub work_directory: PathBuf,
    pub prepared_root: Option<PathBuf>,
    pub package_root: Option<PathBuf>,
    pub run_report_path: Option<PathBuf>,
    pub kept_components: Vec<String>,
    pub stripped_components: Vec<String>,
    pub log: Vec<String>,
    pub messages: Vec<String>,
    pub used_synthetic_fixture: bool,
    pub export_path: Option<PathBuf>,
    pub archive_path: Option<PathBuf>,
    pub launch_command: Option<String>,
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("catalog not found: {0}")]
    CatalogMissing(PathBuf),
    #[error("unknown preset: {0}")]
    UnknownPreset(String),
    #[error("package root missing: {0}")]
    PackageMissing(PathBuf),
    #[error("force-install refused for synthetic/non-PE setup.exe")]
    ForceInstallRefused,
    #[error("force-install blocked: installer is not trusted: {0}")]
    UntrustedInstaller(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
mod tests;
