#[test]
fn platform_gate_matches_compilation_target() {
    assert_eq!(platform_is_supported(), cfg!(windows));
}

fn final_benchmark_capture() -> BenchmarkCapture {
    BenchmarkCapture {
        average_fps: 300.0,
        p1_fps: 180.0,
        runs: 3,
    }
}

fn final_benchmark_progress() -> Progress {
    let mut progress = Progress::default();
    progress.complete(3, 1, "2026-08-10 12:00:00".into());
    for step in 2..13 {
        progress.skip(3, step);
    }
    progress
}

fn phase_three_armed_state() -> State {
    let transaction_id =
        TransactionId::parse("0123456789abcdef0123456789abcdef").expect("transaction id");
    State {
        active_reboot_transaction: Some(frametime_core::RebootTransaction {
            schema_version: 1,
            transaction_id: Some(transaction_id.clone()),
            initiator_user_sid: Some("S-1-5-21-1".into()),
            stage: RebootStage::PhaseThreeArmed,
            runtime: Some(frametime_core::RuntimeRecord {
                generation: transaction_id.to_string(),
                manifest_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .into(),
                payload_contract_hash:
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
                executable_path: "frametime.exe".into(),
                executable_sha256:
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
                unknown: BTreeMap::new(),
            }),
            driver_package: None,
            created_utc: None,
            updated_utc: None,
            unknown: BTreeMap::new(),
        }),
        ..State::default()
    }
}

fn prepared_final_benchmark() -> (State, Progress, FinalBenchmarkCommit) {
    let state = phase_three_armed_state();
    let progress = final_benchmark_progress();
    let config = checked_in_config();
    let commit = prepare_final_benchmark_commit(
        &state,
        &progress,
        &[],
        &config,
        TransactionId::parse("fedcba9876543210fedcba9876543210").expect("receipt id"),
        "2026-08-10 12:34:56".into(),
        final_benchmark_capture(),
    )
    .expect("final benchmark commit");
    (state, progress, commit)
}

#[test]
fn final_benchmark_retries_bind_a_history_prefix_to_its_existing_receipt() {
    let (state, progress, expected) = prepared_final_benchmark();
    let actual = reconcile_final_benchmark(
        &state,
        &progress,
        &expected.history,
        &checked_in_config(),
        TransactionId::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("fresh receipt id"),
        "2026-08-10 13:00:00".into(),
        final_benchmark_capture(),
    )
    .expect("reconcile history prefix");
    let FinalBenchmarkReconciliation::Pending(actual) = actual else {
        panic!("history prefix must still require durable completion");
    };
    assert_eq!(*actual, expected);
}

#[test]
fn final_benchmark_retries_repair_a_state_prefix_without_changing_its_receipt() {
    let (_state, progress, expected) = prepared_final_benchmark();
    let actual = reconcile_final_benchmark(
        &expected.state,
        &progress,
        &[],
        &checked_in_config(),
        TransactionId::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("fresh receipt id"),
        "2026-08-10 13:00:00".into(),
        final_benchmark_capture(),
    )
    .expect("reconcile state prefix");
    let FinalBenchmarkReconciliation::Pending(actual) = actual else {
        panic!("state prefix must still require durable completion");
    };
    assert_eq!(*actual, expected);
}

#[test]
fn final_benchmark_reconciliation_rejects_conflicting_or_mismatched_prefixes() {
    let (state, progress, expected) = prepared_final_benchmark();
    let mut conflicting_history = expected.history.clone();
    conflicting_history[0].receipt_id =
        Some(TransactionId::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("receipt id"));
    assert!(
        reconcile_final_benchmark(
            &expected.state,
            &progress,
            &conflicting_history,
            &checked_in_config(),
            TransactionId::parse("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").expect("fresh receipt id"),
            "2026-08-10 13:00:00".into(),
            final_benchmark_capture(),
        )
        .is_err()
    );
    assert!(
        reconcile_final_benchmark(
            &state,
            &progress,
            &expected.history,
            &checked_in_config(),
            TransactionId::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("fresh receipt id"),
            "2026-08-10 13:00:00".into(),
            BenchmarkCapture {
                average_fps: 301.0,
                ..final_benchmark_capture()
            },
        )
        .is_err()
    );
}

#[test]
fn final_benchmark_reconciliation_returns_an_existing_complete_receipt() {
    let (_state, _progress, expected) = prepared_final_benchmark();
    let actual = reconcile_final_benchmark(
        &expected.state,
        &expected.progress,
        &expected.history,
        &checked_in_config(),
        TransactionId::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("fresh receipt id"),
        "2026-08-10 13:00:00".into(),
        final_benchmark_capture(),
    )
    .expect("complete receipt");
    let FinalBenchmarkReconciliation::Complete(receipt) = actual else {
        panic!("coherent persistence must be idempotent");
    };
    assert_eq!(receipt, expected.receipt);
}

#[test]
fn final_benchmark_complete_retry_rejects_a_different_requested_capture() {
    let (_state, _progress, expected) = prepared_final_benchmark();
    assert!(
        reconcile_final_benchmark(
            &expected.state,
            &expected.progress,
            &expected.history,
            &checked_in_config(),
            TransactionId::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("fresh receipt id"),
            "2026-08-10 13:00:00".into(),
            BenchmarkCapture {
                average_fps: 301.0,
                ..final_benchmark_capture()
            },
        )
        .is_err()
    );
}

