use super::*;
use std::path::PathBuf;

fn settings() -> PathBuf {
    resolve_settings_root(None)
}

#[test]
fn nvidia_dry_run_plans_work() {
    let r = clean_dry_run_vendor("nvidia", None).expect("clean dry-run");
    assert_eq!(r.exit_code, 0);
    assert!(r.dry_run);
    assert!(r.planned > 0, "must plan actions from catalogs");
    assert_eq!(r.executed, 0);
    assert!(r.stages.iter().any(|s| s == "success"));
    assert!(r.stages.iter().any(|s| s == "0_resolve_vendor"));
    assert!(r.stages.iter().any(|s| s == "5_deep_clean"));
    assert!(r.plan_report.contains("planned_total="));
    assert!(r.plan_report.contains("mode=dry-run"));
    assert!(
        r.journal
            .entries
            .iter()
            .any(|e| e.surface == "Service" && e.target.to_ascii_lowercase().contains("nv")),
        "expected NVIDIA service tokens in journal"
    );
}

#[test]
fn amd_and_realtek_dry_run() {
    for v in ["amd", "intel", "lisuan", "realtek"] {
        let r = clean_dry_run_vendor(v, None).unwrap_or_else(|e| panic!("{v}: {e}"));
        assert!(r.planned > 0, "{v} planned>0");
        assert_eq!(r.executed, 0, "{v} executed=0");
        assert_eq!(r.exit_code, 0);
    }
}

#[test]
fn remove_scopes_expand_plan() {
    let base = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: true,
        settings_root: settings(),
        host_probe: false,
        ..CleanOptions::default()
    };
    let with_gfe = CleanOptions {
        scopes: RemoveScopes {
            remove_gfe: true,
            remove_install_cache: true,
            ..RemoveScopes::default()
        },
        ..base.clone()
    };
    let r0 = run_clean(&base).unwrap();
    let r1 = run_clean(&with_gfe).unwrap();
    assert!(
        r1.planned > r0.planned,
        "gfe scope should add planned work: base={} gfe={}",
        r0.planned,
        r1.planned
    );
    assert!(r1.messages.iter().any(|m| m.contains("remove-gfe")));
}

#[test]
fn execute_without_elevation_errors_or_relaunches() {
    let opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: false,
        settings_root: settings(),
        attempt_elevation: false, // do not pop UAC in tests
        host_probe: false,
        ..CleanOptions::default()
    };
    match run_clean(&opts) {
        Ok(r) => {
            // If already admin, execute path runs
            assert!(!r.dry_run);
            assert!(r.planned > 0 || r.elevation_relaunched);
        }
        Err(CleanError::ElevationRequired(msg)) => {
            assert!(
                msg.to_ascii_lowercase().contains("admin")
                    || msg.to_ascii_lowercase().contains("elevation")
            );
        }
        Err(CleanError::LiveCatalogAuthenticationRequired) => {}
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn execute_not_stub_refuse() {
    // Must not return the old "not implemented" error
    let opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: false,
        settings_root: settings(),
        attempt_elevation: false,
        host_probe: false,
        ..CleanOptions::default()
    };
    let err_str = match run_clean(&opts) {
        Ok(_) => String::new(),
        Err(e) => e.to_string(),
    };
    assert!(
        !err_str.to_ascii_lowercase().contains("not implemented"),
        "must not refuse with not-implemented stub: {err_str}"
    );
}

#[test]
fn execute_requires_authenticated_catalogs_before_elevation_or_live_adapters() {
    let opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: false,
        settings_root: std::env::temp_dir().join("untrusted-settings-override"),
        attempt_elevation: false,
        host_probe: false,
        ..CleanOptions::default()
    };
    assert!(matches!(
        run_clean(&opts),
        Err(CleanError::LiveCatalogAuthenticationRequired)
    ));
}

