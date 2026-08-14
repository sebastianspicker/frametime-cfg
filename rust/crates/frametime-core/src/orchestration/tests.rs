use super::*;
use std::collections::BTreeMap;

use crate::{RebootTransaction, RuntimeRecord, TransactionId};
use serde_json::Value;

const ID: &str = "0123456789abcdef0123456789abcdef";
const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn state_for(stage: RebootStage) -> State {
    State {
        phase1_safe_mode_ready: true,
        active_reboot_transaction: Some(RebootTransaction {
            schema_version: 1,
            transaction_id: Some(TransactionId::parse(ID).expect("id")),
            initiator_user_sid: Some("S-1-5-21-1".into()),
            stage,
            runtime: Some(RuntimeRecord {
                generation: ID.into(),
                manifest_sha256: HASH.into(),
                payload_contract_hash: HASH.into(),
                executable_path: "frametime.exe".into(),
                executable_sha256: HASH.into(),
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

fn facts(boot: BootEnvironment) -> PhaseFacts {
    PhaseFacts {
        boot,
        runtime: RuntimeBinding::VerifiedSelectedExecutable,
        handoff: HandoffEvidence {
            phase_two_run_once: Evidence::Verified,
            phase_three_run: Evidence::Verified,
            safe_boot: Evidence::Verified,
            phase_three_same_user: Evidence::Verified,
        },
        phase_one_safe_mode_ready: true,
        final_benchmark_persisted: Evidence::Verified,
    }
}

fn complete(progress: &mut Progress, phase: u8, step: u8) {
    progress.complete(phase, step, "now".into());
}

fn clean_normal_facts() -> PhaseFacts {
    let mut value = facts(BootEnvironment::Normal);
    value.handoff.phase_two_run_once = Evidence::Absent;
    value.handoff.phase_three_run = Evidence::Absent;
    value.handoff.safe_boot = Evidence::Absent;
    value
}

#[test]
fn optimize_is_normal_boot_only_and_never_resumes_later_phase() {
    let state = State::default();
    assert_eq!(
        authorize(
            PhaseRequest::Optimize,
            &state,
            &Progress::default(),
            clean_normal_facts()
        ),
        Ok(Transition::RunPhaseOne)
    );
    let mut progress = Progress::default();
    complete(&mut progress, 2, 1);
    assert_eq!(
        authorize(
            PhaseRequest::Optimize,
            &state,
            &progress,
            clean_normal_facts()
        ),
        Err(GuardError::PhaseOneCannotResumeLaterPhase)
    );
}

#[test]
fn optimize_refuses_unavailable_or_armed_reboot_state() {
    let state = State::default();
    assert_eq!(
        authorize(
            PhaseRequest::Optimize,
            &state,
            &Progress::default(),
            PhaseFacts {
                handoff: HandoffEvidence::default(),
                ..clean_normal_facts()
            }
        ),
        Err(GuardError::LegacyRebootStateInspectionUnavailable)
    );
    let mut armed = clean_normal_facts();
    armed.handoff.phase_two_run_once = Evidence::Verified;
    assert_eq!(
        authorize(PhaseRequest::Optimize, &state, &Progress::default(), armed),
        Err(GuardError::LegacyRebootTransactionArmed)
    );
}

#[test]
fn safe_mode_arm_requires_all_prior_steps_and_selected_runtime() {
    let state = State::default();
    let mut progress = Progress::default();
    for step in 1..=37 {
        progress.skip(1, step);
    }
    assert_eq!(
        authorize(
            PhaseRequest::ArmSafeMode,
            &state,
            &progress,
            facts(BootEnvironment::Normal)
        ),
        Ok(Transition::ArmSafeMode {
            requires_readiness_transition: true
        })
    );
    let mut missing = Progress::default();
    for step in 1..37 {
        complete(&mut missing, 1, step);
    }
    assert_eq!(
        authorize(
            PhaseRequest::ArmSafeMode,
            &state,
            &missing,
            facts(BootEnvironment::Normal)
        ),
        Err(GuardError::MissingResolvedPhaseOneStep(37))
    );
}

#[test]
fn phase_one_resolution_accepts_acknowledged_advisories_but_not_completion_guards() {
    let state = State::default();
    let mut progress = Progress::default();
    for step in 1..=37 {
        progress.skip(1, step);
    }
    progress.acknowledge_advisory(
        1,
        2,
        "XMP/EXPO observation requires authoritative SMBIOS memory-profile data".into(),
    );
    progress.acknowledge_advisory(
        1,
        9,
        "Resizable BAR observation requires PCIe capability inspection".into(),
    );
    assert_eq!(
        authorize(
            PhaseRequest::ArmSafeMode,
            &state,
            &progress,
            facts(BootEnvironment::Normal)
        ),
        Ok(Transition::ArmSafeMode {
            requires_readiness_transition: true
        })
    );
    assert_eq!(
        authorize(
            PhaseRequest::PhaseTwo,
            &state_for(RebootStage::PhaseOneSafeModeArmed),
            &progress,
            facts(BootEnvironment::SafeMode)
        ),
        Err(GuardError::MissingPhaseOneHandoffCompletion)
    );
}

#[test]
fn phase_one_resolution_rejects_advisory_receipts_for_other_steps() {
    let state = State::default();
    let mut progress = Progress::default();
    for step in 1..=37 {
        progress.skip(1, step);
    }
    progress.acknowledge_advisory(1, 10, "untrusted advisory receipt".into());

    assert_eq!(
        authorize(
            PhaseRequest::ArmSafeMode,
            &state,
            &progress,
            facts(BootEnvironment::Normal)
        ),
        Err(GuardError::MissingResolvedPhaseOneStep(10))
    );
}

#[test]
fn runtime_publication_guard_uses_the_same_phase_one_resolution_contract() {
    let mut progress = Progress::default();
    for step in 1..=37 {
        complete(&mut progress, 1, step);
    }
    assert_eq!(require_phase_one_handoff_ready(&progress), Ok(()));

    progress.completed_steps.remove(&Progress::key(1, 2));
    progress.acknowledge_advisory(
        1,
        2,
        "authoritative memory-profile evidence unavailable".into(),
    );
    assert_eq!(require_phase_one_handoff_ready(&progress), Ok(()));

    progress.completed_steps.remove(&Progress::key(1, 19));
    assert_eq!(
        require_phase_one_handoff_ready(&progress),
        Err(GuardError::MissingResolvedPhaseOneStep(19))
    );
}

#[test]
fn phase_two_never_moves_past_safe_boot_clear() {
    let state = state_for(RebootStage::PhaseOneSafeModeArmed);
    let mut progress = Progress::default();
    complete(&mut progress, 1, 38);
    assert_eq!(
        authorize(
            PhaseRequest::PhaseTwo,
            &state,
            &progress,
            facts(BootEnvironment::SafeMode)
        ),
        Ok(Transition::RunPhaseTwoStepOne)
    );
    progress.skip(2, 1);
    assert_eq!(
        authorize(
            PhaseRequest::PhaseTwo,
            &state,
            &progress,
            facts(BootEnvironment::SafeMode)
        ),
        Err(GuardError::PhaseTwoSafeBootClearWasSkipped)
    );
    let mut cleared = Progress::default();
    complete(&mut cleared, 1, 38);
    complete(&mut cleared, 2, 1);
    let mut after_clear = facts(BootEnvironment::SafeMode);
    after_clear.handoff.safe_boot = Evidence::Absent;
    assert_eq!(
        authorize(
            PhaseRequest::PhaseTwo,
            &state_for(RebootStage::PhaseTwoSafeMode),
            &cleared,
            after_clear
        ),
        Ok(Transition::RunRemainingPhaseTwo)
    );
    assert_eq!(
        authorize(PhaseRequest::PhaseTwo, &state, &cleared, after_clear),
        Err(GuardError::RebootTransactionStageInvalid)
    );
    let mut invalid_history = Progress::default();
    complete(&mut invalid_history, 1, 38);
    complete(&mut invalid_history, 2, 2);
    assert_eq!(
        authorize(
            PhaseRequest::PhaseTwo,
            &state,
            &invalid_history,
            facts(BootEnvironment::SafeMode)
        ),
        Err(GuardError::PhaseTwoSafeBootClearNotCompleted)
    );
}

#[test]
fn phase_two_fails_closed_without_readiness_or_handoff_evidence() {
    let mut state = state_for(RebootStage::PhaseOneSafeModeArmed);
    state.phase1_safe_mode_ready = false;
    let mut progress = Progress::default();
    complete(&mut progress, 1, 38);
    let mut no_readiness = facts(BootEnvironment::SafeMode);
    no_readiness.phase_one_safe_mode_ready = false;
    assert_eq!(
        authorize(PhaseRequest::PhaseTwo, &state, &progress, no_readiness),
        Err(GuardError::SafeModeReadinessNotPersisted)
    );
    state.phase1_safe_mode_ready = true;
    let mut unavailable = facts(BootEnvironment::SafeMode);
    unavailable.handoff.phase_two_run_once = Evidence::Unavailable;
    assert_eq!(
        authorize(PhaseRequest::PhaseTwo, &state, &progress, unavailable),
        Err(GuardError::SafeModeHandoffInspectionUnavailable)
    );
}

#[test]
fn phase_three_requires_completed_not_skipped_phase_two_and_same_user_handoff() {
    let state = state_for(RebootStage::PhaseThreeArmed);
    let mut progress = Progress::default();
    for step in 1..=3 {
        complete(&mut progress, 2, step);
    }
    assert_eq!(
        authorize(
            PhaseRequest::PhaseThree,
            &state,
            &progress,
            facts(BootEnvironment::Normal)
        ),
        Ok(Transition::RunPhaseThree)
    );
    progress.skip(2, 2);
    assert_eq!(
        authorize(
            PhaseRequest::PhaseThree,
            &state,
            &progress,
            facts(BootEnvironment::Normal)
        ),
        Err(GuardError::MissingCompletedPhaseTwoStep(2))
    );
    let mut foreign = facts(BootEnvironment::Normal);
    foreign.handoff.phase_three_same_user = Evidence::Absent;
    assert_eq!(
        authorize(
            PhaseRequest::PhaseThree,
            &state,
            &Progress::default(),
            foreign
        ),
        Err(GuardError::SameUserHandoffNotVerified)
    );
}

#[test]
fn final_benchmark_authorizes_armed_or_complete_retries_but_requires_phase_three_identity() {
    let mut progress = Progress::default();
    for step in 1..=3 {
        complete(&mut progress, 2, step);
    }
    let normal = facts(BootEnvironment::Normal);
    assert_eq!(
        authorize(
            PhaseRequest::FinalBenchmark,
            &state_for(RebootStage::PhaseThreeArmed),
            &progress,
            normal
        ),
        Ok(Transition::PersistFinalBenchmark)
    );
    assert_eq!(
        authorize(
            PhaseRequest::FinalBenchmark,
            &state_for(RebootStage::PhaseThreeComplete),
            &progress,
            normal
        ),
        Ok(Transition::PersistFinalBenchmark)
    );

    let mut wrong_identity = normal;
    wrong_identity.handoff.phase_three_same_user = Evidence::Absent;
    assert_eq!(
        authorize(
            PhaseRequest::FinalBenchmark,
            &state_for(RebootStage::PhaseThreeArmed),
            &progress,
            wrong_identity
        ),
        Err(GuardError::SameUserHandoffNotVerified)
    );
    let mut unavailable_runtime = normal;
    unavailable_runtime.runtime = RuntimeBinding::Unavailable;
    assert_eq!(
        authorize(
            PhaseRequest::FinalBenchmark,
            &state_for(RebootStage::PhaseThreeArmed),
            &progress,
            unavailable_runtime
        ),
        Err(GuardError::SelectedRuntimeNotVerified)
    );
    assert!(matches!(
        authorize(
            PhaseRequest::FinalBenchmark,
            &state_for(RebootStage::PhaseThreeArmed),
            &progress,
            facts(BootEnvironment::SafeMode)
        ),
        Err(GuardError::WrongBoot { .. })
    ));
}

#[test]
fn phase_three_handoff_cannot_clear_before_driver_and_persisted_benchmark() {
    let state = state_for(RebootStage::PhaseThreeComplete);
    let mut progress = Progress::default();
    complete(&mut progress, 3, 1);
    complete(&mut progress, 3, 13);
    assert_eq!(
        authorize(
            PhaseRequest::ClearPhaseThreeHandoff,
            &state,
            &progress,
            facts(BootEnvironment::Normal)
        ),
        Ok(Transition::ClearPhaseThreeHandoff)
    );
    let mut not_persisted = facts(BootEnvironment::Normal);
    not_persisted.final_benchmark_persisted = Evidence::Absent;
    assert_eq!(
        authorize(
            PhaseRequest::ClearPhaseThreeHandoff,
            &state,
            &progress,
            not_persisted
        ),
        Err(GuardError::FinalBenchmarkNotPersisted)
    );
}

#[test]
fn state_ready_flag_is_explicitly_read_from_the_persisted_field() {
    let state = State {
        phase1_safe_mode_ready: true,
        ..State::default()
    };
    assert!(serde_json::to_value(state).unwrap()["phase1SafeModeReady"] == Value::Bool(true));
}

#[test]
fn absent_invalid_or_unknown_reboot_stage_never_authorizes_a_reboot_phase() {
    let mut progress = Progress::default();
    complete(&mut progress, 1, 38);
    let facts = facts(BootEnvironment::SafeMode);
    let missing = State {
        phase1_safe_mode_ready: true,
        ..State::default()
    };
    assert_eq!(
        authorize(PhaseRequest::PhaseTwo, &missing, &progress, facts),
        Err(GuardError::RebootTransactionMissing)
    );
    let mut invalid = state_for(RebootStage::Unknown("future".into()));
    invalid.phase1_safe_mode_ready = true;
    assert_eq!(
        authorize(PhaseRequest::PhaseTwo, &invalid, &progress, facts),
        Err(GuardError::RebootTransactionStageInvalid)
    );
}

#[test]
fn recovery_is_a_plan_not_a_claim_of_windows_side_effects() {
    assert_eq!(
        recovery_for(
            PhaseRequest::PhaseThree,
            FailurePoint::FinalBenchmarkPersistence
        ),
        RecoveryPlan {
            retain_handoff: true,
            compensations: vec![
                Compensation::RestoreCapturedPhaseBackups,
                Compensation::RetainPhaseThreeHandoffUntilCompletion,
            ]
        }
    );
}
