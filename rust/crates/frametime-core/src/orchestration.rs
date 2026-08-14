//! Pure guards for the reboot-spanning optimization transaction.
//!
//! This module deliberately models only facts supplied by the platform layer.
//! It neither changes BCD nor creates Run/RunOnce entries.  A caller must prove
//! platform facts before attempting an effect and use the recovery plan if an
//! effect reports a partial failure.

use crate::{Phase, Progress, RebootStage, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootEnvironment {
    Normal,
    SafeMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    Verified,
    Absent,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeBinding {
    VerifiedSelectedExecutable,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandoffEvidence {
    pub phase_two_run_once: Evidence,
    pub phase_three_run: Evidence,
    pub safe_boot: Evidence,
    pub phase_three_same_user: Evidence,
}

impl Default for HandoffEvidence {
    fn default() -> Self {
        Self {
            phase_two_run_once: Evidence::Unavailable,
            phase_three_run: Evidence::Unavailable,
            safe_boot: Evidence::Unavailable,
            phase_three_same_user: Evidence::Unavailable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseFacts {
    pub boot: BootEnvironment,
    pub runtime: RuntimeBinding,
    pub handoff: HandoffEvidence,
    /// This is the persisted, read-back `phase1SafeModeReady` state flag.
    pub phase_one_safe_mode_ready: bool,
    /// A platform read-back proving that P3:13 also persisted its benchmark.
    pub final_benchmark_persisted: Evidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseRequest {
    Optimize,
    ArmSafeMode,
    PhaseTwo,
    PhaseThree,
    /// Authorize the standalone P3:13 persistence/reconciliation command.
    /// This retains the Phase 3 handoff and accepts an armed or completed
    /// transaction so exact crash-prefix retries can repair durable evidence.
    FinalBenchmark,
    ClearPhaseThreeHandoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    RunPhaseOne,
    ArmSafeMode {
        /// The adapter must atomically persist/read back readiness, create the
        /// RunOnce entry bound to the selected runtime, and set/verify safeboot.
        requires_readiness_transition: bool,
    },
    /// After P2:1 clears and verifies Safe Boot, the platform transaction must
    /// persist and read back `phase2SafeMode` before later Phase 2 work.
    RunPhaseTwoStepOne,
    RunRemainingPhaseTwo,
    RunPhaseThree,
    PersistFinalBenchmark,
    ClearPhaseThreeHandoff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardError {
    WrongBoot {
        request: PhaseRequest,
        expected: BootEnvironment,
        actual: BootEnvironment,
    },
    PhaseOneCannotResumeLaterPhase,
    MissingResolvedPhaseOneStep(u8),
    SelectedRuntimeNotVerified,
    SafeModeReadinessNotPersisted,
    SafeModeHandoffInspectionUnavailable,
    SafeModeHandoffNotArmed,
    SafeBootInspectionUnavailable,
    SafeBootNotConfigured,
    SafeBootStillConfigured,
    MissingPhaseOneHandoffCompletion,
    PhaseTwoSafeBootClearNotCompleted,
    PhaseTwoSafeBootClearWasSkipped,
    MissingCompletedPhaseTwoStep(u8),
    PhaseThreeHandoffInspectionUnavailable,
    PhaseThreeHandoffNotArmed,
    SameUserHandoffInspectionUnavailable,
    SameUserHandoffNotVerified,
    PhaseThreeHandoffMustRemain,
    LegacyRebootStateInspectionUnavailable,
    LegacyRebootTransactionArmed,
    ActiveRebootTransactionPresent,
    RebootTransactionMissing,
    RebootTransactionStageInvalid,
    FinalBenchmarkPersistenceUnavailable,
    FinalBenchmarkNotPersisted,
}

/// Computes the only transition the caller may attempt from the supplied,
/// already-observed facts.  No `Unavailable` evidence is treated as success.
pub fn authorize(
    request: PhaseRequest,
    state: &State,
    progress: &Progress,
    facts: PhaseFacts,
) -> Result<Transition, GuardError> {
    match request {
        PhaseRequest::Optimize => {
            require_boot(request, BootEnvironment::Normal, facts.boot)?;
            require_clean_reboot_state(facts.handoff)?;
            if has_phase(progress, Phase::Two) || has_phase(progress, Phase::Three) {
                return Err(GuardError::PhaseOneCannotResumeLaterPhase);
            }
            Ok(Transition::RunPhaseOne)
        }
        PhaseRequest::ArmSafeMode => {
            require_boot(request, BootEnvironment::Normal, facts.boot)?;
            require_resolved_phase_one(progress, 37)?;
            require_runtime(facts.runtime)?;
            if state.active_reboot_transaction.is_some() {
                return Err(GuardError::ActiveRebootTransactionPresent);
            }
            // A stale state flag is not sufficient: the adapter must establish
            // this transition together with the selected runtime handoff.
            Ok(Transition::ArmSafeMode {
                requires_readiness_transition: true,
            })
        }
        PhaseRequest::PhaseTwo => {
            require_boot(request, BootEnvironment::SafeMode, facts.boot)?;
            require_runtime(facts.runtime)?;
            if !state.phase1_safe_mode_ready || !facts.phase_one_safe_mode_ready {
                return Err(GuardError::SafeModeReadinessNotPersisted);
            }
            require_phase_two_handoff(facts.handoff.phase_two_run_once)?;
            require_completed(progress, Phase::One, 38)
                .map_err(|_| GuardError::MissingPhaseOneHandoffCompletion)?;
            if progress.skipped_steps.contains(&Progress::key(2, 1)) {
                return Err(GuardError::PhaseTwoSafeBootClearWasSkipped);
            }
            if progress.completed_steps.contains(&Progress::key(2, 1)) {
                require_transaction_stage(state, RebootStage::PhaseTwoSafeMode)?;
                require_safe_boot_absent(facts.handoff.safe_boot)?;
                Ok(Transition::RunRemainingPhaseTwo)
            } else if has_phase_two_later_step(progress) {
                Err(GuardError::PhaseTwoSafeBootClearNotCompleted)
            } else {
                require_transaction_stage(state, RebootStage::PhaseOneSafeModeArmed)?;
                require_safe_boot_present(facts.handoff.safe_boot)?;
                Ok(Transition::RunPhaseTwoStepOne)
            }
        }
        PhaseRequest::PhaseThree => {
            require_boot(request, BootEnvironment::Normal, facts.boot)?;
            require_runtime(facts.runtime)?;
            require_transaction_stage(state, RebootStage::PhaseThreeArmed)?;
            require_phase_three_handoff(facts.handoff.phase_three_run)?;
            require_same_user(facts.handoff.phase_three_same_user)?;
            for step in 1..=3 {
                require_completed(progress, Phase::Two, step)
                    .map_err(|_| GuardError::MissingCompletedPhaseTwoStep(step))?;
            }
            Ok(Transition::RunPhaseThree)
        }
        PhaseRequest::FinalBenchmark => {
            require_boot(request, BootEnvironment::Normal, facts.boot)?;
            require_runtime(facts.runtime)?;
            require_final_benchmark_transaction(state)?;
            require_phase_three_handoff(facts.handoff.phase_three_run)?;
            require_same_user(facts.handoff.phase_three_same_user)?;
            for step in 1..=3 {
                require_completed(progress, Phase::Two, step)
                    .map_err(|_| GuardError::MissingCompletedPhaseTwoStep(step))?;
            }
            Ok(Transition::PersistFinalBenchmark)
        }
        PhaseRequest::ClearPhaseThreeHandoff => {
            require_transaction_stage(state, RebootStage::PhaseThreeComplete)?;
            require_phase_three_handoff(facts.handoff.phase_three_run)?;
            require_same_user(facts.handoff.phase_three_same_user)?;
            require_completed(progress, Phase::Three, 1)
                .map_err(|_| GuardError::PhaseThreeHandoffMustRemain)?;
            require_completed(progress, Phase::Three, 13)
                .map_err(|_| GuardError::PhaseThreeHandoffMustRemain)?;
            match facts.final_benchmark_persisted {
                Evidence::Verified => Ok(Transition::ClearPhaseThreeHandoff),
                Evidence::Absent => Err(GuardError::FinalBenchmarkNotPersisted),
                Evidence::Unavailable => Err(GuardError::FinalBenchmarkPersistenceUnavailable),
            }
        }
    }
}

/// Prove that every Phase 1 action preceding the reboot handoff is durably
/// resolved before a platform publisher selects or launches a runtime.
pub fn require_phase_one_handoff_ready(progress: &Progress) -> Result<(), GuardError> {
    require_resolved_phase_one(progress, 37)
}

fn require_final_benchmark_transaction(state: &State) -> Result<(), GuardError> {
    let Some(transaction) = &state.active_reboot_transaction else {
        return Err(GuardError::RebootTransactionMissing);
    };
    if transaction.is_authorized_at(&RebootStage::PhaseThreeArmed)
        || transaction.is_authorized_at(&RebootStage::PhaseThreeComplete)
    {
        Ok(())
    } else {
        Err(GuardError::RebootTransactionStageInvalid)
    }
}

fn require_transaction_stage(state: &State, expected: RebootStage) -> Result<(), GuardError> {
    let Some(transaction) = &state.active_reboot_transaction else {
        return Err(GuardError::RebootTransactionMissing);
    };
    if transaction.is_authorized_at(&expected) {
        Ok(())
    } else {
        Err(GuardError::RebootTransactionStageInvalid)
    }
}

fn require_clean_reboot_state(handoff: HandoffEvidence) -> Result<(), GuardError> {
    for evidence in [
        handoff.phase_two_run_once,
        handoff.phase_three_run,
        handoff.safe_boot,
    ] {
        match evidence {
            Evidence::Absent => {}
            Evidence::Verified => return Err(GuardError::LegacyRebootTransactionArmed),
            Evidence::Unavailable => {
                return Err(GuardError::LegacyRebootStateInspectionUnavailable);
            }
        }
    }
    Ok(())
}

fn require_boot(
    request: PhaseRequest,
    expected: BootEnvironment,
    actual: BootEnvironment,
) -> Result<(), GuardError> {
    if actual == expected {
        Ok(())
    } else {
        Err(GuardError::WrongBoot {
            request,
            expected,
            actual,
        })
    }
}

fn require_runtime(runtime: RuntimeBinding) -> Result<(), GuardError> {
    if runtime == RuntimeBinding::VerifiedSelectedExecutable {
        Ok(())
    } else {
        Err(GuardError::SelectedRuntimeNotVerified)
    }
}

fn require_resolved_phase_one(progress: &Progress, final_step: u8) -> Result<(), GuardError> {
    for step in 1..=final_step {
        let key = Progress::key(1, step);
        let resolved = progress.completed_steps.contains(&key)
            || progress.skipped_steps.contains(&key)
            || (phase_one_advisory_is_authorized(step) && progress.advisories.contains_key(&key));
        if !resolved {
            return Err(GuardError::MissingResolvedPhaseOneStep(step));
        }
    }
    Ok(())
}

const fn phase_one_advisory_is_authorized(step: u8) -> bool {
    matches!(step, 2 | 9)
}

fn require_completed(progress: &Progress, phase: Phase, step: u8) -> Result<(), ()> {
    progress
        .completed_steps
        .contains(&Progress::key(phase as u8, step))
        .then_some(())
        .ok_or(())
}

fn require_phase_two_handoff(evidence: Evidence) -> Result<(), GuardError> {
    match evidence {
        Evidence::Verified => Ok(()),
        Evidence::Absent => Err(GuardError::SafeModeHandoffNotArmed),
        Evidence::Unavailable => Err(GuardError::SafeModeHandoffInspectionUnavailable),
    }
}

fn require_safe_boot_present(evidence: Evidence) -> Result<(), GuardError> {
    match evidence {
        Evidence::Verified => Ok(()),
        Evidence::Absent => Err(GuardError::SafeBootNotConfigured),
        Evidence::Unavailable => Err(GuardError::SafeBootInspectionUnavailable),
    }
}

fn require_safe_boot_absent(evidence: Evidence) -> Result<(), GuardError> {
    match evidence {
        Evidence::Absent => Ok(()),
        Evidence::Verified => Err(GuardError::SafeBootStillConfigured),
        Evidence::Unavailable => Err(GuardError::SafeBootInspectionUnavailable),
    }
}

fn require_phase_three_handoff(evidence: Evidence) -> Result<(), GuardError> {
    match evidence {
        Evidence::Verified => Ok(()),
        Evidence::Absent => Err(GuardError::PhaseThreeHandoffNotArmed),
        Evidence::Unavailable => Err(GuardError::PhaseThreeHandoffInspectionUnavailable),
    }
}

fn require_same_user(evidence: Evidence) -> Result<(), GuardError> {
    match evidence {
        Evidence::Verified => Ok(()),
        Evidence::Absent => Err(GuardError::SameUserHandoffNotVerified),
        Evidence::Unavailable => Err(GuardError::SameUserHandoffInspectionUnavailable),
    }
}

fn has_phase(progress: &Progress, phase: Phase) -> bool {
    let prefix = format!("P{}:", phase as u8);
    progress
        .completed_steps
        .iter()
        .chain(&progress.skipped_steps)
        .any(|key| key.starts_with(&prefix))
}

fn has_phase_two_later_step(progress: &Progress) -> bool {
    (2..=3).any(|step| {
        let key = Progress::key(2, step);
        progress.completed_steps.contains(&key) || progress.skipped_steps.contains(&key)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailurePoint {
    BeforeMutation,
    SafeModeHandoffArmed,
    PhaseTwoSafeBootClear,
    PhaseTwoDriverWork,
    PhaseThreeDriverWork,
    FinalBenchmarkPersistence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryPlan {
    pub retain_handoff: bool,
    pub compensations: Vec<Compensation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compensation {
    NoneClaimed,
    RestoreSafeBootFromCapturedBackup,
    RestoreCapturedPhaseBackups,
    RetainPhaseTwoHandoffForRecovery,
    RetainPhaseThreeHandoffUntilCompletion,
}

/// Describes requested compensating work only. The caller must execute and
/// verify these actions through a platform adapter before recording success.
#[must_use]
pub fn recovery_for(request: PhaseRequest, failure: FailurePoint) -> RecoveryPlan {
    let compensations = match (request, failure) {
        (_, FailurePoint::BeforeMutation) => vec![Compensation::NoneClaimed],
        (PhaseRequest::ArmSafeMode, FailurePoint::SafeModeHandoffArmed) => vec![
            Compensation::RestoreSafeBootFromCapturedBackup,
            Compensation::RetainPhaseTwoHandoffForRecovery,
        ],
        (PhaseRequest::PhaseTwo, FailurePoint::PhaseTwoSafeBootClear) => vec![
            Compensation::RestoreSafeBootFromCapturedBackup,
            Compensation::RetainPhaseTwoHandoffForRecovery,
        ],
        (PhaseRequest::PhaseTwo, FailurePoint::PhaseTwoDriverWork) => vec![
            Compensation::RestoreCapturedPhaseBackups,
            Compensation::RetainPhaseTwoHandoffForRecovery,
        ],
        (PhaseRequest::PhaseThree, FailurePoint::PhaseThreeDriverWork)
        | (PhaseRequest::PhaseThree, FailurePoint::FinalBenchmarkPersistence)
        | (PhaseRequest::FinalBenchmark, FailurePoint::FinalBenchmarkPersistence) => vec![
            Compensation::RestoreCapturedPhaseBackups,
            Compensation::RetainPhaseThreeHandoffUntilCompletion,
        ],
        _ => vec![Compensation::NoneClaimed],
    };
    let retain_handoff = compensations.iter().any(|compensation| {
        matches!(
            compensation,
            Compensation::RetainPhaseTwoHandoffForRecovery
                | Compensation::RetainPhaseThreeHandoffUntilCompletion
        )
    });
    RecoveryPlan {
        retain_handoff,
        compensations,
    }
}

#[cfg(test)]
mod tests;
