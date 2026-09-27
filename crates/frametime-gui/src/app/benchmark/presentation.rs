use super::*;

pub(in crate::app) fn visibility(app: &AppState) {
    let active = app.area == Area::Benchmark;
    for handle in app.fps.handles() {
        set_visible(handle, active);
    }
    if !active {
        return;
    }
    let c = app.fps;
    let history = app.fps_history;
    let stage = app.fps_session.stage();
    let import = !history && stage == FpsStage::Import;
    let evaluate = !history && stage == FpsStage::Evaluate;
    let review = !history && matches!(stage, FpsStage::Review | FpsStage::Save);
    let saved = !history && stage == FpsStage::Saved;
    let vrr = unsafe { SendMessageW(c.strategy, CB_GETCURSEL, None, None).0 == 1 };
    for handle in [
        app.action,
        app.secondary,
        app.tertiary,
        app.quaternary,
        app.quinary,
        app.cancel,
        app.fps_label,
        app.fps_input,
        app.min_label,
        app.min_input,
    ] {
        set_visible(handle, false);
    }
    for handle in [c.source, c.open, c.paste, app.vprof_input] {
        set_visible(handle, import);
    }
    for handle in [c.strategy_label, c.strategy] {
        set_visible(handle, evaluate);
    }
    for handle in [c.refresh_label, c.refresh, c.margin_label, c.margin] {
        set_visible(handle, evaluate && vrr);
    }
    for handle in [app.min_label, app.min_input] {
        set_visible(handle, evaluate && !vrr);
    }
    for handle in [c.label_label, c.label] {
        set_visible(handle, review);
    }
    set_visible(c.permission, review || saved);
    set_visible(c.group_left, !history);
    set_visible(c.group_right, !history && !import);
    set_visible(c.summary, !history);
    set_visible(
        c.back,
        !history && matches!(stage, FpsStage::Evaluate | FpsStage::Review),
    );
    set_visible(c.etw, history);
    set_visible(
        c.elevate,
        review && app.package.has_capability() && !is_elevated(),
    );
    set_visible(app.table, evaluate || history);
    set_visible(app.catalog_filter, history);
    set_visible(app.filter_label, history);
    let running = app.is_running();
    for handle in c
        .handles()
        .into_iter()
        .chain([app.vprof_input, app.min_input])
    {
        unsafe {
            let _ = EnableWindow(handle, !running);
        }
    }
    let can_continue = history
        || match stage {
            FpsStage::Import => {
                app.fps_session.capture().is_some()
                    || unsafe { GetWindowTextLengthW(app.vprof_input) > 0 }
            }
            FpsStage::Evaluate => true,
            FpsStage::Review => {
                app.package.has_capability() && is_elevated() && app.fps_session.failure().is_none()
            }
            FpsStage::Save => app.fps_session.failure().is_some(),
            FpsStage::Saved => true,
        };
    unsafe {
        let _ = EnableWindow(c.next, !running && can_continue);
    }
}

pub(in crate::app) fn render(window: HWND) {
    let _ = with_state(window, |app| {
        if app.area != Area::Benchmark {
            return;
        }
        visibility(app);
        let stage = app.fps_session.stage();
        let current = match stage {
            FpsStage::Import => 0,
            FpsStage::Evaluate => 1,
            FpsStage::Review => 2,
            FpsStage::Save | FpsStage::Saved => 3,
        };
        for (index, (handle, label)) in app
            .fps
            .steps
            .iter()
            .zip(["Import", "Evaluate", "Review", "Save"])
            .enumerate()
        {
            let suffix = if index == current { "  [current]" } else { "" };
            set_text(*handle, &format!("{}  {label}{suffix}", index + 1));
        }
        set_text(
            app.fps.source,
            &format!("Selected source: {}", app.fps_source),
        );
        set_text(
            app.fps.history,
            if app.fps_history {
                "< FPS strategy"
            } else {
                "&History / diagnostics"
            },
        );
        if app.fps_history {
            set_text(app.heading, "Benchmark history and diagnostics");
            set_text(
                app.description,
                "Saved observations and read-only ETW capture. Your FPS calculation remains available.",
            );
            set_text(app.fps.next, "Return to FPS strategy");
            return;
        }
        set_text(
            app.fps.group_left,
            match stage {
                FpsStage::Import => "Selected source",
                FpsStage::Evaluate => "Run results",
                _ => "Selected cap and evidence",
            },
        );
        set_text(
            app.fps.group_right,
            if stage == FpsStage::Evaluate {
                "Strategy and calculation"
            } else {
                "Permissions and state"
            },
        );
        let (title, detail, next) = stage_copy(app);
        set_text(app.heading, &title);
        set_text(app.description, detail);
        set_text(app.fps.next, next);
        set_text(app.fps.summary, &summary(app));
        set_text(app.fps.permission, &permission(app));
        if stage == FpsStage::Evaluate {
            evidence_table(app);
        }
        tab_order(app);
    });
    unsafe {
        let _ = InvalidateRect(Some(window), None, false);
    }
    layout(window);
}

