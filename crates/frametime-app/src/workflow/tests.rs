use super::*;

fn final_receipt() -> FinalBenchmarkReceipt {
    crate::test_support::final_benchmark_receipt()
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
fn migration_confirmation_requires_yes_or_interactive_confirmation() {
    let partial = || MigrationDecision::ConfirmPartialPhaseOne {
        completed: 1,
        skipped: 2,
    };
    assert!(require_migration_decision(MigrationDecision::ConfirmIdle, false, |_| true).is_ok());
    assert!(require_migration_decision(MigrationDecision::ConfirmIdle, false, |_| false).is_err());
    assert!(require_migration_decision(partial(), false, |_| true).is_ok());
    let error = require_migration_decision(partial(), false, |_| false).expect_err("declined");
    assert!(error.to_string().contains("frametime optimize --yes"));
    let mut question = String::new();
    assert!(
        require_migration_decision(partial(), false, |message| {
            question = message.to_string();
            true
        })
        .is_ok()
    );
    assert!(question.contains("1 completed, 2 skipped"));
}

#[test]
fn migration_yes_and_not_needed_never_ask_and_refusals_cannot_be_overridden() {
    let never = |_: &str| -> bool { panic!("confirm must not be called") };
    assert!(require_migration_decision(MigrationDecision::ConfirmIdle, true, never).is_ok());
    assert!(require_migration_decision(MigrationDecision::NotNeeded, false, never).is_ok());
    for yes in [false, true] {
        assert!(
            require_migration_decision(
                MigrationDecision::Refuse(
                    frametime_domain::migration::LegacyHandoff::IncompleteRuntime
                ),
                yes,
                never,
            )
            .is_err()
        );
    }
}

// Only non-Windows hosts lack the trusted Windows work directory; on Windows the inventory
// legitimately reports complete when no runtime is selected.
#[cfg(not(windows))]
#[test]
fn unavailable_runtime_boundary_is_incomplete() {
    assert!(runtime_inventory_incomplete());
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
    assert_eq!(
        steps.first().map(|step| step.id),
        Some(frametime_domain::catalog::PHASE_THREE_DRIVER_INSTALL)
    );
    assert!(steps.iter().all(|step| {
        step.id.phase == Phase::Three
            && step.orchestration_role == frametime_domain::catalog::OrchestrationRole::Engine
    }));
    assert!(
        !steps
            .iter()
            .any(|step| { step.id == frametime_domain::catalog::PHASE_THREE_FINAL_BENCHMARK })
    );
}

#[test]
fn phase_one_engine_stops_before_the_protected_runtime_handoff() {
    let steps = step_catalog()
        .iter()
        .filter(|step| is_phase_one_engine_step(step))
        .collect::<Vec<_>>();
    assert!(steps.iter().all(|step| {
        step.id.phase == Phase::One
            && step.orchestration_role == frametime_domain::catalog::OrchestrationRole::Engine
    }));
    assert!(
        !steps
            .iter()
            .any(|step| { step.id == frametime_domain::catalog::PHASE_ONE_SAFE_MODE_HANDOFF })
    );
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

#[test]
fn fresh_machine_needs_no_migration_but_present_default_files_still_confirm() {
    assert_eq!(
        assess_migration(None, None, MigrationInventory::default()),
        MigrationDecision::NotNeeded
    );
    assert_eq!(
        assess_migration(
            Some(&State::default()),
            Some(&Progress::default()),
            MigrationInventory::default(),
        ),
        MigrationDecision::ConfirmIdle
    );
    assert_eq!(
        assess_migration(Some(&State::default()), None, MigrationInventory::default()),
        MigrationDecision::ConfirmIdle
    );
}
