use super::*;

#[test]
fn dry_run_plans_without_execute() {
    let mut env = DryRunEnvironment { probe_host: false };
    let mut j = ActionJournal::default();
    env.stop_delete_service("nvlddmkm", &mut j);
    env.kill_process_match("nvcontainer", &mut j);
    assert_eq!(j.count_planned(), 2);
    assert_eq!(j.count_executed(), 0);
}

#[test]
fn probe_service_does_not_panic() {
    let p = probe_service_windows("Schedule");
    assert_eq!(p.name, "Schedule");
    assert!(!p.state.is_empty());
}

#[test]
fn live_adapter_maps_entries() {
    let mut env = DryRunEnvironment { probe_host: false };
    let entry = driver_foundry_common::JournalEntry {
        surface: "Service".into(),
        action: "stop_delete".into(),
        target: "FakeSvcOgcdTest".into(),
        executed: false,
        detail: String::new(),
    };
    apply_planned_entry(&mut env, &entry);
}

#[test]
fn dry_run_wipe_records_existence() {
    let mut env = DryRunEnvironment { probe_host: true };
    let mut j = ActionJournal::default();
    let p = std::env::temp_dir();
    env.wipe_path(&p, &mut j);
    assert_eq!(j.count_planned(), 1);
    assert!(j.entries[0].detail.contains("exists=true"));
    assert_eq!(j.count_executed(), 0);
}

#[test]
fn parse_pnputil_sample() {
    let sample = r#"
Microsoft PnP Utility

Published Name:     oem12.inf
Original Name:      nv_disp.inf
Provider Name:      NVIDIA Corporation
Class Name:         Display adapters

Published Name:     oem3.inf
Original Name:      hdmaud.inf
Provider Name:      Realtek
Class Name:         Media
"#;
    let pkgs = parse_pnputil_enum_drivers(sample);
    assert_eq!(pkgs.len(), 2);
    assert_eq!(pkgs[0].published_name, "oem12.inf");
    let nv = filter_packages_for_vendor(&pkgs, "NVIDIA", "VEN_10DE");
    assert_eq!(nv.len(), 1);
    assert_eq!(nv[0].published_name, "oem12.inf");
    let rtk = filter_packages_for_vendor(&pkgs, "REALTEK", "VEN_10EC");
    assert_eq!(rtk.len(), 1);
}

#[test]
fn dry_run_clean_driverstore_journals() {
    let mut env = DryRunEnvironment { probe_host: false };
    let mut j = ActionJournal::default();
    env.clean_driverstore("NVIDIA", "VEN_10DE", &mut j);
    env.pnp_lockdown_orphans("NVIDIA", &mut j);
    assert!(j
        .entries
        .iter()
        .any(|e| e.action == "clean_driverstore" && e.target == "NVIDIA"));
    assert!(j.entries.iter().any(|e| e.action == "pnp_lockdown_orphans"));
    assert_eq!(j.count_executed(), 0);
}

#[test]
fn recording_env_tracks_driverstore() {
    let mut env = RecordingEnvironment::default();
    let mut j = ActionJournal::default();
    env.clean_driverstore("AMD", "VEN_1002", &mut j);
    assert!(env.called("clean_driverstore"));
    assert!(env.calls[0].1.contains("AMD"));
}

#[test]
fn dry_run_pci_root_journals_enum_paths() {
    let mut env = DryRunEnvironment { probe_host: false };
    let mut j = ActionJournal::default();
    env.clean_pci_root("NVIDIA", "VEN_10DE", &mut j);
    assert!(j
        .entries
        .iter()
        .any(|e| e.action == "clean_pci_root" && e.target.contains("VEN_10DE")));
    assert!(j
        .entries
        .iter()
        .any(|e| e.action == "pci_enum_leftover" && e.target.contains(r"Enum\PCI")));
    assert!(j
        .entries
        .iter()
        .any(|e| e.action == "pci_filter_stop_delete"));
    assert!(
        j.entries
            .iter()
            .any(|e| e.action == "StripFilterValues" && e.target.contains("nvpciflt")),
        "NVIDIA pci root must plan multi-sz StripFilterValues: {:?}",
        j.entries
            .iter()
            .map(|e| format!("{}:{}", e.action, e.target))
            .collect::<Vec<_>>()
    );
    assert_eq!(j.count_executed(), 0);
}

