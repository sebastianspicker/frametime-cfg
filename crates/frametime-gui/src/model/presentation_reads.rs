use super::snapshots::{ReadRequest, ReadSnapshot, Resource, Rows};

pub fn read_presentation(request: ReadRequest) -> Result<ReadSnapshot, String> {
    match request.resource {
        Resource::Overview => frametime_app::read_overview()
            .map(overview_rows)
            .map(Into::into)
            .map_err(|e| e.to_string()),
        Resource::History => frametime_app::read_benchmark_history()
            .map(benchmark_rows)
            .map(Into::into)
            .map_err(|e| e.to_string()),
        Resource::Recovery => frametime_app::read_recovery()
            .map(recovery_snapshot)
            .map_err(|e| e.to_string()),
        Resource::Drivers => frametime_app::run_driver_inspect()
            .map(|outcome| {
                ReadSnapshot::from(vec![(
                    "Driver lifecycle".into(),
                    "Inspection".into(),
                    outcome.json,
                )])
            })
            .map_err(|e| e.to_string()),
        Resource::Video => {
            let (root, goal) = request.video.ok_or("Video discovery requires inputs")?;
            if root.is_empty() {
                return Err("Steam root is required for trusted read-only discovery.".into());
            }
            let preview = frametime_app::preview_video(std::path::Path::new(&root), goal);
            if preview.rows.is_empty() {
                return Err(preview.discovery);
            }
            let mut rows = vec![(
                "Video discovery".into(),
                "Read-only".into(),
                preview.discovery,
            )];
            rows.extend(preview.rows.into_iter().map(|row| {
                (
                    row.setting,
                    row.current_and_recommended,
                    row.status_and_note,
                )
            }));
            Ok(rows.into())
        }
    }
}

fn recovery_snapshot(backup: frametime_domain::backup::BackupFile) -> ReadSnapshot {
    let mut snapshot = ReadSnapshot::default();
    for (index, entry) in backup.entries.iter().enumerate() {
        if let Some(step) = entry.step() {
            snapshot.recovery_keys.insert(index, step.to_owned());
        }
        snapshot.rows.push((
            entry.step().unwrap_or("Unknown recovery record").into(),
            format!("Entry {}", index + 1),
            if entry.step().is_some() {
                "Select a row before Restore selected."
            } else {
                "Unrecognized evidence has no restore selector."
            }
            .into(),
        ));
    }
    snapshot
}

fn overview_rows(snapshot: frametime_app::OverviewReadModel) -> Rows {
    let progress = snapshot.progress;
    let state = snapshot.state;
    let phase_count = |phase| progress.resolved_count_in_phase(phase);
    let phase_total = |phase| {
        frametime_domain::catalog::step_catalog()
            .iter()
            .filter(|step| step.id.phase == phase)
            .count()
    };
    let history = snapshot.history;
    let latest = history.last().map_or_else(
        || "No saved captures".to_owned(),
        |record| {
            format!(
                "{} avg {:.1}, P1 {:.1}",
                record.label, record.avg_fps, record.p1_fps
            )
        },
    );
    vec![
        (
            "Phase 1".into(),
            format!(
                "{} / {}",
                phase_count(frametime_domain::catalog::Phase::One),
                phase_total(frametime_domain::catalog::Phase::One)
            ),
            "completed or skipped".into(),
        ),
        (
            "Phase 2".into(),
            format!(
                "{} / {}",
                phase_count(frametime_domain::catalog::Phase::Two),
                phase_total(frametime_domain::catalog::Phase::Two)
            ),
            "Safe Mode phase blocked in GUI".into(),
        ),
        (
            "Phase 3".into(),
            format!(
                "{} / {}",
                phase_count(frametime_domain::catalog::Phase::Three),
                phase_total(frametime_domain::catalog::Phase::Three)
            ),
            "completed or skipped".into(),
        ),
        (
            "Profile preference".into(),
            format!("{:?}", state.profile),
            format!("{} mode", state.mode),
        ),
        (
            "Benchmark history".into(),
            format!("{} / 200", history.len()),
            latest,
        ),
    ]
}

fn benchmark_rows(history: Vec<frametime_domain::benchmark::BenchmarkRecord>) -> Rows {
    history
        .iter()
        .rev()
        .take(200)
        .map(|record| {
            (
                record.label.clone(),
                format!("Avg {:.1} / P1 {:.1}", record.avg_fps, record.p1_fps),
                format!("{} run(s), {}", record.runs, record.timestamp),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_recovery_evidence_is_visible_without_a_selector() {
        let snapshot = recovery_snapshot(frametime_domain::backup::BackupFile {
            entries: vec![frametime_domain::backup::BackupEntry::Unknown(
                Default::default(),
            )],
            created: String::new(),
            unknown: Default::default(),
        });
        assert_eq!(snapshot.rows.len(), 1);
        assert!(snapshot.recovery_keys.is_empty());
    }
    #[test]
    fn empty_history_and_missing_video_inputs_are_distinct() {
        assert!(benchmark_rows(Vec::new()).is_empty());
        assert_eq!(
            overview_rows(frametime_app::OverviewReadModel::default()).len(),
            5
        );
        assert!(read_presentation(ReadRequest::new(Resource::Video)).is_err());
        let request = ReadRequest::new(Resource::Drivers);
        assert_eq!(request.resource, Resource::Drivers);
    }
}
