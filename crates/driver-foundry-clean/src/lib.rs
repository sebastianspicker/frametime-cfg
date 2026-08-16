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
        // Keep the private live adapter compiled, but do not call a method on it before catalog
        // authentication exists. Construction alone has no host effect.
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
    let settings = &opts.settings_root;
    let mode_label = if live { "EXECUTE" } else { "DRY-RUN" };

    messages.push(format!(
        "clean: vendor={} dryRun={} settings={}",
        vendor.as_str(),
        opts.dry_run,
        settings.display()
    ));
    messages.push(format!(
        "mode: {mode_label} — {}",
        if live {
            "live Windows adapters (SCM/files/registry/SetupAPI/AppX/tasks)"
        } else {
            "journaling planned actions; host-probe reads may enrich plan"
        }
    ));

    // Standalone safe mode ops
    if opts.clear_safeboot {
        stages.push("safemode_clear".into());
        let r = safemode::clear_safeboot(live, &mut journal);
        messages.extend(r.messages);
    }
    if opts.prepare_safeboot {
        stages.push("safemode_prepare".into());
        let r = safemode::prepare_safeboot(opts.safeboot_network, live, &mut journal);
        messages.extend(r.messages);
    }

    // Realtek audio path
    if vendor.is_audio() {
        run_audio_path(opts, env, &mut journal, &mut stages, &mut messages, live)?;
    } else if opts.cache_only {
        cache::run_cache_only(opts, env, &mut journal, &mut stages, &mut messages, live)?;
    } else {
        run_gpu_stages(opts, env, &mut journal, &mut stages, &mut messages, live)?;
    }

    // Post power hooks
    if opts.restart || opts.shutdown {
        stages.push("power".into());
        safemode::request_power(opts.restart, opts.shutdown, live, &mut journal);
        messages.push(format!(
            "power: restart={} shutdown={} live={live}",
            opts.restart, opts.shutdown
        ));
    }

    let failed_actions = if live { journal.count_failed() } else { 0 };
    if failed_actions == 0 {
        stages.push("success".into());
        messages.push(format!(
            "[Stage] success ({folder}) dryRun={}",
            opts.dry_run
        ));
    } else {
        stages.push("failed".into());
        messages.push(format!(
            "[Stage] failed ({folder}) actions={failed_actions}"
        ));
    }
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

    if let Some(ref path) = opts.plan_report_path {
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

fn run_gpu_stages(
    opts: &CleanOptions,
    env: &mut dyn OsEnvironment,
    journal: &mut ActionJournal,
    stages: &mut Vec<String>,
    messages: &mut Vec<String>,
    live: bool,
) -> Result<(), CleanError> {
    let vendor = opts.vendor;
    let folder = vendor.folder();
    let settings = &opts.settings_root;
    let dry = !live;

    // Stage 0
    stages.push("0_resolve_vendor".into());
    let services = load_lines(settings, folder, "services.cfg")?;
    let driverfiles = load_lines(settings, folder, "driverfiles.cfg")?;
    let services_audio = try_load_lines(settings, folder, "servicesaudio.cfg");
    let classroot = try_load_lines(settings, folder, "classroot.cfg");
    let clsid = try_load_lines(settings, folder, "clsidleftover.cfg");
    let packages = try_load_lines(settings, folder, "packages.cfg");
    let interfaces = try_load_lines(settings, folder, "interface.cfg");

    messages.push(format!(
        "[Stage] 0_resolve_vendor ({folder} {}) dryRun={dry}",
        vendor.ven_id()
    ));
    messages.push(format!(
        "[Identity] vendor={} folder={folder} venId={} services={} driverfiles={} host_probe={}",
        vendor.as_str(),
        vendor.ven_id(),
        services.len(),
        driverfiles.len(),
        opts.host_probe
    ));

    if !opts.no_restore_point {
        env.create_restore_point("Driver Foundry pre-clean", journal);
    }
    if opts.block_driver_search {
        env.set_block_driver_search(true, journal);
    }

    // Stage 1_2
    stages.push("1_2_early_services".into());
    messages.push(format!(
        "[Stage] 1_2_early_services ({}) dryRun={dry}",
        vendor.ven_id()
    ));
    for hint in vendor_process_hints(vendor) {
        env.kill_process_match(hint, journal);
    }
    for svc in &services {
        env.stop_delete_service(svc, journal);
        env.kill_process_match(svc, journal);
    }
    // Scope overlays
    if vendor == CleanVendor::Nvidia && opts.scopes.remove_gfe {
        for svc in try_load_lines(settings, folder, "gfeservice.cfg") {
            env.stop_delete_service(&svc, journal);
        }
        messages.push("[Scope] remove-gfe: gfeservice.cfg loaded".into());
    }
    if vendor == CleanVendor::Nvidia && opts.scopes.remove_nv_broadcast {
        for svc in try_load_lines(settings, folder, "nvbservice.cfg") {
            env.stop_delete_service(&svc, journal);
        }
        messages.push("[Scope] remove-nvbroadcast: nvbservice.cfg loaded".into());
    }
    if vendor == CleanVendor::Intel && opts.scopes.remove_intel_igs {
        for svc in try_load_lines(settings, folder, "servicesigs.cfg") {
            env.stop_delete_service(&svc, journal);
        }
        messages.push("[Scope] remove-intel-igs: servicesigs.cfg loaded".into());
    }
    messages.push(format!(
        "[CleanServices] {folder}: {} names dryRun={dry}",
        services.len()
    ));

    // Stage 3
    stages.push("3_preclean_com_packages".into());
    messages.push(format!(
        "[Stage] 3_preclean_com_packages ({}) dryRun={dry}",
        vendor.ven_id()
    ));
    for token in &classroot {
        env.registry_cleanup("classroot_cleanup", token, journal);
    }
    for token in &clsid {
        env.registry_cleanup("clsid_leftover", token, journal);
    }
    for token in &interfaces {
        env.registry_cleanup("interface_cleanup", token, journal);
    }
    for pkg in &packages {
        env.remove_appx_match(pkg, journal);
    }
    // Scope extra catalogs
    if vendor == CleanVendor::Nvidia && opts.scopes.remove_gfe {
        for token in try_load_lines(settings, folder, "classrootgfe.cfg") {
            env.registry_cleanup("classroot_gfe", &token, journal);
        }
        for token in try_load_lines(settings, folder, "clsidleftoverGFE.cfg") {
            env.registry_cleanup("clsid_gfe", &token, journal);
        }
        for token in try_load_lines(settings, folder, "interfaceGFE.cfg") {
            env.registry_cleanup("interface_gfe", &token, journal);
        }
    }
    if vendor == CleanVendor::Nvidia && opts.scopes.remove_nv_broadcast {
        for token in try_load_lines(settings, folder, "clsidleftoverNVB.cfg") {
            env.registry_cleanup("clsid_nvb", &token, journal);
        }
    }
    if vendor == CleanVendor::Intel && opts.scopes.remove_intel_igs {
        for token in try_load_lines(settings, folder, "clsidleftoverigs.cfg") {
            env.registry_cleanup("clsid_igs", &token, journal);
        }
        for pkg in try_load_lines(settings, folder, "packagesigs.cfg") {
            env.remove_appx_match(&pkg, journal);
        }
    }
    if vendor == CleanVendor::Intel && opts.scopes.remove_intel_npu {
        for pkg in try_load_lines(settings, folder, "packagesnpu.cfg") {
            env.remove_appx_match(&pkg, journal);
        }
    }
    if vendor == CleanVendor::Intel && opts.scopes.remove_oneapi {
        for pkg in try_load_lines(settings, folder, "packagesoneapi.cfg") {
            env.remove_appx_match(&pkg, journal);
        }
    }
    if vendor == CleanVendor::Intel && opts.scopes.remove_endurance {
        for pkg in try_load_lines(settings, folder, "packagesendurance.cfg") {
            env.remove_appx_match(&pkg, journal);
        }
    }

    match vendor {
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
    messages.push(format!(
        "[ClassRoot] preclean: {} tokens dryRun={dry}",
        classroot.len()
    ));

    // Stage 4 SetupAPI
    if !opts.no_setup_api {
        stages.push("4_setupapi_devices".into());
        messages.push(format!(
            "[Stage] 4_setupapi_devices ({}) dryRun={dry}",
            vendor.ven_id()
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
            for token in try_load_lines(settings, folder, "driverfilesKMPFD.cfg") {
                env.delete_file_match(&token, journal);
            }
            messages.push("[Scope] remove-amdkmpfd planned".into());
        }
        for svc in &services_audio {
            env.stop_delete_service(svc, journal);
            env.uninstall_device(svc, journal);
        }
        if opts.scopes.remove_audiobus {
            env.uninstall_device("ROOT\\MEDIA", journal);
            env.uninstall_device("USB\\VID_0955", journal);
            journal.plan("Device", "remove_audiobus", "HDAUDIO\\FUNC_01");
            // MMDevices endpoint purge when audio bus scope is set
            let mm_tokens = mmdevices_tokens_for_vendor(vendor);
            env.clean_mmdevices(&mm_tokens, journal);
            messages.push("[Scope] remove-audiobus planned (incl. MMDevices)".into());
        }
        if opts.scopes.remove_monitors {
            env.uninstall_device("MONITOR\\", journal);
            messages.push("[Scope] remove-monitors planned".into());
        }
        messages.push(format!("[UninstallDevices] targets planned dryRun={dry}"));
    }

    // Stage 5 deep clean
    stages.push("5_deep_clean".into());
    messages.push(format!(
        "[Stage] 5_deep_clean ({}) dryRun={dry}",
        vendor.ven_id()
    ));
    let mut file_tokens = driverfiles.clone();
    if vendor == CleanVendor::Nvidia && opts.scopes.remove_gfe {
        file_tokens.extend(try_load_lines(settings, folder, "gfedriverfiles.cfg"));
    }
    if vendor == CleanVendor::Nvidia && opts.scopes.remove_nv_broadcast {
        file_tokens.extend(try_load_lines(settings, folder, "nvbdriverfiles.cfg"));
    }
    if vendor == CleanVendor::Amd && opts.scopes.remove_amd_kmpfd {
        file_tokens.extend(try_load_lines(settings, folder, "driverfilesKMPFD.cfg"));
        file_tokens.extend(try_load_lines(settings, folder, "driverfilesKMAFD.cfg"));
    }
    if vendor == CleanVendor::Intel {
        file_tokens.extend(try_load_lines(settings, folder, "shareddriverfiles.cfg"));
    }
    // Process **all** catalog driverfile tokens via adapter (no silent cap).
    for token in &file_tokens {
        env.delete_file_match(token, journal);
    }
    for svc in &services {
        env.stop_delete_service(svc, journal);
    }
    for task in vendor_task_tokens(vendor) {
        env.delete_scheduled_task(task, journal);
    }
    if opts.scopes.remove_gfe || opts.scopes.remove_install_cache {
        for cache in vendor_install_caches(vendor) {
            let path = driver_foundry_common::expand_path_tokens(cache);
            env.wipe_path(&path, journal);
        }
        messages.push("[Scope] install-cache wipe planned".into());
    } else {
        // Always plan shader/cache soft targets in deep clean
        for cache in vendor_shader_caches(vendor) {
            let path = driver_foundry_common::expand_path_tokens(cache);
            env.wipe_path(&path, journal);
        }
    }
    if vendor == CleanVendor::Nvidia && opts.scopes.remove_unpack_nvidia {
        for p in [r"C:\NVIDIA", r"C:\NVIDIA\DisplayDriver"] {
            env.wipe_path(Path::new(p), journal);
        }
        messages.push("[Scope] remove-unpack-nvidia planned".into());
    }
    if vendor == CleanVendor::Amd && opts.scopes.remove_unpack_amd {
        for p in [r"C:\AMD", r"C:\AMD\AMD-Software"] {
            env.wipe_path(Path::new(p), journal);
        }
        messages.push("[Scope] remove-unpack-amd planned".into());
    }
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
    messages.push(format!(
        "[FoldersCleanup] {folder}: {} driverfile tokens (all via adapter) dryRun={dry}",
        file_tokens.len()
    ));

    // Stage 6 — real adapter path (pnputil DriverStore + pnp lockdown + PCI root)
    stages.push("6_driverstore_finalize".into());
    messages.push(format!(
        "[Stage] 6_driverstore_finalize ({}) dryRun={dry}",
        vendor.ven_id()
    ));
    env.clean_driverstore(folder, vendor.ven_id(), journal);
    env.registry_cleanup("fix_driverstore_registry", folder, journal);
    env.pnp_lockdown_orphans(folder, journal);
    // CheckPciRoot-class: Enum\PCI leftovers for all GPU vendors
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
        "[DriverStore] clean_driverstore+pnp_lockdown+pci_root via OsEnvironment dryRun={dry}"
    ));

    Ok(())
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
    for svc in &services {
        env.stop_delete_service(svc, journal);
        env.kill_process_match(svc, journal);
    }

    stages.push("2_audio_com_packages".into());
    for token in &classroot {
        env.registry_cleanup("classroot_cleanup", token, journal);
    }
    for token in &clsid {
        env.registry_cleanup("clsid_leftover", token, journal);
    }
    for pkg in &packages {
        env.remove_appx_match(pkg, journal);
    }

    stages.push("3_audio_devices_files".into());
    env.uninstall_device("VEN_10EC", journal);
    env.uninstall_device("HDAUDIO\\FUNC_01&VEN_10EC", journal);
    // MMDevices Realtek / HD Audio endpoint purge (DDU RemoveVendorAudioEndpoints)
    let mm_tokens = mmdevices_tokens_for_vendor(CleanVendor::Realtek);
    env.clean_mmdevices(&mm_tokens, journal);
    for token in &driverfiles {
        env.delete_file_match(token, journal);
    }
    messages.push(format!(
        "[FoldersCleanup] REALTEK: {} driverfile tokens (all via adapter) dryRun={dry}",
        driverfiles.len()
    ));
    messages.push("[MMDevices] Realtek audio endpoint cleanup planned".into());

    stages.push("4_audio_finalize".into());
    // Share real DriverStore adapter path (same as GPU stage 6)
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