#[test]
fn multi_sz_strip_pure_logic() {
    // Exact filter entry removed; neighbor kept
    let parts = parse_multi_sz_filters("mouclass\0nvpciflt\0kbdclass\0");
    assert_eq!(parts.len(), 3);
    let (kept, removed) = strip_filters_from_multi_sz(&parts, &["nvpciflt", "nvkflt"]);
    assert_eq!(removed, 1);
    assert_eq!(kept, vec!["mouclass".to_string(), "kbdclass".to_string()]);

    // Contains-match (DDU semantics)
    let parts2 = vec!["nvpciflt_legacy".into(), "ok".into()];
    let (kept2, rem2) = strip_filters_from_multi_sz(&parts2, &["nvpciflt"]);
    assert_eq!(rem2, 1);
    assert_eq!(kept2, vec!["ok".to_string()]);

    // Empty after strip
    let parts3 = vec!["nvkflt".into()];
    let (kept3, rem3) = strip_filters_from_multi_sz(&parts3, &["nvkflt"]);
    assert_eq!(rem3, 1);
    assert!(kept3.is_empty());

    // No hit
    assert!(!would_strip_filter_value(
        "mouclass\0kbdclass",
        &["nvpciflt"]
    ));
    assert!(would_strip_filter_value(
        "mouclass\0nvpciflt",
        &["nvpciflt"]
    ));

    // AMD tokens present
    assert_eq!(pci_filter_tokens("AMD"), vec!["amdkmpfd", "amdkmafd"]);
    assert_eq!(pci_filter_tokens("NVIDIA"), vec!["nvpciflt", "nvkflt"]);
    assert!(pci_filter_tokens("INTEL").is_empty());
}

#[test]
fn amd_pci_root_plans_acpi_filter_strip() {
    let mut j = ActionJournal::default();
    plan_pci_root_entries("AMD", "VEN_1002", &mut j);
    assert!(
        j.entries.iter().any(|e| {
            e.action == "StripFilterValues"
                && e.target.contains("ACPI")
                && e.target.contains("amdkmpfd")
        }),
        "AMD should plan ACPI multi-sz strip: {:?}",
        j.entries
            .iter()
            .filter(|e| e.action == "StripFilterValues")
            .map(|e| e.target.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn dry_run_mmdevices_journals_audio_paths() {
    let mut env = DryRunEnvironment { probe_host: false };
    let mut j = ActionJournal::default();
    let tokens = vec!["Realtek".into(), "VEN_10EC".into(), "10EC".into()];
    env.clean_mmdevices(&tokens, &mut j);
    assert!(j.entries.iter().any(|e| e.action == "clean_mmdevices"));
    assert!(j.entries.iter().any(|e| {
        e.action == "mmdevices_flow" && e.target.contains("MMDevices\\Audio\\Render")
    }));
    assert!(j.entries.iter().any(|e| {
        e.action == "mmdevices_flow" && e.target.contains("MMDevices\\Audio\\Capture")
    }));
    assert_eq!(j.count_executed(), 0);
}

#[test]
fn file_match_candidates_expand_roots_and_stay_safe() {
    let c = file_match_candidates(r"\nvapi64.dll");
    assert!(!c.is_empty());
    let joined = c
        .iter()
        .map(|p| p.to_string_lossy().to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("|");
    assert!(
        joined.contains("system32") || joined.contains("syswow64"),
        "expected System32/SysWOW64 roots: {joined}"
    );
    // Empty / bare C:\ must not yield candidates
    assert!(file_match_candidates("").is_empty());
    assert!(file_match_candidates(r"C:\").is_empty());
    assert!(file_match_candidates("C:").is_empty());
    // Nested relative token maps under Program Files / ProgramData
    let nested = file_match_candidates(r"NVIDIA Corporation\NVSMI");
    assert!(
        nested.iter().any(|p| {
            let s = p.to_string_lossy().to_ascii_lowercase();
            s.contains("program files") || s.contains("programdata") || s.contains("system32")
        }),
        "nested token should map under common roots: {nested:?}"
    );
}

#[test]
fn recording_env_tracks_pci_and_mmdevices() {
    let mut env = RecordingEnvironment::default();
    let mut j = ActionJournal::default();
    env.clean_pci_root("AMD", "VEN_1002", &mut j);
    env.clean_mmdevices(&["ven_1002".into()], &mut j);
    assert!(env.called("clean_pci_root"));
    assert!(env.called("clean_mmdevices"));
}