#[test]
fn p3_13_receipt_status_distinguishes_absent_coherent_and_incoherent_evidence() {
    let (_state, _progress, commit) = prepared_final_benchmark();
    assert_eq!(
        final_benchmark_status_from_records(&commit.state, &commit.progress, &commit.history),
        FinalBenchmarkStatus::Coherent(commit.receipt.clone())
    );
    assert_eq!(
        final_benchmark_status_from_records(&State::default(), &Progress::default(), &[]),
        FinalBenchmarkStatus::Absent
    );

    let mut prefix = commit.progress.clone();
    prefix.completed_steps.remove(&Progress::key(3, 13));
    prefix.timestamps.remove("3-13");
    assert!(matches!(
        final_benchmark_status_from_records(&commit.state, &prefix, &commit.history),
        FinalBenchmarkStatus::Incoherent(_)
    ));

    let mut mismatch = commit.history.clone();
    mismatch[0].receipt_id =
        Some(TransactionId::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("receipt id"));
    assert!(matches!(
        final_benchmark_status_from_records(&commit.state, &commit.progress, &mismatch),
        FinalBenchmarkStatus::Incoherent(_)
    ));
}

#[test]
fn final_benchmark_complete_and_partial_retries_do_not_generate_receipt_ids() {
    use std::cell::Cell;

    let (_state, _progress, complete) = prepared_final_benchmark();
    let generated = Cell::new(0_u8);
    let result = reconcile_final_benchmark(
        &complete.state,
        &complete.progress,
        &complete.history,
        &checked_in_config(),
        || {
            generated.set(generated.get() + 1);
            Err("receipt id must not be generated for complete retry".into())
        },
        "2026-08-10 13:00:00".into(),
        final_benchmark_capture(),
    )
    .expect("complete retry");
    assert!(matches!(result, FinalBenchmarkReconciliation::Complete(_)));
    assert_eq!(generated.get(), 0);

    let (state, progress, partial) = prepared_final_benchmark();
    let generated = Cell::new(0_u8);
    let result = reconcile_final_benchmark(
        &state,
        &progress,
        &partial.history,
        &checked_in_config(),
        || {
            generated.set(generated.get() + 1);
            Err("receipt id must not be generated for partial retry".into())
        },
        "2026-08-10 13:00:00".into(),
        final_benchmark_capture(),
    )
    .expect("partial retry");
    assert!(matches!(result, FinalBenchmarkReconciliation::Pending(_)));
    assert_eq!(generated.get(), 0);
}

#[test]
fn final_benchmark_new_capture_generates_one_receipt_id() {
    use std::cell::Cell;

    let state = phase_three_armed_state();
    let progress = final_benchmark_progress();
    let generated = Cell::new(0_u8);
    let result = reconcile_final_benchmark(
        &state,
        &progress,
        &[],
        &checked_in_config(),
        || {
            generated.set(generated.get() + 1);
            Ok(TransactionId::parse("fedcba9876543210fedcba9876543210").expect("receipt id"))
        },
        "2026-08-10 13:00:00".into(),
        final_benchmark_capture(),
    )
    .expect("new capture");
    assert!(matches!(result, FinalBenchmarkReconciliation::Pending(_)));
    assert_eq!(generated.get(), 1);
}

#[test]
fn p3_13_status_treats_only_empty_evidence_as_absent() {
    assert_eq!(
        final_benchmark_status_from_records(&State::default(), &Progress::default(), &[]),
        FinalBenchmarkStatus::Absent
    );
}

#[test]
fn p3_13_status_treats_completed_stage_or_timestamp_without_a_receipt_as_incoherent() {
    let mut completed_stage = phase_three_armed_state();
    completed_stage
        .active_reboot_transaction
        .as_mut()
        .expect("transaction")
        .stage = RebootStage::PhaseThreeComplete;
    assert!(matches!(
        final_benchmark_status_from_records(&completed_stage, &Progress::default(), &[]),
        FinalBenchmarkStatus::Incoherent(_)
    ));

    let mut timestamp_only = Progress::default();
    timestamp_only
        .timestamps
        .insert("3-13".into(), "2026-08-10 12:34:56".into());
    assert!(matches!(
        final_benchmark_status_from_records(&State::default(), &timestamp_only, &[]),
        FinalBenchmarkStatus::Incoherent(_)
    ));
}

