use super::*;

#[test]
fn catalog_has_exact_phase_counts_and_unique_keys() {
    assert_eq!(
        STEPS.iter().filter(|s| s.id.phase == Phase::One).count(),
        38
    );
    assert_eq!(STEPS.iter().filter(|s| s.id.phase == Phase::Two).count(), 3);
    assert_eq!(
        STEPS.iter().filter(|s| s.id.phase == Phase::Three).count(),
        13
    );
    let mut keys = STEPS.iter().map(|step| step.id).collect::<Vec<_>>();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 54);
    for step in STEPS {
        assert_eq!(step.id, StepId::new(step.id.phase, step.id.number));
        assert_eq!(step_by_id(step.id), Some(&step));
        assert_eq!(
            StepId::from_progress_key(&step.id.progress_key()),
            Some(step.id)
        );
    }
}

#[test]
fn catalog_assigns_each_intent_once() {
    let intents = STEPS
        .iter()
        .map(|step| step.intent)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(intents.len(), STEPS.len());
}

#[test]
fn catalog_rows_carry_complete_planning_and_orchestration_contracts() {
    for step in STEPS {
        assert_eq!(
            !step.intent.is_mutating(),
            step.check_only,
            "{} intent and check-only state diverged",
            step.id.progress_key()
        );
        assert!(matches!(
            step.operation,
            OperationKind::Setup
                | OperationKind::Inspect
                | OperationKind::Registry
                | OperationKind::Service
                | OperationKind::BootConfiguration
                | OperationKind::Driver
                | OperationKind::Network
                | OperationKind::Filesystem
                | OperationKind::ApplicationConfiguration
        ));
    }
    assert_eq!(
        step_by_id(PHASE_ONE_SAFE_MODE_HANDOFF)
            .expect("P1 handoff")
            .orchestration_role,
        OrchestrationRole::ArmSafeModeHandoff
    );
    assert_eq!(
        step_by_id(PHASE_TWO_SAFE_BOOT_CLEAR)
            .expect("P2 safe-boot clear")
            .orchestration_role,
        OrchestrationRole::ClearSafeBoot
    );
    assert_eq!(
        step_by_id(PHASE_TWO_PHASE_THREE_HANDOFF)
            .expect("P2 handoff")
            .orchestration_role,
        OrchestrationRole::ArmPhaseThreeHandoff
    );
    assert_eq!(
        step_by_id(PHASE_THREE_FINAL_BENCHMARK)
            .expect("P3 final benchmark")
            .orchestration_role,
        OrchestrationRole::PersistFinalBenchmark
    );
}

#[test]
fn august_meta_defaults_do_not_expose_rejected_tweaks_as_mutations() {
    let check_only = [
        3_u8, 4, 6, 7, 10, 11, 13, 14, 15, 23, 25, 26, 27, 28, 29, 31, 32, 33, 36, 37,
    ];
    for number in check_only {
        let step = STEPS
            .iter()
            .find(|step| step.id.phase == Phase::One && step.id.number == number)
            .expect("phase-one meta row");
        assert!(step.check_only, "P1:{number} must remain advisory");
        assert_eq!(
            step.depth,
            Depth::Check,
            "P1:{number} must not advertise a mutation"
        );
    }
    let game_mode = STEPS
        .iter()
        .find(|step| step.id.phase == Phase::One && step.id.number == 12)
        .expect("Game Mode row");
    assert_eq!(game_mode.tier, 1);
    assert!(!game_mode.check_only);

    let rss = STEPS
        .iter()
        .find(|step| step.id.phase == Phase::One && step.id.number == 16)
        .expect("RSS row");
    assert_eq!(rss.tier, 1);
    assert_eq!(rss.risk, Risk::Safe);
    assert!(!rss.check_only);

    let automatic_mutations = STEPS
        .iter()
        .filter(|step| step.id.phase == Phase::One && step.tier == 1 && !step.check_only)
        .map(|step| step.id.number)
        .collect::<Vec<_>>();
    assert_eq!(automatic_mutations, [1, 12, 16]);

    for number in [2_u8, 3] {
        let step = STEPS
            .iter()
            .find(|step| step.id.phase == Phase::Three && step.id.number == number)
            .expect("phase-three interrupt-policy row");
        assert!(step.check_only, "P3:{number} must remain advisory");
        assert_eq!(step.depth, Depth::Check);
    }
}
