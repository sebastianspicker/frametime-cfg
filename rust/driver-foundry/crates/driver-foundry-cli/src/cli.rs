use clap::{Args, Parser, Subcommand};
use driver_foundry_common::{PRODUCT_TAGLINE, PRODUCT_VERSION};
use std::path::PathBuf;

const LONG_ABOUT: &str = "\
Driver Foundry — Remove cleanly. Install only what you need.

Native Rust application for GPU driver cleanup, package customization, and installation.

Safety:
  clean defaults to dry-run (plan/journal only)
  install defaults to dry-run (no setup.exe launch)
  live mutation requires explicit --execute / --force-install
  GUI defaults to dry-run; shares the same engines as CLI
";

#[derive(Parser, Debug)]
#[command(
    name = "dfoundry",
    version = PRODUCT_VERSION,
    about = PRODUCT_TAGLINE,
    long_about = LONG_ABOUT
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Plan or execute GPU/audio driver cleanup
    Clean(Box<CleanArgs>),
    /// Acquire → filter/strip → tweaks → install
    Install(Box<InstallArgs>),
    /// List packages from packages.v1.json catalog
    ListPackages {
        #[arg(long)]
        catalog: Option<PathBuf>,
    },
    /// List shipped language packs under data/settings/Languages
    ListLanguages,
    /// Launch interactive GUI (clean + install; dry-run default)
    Gui,
}

#[derive(Args, Debug)]
pub(crate) struct CleanArgs {
    /// Vendor: nvidia|amd|intel|lisuan|realtek
    #[arg(long)]
    pub vendor: Option<String>,
    /// Plan only — default; no host mutation
    #[arg(long, default_value_t = true)]
    pub dry_run: bool,
    /// Live clean via Windows adapters (requires admin; UAC relaunch attempted)
    #[arg(long, alias = "live")]
    pub execute: bool,
    /// Catalog / admin / safemode health check
    #[arg(long)]
    pub preflight: bool,
    /// Override settings/ root
    #[arg(long)]
    pub settings: Option<PathBuf>,
    /// Write plan report text to path
    #[arg(long)]
    pub plan_report: Option<PathBuf>,
    /// Cache-only path (shader/install caches)
    #[arg(long)]
    pub cache_only: bool,
    /// Block Windows driver search policy
    #[arg(long)]
    pub block_driver_search: bool,
    /// Skip SetupAPI device stage
    #[arg(long)]
    pub no_setup_api: bool,
    /// Skip restore point plan
    #[arg(long)]
    pub no_restore_point: bool,
    /// Restart after clean
    #[arg(long)]
    pub restart: bool,
    /// Shutdown after clean
    #[arg(long)]
    pub shutdown: bool,
    /// Prepare Safe Mode boot
    #[arg(long)]
    pub prepare_safeboot: bool,
    /// Clear Safe Mode boot flag
    #[arg(long)]
    pub clear_safeboot: bool,
    /// Safe Mode with networking
    #[arg(long)]
    pub safeboot_network: bool,
    /// Disable live host-probe reads during dry-run
    #[arg(long)]
    pub no_host_probe: bool,
    /// Enable all optional remove scopes
    #[arg(long)]
    pub clean_complete: bool,
    #[arg(long)]
    pub remove_gfe: bool,
    #[arg(long)]
    pub remove_nvbroadcast: bool,
    #[arg(long)]
    pub remove_amdkmpfd: bool,
    #[arg(long)]
    pub remove_intel_igs: bool,
    #[arg(long)]
    pub remove_intel_npu: bool,
    #[arg(long)]
    pub remove_oneapi: bool,
    #[arg(long)]
    pub remove_endurance_gaming: bool,
    #[arg(long)]
    pub remove_vulkan: bool,
    #[arg(long)]
    pub remove_physx: bool,
    #[arg(long)]
    pub remove_audiobus: bool,
    #[arg(long)]
    pub remove_monitors: bool,
    #[arg(long)]
    pub remove_unpack_nvidia: bool,
    #[arg(long)]
    pub remove_unpack_amd: bool,
    #[arg(long)]
    pub remove_install_cache: bool,
}