#[cfg(not(windows))]
#[test]
fn final_benchmark_refuses_before_creating_a_host_directory() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let work_dir = directory.path().join("must-not-exist");
    assert!(persist_final_benchmark(
        &work_dir,
        final_benchmark_capture(),
        &checked_in_verified_config(),
    )
    .is_err());
    assert!(!work_dir.exists());
}
fn batch_for(step: u8) -> Vec<RegistryChange> {
    match action_for(1, step).expect("catalog action") {
        Action::RegistryBatch(changes) => changes,
        action => panic!("P1:{step} must be an all-or-nothing registry batch, got {action:?}"),
    }
}
fn has_change(changes: &[RegistryChange], hive: Hive, key: &str, name: &str, value: RegValue) {
    assert!(
        changes.iter().any(|change| {
            change.hive == hive && change.key == key && change.name == name && change.value == value
        }),
        "missing {key}\\{name}"
    );
}
#[test]
fn p1_11_12_23_and_26_are_exact_batch_actions() {
    let mpo = batch_for(11);
    assert_eq!(mpo.len(), 1);
    has_change(
        &mpo,
        Hive::LocalMachine,
        "SOFTWARE\\Microsoft\\Windows\\Dwm",
        "OverlayTestMode",
        RegValue::Dword(5),
    );
    let game_mode = batch_for(12);
    assert_eq!(game_mode.len(), 2);
    for name in ["AllowAutoGameMode", "AutoGameModeEnabled"] {
        has_change(
            &game_mode,
            Hive::CurrentUser,
            "SOFTWARE\\Microsoft\\GameBar",
            name,
            RegValue::Dword(1),
        );
    }
    let fast_startup = batch_for(23);
    assert_eq!(fast_startup.len(), 1);
    has_change(
        &fast_startup,
        Hive::LocalMachine,
        "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Power",
        "HiberbootEnabled",
        RegValue::Dword(0),
    );
    let fse = batch_for(26);
    assert_eq!(fse.len(), 4);
    for (name, value) in [
        ("GameDVR_DXGIHonorFSEWindowsCompatible", 1),
        ("GameDVR_FSEBehavior", 2),
        ("GameDVR_FSEBehaviorMode", 2),
        ("GameDVR_HonorUserFSEBehaviorMode", 1),
    ] {
        has_change(
            &fse,
            Hive::CurrentUser,
            "System\\GameConfigStore",
            name,
            RegValue::Dword(value),
        );
    }
}
#[test]
fn p3_7_uses_the_exact_deviceguard_hvci_batch_and_detection_contract() {
    let action = action_for(3, 7).expect("P3:7 catalog action");
    let changes = match &action {
        Action::VbsHvciBatch(changes) => changes,
        other => panic!("P3:7 must be a DeviceGuard-gated batch, got {other:?}"),
    };
    assert_eq!(changes.len(), 1);
    has_change(
        changes,
        Hive::LocalMachine,
        "SYSTEM\\CurrentControlSet\\Control\\DeviceGuard\\Scenarios\\HypervisorEnforcedCodeIntegrity",
        "Enabled",
        RegValue::Dword(0),
    );
    assert_eq!(vbs_hvci_inspection(0, None), Inspection::Satisfied);
    assert_eq!(vbs_hvci_inspection(1, None), Inspection::Satisfied);
    assert_eq!(
        vbs_hvci_inspection(2, Some(&RegValue::Dword(0))),
        Inspection::Satisfied
    );
    assert_eq!(vbs_hvci_inspection(2, None), Inspection::NeedsApply);
    assert_eq!(
        vbs_hvci_inspection(3, Some(&RegValue::Dword(1))),
        Inspection::NeedsApply
    );
}

#[test]
fn p3_7_restore_requires_its_exact_step_and_registry_identity() {
    let key = "SYSTEM\\CurrentControlSet\\Control\\DeviceGuard\\Scenarios\\HypervisorEnforcedCodeIntegrity";
    assert!(validate_registry_restore_binding("P3:7", Hive::LocalMachine, key, "Enabled").is_ok());
    assert!(validate_registry_restore_binding("P1:7", Hive::LocalMachine, key, "Enabled").is_err());
    assert!(
        validate_registry_restore_binding("P3:7", Hive::LocalMachine, key, "Injected").is_err()
    );
}
#[test]
fn p3_10_is_only_the_exact_cs2_image_execution_priority_transaction() {
    let action = action_for(3, 10).expect("P3:10 catalog action");
    let change = match action {
        Action::ProcessPriority(change) => change,
        other => panic!("P3:10 must be a typed registry transaction, got {other:?}"),
    };
    assert_eq!(change.hive, Hive::LocalMachine);
    assert_eq!(
        change.key,
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options\\cs2.exe\\PerfOptions"
    );
    assert_eq!(change.name, "CpuPriorityClass");
    assert_eq!(change.value, RegValue::Dword(3));
}

