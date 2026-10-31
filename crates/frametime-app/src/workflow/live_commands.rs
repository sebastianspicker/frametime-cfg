use frametime_windows::{
    HandoffAbort, HandoffAbortReport, WINDOWS_WORK_DIR, platform_is_supported,
    retain_selected_runtime,
};

use crate::{
    ApplicationError, CommandOutcome, RunSummary, VerificationSummary,
    actions::{cleanup, require_yes, verify_snapshot},
    commands::Command,
    live_log, require_authenticated_package,
};

use super::{
    require_clean_native_reboot_state, run_boot_safe_mode, run_phase_one, run_phase_three,
    run_phase_three_handoff, run_phase_two,
};

pub fn run_live(command: Command) -> Result<CommandOutcome, ApplicationError> {
    if let Some(host_outcome) = require_live_platform(&command)? {
        return Ok(host_outcome);
    }
    let Some(label) = live_log::command_label(&command) else {
        return run_live_command(command);
    };
    let mut sink = live_log::LogSink::default();
    sink.record(true, &[live_log::command_entry(label)]);
    let result = run_live_command(command);
    if let Err(error) = &result {
        sink.record(false, &[live_log::command_failure_entry(label, error)]);
    }
    result
}

fn run_live_command(command: Command) -> Result<CommandOutcome, ApplicationError> {
    match command {
        Command::Optimize { yes } => {
            let package = require_authenticated_package()?;
            run_phase_one(yes, &package).map(CommandOutcome::Run)
        }
        Command::Configure { profile, dry_run } => {
            let package = require_authenticated_package()?;
            require_clean_native_reboot_state("configure")?;
            let state = frametime_windows::configure_profile(&package, profile, dry_run)
                .map_err(ApplicationError::failed)?;
            Ok(CommandOutcome::Run(RunSummary::message(format!(
                "Profile: {:?}; mode: {}; GUI preview preference: {}",
                state.profile, state.mode, dry_run
            ))))
        }
        Command::Cs2Cfg { assets, yes } => {
            require_yes(yes, "cs2-cfg")?;
            let package = require_authenticated_package()?;
            require_clean_native_reboot_state("cs2-cfg")?;
            let written = frametime_windows::deploy_optional_cs2_cfgs(&package, &assets)
                .map_err(ApplicationError::failed)?;
            let mut summary = RunSummary {
                messages: written
                    .into_iter()
                    .map(|path| format!("Deployed and verified: {}", path.display()))
                    .collect(),
            };
            summary
                .messages
                .push("Original bytes are retained in backup.json for native recovery.".into());
            Ok(CommandOutcome::Run(summary))
        }
        Command::BootSafeMode { yes } => run_boot_safe_mode(yes).map(CommandOutcome::Run),
        Command::Phase2 { yes } => run_phase_two(yes).map(CommandOutcome::Run),
        Command::Phase3 { yes } => run_phase_three(yes).map(CommandOutcome::Run),
        Command::Phase3Handoff => run_phase_three_handoff().map(CommandOutcome::Run),
        Command::Cleanup {
            mode,
            yes,
            acknowledge_irreversible,
        } => {
            let package = require_authenticated_package()?;
            cleanup(mode, yes, acknowledge_irreversible, &package).map(CommandOutcome::Cleanup)
        }
        Command::Verify => {
            verify_snapshot().map(|summary| CommandOutcome::Verification(Box::new(summary)))
        }
        Command::Restore { yes } => restore(yes).map(CommandOutcome::Run),
        Command::BackupSummary => crate::read_backup_summary().map(CommandOutcome::BackupSummary),
        Command::ResetProgress { yes } => {
            require_yes(yes, "reset-progress")?;
            let package = require_authenticated_package()?;
            require_clean_native_reboot_state("reset-progress")?;
            frametime_windows::reset_progress(&package).map_err(ApplicationError::failed)?;
            Ok(CommandOutcome::Run(RunSummary::default()))
        }
        Command::AbortHandoff { yes } => abort_handoff(yes).map(CommandOutcome::Run),
        Command::ShowLog => crate::read_log().map(CommandOutcome::Log),
        command => unreachable!("{command:?} is routed outside the live workflow"),
    }
}

fn require_live_platform(command: &Command) -> Result<Option<CommandOutcome>, ApplicationError> {
    if platform_is_supported() {
        return Ok(None);
    }
    if matches!(command, Command::Verify) {
        return Ok(Some(CommandOutcome::Verification(Box::new(
            read_only_host_verification(),
        ))));
    }
    Err(ApplicationError::failed(
        "live commands require x64 Windows 10 or 11; use `frametime dry-run all` on this host",
    ))
}

fn read_only_host_verification() -> VerificationSummary {
    VerificationSummary {
        state: frametime_domain::state::State::default(),
        progress: frametime_domain::state::Progress::default(),
        report: frametime_domain::verification::VerificationReport {
            items: vec![frametime_domain::verification::VerificationItem {
                status: frametime_domain::verification::VerificationStatus::Info,
                name: "platform".into(),
                detail:
                    "Native Windows settings are unavailable on this host; no changes were made."
                        .into(),
            }],
        },
        work_dir: WINDOWS_WORK_DIR,
    }
}

fn restore(yes: bool) -> Result<RunSummary, ApplicationError> {
    require_yes(yes, "restore")?;
    match require_authenticated_package() {
        Ok(package) => {
            frametime_windows::restore_all_from_package(&package)
                .map_err(ApplicationError::failed)?;
        }
        Err(package_error) => {
            let runtime = retain_selected_runtime().map_err(|runtime_error| {
                ApplicationError::failed(format!(
                    "restore requires an authenticated package or selected protected runtime; package: {package_error}; runtime: {runtime_error}"
                ))
            })?;
            frametime_windows::restore_all_from_runtime(&runtime)
                .map_err(ApplicationError::failed)?;
        }
    }
    Ok(RunSummary::default())
}

fn abort_handoff(yes: bool) -> Result<RunSummary, ApplicationError> {
    require_yes(yes, "abort-handoff")?;
    let package = require_authenticated_package()?;
    match frametime_windows::abort_reboot_handoff(&package).map_err(ApplicationError::failed)? {
        HandoffAbort::NoActiveTransaction => Err(ApplicationError::Invalid(
            "abort-handoff: no active reboot handoff to abort; nothing was changed".into(),
        )),
        HandoffAbort::Incomplete(report) => Err(ApplicationError::failed(format!(
            "abort-handoff left the reboot transaction in place because some items remain:\n{}\nRemove them by following the stuck-handoff procedure in recovery.md, then rerun abort-handoff --yes",
            abort_lines(&report).join("\n")
        ))),
        HandoffAbort::Cleared(report) => {
            let mut messages = abort_lines(&report);
            messages.push(
                "The reboot transaction was archived to aborted-reboot-transactions.json and cleared.".into(),
            );
            messages.push("Next: run `verify` and `backup-summary`, then `restore --yes`.".into());
            Ok(RunSummary { messages })
        }
    }
}

fn abort_lines(report: &HandoffAbortReport) -> Vec<String> {
    let removed = report.removed.iter().map(|item| format!("Removed: {item}"));
    let absent = report
        .already_absent
        .iter()
        .map(|item| format!("Already absent: {item}"));
    let remaining = report
        .remaining
        .iter()
        .map(|item| format!("Remaining: {item}"));
    removed.chain(absent).chain(remaining).collect()
}
