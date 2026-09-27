use super::*;

pub(super) fn accelerators() -> windows::core::Result<HACCEL> {
    let mut entries = Area::ALL
        .iter()
        .enumerate()
        .map(|(index, _)| ACCEL {
            fVirt: FCONTROL | FVIRTKEY,
            key: u16::from(b'1')
                + u16::try_from(index).expect("nine fixed navigation accelerators fit in u16"),
            cmd: u16::try_from(NAV_BASE + index).expect("fixed navigation control IDs fit in u16"),
        })
        .collect::<Vec<_>>();
    entries.push(ACCEL {
        fVirt: FVIRTKEY,
        key: VK_F5.0,
        cmd: u16::try_from(REFRESH).expect("fixed refresh control ID fits in u16"),
    });
    entries.push(ACCEL {
        fVirt: FVIRTKEY,
        key: VK_ESCAPE.0,
        cmd: u16::try_from(CANCEL).expect("fixed cancel control ID fits in u16"),
    });
    entries.push(ACCEL {
        fVirt: FVIRTKEY,
        key: VK_F6.0,
        cmd: u16::try_from(FOCUS_RESTORE).expect("fixed focus control ID fits in u16"),
    });
    unsafe { CreateAcceleratorTableW(&entries) }
}

pub(super) fn update_area(window: HWND, area: Area) {
    let Some((heading, action, detail, kind, authenticated)) = with_state(window, |app| {
        app.area = area;
        set_text(app.heading, area.title());
        set_text(app.description, area.description());
        set_text(app.action, area.action_label());
        let authenticated = app.package.has_capability();
        update_area_controls(app, area, authenticated);
        (
            app.heading,
            app.action,
            app.operation.detail.clone(),
            app.operation.status,
            authenticated,
        )
    }) else {
        return;
    };
    let detail = if safe_mode_active() {
        "Safe Mode detected: the graphical application is prohibited. Use the documented native recovery CLI from a normal Windows session.".to_owned()
    } else {
        detail
    };
    let kind = if safe_mode_active() {
        StatusKind::Warning
    } else {
        kind
    };
    update_status(window, kind, &detail);
    unsafe {
        let _ = SetFocus(Some(if area == Area::SetupVerify && !authenticated {
            heading
        } else {
            action
        }));
        NotifyWinEvent(EVENT_OBJECT_NAMECHANGE, heading, OBJID_CLIENT.0, 0);
    }
    refresh_area_data(window, area);
    if area == Area::Benchmark {
        benchmark::render(window);
    }
    layout(window);
}

fn update_area_controls(app: &mut AppState, area: Area, authenticated: bool) {
    set_visible(app.action, area != Area::SetupVerify || authenticated);
    update_secondary_action_controls(app, area, authenticated);
    set_visible_all(benchmark_controls(app), area == Area::Benchmark);
    set_visible_all(
        setup_controls(app),
        area == Area::SetupVerify && authenticated,
    );
    set_visible_all(video_controls(app), area == Area::Video);
    set_visible_all(cs2_cfg_controls(app), area == Area::Cs2Cfg);
    set_visible(app.table, true);
    set_visible(app.filter_label, true);
    set_visible(app.catalog_filter, true);
    benchmark::visibility(app);
}

fn update_secondary_action_controls(app: &AppState, area: Area, authenticated: bool) {
    let labels = area_secondary_actions(area, authenticated);
    for (handle, label) in [
        (app.secondary, labels.0),
        (app.tertiary, labels.1),
        (app.quaternary, labels.2),
        (app.quinary, labels.3),
    ] {
        set_text(handle, label);
        set_visible(handle, !label.is_empty());
    }
    unsafe {
        let _ = EnableWindow(app.secondary, area != Area::Video);
    }
}

fn benchmark_controls(app: &AppState) -> [HWND; 5] {
    [
        app.fps_label,
        app.fps_input,
        app.min_label,
        app.min_input,
        app.vprof_input,
    ]
}
fn setup_controls(app: &AppState) -> [HWND; 2] {
    [app.profile, app.dry_run]
}
fn video_controls(app: &AppState) -> [HWND; 4] {
    [
        app.video_root_label,
        app.video_root,
        app.video_tier_label,
        app.video_tier,
    ]
}
fn cs2_cfg_controls(app: &AppState) -> [HWND; 2] {
    [app.cs2_cfg_asset_label, app.cs2_cfg_asset]
}

pub(super) fn set_visible_all(handles: impl IntoIterator<Item = HWND>, visible: bool) {
    for handle in handles {
        set_visible(handle, visible);
    }
}

pub(super) fn set_visible(handle: HWND, visible: bool) {
    unsafe {
        let _ = ShowWindow(handle, if visible { SW_SHOW } else { SW_HIDE });
    }
}

pub(super) fn area_secondary_actions(
    area: Area,
    authenticated: bool,
) -> (&'static str, &'static str, &'static str, &'static str) {
    match area {
        Area::Overview => ("Assess settings", "Open recovery", "", ""),
        Area::Assess => (
            "CPU identity",
            "GPU inventory",
            "System status",
            "Read WHEA events",
        ),
        Area::SetupVerify if authenticated => ("Start / resume Phase 1", "Verify state", "", ""),
        Area::SetupVerify => ("", "", "", ""),
        Area::Benchmark if authenticated => (
            "Parse VProf",
            "Persist VProf capture",
            "Capture 5s ETW frames",
            "",
        ),
        Area::Benchmark => ("Parse VProf", "", "Capture 5s ETW frames", ""),
        Area::Recovery => ("Restore all", "Restore selected", "Clear backups", ""),
        Area::Drivers => ("", "", "", ""),
        Area::Video => ("", "", "", ""),
        Area::Cs2Cfg => ("", "", "", ""),
        Area::Network => ("", "", "", ""),
    }
}

pub(super) fn update_status(window: HWND, kind: StatusKind, detail: &str) {
    let _ = with_state(window, |app| {
        app.operation = OperationState {
            status: kind,
            detail: detail.into(),
            cancellable: app.is_cancellable(),
        };
        set_text(app.status, &format!("{}: {detail}", kind.text()));
        unsafe {
            let _ = EnableWindow(app.cancel, app.is_cancellable());
            NotifyWinEvent(EVENT_OBJECT_VALUECHANGE, app.status, OBJID_CLIENT.0, 0);
            let _ = InvalidateRect(Some(app.status), None, true);
        }
    });
}