#[test]
fn p3_10_restore_rejects_every_identity_except_the_captured_value() {
    let key = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options\\cs2.exe\\PerfOptions";
    assert!(
        validate_process_priority_restore_binding(Hive::LocalMachine, key, "CpuPriorityClass")
            .is_ok()
    );
    assert!(
        validate_process_priority_restore_binding(Hive::CurrentUser, key, "CpuPriorityClass")
            .is_err()
    );
    assert!(
        validate_process_priority_restore_binding(Hive::LocalMachine, key, "Injected").is_err()
    );
    assert!(
        validate_registry_restore_binding("P3:10", Hive::LocalMachine, key, "CpuPriorityClass")
            .is_ok()
    );
    assert!(
        validate_registry_restore_binding("P1:10", Hive::LocalMachine, key, "CpuPriorityClass")
            .is_err()
    );

    let mut unknown = BTreeMap::new();
    unknown.insert("tampered".into(), Value::Bool(true));
    assert!(restore_registry(RegistryRestore {
        step: "P3:10",
        path: "HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options\\cs2.exe\\PerfOptions",
        name: "CpuPriorityClass",
        value: &Value::from(3),
        original_type: &Some("DWord".into()),
        existed: true,
        unknown: &unknown,
        config: &checked_in_config(),
    })
    .is_err());
}
#[test]
fn hags_is_a_typed_transaction_while_cs2_registry_actions_are_typed() {
    assert!(matches!(action_for(1, 7), Ok(Action::Hags)));
    assert!(matches!(
        action_for(1, 4),
        Ok(Action::Cs2Registry(
            Cs2RegistryAction::DisableFullscreenOptimizations
        ))
    ));
    assert!(matches!(
        action_for(1, 30),
        Ok(Action::Cs2Registry(Cs2RegistryAction::HighPerformanceGpu))
    ));
}
#[test]
fn p1_14_is_typed_and_uses_only_exact_configured_run_identities() {
    assert!(matches!(action_for(1, 14), Ok(Action::Autostart)));
    let mut config = checked_in_config();
    config.autostart_remove = vec!["OneDrive".into(), "Steam".into()];
    assert_eq!(
        autostart_targets(Some(&config)).expect("safe targets"),
        vec![
            (Hive::CurrentUser, "OneDrive".into()),
            (Hive::CurrentUser, "Steam".into()),
            (Hive::LocalMachine, "OneDrive".into()),
            (Hive::LocalMachine, "Steam".into()),
        ]
    );
    assert!(
        validate_autostart_restore_binding(&config, Hive::CurrentUser, AUTOSTART_RUN_KEY, "Steam")
            .is_ok()
    );
    assert!(
        validate_autostart_restore_binding(&config, Hive::CurrentUser, AUTOSTART_RUN_KEY, "steam")
            .is_err()
    );
}
#[test]
fn p1_14_rejects_unsafe_or_duplicate_configured_names_before_registry_io() {
    let mut config = checked_in_config();
    config.autostart_remove = vec!["".into()];
    assert!(autostart_names(Some(&config)).is_err());
    config.autostart_remove = vec!["Steam".into(), "steam".into()];
    assert!(autostart_names(Some(&config)).is_err());
    config.autostart_remove = vec!["Steam\\Run".into()];
    assert!(autostart_names(Some(&config)).is_err());
}
#[test]
fn p1_14_restore_rejects_tampered_key_unknown_fields_or_absent_capture() {
    let mut config = checked_in_config();
    config.autostart_remove = vec!["Steam".into()];
    let value = Value::String("steam.exe".into());
    let original_type = Some("String".into());
    let empty_unknown = BTreeMap::new();
    let mut unknown = BTreeMap::new();
    unknown.insert("untrusted".into(), Value::Bool(true));
    assert!(
        restore_registry(RegistryRestore {
            step: "P1:14",
            path: "HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run",
            name: "Steam",
            value: &value,
            original_type: &original_type,
            existed: true,
            unknown: &unknown,
            config: &config,
        })
        .is_err()
    );
    assert!(
        restore_registry(RegistryRestore {
            step: "P1:14",
            path: "HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run",
            name: "Steam",
            value: &value,
            original_type: &original_type,
            existed: false,
            unknown: &empty_unknown,
            config: &config,
        })
        .is_err()
    );
    assert!(
        validate_autostart_restore_binding(
            &config,
            Hive::CurrentUser,
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\RunOnce",
            "Steam"
        )
        .is_err()
    );
}
#[test]
fn p1_6_power_plan_has_complete_profile_and_vendor_ordered_settings() {
    let safe = power_settings(Profile::Safe, CpuVendor::Unknown);
    assert_eq!(safe.len(), 9);
    assert_eq!(safe[0].value, 100);
    assert_eq!(safe[8].value, 1);
    let recommended_amd = power_settings(Profile::Recommended, CpuVendor::Amd);
    assert_eq!(recommended_amd.len(), 23);
    assert_eq!(
        recommended_amd[8].setting,
        "893dee8e-2bef-41e0-89c6-b55d0929964c"
    );
    assert_eq!(recommended_amd[8].value, 0);
    let recommended_intel = power_settings(Profile::Recommended, CpuVendor::Intel);
    assert_eq!(recommended_intel.len(), 24);
    assert_eq!(recommended_intel[8].value, 100);
    assert!(
        recommended_intel
            .iter()
            .any(|setting| setting.setting == "4d2b0152-7d5c-498b-88e2-34345392a2c5")
    );
    let competitive = power_settings(Profile::Competitive, CpuVendor::Unknown);
    assert_eq!(competitive.len(), 28);
    assert_eq!(competitive.last().expect("T3 setting").value, 100);
    let competitive_amd = power_settings(Profile::Competitive, CpuVendor::Amd);
    assert_eq!(competitive_amd, recommended_amd);
    assert!(!includes_tier_three(Profile::Competitive, CpuVendor::Amd));
    assert!(includes_tier_three(Profile::Competitive, CpuVendor::Intel));
}
#[test]
fn p1_6_power_plan_parsers_reject_ambiguous_or_hostile_output() {
    let output = "Power Scheme GUID: 381B4222-F694-41F0-9685-FF5BB260DF2E  (Balanced) *";
    assert_eq!(
        parse_active_power_plan(output).expect("active plan"),
        ActivePowerPlan {
            guid: "381b4222-f694-41f0-9685-ff5bb260df2e".into(),
            name: "Balanced".into(),
        }
    );
    assert!(parse_active_power_plan("no GUID").is_err());
    assert!(
        parse_active_power_plan(
            "a 381b4222-f694-41f0-9685-ff5bb260df2e (A)\nb 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c (B)"
        )
        .is_err()
    );
    assert_eq!(
        parse_current_ac_value("Current AC Power Setting Index: 0x000000fe"),
        Some(254)
    );
    assert_eq!(parse_current_ac_value("current AC value: 0x01"), None);
}
#[test]
fn p1_6_restore_rejects_tampered_or_duplicate_ownership() {
    assert!(
        restore_power_plan(
            "P1:6",
            "381b4222-f694-41f0-9685-ff5bb260df2e",
            &["not-a-guid".into()],
            &BTreeMap::new(),
        )
        .is_err()
    );
    assert!(
        restore_power_plan(
            "P1:6",
            "381b4222-f694-41f0-9685-ff5bb260df2e",
            &[
                "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".into(),
                "8C5E7FDA-E8BF-4A96-9A85-A6E23A8C635C".into()
            ],
            &BTreeMap::new(),
        )
        .is_err()
    );
    let mut unknown = BTreeMap::new();
    unknown.insert("spoofed".into(), Value::Bool(true));
    assert!(
        restore_power_plan(
            "P1:6",
            "381b4222-f694-41f0-9685-ff5bb260df2e",
            &["8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".into()],
            &unknown,
        )
        .is_err()
    );
}
#[test]
fn only_typed_observations_can_complete_without_mutation() {
    assert!(matches!(action_for(1, 1), Ok(Action::ObserveConfigState)));
    assert!(matches!(action_for(1, 5), Ok(Action::ObserveGpuInventory)));
    assert!(matches!(action_for(3, 5), Ok(Action::FpsCapInfo)));
    for step in STEPS {
        assert!(
            !format!("{:?}", action_for(step.phase as u8, step.number)).contains("Inspect"),
            "{} still uses a generic inspection action",
            step.title
        );
    }
    assert_eq!(
        inspect_gpu_inventory(&HardwareInfo::default()),
        Err("native display-adapter inventory is empty".into())
    );
    assert_eq!(
        inspect_gpu_inventory(&HardwareInfo {
            display_adapters: vec!["NVIDIA RTX".into()],
            gpu_branch: Some(GpuBranch::Nvidia),
        }),
        Ok(Inspection::Satisfied)
    );
}

