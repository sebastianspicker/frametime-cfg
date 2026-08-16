use crate::catalog::list_cfg_files;
use crate::{run_clean, CleanError, CleanOptions, CleanResult, CleanVendor, RemoveScopes};
use driver_foundry_common::{settings_root, ActionJournal};
use std::path::{Path, PathBuf};
pub(crate) fn format_plan_report(
    vendor: CleanVendor,
    dry_run: bool,
    stages: &[String],
    journal: &ActionJournal,
) -> String {
    let mut s = String::new();
    s.push_str("=== DRIVER FOUNDRY PLAN REPORT ===\n");
    s.push_str(&format!(
        "mode={}\n",
        if dry_run { "dry-run" } else { "execute" }
    ));
    s.push_str(&format!("targets={}\n", vendor.as_str()));
    s.push_str(&format!("stages={}\n", stages.join(" -> ")));
    s.push_str("surface_counts:\n");
    for (surface, n) in journal.surface_counts() {
        s.push_str(&format!("  {surface}={n}\n"));
    }
    s.push_str(&format!(
        "planned_total={} executed_total={}\n",
        journal.count_planned(),
        journal.count_executed()
    ));
    s.push_str("=== END PLAN REPORT ===\n");
    s
}

pub(crate) fn vendor_device_targets(v: CleanVendor) -> Vec<&'static str> {
    match v {
        CleanVendor::Nvidia => vec![
            "VEN_10DE&CC_03",
            "VEN_10DE&CC_04",
            "ROOT\\NVVHCI",
            "ROOT\\NVMODULETRACKER",
            "USB\\VID_0955",
        ],
        CleanVendor::Amd => vec!["VEN_1002&CC_03", "VEN_1002&CC_04"],
        CleanVendor::Intel => vec!["VEN_8086&CC_03", "VEN_8086&CC_04"],
        CleanVendor::Lisuan => vec!["VEN_4C54&CC_03"],
        CleanVendor::Realtek => vec!["VEN_10EC"],
    }
}

pub(crate) fn vendor_process_hints(v: CleanVendor) -> Vec<&'static str> {
    match v {
        CleanVendor::Nvidia => vec![
            "nvidia share",
            "nvcontainer",
            "nvdisplay.container",
            "nvidia web helper",
            "o4app",
        ],
        CleanVendor::Amd => vec!["radeonsoftware", "amdow", "cnext"],
        CleanVendor::Intel => vec!["igfxEM", "igfxCUIService"],
        _ => vec![],
    }
}

pub(crate) fn vendor_task_tokens(v: CleanVendor) -> Vec<&'static str> {
    match v {
        CleanVendor::Nvidia => vec![
            "NvBackend",
            "NvTmMon",
            "NvTmRep",
            "NVIDIA GeForce Experience",
            "NvNodeLauncher",
            "NvProfileUpdater",
        ],
        CleanVendor::Amd => vec!["AMDInstallManager", "StartCN"],
        CleanVendor::Intel => vec!["Intel Graphics Command Center"],
        _ => vec!["VendorGpuTask"],
    }
}

pub(crate) fn vendor_install_caches(v: CleanVendor) -> Vec<&'static str> {
    match v {
        CleanVendor::Nvidia => vec![
            r"C:\ProgramData\NVIDIA Corporation\Installer2",
            r"C:\ProgramData\NVIDIA Corporation\Downloader",
            r"C:\ProgramData\NVIDIA",
            r"%LOCALAPPDATA%\NVIDIA Corporation\NV_Cache",
            r"%TEMP%\NVIDIA",
        ],
        CleanVendor::Amd => vec![r"C:\ProgramData\AMD", r"C:\AMD"],
        CleanVendor::Intel => vec![r"C:\ProgramData\Intel"],
        CleanVendor::Lisuan => vec![r"C:\ProgramData\Lisuan"],
        CleanVendor::Realtek => vec![r"C:\ProgramData\Realtek"],
    }
}

