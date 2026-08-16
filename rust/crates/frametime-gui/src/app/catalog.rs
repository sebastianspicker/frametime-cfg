use super::*;

pub(super) fn refresh_overview(window: HWND) {
    refresh_area_data(window, Area::Overview);
    update_status(
        window,
        StatusKind::Complete,
        "Work directory state refreshed. This inspection does not modify files.",
    );
}

pub(super) fn refresh_area_data(window: HWND, area: Area) {
    render_catalog(window, area);
}

pub(super) fn catalog_rows(
    area: Area,
    video_preview: &model::VideoPreview,
    diagnostics: &DiagnosticPresentation,
) -> Vec<(String, String, String)> {
    match area {
        Area::Overview => overview_rows(),
        Area::Assess if diagnostics.belongs_to_assess() => diagnostic_rows(diagnostics),
        Area::Benchmark => {
            let mut rows = if diagnostics.belongs_to_benchmark() {
                diagnostic_rows(diagnostics)
            } else {
                Vec::new()
            };
            rows.extend(benchmark_rows());
            rows
        }
        Area::Recovery => recovery_rows(),
        Area::Video => video_preview
            .rows
            .iter()
            .map(|row| {
                (
                    row.setting.clone(),
                    row.current_and_recommended.clone(),
                    row.status_and_note.clone(),
                )
            })
            .collect(),
        _ => area
            .table_rows()
            .iter()
            .map(|(category, status, detail)| {
                ((*category).into(), (*status).into(), (*detail).into())
            })
            .collect(),
    }
}

pub(super) fn render_catalog(window: HWND, area: Area) {
    let Some((table, filter, video_preview, diagnostics)) = with_state(window, |app| {
        (
            app.table,
            control_text(app.catalog_filter),
            app.video_preview.clone(),
            app.diagnostics.clone(),
        )
    }) else {
        return;
    };
    let rows = catalog_rows(area, &video_preview, &diagnostics)
        .into_iter()
        .filter(|(category, status, _)| {
            model::catalog_row_matches_filter(category, status, &filter)
        })
        .collect::<Vec<_>>();
    let borrowed = rows
        .iter()
        .map(|(first, second, third)| (first.as_str(), second.as_str(), third.as_str()))
        .collect::<Vec<_>>();
    populate_table(table, &borrowed);
}

fn diagnostic_rows(diagnostics: &DiagnosticPresentation) -> Vec<(String, String, String)> {
    diagnostics
        .rows
        .iter()
        .map(|row| (row.item.clone(), row.value.clone(), row.state.clone()))
        .collect()
}

pub(super) fn refresh_catalog_filter(window: HWND) {
    let Some(area) = with_state(window, |app| app.area) else {
        return;
    };
    render_catalog(window, area);
    let _ = with_state(window, |app| {
        // Filtering must not steal focus from the standard EDIT control.
        app.last_focus = app.catalog_filter;
    });
}

pub(super) fn overview_rows() -> Vec<(String, String, String)> {
    let work_dir = Path::new(WORK_DIR);
    let progress = frametime_core::persistence::read_json_tolerant::<frametime_core::Progress>(
        &work_dir.join("progress.json"),
    )
    .unwrap_or_default();
    let state = frametime_core::persistence::read_json_tolerant::<frametime_core::State>(
        &work_dir.join("state.json"),
    )
    .unwrap_or_default();
    let phase_count = |phase: u8| {
        progress
            .completed_steps
            .iter()
            .chain(progress.skipped_steps.iter())
            .filter(|key| key.starts_with(&format!("P{phase}:")))
            .count()
    };
    let history = frametime_core::benchmark::load_benchmark_history(
        &work_dir.join("benchmark_history.json"),
        false,
    );
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
            format!("{} / 38", phase_count(1)),
            "completed or skipped".into(),
        ),
        (
            "Phase 2".into(),
            format!("{} / 3", phase_count(2)),
            "Safe Mode phase blocked in GUI".into(),
        ),
        (
            "Phase 3".into(),
            format!("{} / 13", phase_count(3)),
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

pub(super) fn benchmark_rows() -> Vec<(String, String, String)> {
    let history = frametime_core::benchmark::load_benchmark_history(
        &Path::new(WORK_DIR).join("benchmark_history.json"),
        false,
    );
    if history.is_empty() {
        return vec![(
            "History".into(),
            "0 / 200".into(),
            "Add a valid VProf capture to create history.".into(),
        )];
    }
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

pub(super) fn recovery_rows() -> Vec<(String, String, String)> {
    let path = Path::new(WORK_DIR).join("backup.json");
    match frametime_core::persistence::read_json_tolerant::<frametime_core::BackupFile>(&path) {
        Ok(backup) if backup.entries.is_empty() => vec![(
            "Backup grid".into(),
            "0 retained entries".into(),
            "No recovery record is available.".into(),
        )],
        Ok(backup) => backup
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                (
                    entry.step().unwrap_or("Unknown recovery record").to_owned(),
                    format!("Entry {}", index + 1),
                    "Select a row before Restore selected.".into(),
                )
            })
            .collect(),
        Err(_) => vec![(
            "Backup grid".into(),
            "Unavailable".into(),
            "backup.json is absent or malformed; no restore action is started.".into(),
        )],
    }
}