#[test]
fn preflight_ok() {
    let (ok, msgs) = preflight(&settings());
    assert!(ok, "{msgs:?}");
    assert!(msgs.iter().any(|m| m.contains("NVIDIA")));
    assert!(msgs.iter().any(|m| m.contains("admin:")));
    assert!(msgs.iter().any(|m| m.contains("safemode:")));
}

#[test]
fn preflight_defers_host_tools_even_when_path_cannot_be_trusted() {
    // This test deliberately does not mutate PATH: the test harness is concurrent. The source
    // contract guarantees a hostile PATH cannot be consulted because preflight invokes neither
    // process-backed elevation nor Safe Mode probes.
    let (_, messages) = preflight(&settings());
    assert!(messages
        .iter()
        .any(|message| message == "admin: unknown (host probe deferred)"));
    assert!(messages
        .iter()
        .any(|message| message == "safemode: unknown (host probe deferred)"));
    let source = include_str!("support.rs");
    assert!(!source.contains("is_administrator()"));
    assert!(!source.contains("is_safe_mode()"));
    assert!(!source.contains("Command::new"));
}

#[test]
fn plan_report_file_write() {
    let path = std::env::temp_dir().join(format!("dfoundry-plan-{}.txt", std::process::id()));
    let _ = fs::remove_file(&path);
    let opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: true,
        settings_root: settings(),
        plan_report_path: Some(path.clone()),
        host_probe: false,
        ..CleanOptions::default()
    };
    let r = run_clean(&opts).unwrap();
    assert!(path.is_file());
    let body = fs::read_to_string(&path).unwrap();
    assert!(body.contains("PLAN REPORT"));
    assert!(r.plan_report.contains("planned_total="));
    let _ = fs::remove_file(&path);
}

#[test]
fn plan_report_refuses_to_replace_existing_file() {
    let path =
        std::env::temp_dir().join(format!("dfoundry-plan-existing-{}.txt", std::process::id()));
    let _ = fs::remove_file(&path);
    fs::write(&path, "user-owned report").unwrap();
    let opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: true,
        settings_root: settings(),
        plan_report_path: Some(path.clone()),
        host_probe: false,
        ..CleanOptions::default()
    };
    assert!(matches!(run_clean(&opts), Err(CleanError::Io(_))));
    assert_eq!(fs::read_to_string(&path).unwrap(), "user-owned report");
    let _ = fs::remove_file(path);
}

#[test]
fn cache_only_skips_setupapi() {
    let opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: true,
        settings_root: settings(),
        cache_only: true,
        host_probe: false,
        ..CleanOptions::default()
    };
    let r = run_clean(&opts).unwrap();
    assert!(r.stages.iter().any(|s| s == "0b_cache_only"));
    assert!(!r.stages.iter().any(|s| s == "4_setupapi_devices"));
    assert!(r.planned > 0);
}

#[test]
fn options_from_selection_maps_gui() {
    let o = options_from_selection(
        CleanVendor::Amd,
        true,
        RemoveScopes {
            remove_amd_kmpfd: true,
            ..RemoveScopes::default()
        },
        None,
    );
    assert_eq!(o.vendor, CleanVendor::Amd);
    assert!(o.scopes.remove_amd_kmpfd);
    assert!(o.dry_run);
}

#[test]
fn unknown_vendor() {
    let err = clean_dry_run_vendor("not-a-gpu", None).unwrap_err();
    assert!(matches!(err, CleanError::UnknownVendor(_)));
}

#[test]
fn host_probe_flag_still_keeps_dry_run_process_free() {
    let opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: true,
        settings_root: settings(),
        host_probe: true,
        ..CleanOptions::default()
    };
    let r = run_clean(&opts).unwrap();
    assert!(r.planned > 0);
    assert_eq!(r.executed, 0);
    // Dry-run must not spawn host probes, even when the compatibility flag is set.
    let probed = r
        .journal
        .entries
        .iter()
        .filter(|e| e.surface == "Service" && !e.detail.is_empty())
        .count();
    assert_eq!(probed, 0, "dry-run must not attach host probe results");
}