#[test]
fn typed_observation_verification_rechecks_config_and_gpu_inventory() {
    let config = checked_in_config();
    assert!(verify_config_state(Some(&config), &State::default()).is_ok());
    assert!(verify_config_state(None, &State::default()).is_err());

    let captured = HardwareInfo {
        display_adapters: vec!["NVIDIA RTX".into()],
        gpu_branch: Some(GpuBranch::Nvidia),
    };
    assert!(verify_gpu_inventory(&captured, &captured).is_ok());
    assert!(
        verify_gpu_inventory(
            &captured,
            &HardwareInfo {
                display_adapters: vec!["AMD Radeon".into()],
                gpu_branch: Some(GpuBranch::Amd),
            },
        )
        .is_err()
    );
}

fn raw_chipset_record(ids: &[&str]) -> RawChipsetDriverRecord {
    RawChipsetDriverRecord {
        instance_id: "PCI\\VEN_1022&DEV_14D8&SUBSYS_00000000&REV_00\\3&1".into(),
        hardware_ids: ids.iter().map(|value| (*value).into()).collect(),
        compatible_ids: vec!["PCI\\VEN_1022&CC_0601".into()],
        inf_path: "oem42.inf".into(),
        provider: "Advanced Micro Devices, Inc.".into(),
        driver_version: "6.10.22.027".into(),
        driver_date_filetime: 133_763_040_000_000_000,
    }
}

#[test]
fn p1_35_selects_only_exact_cpu_vendor_pci_driver_records() {
    let record = raw_chipset_record(&["PCI\\VEN_1022&DEV_14D8"]);
    let inventory = chipset_inventory_from_raw(ChipsetVendor::Amd, vec![record])
        .expect("valid AMD chipset record")
        .expect("matching package");
    assert_eq!(inventory.vendor, ChipsetVendor::Amd);
    assert_eq!(inventory.records.len(), 1);
    assert_eq!(inventory.records[0].inf_path, "oem42.inf");
    assert_eq!(
        inventory.records[0].driver_date_filetime,
        133_763_040_000_000_000
    );

    assert_eq!(
        chipset_inventory_from_raw(
            ChipsetVendor::Intel,
            vec![raw_chipset_record(&["PCI\\VEN_1022&DEV_14D8"])]
        ),
        Err("P1:35 CPU vendor 8086 conflicts with system-device PCI vendor 1022".into())
    );
}

#[test]
fn p1_35_rejects_malformed_ambiguous_and_duplicate_driver_evidence() {
    assert!(
        chipset_inventory_from_raw(
            ChipsetVendor::Amd,
            vec![raw_chipset_record(&["PCI\\VEN_102&DEV_14D8"])]
        )
        .is_err()
    );
    assert!(
        chipset_inventory_from_raw(
            ChipsetVendor::Amd,
            vec![raw_chipset_record(&[
                "PCI\\VEN_1022&DEV_14D8",
                "PCI\\VEN_8086&DEV_7A04",
            ])]
        )
        .is_err()
    );
    let first = raw_chipset_record(&["PCI\\VEN_1022&DEV_14D8"]);
    let second = first.clone();
    assert!(chipset_inventory_from_raw(ChipsetVendor::Amd, vec![first, second]).is_err());

    let mut inbox = raw_chipset_record(&["PCI\\VEN_1022&DEV_14D8"]);
    inbox.inf_path = "machine.inf".into();
    assert!(chipset_inventory_from_raw(ChipsetVendor::Amd, vec![inbox]).is_err());
    let mut wrong_provider = raw_chipset_record(&["PCI\\VEN_1022&DEV_14D8"]);
    wrong_provider.provider = "Microsoft".into();
    assert!(chipset_inventory_from_raw(ChipsetVendor::Amd, vec![wrong_provider]).is_err());
    let mut malformed_version = raw_chipset_record(&["PCI\\VEN_1022&DEV_14D8"]);
    malformed_version.driver_version = "6.10-beta.027".into();
    assert!(chipset_inventory_from_raw(ChipsetVendor::Amd, vec![malformed_version]).is_err());
    let mut zero_date = raw_chipset_record(&["PCI\\VEN_1022&DEV_14D8"]);
    zero_date.driver_date_filetime = 0;
    assert!(chipset_inventory_from_raw(ChipsetVendor::Amd, vec![zero_date]).is_err());
}

#[test]
fn p1_35_allows_inapplicable_only_after_a_complete_unmatched_enumeration() {
    let mut record = raw_chipset_record(&["PCI\\VEN_1234&DEV_0001"]);
    record.compatible_ids = vec!["PCI\\VEN_1234&CC_0601".into()];
    assert_eq!(
        chipset_inventory_from_raw(ChipsetVendor::Amd, vec![record]).expect("enumerated"),
        None
    );
}

#[test]
fn p1_35_verification_requires_exact_immutable_record_equality() {
    let captured = chipset_inventory_from_raw(
        ChipsetVendor::Amd,
        vec![raw_chipset_record(&["PCI\\VEN_1022&DEV_14D8"])],
    )
    .expect("captured inventory");
    assert!(chipset_inventory_matches(&captured, &captured));
    let mut changed = captured.clone();
    changed.as_mut().expect("package").records[0].driver_version = "6.10.22.028".into();
    assert!(!chipset_inventory_matches(&captured, &changed));
}

