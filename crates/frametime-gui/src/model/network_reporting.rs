pub fn format_network_apply_report(
    report: &frametime_domain::engine::RunReport,
) -> Result<String, String> {
    let final_event = report
        .events
        .last()
        .map(network_event_summary)
        .unwrap_or_else(|| "no engine event was recorded".into());
    let detail = format!(
        "Ethernet RSS report: {} completed, {} skipped, {} advisories, {} failed; final event: {final_event}.",
        report.completed, report.skipped, report.advisories, report.failed
    );
    if report.skipped != 0 || report.advisories != 0 || report.failed != 0 {
        Err(detail)
    } else {
        Ok(detail)
    }
}
fn network_event_summary(event: &frametime_domain::engine::Event) -> String {
    use frametime_domain::engine::Event;
    match event {
        Event::Advisory { key, reason } => format!("advisory {key}: {reason}"),
        Event::Inspect(key) => format!("inspected {key}"),
        Event::CaptureBackup(key) => format!("captured backup for {key}"),
        Event::PersistBackup(key) => format!("persisted backup for {key}"),
        Event::CaptureAudit(key) => format!("captured audit for {key}"),
        Event::PersistAudit(key) => format!("persisted audit for {key}"),
        Event::CaptureEvidence(key) => format!("captured evidence for {key}"),
        Event::PersistEvidence(key) => format!("persisted evidence for {key}"),
        Event::VerifyEvidence(key) => format!("verified evidence for {key}"),
        Event::Apply(key) => format!("applied {key}"),
        Event::Verify(key) => format!("verified {key}"),
        Event::FinalizeAudit(key) => format!("finalized audit for {key}"),
        Event::FailAudit(key) => format!("recorded failed audit for {key}"),
        Event::Complete(key) => format!("completed {key}"),
        Event::Skip(key) => format!("skipped {key}"),
        Event::Plan(key) => format!("planned {key}"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_preserves_counts_and_final_event() {
        let report = frametime_domain::engine::RunReport {
            events: vec![frametime_domain::engine::Event::Complete("P1:16".into())],
            completed: 1,
            ..Default::default()
        };
        let detail = format_network_apply_report(&report).expect("clean report");
        assert!(detail.contains("1 completed"));
        assert!(detail.contains("completed P1:16"));
    }
    #[test]
    fn partial_reports_are_warnings() {
        let report = frametime_domain::engine::RunReport {
            events: vec![frametime_domain::engine::Event::Skip("P1:16".into())],
            skipped: 1,
            ..Default::default()
        };
        let detail = format_network_apply_report(&report).expect_err("partial report");
        assert!(detail.contains("1 skipped"));
        assert!(detail.contains("skipped P1:16"));
    }
}
