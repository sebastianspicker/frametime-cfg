use super::*;

pub(super) fn accelerators() -> windows::core::Result<HACCEL> {
    let mut entries = Area::ALL
        .iter()
        .enumerate()
        .map(|(index, _)| ACCEL {
            fVirt: FCONTROL | FVIRTKEY,
            key: b'1' as u16 + index as u16,
            cmd: (NAV_BASE + index) as u16,
        })
        .collect::<Vec<_>>();
    entries.push(ACCEL {
        fVirt: FVIRTKEY,
        key: VK_ESCAPE.0,
        cmd: CANCEL as u16,
    });
    entries.push(ACCEL {
        fVirt: FVIRTKEY,
        key: VK_F6.0,
        cmd: FOCUS_RESTORE as u16,
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
    if area == Area::Video {
        refresh_video_preview(window);
    } else {
        render_catalog(window, area);
    }
    layout(window);
}

fn update_area_controls(app: &mut AppState, area: Area, authenticated: bool) {
    let show_primary_action = area != Area::SetupVerify || authenticated;
    unsafe {
        let _ = ShowWindow(
            app.action,
            if show_primary_action {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
    }
    let (secondary, tertiary, quaternary, quinary) = area_secondary_actions(area, authenticated);
    for (handle, label) in [
        (app.secondary, secondary),
        (app.tertiary, tertiary),
        (app.quaternary, quaternary),
        (app.quinary, quinary),
    ] {
        set_text(handle, label);
        unsafe {
            let _ = ShowWindow(handle, if label.is_empty() { SW_HIDE } else { SW_SHOW });
        }
    }
    unsafe {
        let _ = EnableWindow(
            app.secondary,
            area != Area::Video || app.video_preview.apply_available(),
        );
    }
    let show_benchmark = area == Area::Benchmark;
    for handle in [
        app.fps_label,
        app.fps_input,
        app.min_label,
        app.min_input,
        app.vprof_input,
    ] {
        unsafe {
            let _ = ShowWindow(handle, if show_benchmark { SW_SHOW } else { SW_HIDE });
        }
    }
    let show_setup = area == Area::SetupVerify && authenticated;
    for handle in [app.profile, app.dry_run] {
        unsafe {
            let _ = ShowWindow(handle, if show_setup { SW_SHOW } else { SW_HIDE });
        }
    }
    let show_video = area == Area::Video;
    for handle in [
        app.video_root_label,
        app.video_root,
        app.video_tier_label,
        app.video_tier,
    ] {
        unsafe {
            let _ = ShowWindow(handle, if show_video { SW_SHOW } else { SW_HIDE });
        }
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
        Area::Video => ("Apply 13-setting preset", "", "", ""),
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