#[test]
fn p1_35_is_a_check_only_action_and_the_planner_never_claims_completion() {
    let operation = Operation {
        step: *STEPS
            .iter()
            .find(|step| step.phase as u8 == 1 && step.number == 35)
            .expect("P1:35 catalog entry"),
    };
    let action = action_for(1, 35).expect("typed action");
    assert!(matches!(action, Action::ObserveChipsetDriver));
    assert!(capture_action(&action, "P1:35".into()).is_err());
    assert!(apply_action(&action, None, None, None, None).is_err());
    assert!(verify_action(&action, None, None, None, None).is_err());

    let mut planner = PlannerBackend::new(1);
    assert_eq!(
        planner.inspect(operation).expect("inspect"),
        Inspection::NeedsApply
    );
    let plan = planner.plan(operation).expect("plan");
    assert!(plan[0].contains("P1:35"));
    assert!(planner.capture_backups(operation).is_err());
    assert!(planner.apply(operation).is_err());
    assert!(planner.persist_progress(&Progress::default()).is_err());
}

#[test]
fn p1_24_is_a_read_only_firmware_topology_observation() {
    let operation = Operation {
        step: *STEPS
            .iter()
            .find(|step| step.phase as u8 == 1 && step.number == 24)
            .expect("P1:24 catalog entry"),
    };
    let action = action_for(1, 24).expect("typed action");
    assert!(matches!(action, Action::ObserveMemoryTopology));
    assert!(capture_action(&action, "P1:24".into()).is_err());
    assert!(apply_action(&action, None, None, None, None).is_err());
    assert!(verify_action(&action, None, None, None, None).is_err());

    let mut planner = PlannerBackend::new(1);
    assert_eq!(
        planner.inspect(operation).expect("inspect"),
        Inspection::NeedsApply
    );
    let plan = planner.plan(operation).expect("plan");
    assert!(plan[0].contains("SMBIOS firmware channel associations only"));
    assert!(plan[0].contains("no active channel mode is inferred"));
    assert!(plan[0].contains("zero persistence"));
    assert!(planner.capture_backups(operation).is_err());
    assert!(planner.apply(operation).is_err());
    assert!(planner.persist_progress(&Progress::default()).is_err());
}

#[test]
fn p3_5_accepts_deferred_or_exactly_persisted_fps_cap_state() {
    let config = checked_in_config();
    assert_eq!(
        inspect_fps_cap_info(None, &State::default()).expect("deferred state"),
        Inspection::Satisfied
    );
    assert!(verify_fps_cap_info(None, &State::default()).is_ok());

    let average_fps = 300.0;
    let saved = State {
        avg_fps: average_fps,
        fps_cap: frametime_core::fps::recommended_cap(
            average_fps,
            config.fps_cap.percent,
            config.fps_cap.minimum,
        ),
        ..State::default()
    };
    assert_eq!(
        inspect_fps_cap_info(Some(&config), &saved).expect("saved state"),
        Inspection::Satisfied
    );
    assert!(verify_fps_cap_info(Some(&config), &saved).is_ok());
}

#[test]
fn p3_5_rejects_inconsistent_or_unbound_fps_cap_state() {
    let config = checked_in_config();
    let average_fps = 300.0;
    let inconsistent = State {
        avg_fps: average_fps,
        fps_cap: 1,
        ..State::default()
    };
    assert_eq!(
        inspect_fps_cap_info(Some(&config), &inconsistent).expect("inconsistent state"),
        Inspection::Unsupported
    );
    assert!(verify_fps_cap_info(Some(&config), &inconsistent).is_err());

    let saved = State {
        avg_fps: average_fps,
        fps_cap: frametime_core::fps::recommended_cap(
            average_fps,
            config.fps_cap.percent,
            config.fps_cap.minimum,
        ),
        ..State::default()
    };
    assert_eq!(
        inspect_fps_cap_info(None, &saved).expect("missing config"),
        Inspection::Unsupported
    );
    assert!(verify_fps_cap_info(None, &saved).is_err());
}

#[test]
fn p3_5_planner_is_supported_and_has_zero_persistence() {
    let operation = Operation {
        step: *STEPS
            .iter()
            .find(|step| step.phase as u8 == 3 && step.number == 5)
            .expect("P3:5 catalog entry"),
    };
    let mut planner = PlannerBackend::new(1);
    assert_eq!(
        planner.inspect(operation).expect("inspect"),
        Inspection::Satisfied
    );
    let plan = planner.plan(operation).expect("plan");
    assert!(plan[0].contains("final step will calculate the FPS cap"));
    assert!(plan[0].contains("no files or system settings will be changed"));
    assert!(planner.capture_backups(operation).is_err());
    assert!(planner.persist_backups(&[]).is_err());
    assert!(planner.apply(operation).is_err());
    assert!(planner.persist_progress(&Progress::default()).is_err());
}

#[test]
fn p3_information_guides_complete_without_backup_or_apply_paths() {
    let cases = [
        (
            6,
            Action::Cs2LaunchVideoGuide,
            "Steam launch options and video.txt will not be written.",
        ),
        (
            11,
            Action::VramUsageGuide,
            "no telemetry is collected and no files or system settings will be changed.",
        ),
        (
            12,
            Action::FinalChecklistGuide,
            "without requiring optional hardware observations; no files or system settings will be changed.",
        ),
    ];
    for (number, expected, preview) in cases {
        let operation = Operation {
            step: *STEPS
                .iter()
                .find(|step| step.phase as u8 == 3 && step.number == number)
                .expect("P3 information catalog entry"),
        };
        let action = action_for(3, number).expect("typed P3 information action");
        assert_eq!(action, expected);
        assert_eq!(
            inspect_action(&action).expect("information inspection"),
            Inspection::Satisfied
        );
        assert!(capture_action(&action, format!("P3:{number}")).is_err());
        assert!(apply_action(&action, None, None, None, None).is_err());
        assert!(verify_action(&action, None, None, None, None).is_ok());

        let mut planner = PlannerBackend::new(1);
        assert_eq!(
            planner.inspect(operation).expect("planner inspection"),
            Inspection::Satisfied
        );
        assert!(planner.plan(operation).expect("planner plan")[0].contains(preview));
        assert!(planner.capture_backups(operation).is_err());
        assert!(planner.persist_backups(&[]).is_err());
        assert!(planner.apply(operation).is_err());
        assert!(planner.persist_progress(&Progress::default()).is_err());
    }
}