#[derive(Args, Debug)]
pub(crate) struct InstallArgs {
    /// Work directory (default: under %TEMP%)
    #[arg(long)]
    pub work: Option<PathBuf>,
    /// Component preset: minimal|clean|recommended|notebook|gaming|full
    #[arg(long, default_value = "clean")]
    pub preset: String,
    /// Local NVIDIA package tree
    #[arg(long)]
    pub package_root: Option<PathBuf>,
    /// Package archive (zip/7z/exe) to extract
    #[arg(long)]
    pub package_archive: Option<PathBuf>,
    /// HTTPS URL to download package
    #[arg(long, alias = "download-url")]
    pub package_url: Option<String>,
    /// SHA-256 pin for --package-url, obtained from the driver vendor.
    #[arg(long)]
    pub package_sha256: Option<String>,
    /// driver-index.v1.json path
    #[arg(long)]
    pub driver_index: Option<PathBuf>,
    /// Package id inside driver-index
    #[arg(long)]
    pub driver_index_id: Option<String>,
    /// packages.v1.json path
    #[arg(long)]
    pub catalog: Option<PathBuf>,
    /// Enable install stage (still dry-run unless --force-install)
    #[arg(long, default_value_t = true)]
    pub install: bool,
    /// Actually launch PE setup.exe
    #[arg(long)]
    pub force_install: bool,
    /// Write run-report JSON path
    #[arg(long)]
    pub report: Option<PathBuf>,
    /// Skip run report
    #[arg(long)]
    pub no_report: bool,
    /// Extra components to select
    #[arg(long = "select", value_name = "ID")]
    pub select: Vec<String>,
    /// Components to deselect
    #[arg(long = "deselect", value_name = "ID")]
    pub deselect: Vec<String>,
    /// Import selection JSON
    #[arg(long)]
    pub import_selection: Option<PathBuf>,
    /// Export prepared workspace directory
    #[arg(long)]
    pub export: Option<PathBuf>,
    /// Build portable archive (zip/7z/sfx)
    #[arg(long)]
    pub archive: Option<PathBuf>,
    /// Archive format: zip|7z|sfx (default zip)
    #[arg(long, default_value = "zip")]
    pub archive_format: String,
    /// Copy shipped embedded helpers into work/embedded
    #[arg(long)]
    pub materialize_embedded: bool,
    /// Uninstall-drivers stage before install
    #[arg(long)]
    pub uninstall_drivers: bool,
    /// Deep INF option path
    #[arg(long)]
    pub deep_inf: bool,
    /// Disable driver telemetry tweaks
    #[arg(long, default_value_t = true)]
    pub disable_telemetry: bool,
    /// Disable installer telemetry
    #[arg(long, default_value_t = true)]
    pub disable_installer_telemetry: bool,
    /// Disable NvContainer
    #[arg(long, default_value_t = true)]
    pub disable_nvcontainer: bool,
    /// Disable NvCamera
    #[arg(long, default_value_t = true)]
    pub disable_nvcamera: bool,
    /// Disable HDCP
    #[arg(long, default_value_t = false)]
    pub disable_hdcp: bool,
    /// Disable MPO
    #[arg(long, default_value_t = false)]
    pub disable_mpo: bool,
    /// Disable HDAudio sleep
    #[arg(long, default_value_t = false)]
    pub disable_hdaudio_sleep: bool,
    /// Enable MSI mode marker
    #[arg(long, default_value_t = true)]
    pub enable_msi: bool,
    /// Clean install setup flag
    #[arg(long, default_value_t = true)]
    pub clean_install: bool,
    /// Unattended setup.cfg
    #[arg(long, default_value_t = true)]
    pub unattended: bool,
    /// Apply post-install .reg via reg import (no reboot)
    #[arg(long)]
    pub live_registry_apply: bool,
    /// Try signtool / catalog rebuild (Not WHQL if tools absent)
    #[arg(long)]
    pub try_sign: bool,
    /// Extra setup.exe argument (repeatable)
    #[arg(long = "setup-arg")]
    pub setup_args: Vec<String>,
}
