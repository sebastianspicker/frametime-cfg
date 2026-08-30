use frametime_domain::{
    BootEnvironment, Engine, Evidence, FinalBenchmarkReceipt, HandoffEvidence, MigrationDecision,
    MigrationInventory, OrchestrationRole, PHASE_ONE_SAFE_MODE_HANDOFF, PHASE_TWO_DRIVER_CLEANUP,
    Phase, PhaseFacts, PhaseRequest, Profile, Progress, RuntimeBinding, State, Transition,
    assess_inventory, authorize, require_phase_one_handoff_ready, step_by_id, step_catalog,
};
use frametime_windows::{
    AuthenticatedPackage, BootMode, BootModeEvidence, FinalBenchmarkStatus,
    HandoffEvidence as NativeHandoffEvidence, LiveBackend, RebootHandoffState, SafebootEvidence,
    arm_phase_three_handoff, arm_safe_mode_handoff, clear_phase_three_handoff,
    complete_phase_two_safe_boot_clear, current_boot_mode, final_benchmark_status,
    inspect_reboot_handoff_state, launch_published_safe_mode_handoff, load_progress, load_state,
    publish_current_packaged_runtime, relaunch_phase_three_handoff, retain_selected_runtime,
    runtime_inventory_incomplete as native_runtime_inventory_incomplete,
};

use crate::{
    ApplicationError, BenchmarkOutcome, RunSummary,
    actions::{persist_final_benchmark_capture, read_final_benchmark_capture, require_yes},
    cancellation_requested,
    commands::VprofBenchmarkRequest,
    prompt_for_step,
};

mod live_commands;
pub use live_commands::run_live;

fn run_phase_one(
    yes: bool,
    package: &AuthenticatedPackage,
) -> Result<RunSummary, ApplicationError> {
    require_boot_mode(BootMode::Normal, "optimize")?;
    let (state, progress) = load_session()?;
    require_legacy_phase_one_migration(&state, &progress, yes)?;
    authorize_live(
        PhaseRequest::Optimize,
        &state,
        &progress,
        BootMode::Normal,
        RuntimeBinding::Unavailable,
    )?;
    let mut summary = run_live_steps(
        is_phase_one_engine_step,
        state.profile,
        progress,
        yes,
        LiveMutationAuthority::Package(package),
    )?;
    let (_, progress) = load_session()?;
    require_phase_one_handoff_ready(&progress).map_err(|error| {
        ApplicationError::failed(format!(
            "runtime publication refused because Phase 1 is unresolved: {error:?}"
        ))
    })?;
    let handoff = step_by_id(PHASE_ONE_SAFE_MODE_HANDOFF)
        .ok_or_else(|| ApplicationError::failed("P1:38 is absent from the compiled catalog"))?;
    if !yes && !prompt_for_step(handoff) {
        summary.messages.push(
            "P1:38 was not armed; rerun optimize to publish the protected runtime and continue."
                .into(),
        );
        return Ok(summary);
    }
    let runtime = publish_current_packaged_runtime(package).map_err(ApplicationError::failed)?;
    summary.messages.push(format!(
        "Published protected runtime generation {}.",
        runtime.record().generation
    ));
    launch_published_safe_mode_handoff(&runtime).map_err(ApplicationError::failed)?;
    let (_, progress) = load_session()?;
    if !progress.is_completed(PHASE_ONE_SAFE_MODE_HANDOFF) {
        return Err(ApplicationError::failed(
            "the selected runtime exited without durably completing P1:38",
        ));
    }
    summary
        .messages
        .push("P1:38 Safe Mode handoff is armed; restart Windows to continue Phase 2.".into());
    Ok(summary)
}

fn is_phase_one_engine_step(step: &frametime_domain::Step) -> bool {
    step.id.phase == Phase::One && step.orchestration_role == OrchestrationRole::Engine
}