#[test]
fn typed_preparation_and_amd_guides_complete_without_mutation_paths() {
    let cases = [
        (
            1,
            18,
            1,
            Action::GpuDriverCleanPreparation,
            "signed replacement driver",
        ),
        (
            1,
            20,
            2,
            Action::NvidiaProfilePreparation,
            "NVAPI, DRS profiles, registry settings",
        ),
        (
            1,
            21,
            1,
            Action::MsiPreparation,
            "registry request does not prove MSI or MSI-X is active",
        ),
        (
            1,
            22,
            1,
            Action::NicAffinityPreparation,
            "an unsuitable mask can increase latency or concentrate load",
        ),
        (3, 8, 3, Action::AmdRadeonGuide, "anti-cheat compatibility"),
    ];
    for (phase, number, branch, expected, preview) in cases {
        let operation = Operation {
            step: *STEPS
                .iter()
                .find(|step| step.phase as u8 == phase && step.number == number)
                .expect("typed guide catalog entry"),
        };
        assert!(operation.step.check_only);
        let action = action_for(phase, number).expect("typed guide action");
        assert_eq!(action, expected);
        assert_eq!(
            inspect_action(&action).expect("guide inspection"),
            Inspection::Satisfied
        );
        assert!(capture_action(&action, format!("P{phase}:{number}")).is_err());
        assert!(apply_action(&action, None, None, None, None).is_err());
        assert!(verify_action(&action, None, None, None, None).is_ok());

        let mut planner = PlannerBackend::new(branch);
        assert_eq!(
            planner.inspect(operation).expect("planner inspection"),
            Inspection::Satisfied
        );
        assert!(planner.plan(operation).expect("planner plan")[0].contains(preview));
        assert!(planner.capture_backups(operation).is_err());
        assert!(planner.persist_backups(&[]).is_err());
        assert!(planner.apply(operation).is_err());
        assert!(planner.persist_progress(&Progress::default()).is_err());
    }

    for (phase, number, branch) in [(1, 20, 3), (3, 8, 2)] {
        let operation = Operation {
            step: *STEPS
                .iter()
                .find(|step| step.phase as u8 == phase && step.number == number)
                .expect("branch-scoped guide catalog entry"),
        };
        let mut planner = PlannerBackend::new(branch);
        assert_eq!(
            planner
                .inspect(operation)
                .expect("inapplicable guide inspection"),
            Inspection::Inapplicable
        );
        assert!(
            planner.plan(operation).expect("inapplicable guide plan")[0]
                .contains("Would skip inapplicable")
        );
    }
}