#[test]
fn clean_complete_scopes_expand_all_vendors_catalog_overlays() {
    let opts = CleanOptions {
        vendor: CleanVendor::Intel,
        dry_run: true,
        settings_root: settings(),
        scopes: RemoveScopes::clean_complete(),
        host_probe: false,
        ..CleanOptions::default()
    };
    let r = run_clean(&opts).unwrap();
    assert!(r.planned > 50);
    assert!(
        r.messages.iter().any(|m| m.contains("remove-intel")
            || m.contains("Scope")
            || m.contains("oneapi")
            || m.contains("igs")
            || m.contains("npu")
            || r.planned > 100),
        "clean-complete should pull intel scopes: msgs={:?}",
        r.messages
            .iter()
            .filter(|m| m.contains("Scope") || m.contains("remove"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn power_flags_journal_only() {
    let opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: true,
        settings_root: settings(),
        restart: true,
        shutdown: true,
        host_probe: false,
        ..CleanOptions::default()
    };
    let r = run_clean(&opts).unwrap();
    assert!(r.stages.iter().any(|s| s == "power"));
    assert_eq!(
        r.journal
            .entries
            .iter()
            .filter(|e| e.surface == "Power" && e.executed)
            .count(),
        0
    );
}

#[test]
fn stage6_goes_through_os_environment() {
    use crate::adapters::RecordingEnvironment;
    let opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: true,
        settings_root: settings(),
        host_probe: false,
        attempt_elevation: false,
        ..CleanOptions::default()
    };
    let mut rec = RecordingEnvironment::default();
    let r = run_clean_with_env(&opts, &mut rec).expect("clean with recording env");
    assert!(
        r.stages.iter().any(|s| s == "6_driverstore_finalize"),
        "stage 6 present: {:?}",
        r.stages
    );
    assert!(
        rec.called("clean_driverstore"),
        "clean_driverstore must be invoked on OsEnvironment; calls={:?}",
        rec.calls
    );
    assert!(
        rec.called("pnp_lockdown_orphans"),
        "pnp_lockdown_orphans must be invoked on OsEnvironment; calls={:?}",
        rec.calls
    );
    assert!(
        r.journal
            .entries
            .iter()
            .any(|e| e.action == "clean_driverstore"),
        "journal must record clean_driverstore"
    );
    // Must not be a silent mark_executed-only path with zero adapter involvement
    assert!(rec.call_count("clean_driverstore") >= 1);
}

#[test]
fn gpu_dry_run_journals_pci_root_plans() {
    use crate::adapters::RecordingEnvironment;
    let opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: true,
        settings_root: settings(),
        host_probe: false,
        ..CleanOptions::default()
    };
    let mut rec = RecordingEnvironment::default();
    let r = run_clean_with_env(&opts, &mut rec).expect("gpu clean");
    assert!(
        rec.called("clean_pci_root"),
        "clean_pci_root must be called for GPU path; calls={:?}",
        rec.calls
    );
    assert!(
        r.journal.entries.iter().any(|e| {
            e.action == "clean_pci_root" && e.target.to_ascii_uppercase().contains("VEN_10DE")
        }),
        "journal must plan PCI\\VEN_* target"
    );
    assert!(
        r.journal
            .entries
            .iter()
            .any(|e| { e.action == "pci_enum_leftover" && e.target.contains(r"Enum\PCI") }),
        "journal must plan Enum\\PCI leftover path"
    );
    assert_eq!(r.executed, 0);
}

#[test]
fn realtek_and_remove_audiobus_expand_mmdevices_plans() {
    use crate::adapters::RecordingEnvironment;

    // Realtek audio path always plans MMDevices
    let audio_opts = CleanOptions {
        vendor: CleanVendor::Realtek,
        dry_run: true,
        settings_root: settings(),
        host_probe: false,
        ..CleanOptions::default()
    };
    let mut rec = RecordingEnvironment::default();
    let r = run_clean_with_env(&audio_opts, &mut rec).expect("audio clean");
    assert!(
        rec.called("clean_mmdevices"),
        "Realtek path must call clean_mmdevices; calls={:?}",
        rec.calls
    );
    assert!(
        r.journal
            .entries
            .iter()
            .any(|e| e.action == "clean_mmdevices" || e.action == "mmdevices_flow"),
        "journal must include MMDevices plans"
    );
    assert!(r.messages.iter().any(|m| m.contains("MMDevices")));

    // remove_audiobus on GPU vendor expands MMDevices
    let mut rec2 = RecordingEnvironment::default();
    let gpu_opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: true,
        settings_root: settings(),
        host_probe: false,
        scopes: RemoveScopes {
            remove_audiobus: true,
            ..RemoveScopes::default()
        },
        ..CleanOptions::default()
    };
    let r2 = run_clean_with_env(&gpu_opts, &mut rec2).expect("gpu+audiobus");
    assert!(
        rec2.called("clean_mmdevices"),
        "remove_audiobus must call clean_mmdevices"
    );
    assert!(r2.messages.iter().any(|m| m.contains("remove-audiobus")));
    assert!(
        r2.journal.entries.iter().any(|e| {
            e.surface == "Registry"
                && (e.action.contains("mmdevices") || e.action == "clean_mmdevices")
        }),
        "audiobus scope journals MMDevices registry plans"
    );
}