fn run_boot_safe_mode(yes: bool) -> Result<RunSummary, ApplicationError> {
    require_yes(yes, "boot-safe-mode")?;
    require_boot_mode(BootMode::Normal, "boot-safe-mode")?;
    let runtime = require_selected_runtime("boot-safe-mode")?;
    let (state, progress) = load_session()?;
    match authorize_live(
        PhaseRequest::ArmSafeMode,
        &state,
        &progress,
        BootMode::Normal,
        RuntimeBinding::VerifiedSelectedExecutable,
    )? {
        Transition::ArmSafeMode {
            requires_readiness_transition: true,
        } => {
            arm_safe_mode_handoff(&runtime).map_err(ApplicationError::failed)?;
            Ok(RunSummary::default())
        }
        _ => unreachable!("core returned an unexpected safe-mode transition"),
    }
}

fn run_phase_two(yes: bool) -> Result<RunSummary, ApplicationError> {
    require_boot_mode(BootMode::SafeMode, "phase2")?;
    let runtime = require_selected_runtime("phase2")?;
    let (state, progress) = load_session()?;
    match authorize_live(
        PhaseRequest::PhaseTwo,
        &state,
        &progress,
        BootMode::SafeMode,
        RuntimeBinding::VerifiedSelectedExecutable,
    )? {
        Transition::RunPhaseTwoStepOne => {
            complete_phase_two_safe_boot_clear(&runtime).map_err(ApplicationError::failed)?;
            Ok(RunSummary::default())
        }
        Transition::RunRemainingPhaseTwo => {
            let summary = run_live_steps(
                |step| step.id == PHASE_TWO_DRIVER_CLEANUP,
                state.profile,
                progress,
                yes,
                LiveMutationAuthority::Runtime(&runtime),
            )?;
            let (_, updated_progress) = load_session()?;
            if updated_progress.is_completed(PHASE_TWO_DRIVER_CLEANUP) {
                arm_phase_three_handoff(&runtime).map_err(ApplicationError::failed)?;
            }
            Ok(summary)
        }
        _ => unreachable!("core returned an unexpected Phase 2 transition"),
    }
}

fn run_phase_three(yes: bool) -> Result<RunSummary, ApplicationError> {
    require_boot_mode(BootMode::Normal, "phase3")?;
    let runtime = require_selected_runtime("phase3")?;
    let (state, progress) = load_session()?;
    let receipt_route =
        phase_three_receipt_route(final_benchmark_status().map_err(ApplicationError::failed)?)?;
    if let PhaseThreeReceiptRoute::Complete(receipt) = receipt_route {
        authorize_live_with_final_benchmark_evidence(
            PhaseRequest::ClearPhaseThreeHandoff,
            &state,
            &progress,
            BootMode::Normal,
            RuntimeBinding::VerifiedSelectedExecutable,
            Evidence::Verified,
        )?;
        clear_phase_three_handoff(&runtime).map_err(ApplicationError::failed)?;
        return Ok(RunSummary::message(format!(
            "P3:13 final benchmark receipt is coherent: {}.",
            receipt.receipt_id
        )));
    }
    authorize_live_with_final_benchmark_evidence(
        PhaseRequest::PhaseThree,
        &state,
        &progress,
        BootMode::Normal,
        RuntimeBinding::VerifiedSelectedExecutable,
        Evidence::Absent,
    )?;
    let phase_three_steps = phase_three_engine_steps();
    let mut summary = run_live_steps(
        |step| phase_three_steps.contains(step),
        state.profile,
        progress,
        yes,
        LiveMutationAuthority::Runtime(&runtime),
    )?;
    match final_benchmark_status().map_err(ApplicationError::failed)? {
        FinalBenchmarkStatus::Absent => Err(ApplicationError::failed(
            "P3:13 is not persisted; run `frametime final-benchmark` with one complete VProf capture",
        )),
        FinalBenchmarkStatus::Coherent(receipt) => {
            let (state, progress) = load_session()?;
            authorize_live_with_final_benchmark_evidence(
                PhaseRequest::ClearPhaseThreeHandoff,
                &state,
                &progress,
                BootMode::Normal,
                RuntimeBinding::VerifiedSelectedExecutable,
                Evidence::Verified,
            )?;
            clear_phase_three_handoff(&runtime).map_err(ApplicationError::failed)?;
            summary.messages.push(format!(
                "P3:13 final benchmark receipt is coherent: {}.",
                receipt.receipt_id
            ));
            Ok(summary)
        }
        FinalBenchmarkStatus::Incoherent(error) => Err(ApplicationError::failed(format!(
            "P3:13 receipt is incoherent and was not repaired: {error}"
        ))),
    }
}

