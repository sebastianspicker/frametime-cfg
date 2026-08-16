//! Cleanup options and optional removal scopes.

use std::path::PathBuf;

/// GPU or audio clean target vendor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CleanVendor {
    Nvidia,
    Amd,
    Intel,
    Lisuan,
    Realtek,
}

impl CleanVendor {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "nvidia" | "nv" => Some(Self::Nvidia),
            "amd" | "ati" => Some(Self::Amd),
            "intel" => Some(Self::Intel),
            "lisuan" | "ls" => Some(Self::Lisuan),
            "realtek" | "audio" | "rtk" => Some(Self::Realtek),
            _ => None,
        }
    }

    pub fn folder(self) -> &'static str {
        match self {
            Self::Nvidia => "NVIDIA",
            Self::Amd => "AMD",
            Self::Intel => "INTEL",
            Self::Lisuan => "LISUAN",
            Self::Realtek => "REALTEK",
        }
    }

    pub fn ven_id(self) -> &'static str {
        match self {
            Self::Nvidia => "VEN_10DE",
            Self::Amd => "VEN_1002",
            Self::Intel => "VEN_8086",
            Self::Lisuan => "VEN_4C54",
            Self::Realtek => "VEN_10EC",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nvidia => "nvidia",
            Self::Amd => "amd",
            Self::Intel => "intel",
            Self::Lisuan => "lisuan",
            Self::Realtek => "realtek",
        }
    }

    pub fn is_audio(self) -> bool {
        matches!(self, Self::Realtek)
    }

    /// GPU vendors only (excludes Realtek).
    pub fn all_gpu() -> [Self; 4] {
        [Self::Nvidia, Self::Amd, Self::Intel, Self::Lisuan]
    }
}

/// Optional remove-scopes that pull extra catalogs into the plan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoveScopes {
    pub remove_gfe: bool,
    pub remove_nv_broadcast: bool,
    pub remove_amd_kmpfd: bool,
    pub remove_intel_igs: bool,
    pub remove_intel_npu: bool,
    pub remove_oneapi: bool,
    pub remove_endurance: bool,
    pub remove_vulkan: bool,
    pub remove_physx: bool,
    pub remove_audiobus: bool,
    pub remove_monitors: bool,
    pub remove_unpack_nvidia: bool,
    pub remove_unpack_amd: bool,
    pub remove_install_cache: bool,
}

impl RemoveScopes {
    /// Apply clean-complete: enable all optional scopes.
    pub fn clean_complete() -> Self {
        Self {
            remove_gfe: true,
            remove_nv_broadcast: true,
            remove_amd_kmpfd: true,
            remove_intel_igs: true,
            remove_intel_npu: true,
            remove_oneapi: true,
            remove_endurance: true,
            remove_vulkan: true,
            remove_physx: true,
            remove_audiobus: true,
            remove_monitors: true,
            remove_unpack_nvidia: true,
            remove_unpack_amd: true,
            remove_install_cache: true,
        }
    }
}

/// Full clean run options.
#[derive(Debug, Clone)]
pub struct CleanOptions {
    pub vendor: CleanVendor,
    /// When true (default), plan only — no host mutation.
    pub dry_run: bool,
    pub settings_root: PathBuf,
    pub scopes: RemoveScopes,
    /// Cache-only path: stages 0 + 0b only.
    pub cache_only: bool,
    pub block_driver_search: bool,
    pub no_restore_point: bool,
    pub no_setup_api: bool,
    pub restart: bool,
    pub shutdown: bool,
    pub plan_report_path: Option<PathBuf>,
    /// Prepare Safe Mode + reboot helper (plan or live BCD hooks).
    pub prepare_safeboot: bool,
    /// Clear Safe Mode helper / BCD boot entry.
    pub clear_safeboot: bool,
    pub safeboot_network: bool,
    /// Read live SCM/SetupAPI to enrich plans (non-mutating).
    pub host_probe: bool,
    /// Attempt UAC relaunch when --execute and not admin.
    pub attempt_elevation: bool,
    /// Args to pass if elevation relaunch is needed (from CLI).
    pub elevation_args: Vec<String>,
}

impl Default for CleanOptions {
    fn default() -> Self {
        Self {
            vendor: CleanVendor::Nvidia,
            dry_run: true,
            settings_root: driver_foundry_common::settings_root(
                &driver_foundry_common::resolve_data_root(),
            ),
            scopes: RemoveScopes::default(),
            cache_only: false,
            block_driver_search: false,
            no_restore_point: false,
            no_setup_api: false,
            restart: false,
            shutdown: false,
            plan_report_path: None,
            prepare_safeboot: false,
            clear_safeboot: false,
            safeboot_network: false,
            host_probe: true,
            attempt_elevation: true,
            elevation_args: Vec::new(),
        }
    }
}

/// Backward-compatible alias used by older call sites / GUI.
pub type GpuVendor = CleanVendor;