#[test]
fn realtek_finalize_uses_driverstore_adapter() {
    use crate::adapters::RecordingEnvironment;
    let opts = CleanOptions {
        vendor: CleanVendor::Realtek,
        dry_run: true,
        settings_root: settings(),
        host_probe: false,
        ..CleanOptions::default()
    };
    let mut rec = RecordingEnvironment::default();
    let r = run_clean_with_env(&opts, &mut rec).expect("audio clean");
    assert!(rec.called("clean_driverstore"));
    assert!(rec.called("pnp_lockdown_orphans"));
    assert!(rec.called("clean_pci_root"));
    assert!(r.messages.iter().any(|m| m.contains("driverstore")));
}

#[test]
fn all_driverfile_tokens_go_through_delete_file_match() {
    use crate::adapters::RecordingEnvironment;
    use crate::catalog::load_lines;
    let opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: true,
        settings_root: settings(),
        host_probe: false,
        ..CleanOptions::default()
    };
    let catalog_count = load_lines(&settings(), "NVIDIA", "driverfiles.cfg")
        .expect("driverfiles")
        .len();
    assert!(
        catalog_count > 128,
        "test assumes NVIDIA catalog exceeds old 128 cap; got {catalog_count}"
    );
    let mut rec = RecordingEnvironment::default();
    let _ = run_clean_with_env(&opts, &mut rec).expect("clean");
    let deletes = rec.call_count("delete_file_match");
    assert!(
            deletes >= catalog_count,
            "every driverfiles token must call delete_file_match; catalog={catalog_count} calls={deletes}"
        );
}

#[test]
fn injected_environment_cannot_bypass_live_catalog_authentication() {
    use crate::adapters::RecordingEnvironment;

    let opts = CleanOptions {
        vendor: CleanVendor::Nvidia,
        dry_run: false,
        settings_root: settings(),
        host_probe: false,
        ..CleanOptions::default()
    };
    let mut env = RecordingEnvironment {
        fail_services: true,
        ..RecordingEnvironment::default()
    };
    assert!(matches!(
        run_clean_with_env(&opts, &mut env),
        Err(CleanError::LiveCatalogAuthenticationRequired)
    ));
    assert!(env.calls.is_empty(), "live adapter must not be invoked");
}
