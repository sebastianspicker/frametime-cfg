use std::path::{Path, PathBuf};

use frametime_core::{
    BootEnvironment, Engine, Evidence, FinalBenchmarkReceipt, HandoffEvidence, MigrationDecision,
    MigrationInventory, Phase, PhaseFacts, PhaseRequest, Profile, Progress, RuntimeBinding, State,
    Transition, assess_inventory, authorize, require_phase_one_handoff_ready,
    runtime::load_selected_portable_generation, step_catalog,
};
use frametime_windows::{
    AuthenticatedPackage, BootMode, BootModeEvidence, FinalBenchmarkStatus,
    HandoffEvidence as NativeHandoffEvidence, LiveBackend, RebootHandoffState, SafebootEvidence,
    WINDOWS_WORK_DIR, arm_phase_three_handoff, arm_safe_mode_handoff, clear_phase_three_handoff,
    complete_phase_two_safe_boot_clear, current_boot_mode, final_benchmark_status,
    inspect_reboot_handoff_state, launch_published_safe_mode_handoff, load_progress, load_state,
    platform_is_supported, publish_current_packaged_runtime, relaunch_phase_three_handoff,
    retain_selected_runtime,
};

use crate::{
    actions::{
        cleanup, persist_final_benchmark_capture, read_final_benchmark_capture, require_yes,
        verify_snapshot,
    },
    cli::{Command, VprofBenchmarkRequest},
    console::{cancellation_requested, prompt_for_step},
    error::AppError,
    package_auth::require_authenticated_package,
};

pub(crate) fn run_live(command: Command) -> Result<(), AppError> {
    if matches!(command, Command::Verify) && !platform_is_supported() {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "readOnly": true,
            "items": [{
                "status": "INFO",
                "name": "platform",
                "detail": "Native Windows settings are unavailable on this host; no changes were made."
            }]
        })).map_err(|error| AppError::failed(error.to_string()))?);
        return Ok(());
    }
    if !platform_is_supported() {
        return Err(AppError::failed(
            "live commands require x64 Windows 10 or 11; use `frametime dry-run all` on this host",
        ));
    }
    let work_dir = PathBuf::from(WINDOWS_WORK_DIR);
    match command {
        Command::Optimize { yes } => {
            let package = require_authenticated_package()?;
            run_phase_one(&work_dir, yes, &package)
        }
        Command::Configure { profile, dry_run } => {
            let _package = require_authenticated_package()?;
            require_clean_native_reboot_state(&work_dir, "configure")?;
            let state = frametime_windows::configure_profile(&work_dir, profile.into(), dry_run)
                .map_err(AppError::failed)?;
            println!(
                "Profile: {:?}; mode: {}; GUI preview preference: {}",
                state.profile, state.mode, dry_run
            );
            Ok(())
        }
        Command::BootSafeMode { yes } => run_boot_safe_mode(&work_dir, yes),
        Command::Phase2 { yes } => run_phase_two(&work_dir, yes),
        Command::Phase3 { yes } => run_phase_three(&work_dir, yes),
        Command::Phase3Handoff => run_phase_three_handoff(&work_dir),
        Command::Cleanup {
            mode,
            yes,
            acknowledge_irreversible,
        } => {
            let package = require_authenticated_package()?;
            cleanup(mode, yes, acknowledge_irreversible, &package)
        }
        Command::Verify => verify_snapshot(&work_dir),
        Command::Restore { yes } => {
            require_yes(yes, "restore")?;
            match require_authenticated_package() {
                Ok(package) => frametime_windows::restore_all(&work_dir, package.config())
                    .map_err(AppError::failed),
                Err(package_error) => {
                    let runtime = retain_selected_runtime(&work_dir).map_err(|runtime_error| {
                        AppError::failed(format!(
                            "restore requires an authenticated package or selected protected runtime; package: {package_error}; runtime: {runtime_error}"
                        ))
                    })?;
                    frametime_windows::restore_all(&work_dir, runtime.config())
                        .map_err(AppError::failed)
                }
            }
        }
        Command::BackupSummary => {
            frametime_windows::backup_summary(&work_dir).map_err(AppError::failed)
        }
        Command::ResetProgress { yes } => {
            require_yes(yes, "reset-progress")?;
            let _package = require_authenticated_package()?;
            require_clean_native_reboot_state(&work_dir, "reset-progress")?;
            frametime_windows::reset_progress(&work_dir).map_err(AppError::failed)
        }
        Command::ShowLog => frametime_windows::show_log(&work_dir).map_err(AppError::failed),
        Command::DryRun { .. }
        | Command::FpsCap { .. }
        | Command::BaselineBenchmark { .. }
        | Command::FinalBenchmark { .. }
        | Command::Driver { .. }
        | Command::Hardware { .. }
        | Command::SmokeTest
        | Command::PackageAuthSmoke
        | Command::Exit => unreachable!(),
    }
}

