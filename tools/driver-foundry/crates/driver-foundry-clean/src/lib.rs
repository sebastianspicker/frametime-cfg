//! Catalog-backed GPU/audio driver cleanup domain.
//!
//! Default mode is **dry-run**: load vendor catalogs, walk stages 0→6 (or audio path),
//! journal planned actions. Live execution is blocked until authenticated catalog metadata is
//! shipped.

use driver_foundry_common::ActionJournal;
use std::fs;
use std::path::Path;
use thiserror::Error;

mod cache;
mod support;

pub mod adapters;
pub mod catalog;
pub mod options;
mod safemode;

pub use options::{CleanOptions, CleanVendor, GpuVendor, RemoveScopes};
pub use support::{clean_dry_run_vendor, options_from_selection, preflight, resolve_settings_root};

use adapters::{DryRunEnvironment, LiveWindowsEnvironment, OsEnvironment};
use catalog::{load_lines, try_load_lines, CatalogError};
use support::{
    format_plan_report, mmdevices_tokens_for_vendor, vendor_device_targets, vendor_install_caches,
    vendor_process_hints, vendor_shader_caches, vendor_task_tokens,
};

#[derive(Debug, Clone)]
pub struct CleanResult {
    pub exit_code: i32,
    pub dry_run: bool,
    pub stages: Vec<String>,
    pub planned: usize,
    pub executed: usize,
    pub plan_report: String,
    pub messages: Vec<String>,
    pub journal: ActionJournal,
    pub elevation_relaunched: bool,
}