pub(crate) fn vendor_shader_caches(v: CleanVendor) -> Vec<&'static str> {
    match v {
        CleanVendor::Nvidia => vec![
            r"%LOCALAPPDATA%\NVIDIA\DXCache",
            r"%LOCALAPPDATA%\NVIDIA\GLCache",
            r"%LOCALAPPDATA%\NVIDIA Corporation\NV_Cache",
            r"%LOCALAPPDATA%\D3DSCache",
        ],
        CleanVendor::Amd => vec![r"%LOCALAPPDATA%\AMD\DxCache", r"%LOCALAPPDATA%\D3DSCache"],
        CleanVendor::Intel => vec![
            r"%LOCALAPPDATA%\Intel\ShaderCache",
            r"%LOCALAPPDATA%\D3DSCache",
        ],
        _ => vec![r"%LOCALAPPDATA%\D3DSCache"],
    }
}

/// Codec / property needles for MMDevices endpoint purge.
/// Intel uses dev_28-qualified pattern (bare 8086 would match Bluetooth/DMIC).
pub(crate) fn mmdevices_tokens_for_vendor(v: CleanVendor) -> Vec<String> {
    match v {
        CleanVendor::Nvidia => vec![
            "ven_10de".into(),
            "10de".into(),
            "NVIDIA HD Audio".into(),
            "nvhda".into(),
        ],
        CleanVendor::Amd => vec![
            "ven_1002".into(),
            "1002".into(),
            "AMD High Definition Audio".into(),
        ],
        CleanVendor::Intel => vec![
            "ven_8086&dev_28".into(),
            "dev_28".into(),
            "Intel Display Audio".into(),
        ],
        CleanVendor::Lisuan => vec!["ven_4c54".into(), "4c54".into()],
        CleanVendor::Realtek => vec![
            "ven_10ec".into(),
            "VEN_10EC".into(),
            "10ec".into(),
            "Realtek".into(),
            "Realtek High Definition Audio".into(),
        ],
    }
}

/// Resolve settings root from data root or explicit path.
pub fn resolve_settings_root(explicit: Option<&Path>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    settings_root(&driver_foundry_common::resolve_data_root())
}

/// Preflight: core catalogs. Host privilege and Safe Mode probes are intentionally deferred:
/// preflight must not execute mutable/PATH-resolved system tools.
pub fn preflight(settings: &Path) -> (bool, Vec<String>) {
    let mut msgs = Vec::new();
    let mut ok = true;
    for v in [
        CleanVendor::Nvidia,
        CleanVendor::Amd,
        CleanVendor::Intel,
        CleanVendor::Lisuan,
        CleanVendor::Realtek,
    ] {
        let folder = v.folder();
        let required: &[&str] = if v == CleanVendor::Realtek {
            &["services.cfg", "driverfiles.cfg"]
        } else {
            &["services.cfg", "driverfiles.cfg", "servicesaudio.cfg"]
        };
        for file in required {
            let p = settings.join(folder).join(file);
            if p.is_file() {
                msgs.push(format!("ok: {folder}/{file}"));
            } else {
                // Lisuan may omit servicesaudio in some trees — tolerate only if NVIDIA/AMD/INTEL missing is hard fail
                if matches!(
                    v,
                    CleanVendor::Nvidia
                        | CleanVendor::Amd
                        | CleanVendor::Intel
                        | CleanVendor::Realtek
                ) {
                    ok = false;
                }
                msgs.push(format!("missing: {folder}/{file}"));
            }
        }
    }
    let cfgs = list_cfg_files(settings, "NVIDIA").unwrap_or_default();
    msgs.push(format!("NVIDIA catalog files: {}", cfgs.len()));
    msgs.push("admin: unknown (host probe deferred)".into());
    msgs.push("safemode: unknown (host probe deferred)".into());
    (ok, msgs)
}

/// Convenience: dry-run from vendor string.
pub fn clean_dry_run_vendor(
    vendor: &str,
    settings: Option<&Path>,
) -> Result<CleanResult, CleanError> {
    let v = CleanVendor::parse(vendor).ok_or_else(|| CleanError::UnknownVendor(vendor.into()))?;
    let opts = CleanOptions {
        vendor: v,
        dry_run: true,
        settings_root: resolve_settings_root(settings),
        ..CleanOptions::default()
    };
    run_clean(&opts)
}

/// Build CleanOptions from GUI/CLI-facing selection (shared engine mapping).
pub fn options_from_selection(
    vendor: CleanVendor,
    dry_run: bool,
    scopes: RemoveScopes,
    settings: Option<PathBuf>,
) -> CleanOptions {
    CleanOptions {
        vendor,
        dry_run,
        settings_root: settings.unwrap_or_else(|| resolve_settings_root(None)),
        scopes,
        ..CleanOptions::default()
    }
}