fn run_phase_one(
    work_dir: &Path,
    yes: bool,
    package: &AuthenticatedPackage,
) -> Result<(), AppError> {
    require_boot_mode(BootMode::Normal, "optimize")?;
    let (state, progress) = load_session(work_dir)?;
    require_legacy_phase_one_migration(work_dir, &state, &progress, yes)?;
    authorize_live(
        PhaseRequest::Optimize,
        &state,
        &progress,
        BootMode::Normal,
        RuntimeBinding::Unavailable,
    )?;
    run_live_steps(
        work_dir,
        is_phase_one_engine_step,
        state.profile,
        progress,
        yes,
        package.config().clone(),
    )?;
    let (_, progress) = load_session(work_dir)?;
    require_phase_one_handoff_ready(&progress).map_err(|error| {
        AppError::failed(format!(
            "runtime publication refused because Phase 1 is unresolved: {error:?}"
        ))
    })?;
    let handoff = step_catalog()
        .iter()
        .find(|step| step.phase == Phase::One && step.number == 38)
        .ok_or_else(|| AppError::failed("P1:38 is absent from the compiled catalog"))?;
    if !yes && !prompt_for_step(handoff) {
        println!(
            "P1:38 was not armed; rerun optimize to publish the protected runtime and continue."
        );
        return Ok(());
    }
    let runtime = publish_current_packaged_runtime(package).map_err(AppError::failed)?;
    println!(
        "Published protected runtime generation {}.",
        runtime.record().generation
    );
    launch_published_safe_mode_handoff(&runtime).map_err(AppError::failed)?;
    let (_, progress) = load_session(work_dir)?;
    if !progress.completed_steps.contains(&Progress::key(1, 38)) {
        return Err(AppError::failed(
            "the selected runtime exited without durably completing P1:38",
        ));
    }
    println!("P1:38 Safe Mode handoff is armed; restart Windows to continue Phase 2.");
    Ok(())
}

fn is_phase_one_engine_step(step: &frametime_core::Step) -> bool {
    step.phase == Phase::One && step.number <= 37
}

fn run_boot_safe_mode(work_dir: &Path, yes: bool) -> Result<(), AppError> {
    require_yes(yes, "boot-safe-mode")?;
    require_boot_mode(BootMode::Normal, "boot-safe-mode")?;
    let _runtime = require_selected_runtime(work_dir, "boot-safe-mode")?;
    let (state, progress) = load_session(work_dir)?;
    match authorize_live(
        PhaseRequest::ArmSafeMode,
        &state,
        &progress,
        BootMode::Normal,
        RuntimeBinding::VerifiedSelectedExecutable,
    )? {
        Transition::ArmSafeMode {
            requires_readiness_transition: true,
        } => arm_safe_mode_handoff(work_dir).map_err(AppError::failed),
        _ => unreachable!("core returned an unexpected safe-mode transition"),
    }
}

fn run_phase_two(work_dir: &Path, yes: bool) -> Result<(), AppError> {
    require_boot_mode(BootMode::SafeMode, "phase2")?;
    let runtime = require_selected_runtime(work_dir, "phase2")?;
    let (state, progress) = load_session(work_dir)?;
    match authorize_live(
        PhaseRequest::PhaseTwo,
        &state,
        &progress,
        BootMode::SafeMode,
        RuntimeBinding::VerifiedSelectedExecutable,
    )? {
        Transition::RunPhaseTwoStepOne => {
            complete_phase_two_safe_boot_clear(work_dir).map_err(AppError::failed)
        }
        Transition::RunRemainingPhaseTwo => {
            run_live_steps(
                work_dir,
                |step| step.phase == Phase::Two && step.number == 2,
                state.profile,
                progress,
                yes,
                runtime.config().clone(),
            )?;
            let (_, updated_progress) = load_session(work_dir)?;
            if updated_progress
                .completed_steps
                .contains(&Progress::key(2, 2))
            {
                arm_phase_three_handoff(work_dir).map_err(AppError::failed)?;
            }
            Ok(())
        }
        _ => unreachable!("core returned an unexpected Phase 2 transition"),
    }
}