#[derive(Debug, Error)]
pub enum CleanError {
    #[error("unknown vendor: {0}")]
    UnknownVendor(String),
    #[error("elevation required: {0}")]
    ElevationRequired(String),
    #[error("live clean is blocked: packaged catalog authentication is unavailable; use dry-run planning until signed catalog metadata is shipped")]
    LiveCatalogAuthenticationRequired,
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Run catalog-backed clean (dry-run or live execute).
pub fn run_clean(opts: &CleanOptions) -> Result<CleanResult, CleanError> {
    // Catalog entries are deletion targets. The checkout has no signed manifest or immutable
    // digest set for the packaged catalogs, so no local path (including the default) can safely
    // authorize elevated cleanup. Keep injected environments and dry-runs useful for tests/plans.
    if !opts.dry_run {
        let _ = LiveWindowsEnvironment;
        return Err(CleanError::LiveCatalogAuthenticationRequired);
    }
    let mut env = DryRunEnvironment {
        probe_host: opts.host_probe,
    };
    run_clean_with_env(opts, &mut env)
}

/// Run clean using an injected OS environment for in-crate dry-run tests.
///
/// This is deliberately not a public alternate execution path. Catalog authentication must be
/// established before *any* live adapter can be driven, including a test or custom adapter.
pub(crate) fn run_clean_with_env(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
) -> Result<CleanResult, CleanError> {
    let mut messages = Vec::new();
    let mut journal = ActionJournal::default();
    let mut stages = Vec::new();
    let elevation_relaunched = false;
    let live = !opts.dry_run;
    if live {
        return Err(CleanError::LiveCatalogAuthenticationRequired);
    }
    let vendor = opts.vendor;
    let folder = vendor.folder();
    let mode_label = if live { "EXECUTE" } else { "DRY-RUN" };
    messages.push(format!(
        "clean: vendor={} dryRun={} settings={}",
        vendor.as_str(),
        opts.dry_run,
        opts.settings_root.display()
    ));
    messages.push(format!(
        "mode: {mode_label}; {}",
        if live {
            "live Windows adapters (SCM/files/registry/SetupAPI/AppX/tasks)"
        } else {
            "journaling planned actions; host-probe reads may enrich plan"
        }
    ));
    if opts.clear_safeboot {
        stages.push("safemode_clear".into());
        messages.extend(safemode::clear_safeboot(live, &mut journal).messages);
    }
    if opts.prepare_safeboot {
        stages.push("safemode_prepare".into());
        messages
            .extend(safemode::prepare_safeboot(opts.safeboot_network, live, &mut journal).messages);
    }
    if vendor.is_audio() {
        run_audio_path(opts, env, &mut journal, &mut stages, &mut messages, live)?;
    } else if opts.cache_only {
        cache::run_cache_only(opts, env, &mut journal, &mut stages, &mut messages, live)?;
    } else {
        run_gpu_stages(opts, env, &mut journal, &mut stages, &mut messages, live)?;
    }
    if opts.restart || opts.shutdown {
        stages.push("power".into());
        safemode::request_power(opts.restart, opts.shutdown, live, &mut journal);
        messages.push(format!(
            "power: restart={} shutdown={} live={live}",
            opts.restart, opts.shutdown
        ));
    }
    let failed_actions = if live { journal.count_failed() } else { 0 };
    stages.push(
        if failed_actions == 0 {
            "success"
        } else {
            "failed"
        }
        .into(),
    );
    messages.push(if failed_actions == 0 {
        format!("[Stage] success ({folder}) dryRun={}", opts.dry_run)
    } else {
        format!("[Stage] failed ({folder}) actions={failed_actions}")
    });
    messages.push(format!(
        "[Journal] planned={} executed={}",
        journal.count_planned(),
        journal.count_executed()
    ));
    messages.push(if failed_actions == 0 {
        format!(
            "[Done] {} complete",
            if live { "execute" } else { "dry-run" }
        )
    } else {
        format!("[Done] execute incomplete: {failed_actions} action(s) failed")
    });
    let plan_report = format_plan_report(vendor, opts.dry_run, &stages, &journal);
    messages.push(plan_report.clone());
    if let Some(path) = opts.plan_report_path.as_ref() {
        let mut report = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        use std::io::Write;
        report.write_all(plan_report.as_bytes())?;
        report.sync_all()?;
        messages.push(format!("plan-report written: {}", path.display()));
    }
    Ok(CleanResult {
        exit_code: i32::from(failed_actions > 0),
        dry_run: opts.dry_run,
        stages,
        planned: journal.count_planned(),
        executed: journal.count_executed(),
        plan_report,
        messages,
        journal,
        elevation_relaunched,
    })
}

struct GpuCatalog {
    services: Vec<String>,
    driverfiles: Vec<String>,
    services_audio: Vec<String>,
    classroot: Vec<String>,
    clsid: Vec<String>,
    packages: Vec<String>,
    interfaces: Vec<String>,
}

impl GpuCatalog {
    fn load(opts: &CleanOptions) -> Result<Self, CleanError> {
        let folder = opts.vendor.folder();
        let settings = &opts.settings_root;
        Ok(Self {
            services: load_lines(settings, folder, "services.cfg")?,
            driverfiles: load_lines(settings, folder, "driverfiles.cfg")?,
            services_audio: try_load_lines(settings, folder, "servicesaudio.cfg"),
            classroot: try_load_lines(settings, folder, "classroot.cfg"),
            clsid: try_load_lines(settings, folder, "clsidleftover.cfg"),
            packages: try_load_lines(settings, folder, "packages.cfg"),
            interfaces: try_load_lines(settings, folder, "interface.cfg"),
        })
    }
}

fn run_gpu_stages(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    stages: &mut Vec<String>,
    messages: &mut Vec<String>,
    live: bool,
) -> Result<(), CleanError> {
    let catalog = GpuCatalog::load(opts)?;
    stage_resolve_gpu(opts, env, journal, stages, messages, live, &catalog);
    stage_early_services(opts, env, journal, stages, messages, live, &catalog);
    stage_preclean_packages(opts, env, journal, stages, messages, live, &catalog);
    stage_setupapi(opts, env, journal, stages, messages, live, &catalog);
    stage_deep_clean(opts, env, journal, stages, messages, live, &catalog);
    stage_finalize_gpu(opts, env, journal, stages, messages, live);
    Ok(())
}

fn stage_resolve_gpu(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    stages: &mut Vec<String>,
    messages: &mut Vec<String>,
    live: bool,
    catalog: &GpuCatalog,
) {
    let vendor = opts.vendor;
    stages.push("0_resolve_vendor".into());
    messages.push(format!(
        "[Stage] 0_resolve_vendor ({} {}) dryRun={}",
        vendor.folder(),
        vendor.ven_id(),
        !live
    ));
    messages.push(format!(
        "[Identity] vendor={} folder={} venId={} services={} driverfiles={} host_probe={}",
        vendor.as_str(),
        vendor.folder(),
        vendor.ven_id(),
        catalog.services.len(),
        catalog.driverfiles.len(),
        opts.host_probe
    ));
    if !opts.no_restore_point {
        env.create_restore_point("Driver Foundry pre-clean", journal);
    }
    if opts.block_driver_search {
        env.set_block_driver_search(true, journal);
    }
}

fn stage_early_services(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    stages: &mut Vec<String>,
    messages: &mut Vec<String>,
    live: bool,
    catalog: &GpuCatalog,
) {
    let vendor = opts.vendor;
    let folder = vendor.folder();
    stages.push("1_2_early_services".into());
    messages.push(format!(
        "[Stage] 1_2_early_services ({}) dryRun={}",
        vendor.ven_id(),
        !live
    ));
    for hint in vendor_process_hints(vendor) {
        env.kill_process_match(hint, journal);
    }
    for service in &catalog.services {
        env.stop_delete_service(service, journal);
        env.kill_process_match(service, journal);
    }
    let overlays = [
        (
            vendor == CleanVendor::Nvidia && opts.scopes.remove_gfe,
            "gfeservice.cfg",
            "[Scope] remove-gfe: gfeservice.cfg loaded",
        ),
        (
            vendor == CleanVendor::Nvidia && opts.scopes.remove_nv_broadcast,
            "nvbservice.cfg",
            "[Scope] remove-nvbroadcast: nvbservice.cfg loaded",
        ),
        (
            vendor == CleanVendor::Intel && opts.scopes.remove_intel_igs,
            "servicesigs.cfg",
            "[Scope] remove-intel-igs: servicesigs.cfg loaded",
        ),
    ];
    for (enabled, file, message) in overlays {
        if enabled {
            for service in try_load_lines(&opts.settings_root, folder, file) {
                env.stop_delete_service(&service, journal);
            }
            messages.push(message.into());
        }
    }
    messages.push(format!(
        "[CleanServices] {folder}: {} names dryRun={}",
        catalog.services.len(),
        !live
    ));
}

fn stage_preclean_packages(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    stages: &mut Vec<String>,
    messages: &mut Vec<String>,
    live: bool,
    catalog: &GpuCatalog,
) {
    let vendor = opts.vendor;
    let folder = vendor.folder();
    stages.push("3_preclean_com_packages".into());
    messages.push(format!(
        "[Stage] 3_preclean_com_packages ({}) dryRun={}",
        vendor.ven_id(),
        !live
    ));
    cleanup_registry(env, journal, "classroot_cleanup", &catalog.classroot);
    cleanup_registry(env, journal, "clsid_leftover", &catalog.clsid);
    cleanup_registry(env, journal, "interface_cleanup", &catalog.interfaces);
    remove_packages(env, journal, &catalog.packages);
    if vendor == CleanVendor::Nvidia && opts.scopes.remove_gfe {
        cleanup_catalog_registry(
            opts,
            env,
            journal,
            folder,
            "classroot_gfe",
            "classrootgfe.cfg",
        );
        cleanup_catalog_registry(
            opts,
            env,
            journal,
            folder,
            "clsid_gfe",
            "clsidleftoverGFE.cfg",
        );
        cleanup_catalog_registry(
            opts,
            env,
            journal,
            folder,
            "interface_gfe",
            "interfaceGFE.cfg",
        );
    }
    if vendor == CleanVendor::Nvidia && opts.scopes.remove_nv_broadcast {
        cleanup_catalog_registry(
            opts,
            env,
            journal,
            folder,
            "clsid_nvb",
            "clsidleftoverNVB.cfg",
        );
    }
    if vendor == CleanVendor::Intel {
        stage_intel_package_scopes(opts, env, journal, folder);
    }
    remove_vendor_appx(opts, env, journal);
    messages.push(format!(
        "[ClassRoot] preclean: {} tokens dryRun={}",
        catalog.classroot.len(),
        !live
    ));
}

fn cleanup_registry(
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    action: &str,
    tokens: &[String],
) {
    for token in tokens {
        env.registry_cleanup(action, token, journal);
    }
}
fn remove_packages(env: &mut dyn OsEnvironment, journal: &mut ActionJournal, packages: &[String]) {
    for package in packages {
        env.remove_appx_match(package, journal);
    }
}
fn cleanup_catalog_registry(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    folder: &str,
    action: &str,
    file: &str,
) {
    for token in try_load_lines(&opts.settings_root, folder, file) {
        env.registry_cleanup(action, &token, journal);
    }
}
fn stage_intel_package_scopes(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    folder: &str,
) {
    if opts.scopes.remove_intel_igs {
        cleanup_catalog_registry(
            opts,
            env,
            journal,
            folder,
            "clsid_igs",
            "clsidleftoverigs.cfg",
        );
        remove_packages(
            env,
            journal,
            &try_load_lines(&opts.settings_root, folder, "packagesigs.cfg"),
        );
    }
    for (enabled, file) in [
        (opts.scopes.remove_intel_npu, "packagesnpu.cfg"),
        (opts.scopes.remove_oneapi, "packagesoneapi.cfg"),
        (opts.scopes.remove_endurance, "packagesendurance.cfg"),
    ] {
        if enabled {
            remove_packages(
                env,
                journal,
                &try_load_lines(&opts.settings_root, folder, file),
            );
        }
    }
}
fn remove_vendor_appx(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
) {
    match opts.vendor {
        CleanVendor::Nvidia => {
            env.remove_appx_match("NVIDIAControlPanel", journal);
            if opts.scopes.remove_gfe {
                env.remove_appx_match("NVIDIA", journal);
            }
        }
        CleanVendor::Amd => {
            env.remove_appx_match("AMDRadeon", journal);
            env.remove_appx_match("AdvancedMicroDevicesInc", journal);
        }
        CleanVendor::Intel => {
            env.remove_appx_match("IntelGraphicsControlPanel", journal);
            env.remove_appx_match("IntelGraphicsExperience", journal);
        }
        _ => {}
    }
}

fn stage_setupapi(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    stages: &mut Vec<String>,
    messages: &mut Vec<String>,
    live: bool,
    catalog: &GpuCatalog,
) {
    if opts.no_setup_api {
        return;
    }
    let vendor = opts.vendor;
    let folder = vendor.folder();
    stages.push("4_setupapi_devices".into());
    messages.push(format!(
        "[Stage] 4_setupapi_devices ({}) dryRun={}",
        vendor.ven_id(),
        !live
    ));
    for id in vendor_device_targets(vendor) {
        env.uninstall_device(id, journal);
    }
    if vendor == CleanVendor::Intel && opts.scopes.remove_intel_npu {
        for id in ["VEN_8086&CC_0B40", "PCI\\VEN_8086&CC_1200"] {
            env.uninstall_device(id, journal);
        }
    }
    if vendor == CleanVendor::Amd && opts.scopes.remove_amd_kmpfd {
        env.uninstall_device("KMPFD", journal);
        for token in try_load_lines(&opts.settings_root, folder, "driverfilesKMPFD.cfg") {
            env.delete_file_match(&token, journal);
        }
        messages.push("[Scope] remove-amdkmpfd planned".into());
    }
    for service in &catalog.services_audio {
        env.stop_delete_service(service, journal);
        env.uninstall_device(service, journal);
    }
    if opts.scopes.remove_audiobus {
        env.uninstall_device("ROOT\\MEDIA", journal);
        env.uninstall_device("USB\\VID_0955", journal);
        journal.plan("Device", "remove_audiobus", "HDAUDIO\\FUNC_01");
        env.clean_mmdevices(&mmdevices_tokens_for_vendor(vendor), journal);
        messages.push("[Scope] remove-audiobus planned (incl. MMDevices)".into());
    }
    if opts.scopes.remove_monitors {
        env.uninstall_device("MONITOR\\", journal);
        messages.push("[Scope] remove-monitors planned".into());
    }
    messages.push(format!(
        "[UninstallDevices] targets planned dryRun={}",
        !live
    ));
}

fn stage_deep_clean(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    stages: &mut Vec<String>,
    messages: &mut Vec<String>,
    live: bool,
    catalog: &GpuCatalog,
) {
    let vendor = opts.vendor;
    let folder = vendor.folder();
    stages.push("5_deep_clean".into());
    messages.push(format!(
        "[Stage] 5_deep_clean ({}) dryRun={}",
        vendor.ven_id(),
        !live
    ));
    let file_tokens = deep_file_tokens(opts, catalog);
    for token in &file_tokens {
        env.delete_file_match(token, journal);
    }
    for service in &catalog.services {
        env.stop_delete_service(service, journal);
    }
    for task in vendor_task_tokens(vendor) {
        env.delete_scheduled_task(task, journal);
    }
    clean_caches(opts, env, journal, messages);
    clean_unpack_targets(opts, env, journal, messages);
    clean_optional_runtime_scopes(opts, env, journal, messages);
    messages.push(format!(
        "[FoldersCleanup] {folder}: {} driverfile tokens (all via adapter) dryRun={}",
        file_tokens.len(),
        !live
    ));
}

fn deep_file_tokens(opts: &CleanOptions, catalog: &GpuCatalog) -> Vec<String> {
    let mut tokens = catalog.driverfiles.clone();
    let folder = opts.vendor.folder();
    let additions = match opts.vendor {
        CleanVendor::Nvidia => vec![
            (opts.scopes.remove_gfe, "gfedriverfiles.cfg"),
            (opts.scopes.remove_nv_broadcast, "nvbdriverfiles.cfg"),
        ],
        CleanVendor::Amd => vec![
            (opts.scopes.remove_amd_kmpfd, "driverfilesKMPFD.cfg"),
            (opts.scopes.remove_amd_kmpfd, "driverfilesKMAFD.cfg"),
        ],
        CleanVendor::Intel => vec![(true, "shareddriverfiles.cfg")],
        _ => Vec::new(),
    };
    for (enabled, file) in additions {
        if enabled {
            tokens.extend(try_load_lines(&opts.settings_root, folder, file));
        }
    }
    tokens
}
fn clean_caches(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    messages: &mut Vec<String>,
) {
    let caches = if opts.scopes.remove_gfe || opts.scopes.remove_install_cache {
        messages.push("[Scope] install-cache wipe planned".into());
        vendor_install_caches(opts.vendor)
    } else {
        vendor_shader_caches(opts.vendor)
    };
    for cache in caches {
        env.wipe_path(&driver_foundry_common::expand_path_tokens(cache), journal);
    }
}
fn clean_unpack_targets(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    messages: &mut Vec<String>,
) {
    match opts.vendor {
        CleanVendor::Nvidia if opts.scopes.remove_unpack_nvidia => {
            for path in [r"C:\NVIDIA", r"C:\NVIDIA\DisplayDriver"] {
                env.wipe_path(Path::new(path), journal);
            }
            messages.push("[Scope] remove-unpack-nvidia planned".into());
        }
        CleanVendor::Amd if opts.scopes.remove_unpack_amd => {
            for path in [r"C:\AMD", r"C:\AMD\AMD-Software"] {
                env.wipe_path(Path::new(path), journal);
            }
            messages.push("[Scope] remove-unpack-amd planned".into());
        }
        _ => {}
    }
}
fn clean_optional_runtime_scopes(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    messages: &mut Vec<String>,
) {
    if opts.scopes.remove_physx {
        env.delete_file_match("PhysX", journal);
        env.registry_cleanup(
            "physx_leftover",
            r"HKLM\SOFTWARE\NVIDIA Corporation\PhysX",
            journal,
        );
        messages.push("[Scope] remove-physx planned".into());
    }
    if opts.scopes.remove_vulkan {
        env.delete_file_match("vulkan-1.dll", journal);
        env.registry_cleanup(
            "vulkan_implicit_layers",
            r"HKLM\SOFTWARE\Khronos\Vulkan",
            journal,
        );
        messages.push("[Scope] remove-vulkan planned".into());
    }
}

fn stage_finalize_gpu(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    stages: &mut Vec<String>,
    messages: &mut Vec<String>,
    live: bool,
) {
    let vendor = opts.vendor;
    let folder = vendor.folder();
    stages.push("6_driverstore_finalize".into());
    messages.push(format!(
        "[Stage] 6_driverstore_finalize ({}) dryRun={}",
        vendor.ven_id(),
        !live
    ));
    env.clean_driverstore(folder, vendor.ven_id(), journal);
    env.registry_cleanup("fix_driverstore_registry", folder, journal);
    env.pnp_lockdown_orphans(folder, journal);
    env.clean_pci_root(folder, vendor.ven_id(), journal);
    let filters = adapters::pci_filter_tokens(folder);
    if !filters.is_empty() {
        messages.push(format!(
            "[StripFilterValues] multi-sz UpperFilters/LowerFilters plan for {} ({})",
            folder,
            filters.join(",")
        ));
    }
    messages.push(format!(
        "[DriverStore] clean_driverstore+pnp_lockdown+pci_root via OsEnvironment dryRun={}",
        !live
    ));
}

fn run_audio_path(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    stages: &mut Vec<String>,
    messages: &mut Vec<String>,
    live: bool,
) -> Result<(), CleanError> {
    let folder = "REALTEK";
    let settings = &opts.settings_root;
    let dry = !live;
    stages.push("0_resolve_audio".into());
    let services = load_lines(settings, folder, "services.cfg")?;
    let driverfiles = load_lines(settings, folder, "driverfiles.cfg")?;
    let classroot = try_load_lines(settings, folder, "classroot.cfg");
    let clsid = try_load_lines(settings, folder, "clsidleftover.cfg");
    let packages = try_load_lines(settings, folder, "packages.cfg");
    messages.push(format!(
        "[Stage] 0_resolve_audio (REALTEK VEN_10EC) dryRun={dry}"
    ));
    messages.push(format!(
        "[Identity] vendor=realtek services={} driverfiles={}",
        services.len(),
        driverfiles.len()
    ));
    stages.push("1_audio_services".into());
    for service in &services {
        env.stop_delete_service(service, journal);
        env.kill_process_match(service, journal);
    }
    stages.push("2_audio_com_packages".into());
    cleanup_registry(env, journal, "classroot_cleanup", &classroot);
    cleanup_registry(env, journal, "clsid_leftover", &clsid);
    remove_packages(env, journal, &packages);
    stages.push("3_audio_devices_files".into());
    env.uninstall_device("VEN_10EC", journal);
    env.uninstall_device("HDAUDIO\\FUNC_01&VEN_10EC", journal);
    env.clean_mmdevices(&mmdevices_tokens_for_vendor(CleanVendor::Realtek), journal);
    for token in &driverfiles {
        env.delete_file_match(token, journal);
    }
    messages.push(format!(
        "[FoldersCleanup] REALTEK: {} driverfile tokens (all via adapter) dryRun={dry}",
        driverfiles.len()
    ));
    messages.push("[MMDevices] Realtek audio endpoint cleanup planned".into());
    stages.push("4_audio_finalize".into());
    env.clean_driverstore("REALTEK", "VEN_10EC", journal);
    env.pnp_lockdown_orphans("REALTEK", journal);
    env.clean_pci_root("REALTEK", "VEN_10EC", journal);
    messages.push(format!(
        "[AudioClean] REALTEK stages complete; driverstore+pci_root via OsEnvironment dryRun={dry}"
    ));
    Ok(())
}

#[cfg(test)]
mod tests;