#[test]
fn p3_13_is_a_check_only_final_benchmark_binding() {
    let operation = Operation {
        step: *STEPS
            .iter()
            .find(|step| step.phase as u8 == 3 && step.number == 13)
            .expect("P3:13 catalog entry"),
    };
    let action = action_for(3, 13).expect("typed P3:13 action");
    assert_eq!(action, Action::FinalBenchmark);
    assert!(inspect_action(&action).is_err());
    assert!(capture_action(&action, "P3:13".into()).is_err());
    assert!(apply_action(&action, None, None, None, None).is_err());
    assert!(verify_action(&action, None, None, None, None).is_err());

    let mut planner = PlannerBackend::new(1);
    assert_eq!(
        planner.inspect(operation).expect("planner inspection"),
        Inspection::NeedsApply
    );
    let preview = planner.plan(operation).expect("planner preview");
    assert!(preview[0].contains("complete VProf capture"));
    assert!(preview[0].contains("final-benchmark"));
    assert!(preview[0].contains("zero persistence"));
    assert!(planner.capture_backups(operation).is_err());
    assert!(planner.persist_backups(&[]).is_err());
    assert!(planner.apply(operation).is_err());
    assert!(planner.persist_progress(&Progress::default()).is_err());
}
#[test]
fn timer_resolution_never_applies_without_an_authoritative_supported_build() {
    let action = action_for(1, 28).expect("P1:28 action");
    assert_eq!(
        timer_resolution_inspection_for_build(Some(19_040), &action).expect("old build"),
        Inspection::Inapplicable
    );
    assert_eq!(
        timer_resolution_inspection_for_build(None, &action).expect("unknown build"),
        Inspection::Unsupported
    );
}
#[test]
fn p1_27_is_the_complete_mmcss_registry_batch() {
    let changes = batch_for(27);
    assert_eq!(changes.len(), 12);
    has_change(
        &changes,
        Hive::LocalMachine,
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile",
        "SystemResponsiveness",
        RegValue::Dword(10),
    );
    has_change(
        &changes,
        Hive::LocalMachine,
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile",
        "NoLazyMode",
        RegValue::Dword(1),
    );
    has_change(
        &changes,
        Hive::LocalMachine,
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile\\Tasks\\Games",
        "Priority",
        RegValue::Dword(6),
    );
    has_change(
        &changes,
        Hive::LocalMachine,
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile\\Tasks\\Games",
        "Scheduling Category",
        RegValue::String("High"),
    );
    has_change(
        &changes,
        Hive::LocalMachine,
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile\\Tasks\\Games",
        "GPU Priority",
        RegValue::Dword(8),
    );
    has_change(
        &changes,
        Hive::LocalMachine,
        "SYSTEM\\CurrentControlSet\\Control\\PriorityControl",
        "Win32PrioritySeparation",
        RegValue::Dword(0x2A),
    );
    has_change(
        &changes,
        Hive::LocalMachine,
        "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Memory Management",
        "DisablePagingExecutive",
        RegValue::Dword(1),
    );
    has_change(
        &changes,
        Hive::LocalMachine,
        "SOFTWARE\\Microsoft\\FTH",
        "Enabled",
        RegValue::Dword(0),
    );
    has_change(
        &changes,
        Hive::LocalMachine,
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Device Installer",
        "DisableCoInstallers",
        RegValue::Dword(1),
    );
    has_change(
        &changes,
        Hive::LocalMachine,
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Schedule\\Maintenance",
        "MaintenanceDisabled",
        RegValue::Dword(1),
    );
    has_change(
        &changes,
        Hive::LocalMachine,
        "SYSTEM\\CurrentControlSet\\Control\\FileSystem",
        "NtfsDisableLastAccessUpdate",
        RegValue::Dword(0x8000_0001),
    );
    has_change(
        &changes,
        Hive::LocalMachine,
        "SYSTEM\\CurrentControlSet\\Control\\FileSystem",
        "NtfsDisable8dot3NameCreation",
        RegValue::Dword(1),
    );
}
#[test]
fn p1_29_has_exact_flat_mouse_curves_and_queue_depth() {
    let changes = batch_for(29);
    assert_eq!(changes.len(), 6);
    for name in ["MouseSpeed", "MouseThreshold1", "MouseThreshold2"] {
        has_change(
            &changes,
            Hive::CurrentUser,
            "Control Panel\\Mouse",
            name,
            RegValue::String("0"),
        );
    }
    has_change(
        &changes,
        Hive::CurrentUser,
        "Control Panel\\Mouse",
        "SmoothMouseXCurve",
        RegValue::Binary(&FLAT_MOUSE_CURVE),
    );
    has_change(
        &changes,
        Hive::CurrentUser,
        "Control Panel\\Mouse",
        "SmoothMouseYCurve",
        RegValue::Binary(&FLAT_MOUSE_CURVE),
    );
    assert_eq!(FLAT_MOUSE_CURVE.len(), 40);
    has_change(
        &changes,
        Hive::LocalMachine,
        "SYSTEM\\CurrentControlSet\\Services\\mouclass\\Parameters",
        "MouseDataQueueSize",
        RegValue::Dword(50),
    );
}
#[test]
fn p1_31_32_33_and_36_are_complete_registry_batches() {
    let dvr = batch_for(31);
    assert_eq!(dvr.len(), 4);
    has_change(
        &dvr,
        Hive::CurrentUser,
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\GameDVR",
        "AppCaptureEnabled",
        RegValue::Dword(0),
    );
    has_change(
        &dvr,
        Hive::CurrentUser,
        "SOFTWARE\\Microsoft\\GameBar",
        "UseNexusForGameBarEnabled",
        RegValue::Dword(0),
    );
    has_change(
        &dvr,
        Hive::LocalMachine,
        "SOFTWARE\\Policies\\Microsoft\\Windows\\GameDVR",
        "AllowGameDVR",
        RegValue::Dword(0),
    );
    has_change(
        &dvr,
        Hive::CurrentUser,
        "System\\GameConfigStore",
        "GameDVR_Enabled",
        RegValue::Dword(0),
    );
    let overlay = batch_for(32);
    assert_eq!(overlay.len(), 1);
    has_change(
        &overlay,
        Hive::CurrentUser,
        "Software\\Valve\\Steam",
        "GameOverlayDisabled",
        RegValue::Dword(1),
    );
    let audio = batch_for(33);
    assert_eq!(audio.len(), 1);
    has_change(
        &audio,
        Hive::CurrentUser,
        "Software\\Microsoft\\Multimedia\\Audio",
        "UserDuckingPreference",
        RegValue::Dword(3),
    );
    let visual = batch_for(36);
    assert_eq!(visual.len(), 4);
    has_change(
        &visual,
        Hive::CurrentUser,
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VisualEffects",
        "VisualFXSetting",
        RegValue::Dword(2),
    );
    has_change(
        &visual,
        Hive::CurrentUser,
        "Control Panel\\Desktop",
        "UserPreferencesMask",
        RegValue::Binary(&VISUAL_EFFECTS_MASK),
    );
    has_change(
        &visual,
        Hive::CurrentUser,
        "Control Panel\\Desktop",
        "FontSmoothing",
        RegValue::String("2"),
    );
    has_change(
        &visual,
        Hive::CurrentUser,
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\VideoSettings",
        "AutoHDREnabled",
        RegValue::Dword(0),
    );
    assert_eq!(VISUAL_EFFECTS_MASK, [0x90, 0x12, 0x03, 0x80, 0x10, 0, 0, 0]);
}
#[test]
fn every_registry_batch_identity_is_restore_allowlisted() {
    for step in [27, 29, 31, 32, 33, 36] {
        for change in batch_for(step) {
            assert!(
                validate_registry_restore_binding(
                    &Progress::key(1, step),
                    change.hive,
                    change.key,
                    change.name,
                )
                .is_ok()
            );
        }
    }
}
#[cfg(not(windows))]
#[test]
fn live_operations_refuse_before_touching_files() {
    let root = Path::new(WINDOWS_WORK_DIR);
    assert!(reset_progress(root).is_err());
    assert!(restore_all(root, &checked_in_verified_config()).is_err());
    assert!(load_progress(root).is_err());
}
