use super::*;

fn final_receipt() -> FinalBenchmarkReceipt {
    FinalBenchmarkReceipt {
        schema_version: 1,
        receipt_id: frametime_core::TransactionId::parse("fedcba9876543210fedcba9876543210")
            .expect("receipt id"),
        transaction_id: frametime_core::TransactionId::parse("0123456789abcdef0123456789abcdef")
            .expect("transaction id"),
        captured_utc: "2026-08-10 12:34:56".into(),
        avg_fps: 300.0,
        p1_fps: 180.0,
        runs: 3,
        fps_cap: 270,
        label: "After all optimizations".into(),
        unknown: std::collections::BTreeMap::new(),
    }
}

fn native() -> RebootHandoffState {
    RebootHandoffState {
        boot_mode: BootModeEvidence::Normal,
        safeboot: SafebootEvidence::Absent,
        phase2_runonce_armed: NativeHandoffEvidence::Absent,
        phase3_run_armed: NativeHandoffEvidence::Absent,
        phase3_handoff_same_user: NativeHandoffEvidence::Absent,
        selected_runtime_binding: NativeHandoffEvidence::Verified,
        token_user_sid: None,
    }
}

#[test]
fn migration_inventory_maps_armed_and_unavailable_native_evidence_fail_closed() {
    assert_eq!(
        migration_inventory_from_native(&native(), false),
        MigrationInventory::default()
    );

    let mut phase_two = native();
    phase_two.phase2_runonce_armed = NativeHandoffEvidence::Verified;
    assert!(migration_inventory_from_native(&phase_two, false).phase_two_run_once_armed);

    let mut phase_three = native();
    phase_three.phase3_run_armed = NativeHandoffEvidence::Verified;
    assert!(migration_inventory_from_native(&phase_three, false).phase_three_run_armed);

    let mut safeboot = native();
    safeboot.safeboot = SafebootEvidence::Configured("minimal".into());
    assert!(migration_inventory_from_native(&safeboot, false).safe_boot_armed);

    let mut unavailable = native();
    unavailable.phase2_runonce_armed = NativeHandoffEvidence::Unavailable;
    assert!(migration_inventory_from_native(&unavailable, false).incomplete_runtime);
    assert!(migration_inventory_from_native(&native(), true).incomplete_runtime);
}

#[test]
fn migration_confirmation_requires_yes_but_refusals_cannot_be_overridden() {
    assert!(require_migration_decision(MigrationDecision::ConfirmIdle, false).is_err());
    assert!(require_migration_decision(MigrationDecision::ConfirmIdle, true).is_ok());
    assert!(
        require_migration_decision(
            MigrationDecision::ConfirmPartialPhaseOne {
                completed: 1,
                skipped: 2,
            },
            false,
        )
        .is_err()
    );
    assert!(
        require_migration_decision(
            MigrationDecision::Refuse(frametime_core::LegacyHandoff::IncompleteRuntime),
            true,
        )
        .is_err()
    );
}

#[test]
fn missing_runtime_selector_is_not_an_incomplete_runtime_transaction() {
    let missing =
        std::env::temp_dir().join(format!("frametime-missing-runtime-{}", std::process::id()));
    assert!(!runtime_inventory_incomplete(&missing));
}

#[test]
fn typed_or_malformed_reboot_state_is_never_clean() {
    assert!(!state_has_reboot_transaction(&State::default()));
    assert!(state_has_reboot_transaction(&State {
        phase1_safe_mode_ready: true,
        ..State::default()
    }));
    let malformed: State = serde_json::from_str(r#"{"activeRebootTransaction":false}"#).unwrap();
    assert!(state_has_reboot_transaction(&malformed));
}

#[test]
fn phase_three_engine_stops_before_the_standalone_final_benchmark_receipt() {
    let steps = phase_three_engine_steps();
    assert_eq!(steps.first().map(|step| step.number), Some(1));
    assert_eq!(steps.last().map(|step| step.number), Some(12));
    assert!(steps.iter().all(|step| step.phase == Phase::Three));
    assert!(!steps.iter().any(|step| step.number == 13));
}

#[test]
fn phase_one_engine_stops_before_the_protected_runtime_handoff() {
    let steps = step_catalog()
        .iter()
        .filter(|step| is_phase_one_engine_step(step))
        .collect::<Vec<_>>();
    assert_eq!(steps.first().map(|step| step.number), Some(1));
    assert_eq!(steps.last().map(|step| step.number), Some(37));
    assert!(steps.iter().all(|step| step.phase == Phase::One));
    assert!(!steps.iter().any(|step| step.number == 38));
}

#[test]
fn final_benchmark_status_maps_to_fail_closed_preflight_evidence() {
    assert_eq!(
        final_benchmark_evidence(FinalBenchmarkStatus::Absent),
        Evidence::Absent
    );
    assert_eq!(
        final_benchmark_evidence(FinalBenchmarkStatus::Incoherent("prefix".into())),
        Evidence::Unavailable
    );
}

#[test]
fn phase_three_receipt_routing_never_runs_the_engine_for_coherent_or_incoherent_evidence() {
    assert!(matches!(
        phase_three_receipt_route(FinalBenchmarkStatus::Absent),
        Ok(PhaseThreeReceiptRoute::RunEngine)
    ));
    assert!(matches!(
        phase_three_receipt_route(FinalBenchmarkStatus::Coherent(final_receipt())),
        Ok(PhaseThreeReceiptRoute::Complete(_))
    ));
    assert!(phase_three_receipt_route(FinalBenchmarkStatus::Incoherent("prefix".into())).is_err());
}