enum PhaseThreeReceiptRoute {
    RunEngine,
    Complete(FinalBenchmarkReceipt),
}

fn phase_three_receipt_route(
    status: FinalBenchmarkStatus,
) -> Result<PhaseThreeReceiptRoute, ApplicationError> {
    match status {
        FinalBenchmarkStatus::Absent => Ok(PhaseThreeReceiptRoute::RunEngine),
        FinalBenchmarkStatus::Coherent(receipt) => Ok(PhaseThreeReceiptRoute::Complete(receipt)),
        FinalBenchmarkStatus::Incoherent(error) => Err(ApplicationError::failed(format!(
            "P3:13 receipt is incoherent and was not repaired: {error}"
        ))),
    }
}

fn phase_three_engine_steps() -> Vec<frametime_domain::Step> {
    step_catalog()
        .iter()
        .filter(|step| {
            step.id.phase == Phase::Three && step.orchestration_role == OrchestrationRole::Engine
        })
        .copied()
        .collect()
}

pub fn run_final_benchmark(
    request: VprofBenchmarkRequest,
) -> Result<BenchmarkOutcome, ApplicationError> {
    let capture = read_final_benchmark_capture(request)?;
    require_boot_mode(BootMode::Normal, "final-benchmark")?;
    let runtime = require_selected_runtime("final-benchmark")?;
    let (state, progress) = load_session()?;
    authorize_live(
        PhaseRequest::FinalBenchmark,
        &state,
        &progress,
        BootMode::Normal,
        RuntimeBinding::VerifiedSelectedExecutable,
    )?;
    let outcome = persist_final_benchmark_capture(capture, &runtime)?;
    let (state, progress) = load_session()?;
    authorize_live_with_final_benchmark_evidence(
        PhaseRequest::ClearPhaseThreeHandoff,
        &state,
        &progress,
        BootMode::Normal,
        RuntimeBinding::VerifiedSelectedExecutable,
        Evidence::Verified,
    )?;
    clear_phase_three_handoff(&runtime).map_err(ApplicationError::failed)?;
    Ok(outcome)
}

fn run_phase_three_handoff() -> Result<RunSummary, ApplicationError> {
    require_boot_mode(BootMode::Normal, "phase3-handoff")?;
    let runtime = require_selected_runtime("phase3-handoff")?;
    let (state, progress) = load_session()?;
    authorize_live(
        PhaseRequest::PhaseThree,
        &state,
        &progress,
        BootMode::Normal,
        RuntimeBinding::VerifiedSelectedExecutable,
    )?;
    relaunch_phase_three_handoff(&runtime).map_err(ApplicationError::failed)?;
    Ok(RunSummary::default())
}

fn require_selected_runtime(
    command: &str,
) -> Result<frametime_windows::VerifiedSelectedRuntime, ApplicationError> {
    retain_selected_runtime().map_err(|error| {
        ApplicationError::failed(format!(
            "{command} requires the retained verified selected runtime: {error}"
        ))
    })
}

fn load_session() -> Result<(State, Progress), ApplicationError> {
    Ok((
        load_state().map_err(ApplicationError::failed)?,
        load_progress().map_err(ApplicationError::failed)?,
    ))
}

fn require_boot_mode(expected: BootMode, command: &str) -> Result<(), ApplicationError> {
    let actual = current_boot_mode().map_err(ApplicationError::failed)?;
    if actual == expected {
        Ok(())
    } else {
        Err(ApplicationError::failed(format!(
            "{command} requires {:?} boot; current boot is {actual:?}",
            expected
        )))
    }
}

