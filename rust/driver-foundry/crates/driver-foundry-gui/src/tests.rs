use super::*;
use driver_foundry_clean::CleanVendor;

#[test]
fn clean_ui_maps_to_engine_options() {
    let mut ui = CleanUiState {
        vendor_idx: 0,
        dry_run: true,
        remove_gfe: true,
        prepare_safeboot: true,
        ..CleanUiState::default()
    };
    let options = ui.to_options();
    assert_eq!(options.vendor, CleanVendor::Nvidia);
    assert!(options.scopes.remove_gfe);
    assert!(options.prepare_safeboot);
    assert!(options.dry_run);
    ui.run();
    assert!(ui.last_planned > 0);
    assert_eq!(ui.last_executed, 0);
    assert!(!ui.last_log.is_empty());
}

#[test]
fn install_wizard_maps_and_runs_synthetic() {
    let export =
        std::env::temp_dir().join(format!("driver-foundry-gui-export-{}", std::process::id()));
    let mut ui = InstallUiState {
        preset: "clean".into(),
        dry_run: true,
        deep_inf: true,
        export_path: export.display().to_string(),
        ..InstallUiState::default()
    };
    let options = ui.to_options();
    assert!(options.dry_run_install);
    assert_eq!(options.preset, "clean");
    assert!(options.deep_inf);
    assert!(options.export_path.is_some());
    ui.run();
    assert!(ui.last_kept.iter().any(|item| item == "Display.Driver"));
    assert!(!ui.last_log.is_empty());
    assert_eq!(ui.page, InstallPage::Results);
    let _ = std::fs::remove_dir_all(export);
}

#[test]
fn app_defaults_to_dry_run() {
    let app = DriverFoundryApp::default();
    assert!(app.clean.dry_run);
    assert!(app.install.dry_run);
    assert!(!app.install.force_install);
    assert!(app.clean.to_options().dry_run);
    assert!(app.install.to_options().dry_run_install);
}

#[test]
fn run_gui_function_exists() {
    let function: fn() -> eframe::Result<()> = run_gui;
    assert!(std::mem::size_of_val(&function) > 0);
}

#[test]
fn clean_ui_all_scopes_and_flags_map() {
    let ui = CleanUiState {
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
        cache_only: true,
        block_driver_search: true,
        no_setup_api: true,
        no_restore_point: true,
        prepare_safeboot: true,
        clear_safeboot: true,
        safeboot_network: true,
        restart: true,
        shutdown: true,
        dry_run: true,
        ..CleanUiState::default()
    };
    let options = ui.to_options();
    assert!(options.scopes.remove_gfe);
    assert!(options.scopes.remove_nv_broadcast);
    assert!(options.scopes.remove_amd_kmpfd);
    assert!(options.scopes.remove_intel_igs);
    assert!(options.scopes.remove_intel_npu);
    assert!(options.scopes.remove_oneapi);
    assert!(options.scopes.remove_endurance);
    assert!(options.scopes.remove_vulkan);
    assert!(options.scopes.remove_physx);
    assert!(options.scopes.remove_audiobus);
    assert!(options.scopes.remove_monitors);
    assert!(options.scopes.remove_unpack_nvidia);
    assert!(options.scopes.remove_unpack_amd);
    assert!(options.scopes.remove_install_cache);
    assert!(options.cache_only);
    assert!(options.block_driver_search);
    assert!(options.no_setup_api);
    assert!(options.no_restore_point);
    assert!(options.prepare_safeboot);
    assert!(options.clear_safeboot);
    assert!(options.safeboot_network);
    assert!(options.restart);
    assert!(options.shutdown);
}

#[test]
fn install_force_install_maps_dry_run_false() {
    let ui = InstallUiState {
        force_install: true,
        dry_run: true,
        deep_inf: true,
        package_url: "https://example.invalid/pkg.exe".into(),
        package_archive: r"C:\tmp\pkg.zip".into(),
        select_extra: "PhysX, HDAudio".into(),
        deselect_extra: "FrameViewSdk".into(),
        archive_format: "7z".into(),
        ..InstallUiState::default()
    };
    let options = ui.to_options();
    assert!(!options.dry_run_install);
    assert!(options.deep_inf);
    assert_eq!(
        options.package_url.as_deref(),
        Some("https://example.invalid/pkg.exe")
    );
    assert!(options.package_archive.is_some());
    assert!(options.select.iter().any(|item| item == "PhysX"));
    assert!(options.select.iter().any(|item| item == "HDAudio"));
    assert!(options.deselect.iter().any(|item| item == "FrameViewSdk"));
    assert_eq!(options.archive_format, "7z");
}

#[test]
fn clearing_gui_dry_run_without_force_cannot_launch_installer() {
    let ui = InstallUiState {
        dry_run: false,
        force_install: false,
        ..InstallUiState::default()
    };
    assert!(ui.to_options().dry_run_install);
}
