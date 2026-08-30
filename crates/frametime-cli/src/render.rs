use frametime_app::{
    BackupSummary, BenchmarkOutcome, BenchmarkPersistence, CleanupSummary, CommandOutcome,
    DryRunSummary, FpsCapOutcome, FpsCapStrategyLabel, HardwareDiagnosticOutcome, RunSummary,
    VerificationSummary,
};

pub(crate) fn dry_run(summary: DryRunSummary) {
    render_lines(&summary.lines);
}
pub(crate) fn driver_json(json: String) {
    println!("{json}");
}
pub(crate) fn fps_cap(outcome: FpsCapOutcome) {
    println!("Selected fps_max: {}", outcome.cap);
    println!(
        "Strategy: {}",
        match outcome.strategy {
            FpsCapStrategyLabel::RawUncapped => "raw latency, uncapped",
            FpsCapStrategyLabel::RawMeasured => "raw latency, measured cap",
            FpsCapStrategyLabel::VrrCeiling => "VRR ceiling",
        }
    );
    println!(
        "Average FPS: {:.1}; P1 FPS: {:.1}; P1 ratio: {}; Runs: {}",
        outcome.capture.average_fps,
        outcome.capture.p1_fps,
        outcome
            .capture
            .p1_ratio()
            .map_or_else(|| "n/a".into(), |ratio| format!("{ratio:.3}")),
        outcome.capture.runs
    );
    if outcome.copied_to_clipboard {
        println!("Copied fps_max to the native clipboard.");
    }
    match outcome.persistence {
        BenchmarkPersistence::Persisted => {
            println!("Persisted benchmark state and history in C:\\FRAMETIME_CFG.")
        }
        BenchmarkPersistence::Disabled => println!("Persistence disabled by --no-persist."),
        BenchmarkPersistence::UnsupportedHost => {
            println!("Not persisted: this host is not Windows.")
        }
    }
}
pub(crate) fn benchmark(outcome: BenchmarkOutcome, baseline: bool) {
    if let Some(receipt) = outcome.receipt {
        println!(
            "Persisted After all optimizations: Avg {:.1}; P1 {:.1}; Runs: {}; fps_max {}.",
            receipt.avg_fps, receipt.p1_fps, receipt.runs, receipt.fps_cap
        );
        println!("Final benchmark receipt: {}.", receipt.receipt_id);
    } else if baseline {
        println!(
            "Persisted Baseline (before optimizations): Avg {:.1}; P1 {:.1}; Runs: {}.",
            outcome.capture.average_fps, outcome.capture.p1_fps, outcome.capture.runs
        );
    }
}
pub(crate) fn hardware(outcome: HardwareDiagnosticOutcome) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(&outcome.envelope).map_err(|error| error.to_string())?
    );
    Ok(())
}
pub(crate) fn command(outcome: CommandOutcome) -> Result<(), String> {
    match outcome {
        CommandOutcome::Run(summary) => render_run(summary),
        CommandOutcome::Cleanup(summary) => cleanup(summary),
        CommandOutcome::Verification(summary) => verification(*summary)?,
        CommandOutcome::BackupSummary(summary) => backup_summary(summary),
        CommandOutcome::Log(log) => print!("{}", log.text),
    }
    Ok(())
}
fn render_run(summary: RunSummary) {
    render_lines(&summary.messages);
}
fn cleanup(summary: CleanupSummary) {
    for result in &summary.report.action_results {
        let line = match &result.outcome {
            frametime_domain::CleanupActionOutcome::Completed { affected_items } => {
                format!("COMPLETED: {:?} ({affected_items} items)", result.action)
            }
            frametime_domain::CleanupActionOutcome::Inapplicable { reason } => {
                format!("INAPPLICABLE: {:?}: {reason}", result.action)
            }
            frametime_domain::CleanupActionOutcome::Deferred { reason } => {
                format!("DEFERRED: {:?}: {reason}", result.action)
            }
            frametime_domain::CleanupActionOutcome::Skipped { reason } => {
                format!("SKIPPED: {:?}: {reason}", result.action)
            }
            frametime_domain::CleanupActionOutcome::Failed { reason } => {
                eprintln!("FAILED: {:?}: {reason}", result.action);
                continue;
            }
        };
        println!("{line}");
    }
    println!("Affected items: {}", summary.report.affected_items());
    if summary.report.restart_required {
        println!("Restart required: yes (Winsock catalog reset).");
    }
}
fn verification(summary: VerificationSummary) -> Result<(), String> {
    let items = summary.report.items.iter().map(|item| serde_json::json!({ "status": item.status.label(), "name": item.name, "detail": item.detail })).collect::<Vec<_>>();
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "readOnly": true, "workDir": summary.work_dir, "state": summary.state, "progress": summary.progress, "items": items })).map_err(|error| error.to_string())?);
    Ok(())
}
fn backup_summary(summary: BackupSummary) {
    if summary.entries.is_empty() {
        println!("No backup entries found.");
    } else {
        for entry in summary.entries {
            println!("{}: {}", entry.kind, entry.count);
        }
    }
}
fn render_lines(lines: &[String]) {
    for line in lines {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_accepts_a_typed_run_summary() {
        render_run(RunSummary::message("typed renderer input"));
    }

    #[test]
    fn renderer_accepts_an_empty_backup_read_model() {
        backup_summary(BackupSummary {
            entries: Vec::new(),
        });
    }
}