fn run_phase_three(work_dir: &Path, yes: bool) -> Result<(), AppError> {
    require_boot_mode(BootMode::Normal, "phase3")?;
    let runtime = require_selected_runtime(work_dir, "phase3")?;
    let (state, progress) = load_session(work_dir)?;
    let receipt_route =
        phase_three_receipt_route(final_benchmark_status(work_dir).map_err(AppError::failed)?)?;
    if let PhaseThreeReceiptRoute::Complete(receipt) = receipt_route {
        authorize_live_with_final_benchmark_evidence(
            PhaseRequest::ClearPhaseThreeHandoff,
            &state,
            &progress,
            BootMode::Normal,
            RuntimeBinding::VerifiedSelectedExecutable,
            Evidence::Verified,
        )?;
        clear_phase_three_handoff(work_dir).map_err(AppError::failed)?;
        println!(
            "P3:13 final benchmark receipt is coherent: {}.",
            receipt.receipt_id
        );
        return Ok(());
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
    run_live_steps(
        work_dir,
        |step| phase_three_steps.contains(step),
        state.profile,
        progress,
        yes,
        runtime.config().clone(),
    )?;
    match final_benchmark_status(work_dir).map_err(AppError::failed)? {
        FinalBenchmarkStatus::Absent => Err(AppError::failed(
            "P3:13 is not persisted; run `frametime final-benchmark` with one complete VProf capture",
        )),
        FinalBenchmarkStatus::Coherent(receipt) => {
            let (state, progress) = load_session(work_dir)?;
            authorize_live_with_final_benchmark_evidence(
                PhaseRequest::ClearPhaseThreeHandoff,
                &state,
                &progress,
                BootMode::Normal,
                RuntimeBinding::VerifiedSelectedExecutable,
                Evidence::Verified,
            )?;
            clear_phase_three_handoff(work_dir).map_err(AppError::failed)?;
            println!(
                "P3:13 final benchmark receipt is coherent: {}.",
                receipt.receipt_id
            );
            Ok(())
        }
        FinalBenchmarkStatus::Incoherent(error) => Err(AppError::failed(format!(
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
) -> Result<PhaseThreeReceiptRoute, AppError> {
    match status {
        FinalBenchmarkStatus::Absent => Ok(PhaseThreeReceiptRoute::RunEngine),
        FinalBenchmarkStatus::Coherent(receipt) => Ok(PhaseThreeReceiptRoute::Complete(receipt)),
        FinalBenchmarkStatus::Incoherent(error) => Err(AppError::failed(format!(
            "P3:13 receipt is incoherent and was not repaired: {error}"
        ))),
    }
}

fn phase_three_engine_steps() -> Vec<frametime_core::Step> {
    step_catalog()
        .iter()
        .filter(|step| step.phase == Phase::Three && step.number <= 12)
        .copied()
        .collect()
}

pub(crate) fn run_final_benchmark(request: VprofBenchmarkRequest) -> Result<(), AppError> {
    let capture = read_final_benchmark_capture(request)?;
    let work_dir = PathBuf::from(WINDOWS_WORK_DIR);
    require_boot_mode(BootMode::Normal, "final-benchmark")?;
    let runtime = require_selected_runtime(&work_dir, "final-benchmark")?;
    let (state, progress) = load_session(&work_dir)?;
    authorize_live(
        PhaseRequest::FinalBenchmark,
        &state,
        &progress,
        BootMode::Normal,
        RuntimeBinding::VerifiedSelectedExecutable,
    )?;
    persist_final_benchmark_capture(capture, runtime.config())?;
    let (state, progress) = load_session(&work_dir)?;
    authorize_live_with_final_benchmark_evidence(
        PhaseRequest::ClearPhaseThreeHandoff,
        &state,
        &progress,
        BootMode::Normal,
        RuntimeBinding::VerifiedSelectedExecutable,
        Evidence::Verified,
    )?;
    clear_phase_three_handoff(&work_dir).map_err(AppError::failed)
}

fn run_phase_three_handoff(work_dir: &Path) -> Result<(), AppError> {
    require_boot_mode(BootMode::Normal, "phase3-handoff")?;
    let _runtime = require_selected_runtime(work_dir, "phase3-handoff")?;
    let (state, progress) = load_session(work_dir)?;
    authorize_live(
        PhaseRequest::PhaseThree,
        &state,
        &progress,
        BootMode::Normal,
        RuntimeBinding::VerifiedSelectedExecutable,
    )?;
    relaunch_phase_three_handoff(work_dir).map_err(AppError::failed)
}

fn require_selected_runtime(
    work_dir: &Path,
    command: &str,
) -> Result<frametime_windows::VerifiedSelectedRuntime, AppError> {
    retain_selected_runtime(work_dir).map_err(|error| {
        AppError::failed(format!(
            "{command} requires the retained verified selected runtime: {error}"
        ))
    })
}

fn load_session(work_dir: &Path) -> Result<(State, Progress), AppError> {
    Ok((
        load_state(work_dir).map_err(AppError::failed)?,
        load_progress(work_dir).map_err(AppError::failed)?,
    ))
}

fn require_boot_mode(expected: BootMode, command: &str) -> Result<(), AppError> {
    let actual = current_boot_mode().map_err(AppError::failed)?;
    if actual == expected {
        Ok(())
    } else {
        Err(AppError::failed(format!(
            "{command} requires {:?} boot; current boot is {actual:?}",
            expected
        )))
    }
}

fn require_clean_native_reboot_state(work_dir: &Path, command: &str) -> Result<(), AppError> {
    let native = inspect_reboot_handoff_state(work_dir).map_err(AppError::failed)?;
    let state = load_state(work_dir).map_err(AppError::failed)?;
    if native.boot_mode == BootModeEvidence::Normal
        && native.safeboot == SafebootEvidence::Absent
        && native.phase2_runonce_armed == NativeHandoffEvidence::Absent
        && native.phase3_run_armed == NativeHandoffEvidence::Absent
        && !state_has_reboot_transaction(&state)
    {
        Ok(())
    } else {
        Err(AppError::failed(format!(
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
    work_dir: &Path,
    state: &State,
    progress: &Progress,
    yes: bool,
) -> Result<(), AppError> {
    let native = inspect_reboot_handoff_state(work_dir).map_err(AppError::failed)?;
    let decision = assess_inventory(
        Some(state),
        Some(progress),
        migration_inventory_from_native(&native, runtime_inventory_incomplete(work_dir)),
    );
    require_migration_decision(decision, yes)
}

fn require_migration_decision(decision: MigrationDecision, yes: bool) -> Result<(), AppError> {
    match decision {
        MigrationDecision::NotNeeded => Ok(()),
        MigrationDecision::ConfirmIdle => require_yes(yes, "optimize legacy Phase 1 migration"),
        MigrationDecision::ConfirmPartialPhaseOne { .. } if yes => Ok(()),
        MigrationDecision::ConfirmPartialPhaseOne { completed, skipped } => {
            Err(AppError::Invalid(format!(
                "optimize legacy Phase 1 migration ({completed} completed, {skipped} skipped) requires --yes"
            )))
        }
        MigrationDecision::Refuse(reason) => Err(AppError::failed(format!(
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

fn runtime_inventory_incomplete(work_dir: &Path) -> bool {
    match work_dir.join("runtime-current.json").try_exists() {
        Ok(false) => false,
        Ok(true) => load_selected_portable_generation(work_dir).is_err(),
        Err(_) => true,
    }
}

fn authorize_live(
    request: PhaseRequest,
    state: &State,
    progress: &Progress,
    boot: BootMode,
    runtime: RuntimeBinding,
) -> Result<Transition, AppError> {
    let final_benchmark_persisted = final_benchmark_status(Path::new(WINDOWS_WORK_DIR))
        .map_or(Evidence::Unavailable, final_benchmark_evidence);
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
) -> Result<Transition, AppError> {
    let native =
        inspect_reboot_handoff_state(Path::new(WINDOWS_WORK_DIR)).map_err(AppError::failed)?;
    let observed_boot = match native.boot_mode {
        BootModeEvidence::Normal => BootEnvironment::Normal,
        BootModeEvidence::SafeMode => BootEnvironment::SafeMode,
        BootModeEvidence::Unavailable => {
            return Err(AppError::failed(
                "native reboot-state inspection could not determine the current boot mode",
            ));
        }
    };
    let expected_boot = match boot {
        BootMode::Normal => BootEnvironment::Normal,
        BootMode::SafeMode => BootEnvironment::SafeMode,
    };
    if observed_boot != expected_boot {
        return Err(AppError::failed(
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
        AppError::failed(format!(
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

fn run_live_steps(
    work_dir: &Path,
    filter: impl Fn(&frametime_core::Step) -> bool,
    profile: Profile,
    progress: Progress,
    yes: bool,
    config: frametime_windows::VerifiedConfig,
) -> Result<(), AppError> {
    let steps = step_catalog()
        .iter()
        .filter(|step| filter(step))
        .copied()
        .collect::<Vec<_>>();
    let backend = LiveBackend::new(work_dir.to_path_buf(), config).map_err(AppError::failed)?;
    let mut engine = Engine::new(backend, progress);
    let report = engine
        .run_with_control(
            &steps,
            profile,
            |step| yes || prompt_for_step(step),
            cancellation_requested,
        )
        .map_err(|error| AppError::failed(error.to_string()))?;
    println!(
        "Completed: {}; skipped: {}; advisories: {}",
        report.completed, report.skipped, report.advisories
    );
    Ok(())
}

#[cfg(test)]
mod tests;
