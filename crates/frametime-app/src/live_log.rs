//! File-backed audit log for live commands. Log failures warn on stderr and
//! never change the outcome of the command being recorded.

use frametime_domain::{
    catalog::Step,
    engine::{Backend, Engine, EngineError, Event, RunReport},
    policy::Profile,
    state::Progress,
};
use frametime_windows::LogEntry;

use crate::commands::Command;

/// The log label of a state-changing live command; read-only commands have none.
pub(crate) fn command_label(command: &Command) -> Option<&'static str> {
    Some(match command {
        Command::Optimize { .. } => "optimize",
        Command::Configure { .. } => "configure",
        Command::Cs2Cfg { .. } => "cs2-cfg",
        Command::BootSafeMode { .. } => "boot-safe-mode",
        Command::Phase2 { .. } => "phase2",
        Command::Phase3 { .. } => "phase3",
        Command::Phase3Handoff => "phase3-handoff",
        Command::Cleanup { .. } => "cleanup",
        Command::Restore { .. } => "restore",
        Command::ResetProgress { .. } => "reset-progress",
        Command::AbortHandoff { .. } => "abort-handoff",
        _ => return None,
    })
}

pub(crate) fn command_entry(label: &str) -> LogEntry {
    LogEntry {
        level: "INFO",
        message: format!("command {label}"),
    }
}

pub(crate) fn command_failure_entry(label: &str, error: &dyn std::fmt::Display) -> LogEntry {
    LogEntry {
        level: "ERROR",
        message: format!("command {label} failed: {error}"),
    }
}

/// Describes what happened to one step run on its own through the engine.
pub(crate) fn step_entry(step: &Step, result: &Result<RunReport, EngineError>) -> LogEntry {
    let key = Progress::key_for(step.id);
    let prefix = format!("step {key} \"{}\"", step.title);
    match result {
        Err(error) => LogEntry {
            level: "ERROR",
            message: format!("{prefix}: failed: {error}"),
        },
        Ok(report) => LogEntry {
            level: "INFO",
            message: format!("{prefix}: {}", step_outcome(step, report)),
        },
    }
}

fn step_outcome(step: &Step, report: &RunReport) -> String {
    let events = &report.events;
    if let Some(reason) = events.iter().find_map(|event| match event {
        Event::Advisory { reason, .. } => Some(reason),
        _ => None,
    }) {
        return format!("advisory ({reason})");
    }
    if events.iter().any(|event| matches!(event, Event::Skip(_))) {
        "skipped".into()
    } else if !events
        .iter()
        .any(|event| matches!(event, Event::Complete(_)))
    {
        "already recorded; not run again".into()
    } else if step.check_only {
        "inspect-only".into()
    } else if events.iter().any(|event| matches!(event, Event::Apply(_))) {
        "applied".into()
    } else {
        "satisfied".into()
    }
}

/// Appends records, warning once per sink when the log cannot be written.
#[derive(Default)]
pub(crate) struct LogSink {
    warned: bool,
}

impl LogSink {
    pub(crate) fn record(&mut self, start_session: bool, entries: &[LogEntry]) {
        if let Err(error) = frametime_windows::append_log(start_session, entries)
            && !self.warned
        {
            self.warned = true;
            eprintln!("warning: could not write log: {error}");
        }
    }
}

/// Runs each step through the engine on its own so every outcome, including
/// the failing one, is logged as it happens. The engine's semantics are
/// unchanged: it keeps its progress between calls and stops at the first error.
pub(crate) fn run_steps_logged<B: Backend>(
    engine: &mut Engine<B>,
    steps: &[Step],
    profile: Profile,
    mut consent: impl FnMut(&Step) -> bool,
    mut cancelled: impl FnMut() -> bool,
) -> Result<RunReport, EngineError> {
    let mut sink = LogSink::default();
    let mut total = RunReport::default();
    for step in steps {
        let result = engine.run_with_control(
            std::slice::from_ref(step),
            profile,
            &mut consent,
            &mut cancelled,
        );
        sink.record(false, &[step_entry(step, &result)]);
        let report = result?;
        total.events.extend(report.events);
        total.completed += report.completed;
        total.skipped += report.skipped;
        total.advisories += report.advisories;
        total.failed += report.failed;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use frametime_domain::catalog::step_catalog;

    use super::*;

    fn catalog_step(check_only: bool) -> Step {
        *step_catalog()
            .iter()
            .find(|step| step.check_only == check_only)
            .expect("catalog contains the requested step kind")
    }

    fn report(events: Vec<Event>) -> Result<RunReport, EngineError> {
        Ok(RunReport {
            events,
            ..RunReport::default()
        })
    }

    fn message(step: &Step, result: &Result<RunReport, EngineError>) -> String {
        step_entry(step, result).message
    }

    #[test]
    fn only_state_changing_commands_have_a_log_label() {
        assert_eq!(
            command_label(&Command::Optimize { yes: true }),
            Some("optimize")
        );
        assert_eq!(
            command_label(&Command::Restore { yes: false }),
            Some("restore")
        );
        for read_only in [Command::Verify, Command::ShowLog, Command::BackupSummary] {
            assert_eq!(command_label(&read_only), None);
        }
    }

    #[test]
    fn step_outcomes_are_classified_from_engine_events() {
        let step = catalog_step(false);
        let key = Progress::key_for(step.id);
        let applied = report(vec![
            Event::Apply(key.clone()),
            Event::Complete(key.clone()),
        ]);
        assert!(message(&step, &applied).ends_with(": applied"));
        let satisfied = report(vec![
            Event::Verify(key.clone()),
            Event::Complete(key.clone()),
        ]);
        assert!(message(&step, &satisfied).ends_with(": satisfied"));
        let skipped = report(vec![Event::Skip(key.clone())]);
        assert!(message(&step, &skipped).ends_with(": skipped"));
        let recorded = report(Vec::new());
        assert!(message(&step, &recorded).ends_with(": already recorded; not run again"));
        let advisory = report(vec![Event::Advisory {
            key: key.clone(),
            reason: "check manually".into(),
        }]);
        assert!(message(&step, &advisory).ends_with(": advisory (check manually)"));
        assert!(message(&step, &applied).starts_with(&format!("step {key} \"{}\"", step.title)));
    }

    #[test]
    fn check_only_steps_are_reported_as_inspect_only() {
        let step = catalog_step(true);
        let key = Progress::key_for(step.id);
        let completed = report(vec![Event::Verify(key.clone()), Event::Complete(key)]);
        assert!(message(&step, &completed).ends_with(": inspect-only"));
    }

    #[test]
    fn failed_steps_carry_the_error_text_at_error_level() {
        let step = catalog_step(false);
        let key = Progress::key_for(step.id);
        let failed = Err(EngineError::Apply {
            key,
            message: "access denied".into(),
        });
        let entry = step_entry(&step, &failed);
        assert_eq!(entry.level, "ERROR");
        assert!(entry.message.contains(": failed: "));
        assert!(entry.message.contains("access denied"));
    }

    #[test]
    fn command_records_name_the_command_and_failure() {
        assert_eq!(command_entry("optimize").message, "command optimize");
        let entry = command_failure_entry("optimize", &"boom");
        assert_eq!(entry.level, "ERROR");
        assert_eq!(entry.message, "command optimize failed: boom");
    }
}