fn require_clean_native_reboot_state(command: &str) -> Result<(), ApplicationError> {
    let native = inspect_reboot_handoff_state().map_err(ApplicationError::failed)?;
    let state = load_state().map_err(ApplicationError::failed)?;
    if native.boot_mode == BootModeEvidence::Normal
        && native.safeboot == SafebootEvidence::Absent
        && native.phase2_runonce_armed == NativeHandoffEvidence::Absent
        && native.phase3_run_armed == NativeHandoffEvidence::Absent
        && !state_has_reboot_transaction(&state)
    {
        Ok(())
    } else {
        Err(ApplicationError::failed(format!(
            "{command} refused because the native reboot transaction is armed or could not be completely inspected"
        )))
    }
}

fn state_has_reboot_transaction(state: &State) -> bool {
    state.phase1_safe_mode_ready
        || state.active_reboot_transaction.is_some()
        || state.unknown.contains_key("activeRebootTransaction")
}

fn require_legacy_phase_one_migration(
    state: &State,
    progress: &Progress,
    yes: bool,
) -> Result<(), ApplicationError> {
    let native = inspect_reboot_handoff_state().map_err(ApplicationError::failed)?;
    let decision = assess_inventory(
        Some(state),
        Some(progress),
        migration_inventory_from_native(&native, runtime_inventory_incomplete()),
    );
    require_migration_decision(decision, yes)
}

fn require_migration_decision(
    decision: MigrationDecision,
    yes: bool,
) -> Result<(), ApplicationError> {
    match decision {
        MigrationDecision::NotNeeded => Ok(()),
        MigrationDecision::ConfirmIdle => require_yes(yes, "optimize legacy Phase 1 migration"),
        MigrationDecision::ConfirmPartialPhaseOne { .. } if yes => Ok(()),
        MigrationDecision::ConfirmPartialPhaseOne { completed, skipped } => {
            Err(ApplicationError::Invalid(format!(
                "optimize legacy Phase 1 migration ({completed} completed, {skipped} skipped) requires --yes"
            )))
        }
        MigrationDecision::Refuse(reason) => Err(ApplicationError::failed(format!(
            "optimize refused: legacy reboot transaction is armed or incomplete ({reason:?})"
        ))),
    }
}

fn migration_inventory_from_native(
    native: &RebootHandoffState,
    incomplete_runtime: bool,
) -> MigrationInventory {
    MigrationInventory {
        phase_two_run_once_armed: native.phase2_runonce_armed == NativeHandoffEvidence::Verified,
        phase_three_run_armed: native.phase3_run_armed == NativeHandoffEvidence::Verified,
        safe_boot_armed: matches!(&native.safeboot, SafebootEvidence::Configured(_)),
        incomplete_runtime: incomplete_runtime
            || matches!(
                &native.phase2_runonce_armed,
                NativeHandoffEvidence::Unavailable
            )
            || matches!(&native.phase3_run_armed, NativeHandoffEvidence::Unavailable)
            || matches!(&native.safeboot, SafebootEvidence::Unavailable),
    }
}

fn runtime_inventory_incomplete() -> bool {
    native_runtime_inventory_incomplete()
}

fn authorize_live(
    request: PhaseRequest,
    state: &State,
    progress: &Progress,
    boot: BootMode,
    runtime: RuntimeBinding,
) -> Result<Transition, ApplicationError> {
    let final_benchmark_persisted =
        final_benchmark_status().map_or(Evidence::Unavailable, final_benchmark_evidence);
    authorize_live_with_final_benchmark_evidence(
        request,
        state,
        progress,
        boot,
        runtime,
        final_benchmark_persisted,
    )
}

