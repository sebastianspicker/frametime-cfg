use super::*;
use model::snapshots::{ReadRequest, Resource, Rows};

pub(super) fn resource_for_area(area: Area) -> Option<Resource> {
    match area {
        Area::Overview => Some(Resource::Overview),
        Area::Benchmark => Some(Resource::History),
        Area::Recovery => Some(Resource::Recovery),
        Area::Video => Some(Resource::Video),
        Area::Drivers => Some(Resource::Drivers),
        _ => None,
    }
}
pub(super) fn refresh_overview(window: HWND) {
    refresh_area_data(window, Area::Overview);
}

pub(super) fn refresh_area_data(window: HWND, area: Area) {
    let _ = with_state(window, |app| {
        if let Some(resource) = resource_for_area(area) {
            let mut request = ReadRequest::new(resource);
            if resource == Resource::Video {
                request.video = Some((
                    control_text(app.video_root).trim().into(),
                    core_video_goal(selected_video_tier(app.video_tier)),
                ));
            }
            app.reads.request(request);
        }
    });
    if with_state(window, |app| app.area == area).unwrap_or(false) {
        render_catalog(window, area);
    }
}

pub(super) fn invalidate_after_operation(window: HWND) {
    let area = with_state(window, |app| {
        app.reads.invalidate();
        app.area
    });
    if let Some(area) = area {
        refresh_area_data(window, area);
    }
}

pub(super) fn poll_reads(window: HWND) {
    let area = with_state(window, |app| {
        let changed = app.reads.poll();
        resource_for_area(app.area)
            .filter(|resource| changed.contains(resource))
            .map(|_| app.area)
    })
    .flatten();
    if let Some(area) = area {
        render_catalog(window, area);
    }
}

/// Rendering and filtering only inspect cached presentation values.
pub(super) fn render_catalog(window: HWND, area: Area) {
    if area == Area::Benchmark && with_state(window, |app| !app.fps_history).unwrap_or(false) {
        benchmark::render(window);
        return;
    }
    let Some((table, rows)) = with_state(window, |app| {
        let filter = control_text(app.catalog_filter);
        let mut rows = if let Some(resource) = resource_for_area(area) {
            app.reads
                .snapshot(resource)
                .map(|snapshot| snapshot.filtered(&filter))
                .unwrap_or_default()
        } else {
            area.table_rows()
                .iter()
                .filter(|(a, b, _)| model::catalog_row_matches_filter(a, b, &filter))
                .map(|(a, b, c)| ((*a).into(), (*b).into(), (*c).into()))
                .collect::<Rows>()
        };
        if (area == Area::Assess && app.diagnostics.belongs_to_assess())
            || (area == Area::Benchmark && app.diagnostics.belongs_to_benchmark())
        {
            rows.extend(
                app.diagnostics
                    .rows
                    .iter()
                    .filter(|row| model::catalog_row_matches_filter(&row.item, &row.value, &filter))
                    .map(|row| (row.item.clone(), row.value.clone(), row.state.clone())),
            );
        }
        if area == Area::Benchmark {
            rows.extend(
                app.benchmark_preview
                    .iter()
                    .filter(|(a, b, _)| model::catalog_row_matches_filter(a, b, &filter))
                    .cloned(),
            );
        }
        (app.table, rows)
    }) else {
        return;
    };
    let borrowed = rows
        .iter()
        .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
        .collect::<Vec<_>>();
    populate_table(table, &borrowed);
}

pub(super) fn refresh_catalog_filter(window: HWND) {
    if let Some(area) = with_state(window, |app| app.area) {
        render_catalog(window, area);
    }
    let _ = with_state(window, |app| app.last_focus = app.catalog_filter);
}