fn stage_copy(app: &AppState) -> (String, &'static str, &'static str) {
    match app.fps_session.stage() {
        FpsStage::Import => (
            "Bring your CS2 results".into(),
            "Import VProf output from at least five runs to evaluate a nonzero cap. Choose a file or paste complete results below.",
            "&Evaluate capture >",
        ),
        FpsStage::Evaluate => {
            let title = app.fps_session.outcome().map_or_else(
                || "Evaluate a frame-rate ceiling".into(),
                |outcome| format!("{} FPS is supported", outcome.cap),
            );
            let next = if app.fps_session.outcome().is_some() {
                "&Review save >"
            } else {
                "&Evaluate strategy"
            };
            (
                title,
                "Every unrounded P1 must support a nonzero cap. Imported evidence does not predict a performance gain.",
                next,
            )
        }
        FpsStage::Review => (
            "Review before saving".into(),
            "Save the selected cap and its run evidence to frametime. CS2 configuration files will not change.",
            "&Save cap and evidence",
        ),
        FpsStage::Save if app.fps_session.failure().is_some() => (
            "Save may be incomplete".into(),
            "Inspect saved history and state before starting another calculation. A failed save may already have changed cap state.",
            "New calculation",
        ),
        FpsStage::Save => (
            "Saving cap and evidence".into(),
            "Waiting for state and history readback. Keep this window open until the operation reports its result.",
            "Saving...",
        ),
        FpsStage::Saved => (
            "Cap and evidence saved".into(),
            "Saved to frametime. Game configuration is unchanged.",
            "&Done",
        ),
    }
}

fn summary(app: &AppState) -> String {
    if let Some(error) = app.fps_session.failure() {
        return format!("Unable to complete\r\n\r\n{error}");
    }
    let Some(capture) = app.fps_session.capture() else {
        return "UTF-8 text - Up to 8 MiB / 1,000 accepted runs. Nothing saved.".into();
    };
    let aggregate = capture.aggregate();
    if app.fps_session.stage() == FpsStage::Import {
        return format!("{} accepted runs loaded. Nothing saved.", aggregate.runs);
    }
    let (minimum, maximum) = capture.p1_range();
    let data = format!(
        "{} runs retained\r\nAverage FPS: {:.1}\r\nMean P1 FPS: {:.1}\r\nP1 range: {minimum:.1}-{maximum:.1} FPS",
        aggregate.runs, aggregate.average_fps, aggregate.p1_fps
    );
    match app.fps_session.outcome() {
        Some(outcome) => format!(
            "{} FPS\r\n\r\n{data}\r\n\r\n{} supporting - {} failing",
            outcome.cap,
            capture.supporting_runs(outcome.cap),
            capture.failing_runs(outcome.cap)
        ),
        None => format!("{data}\r\n\r\nChoose a strategy, then evaluate."),
    }
}

fn permission(app: &AppState) -> String {
    if app.fps_session.stage() == FpsStage::Saved {
        return "Saved state and history read back successfully.\r\n\r\nThe selected cap and accepted run evidence are retained.\r\n\r\nGame configuration is unchanged.".into();
    }
    let package = if app.package.has_capability() {
        "Authenticated Windows package".to_owned()
    } else {
        app.package.unavailable_detail()
    };
    let privilege = if is_elevated() {
        "Administrator access confirmed"
    } else {
        "Administrator access required. Opening an elevated window starts a separate session; import the source there before saving."
    };
    format!(
        "Permissions and state\r\n\r\n{package}\r\n\r\n{privilege}\r\n\r\nSaving updates cap state and benchmark history. A completed final-benchmark receipt cannot be replaced."
    )
}

fn evidence_table(app: &AppState) {
    let rows = app
        .fps_session
        .capture()
        .map(|capture| {
            capture
                .observations()
                .iter()
                .enumerate()
                .map(|(index, run)| {
                    (
                        format!("{:02}", index + 1),
                        run.average_fps.to_string(),
                        run.p1_fps.to_string(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let borrowed = rows
        .iter()
        .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
        .collect::<Vec<_>>();
    populate_table(app.table, &borrowed);
}

fn tab_order(app: &AppState) {
    let c = app.fps;
    let mut handles = app.nav.to_vec();
    handles.extend([
        c.open,
        c.paste,
        app.vprof_input,
        c.strategy_label,
        c.strategy,
        app.min_label,
        app.min_input,
        c.refresh_label,
        c.refresh,
        c.margin_label,
        c.margin,
        app.table,
        c.summary,
        c.permission,
        c.label_label,
        c.label,
        c.elevate,
        c.history,
        c.etw,
        c.back,
        c.next,
    ]);
    let mut previous = HWND_TOP;
    for handle in handles {
        if unsafe { IsWindowVisible(handle) }.as_bool() {
            unsafe {
                let _ = SetWindowPos(
                    handle,
                    Some(previous),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            previous = handle;
        }
    }
}