fn authorize_live_with_final_benchmark_evidence(
    request: PhaseRequest,
    state: &State,
    progress: &Progress,
    boot: BootMode,
    runtime: RuntimeBinding,
    final_benchmark_persisted: Evidence,
) -> Result<Transition, ApplicationError> {
    let native = inspect_reboot_handoff_state().map_err(ApplicationError::failed)?;
    let observed_boot = match native.boot_mode {
        BootModeEvidence::Normal => BootEnvironment::Normal,
        BootModeEvidence::SafeMode => BootEnvironment::SafeMode,
        BootModeEvidence::Unavailable => {
            return Err(ApplicationError::failed(
                "native reboot-state inspection could not determine the current boot mode",
            ));
        }
    };
    let expected_boot = match boot {
        BootMode::Normal => BootEnvironment::Normal,
        BootMode::SafeMode => BootEnvironment::SafeMode,
    };
    if observed_boot != expected_boot {
        return Err(ApplicationError::failed(
            "native reboot-state evidence disagrees with the command boot-mode preflight",
        ));
    }
    let runtime = if runtime == RuntimeBinding::VerifiedSelectedExecutable
        && native.selected_runtime_binding == NativeHandoffEvidence::Verified
    {
        RuntimeBinding::VerifiedSelectedExecutable
    } else {
        RuntimeBinding::Unavailable
    };
    let facts = PhaseFacts {
        boot: observed_boot,
        runtime,
        handoff: HandoffEvidence {
            phase_two_run_once: map_native_evidence(native.phase2_runonce_armed),
            phase_three_run: map_native_evidence(native.phase3_run_armed),
            safe_boot: match native.safeboot {
                SafebootEvidence::Configured(_) => Evidence::Verified,
                SafebootEvidence::Absent => Evidence::Absent,
                SafebootEvidence::Unavailable => Evidence::Unavailable,
            },
            phase_three_same_user: map_native_evidence(native.phase3_handoff_same_user),
        },
        phase_one_safe_mode_ready: state.phase1_safe_mode_ready,
        final_benchmark_persisted,
    };
    authorize(request, state, progress, facts).map_err(|error| {
        ApplicationError::failed(format!(
            "{request:?} refused by reboot state machine: {error:?}; no phase action was started"
        ))
    })
}

fn final_benchmark_evidence(status: FinalBenchmarkStatus) -> Evidence {
    match status {
        FinalBenchmarkStatus::Absent => Evidence::Absent,
        FinalBenchmarkStatus::Coherent(_) => Evidence::Verified,
        FinalBenchmarkStatus::Incoherent(_) => Evidence::Unavailable,
    }
}

const fn map_native_evidence(value: NativeHandoffEvidence) -> Evidence {
    match value {
        NativeHandoffEvidence::Verified => Evidence::Verified,
        NativeHandoffEvidence::Absent => Evidence::Absent,
        NativeHandoffEvidence::Unavailable => Evidence::Unavailable,
    }
}

enum LiveMutationAuthority<'authority> {
    Package(&'authority AuthenticatedPackage),
    Runtime(&'authority frametime_windows::VerifiedSelectedRuntime),
}

fn run_live_steps(
    filter: impl Fn(&frametime_domain::Step) -> bool,
    profile: Profile,
    progress: Progress,
    yes: bool,
    authority: LiveMutationAuthority<'_>,
) -> Result<RunSummary, ApplicationError> {
    let steps = step_catalog()
        .iter()
        .filter(|step| filter(step))
        .copied()
        .collect::<Vec<_>>();
    let backend = match authority {
        LiveMutationAuthority::Package(package) => LiveBackend::from_package(package),
        LiveMutationAuthority::Runtime(runtime) => LiveBackend::from_runtime(runtime),
    }
    .map_err(ApplicationError::failed)?;
    let mut engine = Engine::new(backend, progress);
    let report = engine
        .run_with_control(
            &steps,
            profile,
            |step| yes || prompt_for_step(step),
            cancellation_requested,
        )
        .map_err(|error| ApplicationError::failed(error.to_string()))?;
    let mut summary = RunSummary::message(format!(
        "Completed: {}; skipped: {}; advisories: {}",
        report.completed, report.skipped, report.advisories
    ));
    summary.messages.extend(
        frametime_windows::read_presentable_step_messages(
            &steps,
            &load_state().map_err(ApplicationError::failed)?,
        )
        .map_err(ApplicationError::failed)?,
    );
    Ok(summary)
}

#[cfg(test)]
mod tests;
