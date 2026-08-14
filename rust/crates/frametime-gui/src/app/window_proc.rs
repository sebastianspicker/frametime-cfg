use super::*;

pub(super) unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_COMMAND => handle_command(window, wparam),
        WM_TIMER if wparam.0 == POLL_TIMER => handle_poll_timer(window),
        WM_SIZE => handle_size(window),
        WM_DPICHANGED => handle_dpi_changed(window, lparam),
        WM_SETTINGCHANGE => handle_setting_change(window),
        WM_GETMINMAXINFO => handle_min_max_info(window, lparam),
        WM_KEYDOWN => handle_key_down(window, message, wparam, lparam),
        WM_SETFOCUS => handle_set_focus(window),
        WM_CTLCOLORSTATIC => handle_static_color(window, message, wparam, lparam),
        WM_CLOSE => handle_close(window),
        WM_DESTROY => handle_destroy(window),
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn handle_command(window: HWND, wparam: WPARAM) -> LRESULT {
    let _ = with_state(window, |app| app.last_focus = unsafe { GetFocus() });
    let id = command_id(wparam);
    if (NAV_BASE..NAV_BASE + 7).contains(&id) {
        update_area(window, Area::ALL[id - NAV_BASE]);
    } else if id == ACTION {
        launch_or_act(window);
    } else if id == SECONDARY {
        secondary_action(window, false);
    } else if id == TERTIARY {
        secondary_action(window, true);
    } else if id == QUATERNARY {
        quaternary_action(window);
    } else if id == QUINARY {
        quinary_action(window);
    } else if id == CATALOG_FILTER && (wparam.0 >> 16) == EN_CHANGE as usize {
        refresh_catalog_filter(window);
    } else if id == VIDEO_TIER && (wparam.0 >> 16) == CBN_SELCHANGE as usize {
        refresh_video_preview(window);
    } else if id == CANCEL {
        cancel(window);
    } else if id == FOCUS_RESTORE {
        restore_focus(window);
    }
    LRESULT(0)
}

fn handle_poll_timer(window: HWND) -> LRESULT {
    poll_terminal(window);
    LRESULT(0)
}

fn handle_size(window: HWND) -> LRESULT {
    layout(window);
    LRESULT(0)
}

fn handle_dpi_changed(window: HWND, lparam: LPARAM) -> LRESULT {
    let suggested = unsafe { *(lparam.0 as *const RECT) };
    unsafe {
        let _ = SetWindowPos(
            window,
            None,
            suggested.left,
            suggested.top,
            suggested.right - suggested.left,
            suggested.bottom - suggested.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    LRESULT(0)
}

fn handle_setting_change(window: HWND) -> LRESULT {
    let _ = with_state(window, |app| app.high_contrast = high_contrast_enabled());
    if let Some(status) = with_state(window, |app| app.status) {
        unsafe {
            let _ = InvalidateRect(Some(status), None, true);
        }
    }
    LRESULT(0)
}

fn handle_min_max_info(window: HWND, lparam: LPARAM) -> LRESULT {
    let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
    let dpi = unsafe { GetDpiForWindow(window) }.max(96);
    let mut client = RECT {
        left: 0,
        top: 0,
        right: (960 * dpi / 96) as i32,
        bottom: (540 * dpi / 96) as i32,
    };
    unsafe {
        let _ = AdjustWindowRectExForDpi(
            &mut client,
            WS_OVERLAPPEDWINDOW,
            false,
            WINDOW_EX_STYLE::default(),
            dpi,
        );
    }
    info.ptMinTrackSize.x = client.right - client.left;
    info.ptMinTrackSize.y = client.bottom - client.top;
    LRESULT(0)
}

fn handle_key_down(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
    let key = wparam.0 as u32;
    if ctrl && (0x31..=0x37).contains(&key) {
        update_area(window, Area::ALL[(key - 0x31) as usize]);
        return LRESULT(0);
    }
    if key == VK_ESCAPE.0 as u32 {
        cancel(window);
        return LRESULT(0);
    }
    if key == VK_F6.0 as u32 {
        restore_focus(window);
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn handle_set_focus(window: HWND) -> LRESULT {
    restore_focus(window);
    LRESULT(0)
}

fn handle_static_color(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some((status, high_contrast, kind)) = with_state(window, |app| {
        (app.status, app.high_contrast, app.operation.status)
    }) {
        let target = HWND(lparam.0 as *mut c_void);
        if target == status {
            unsafe {
                let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut c_void);
                let _ = windows::Win32::Graphics::Gdi::SetTextColor(
                    hdc,
                    status_color(kind, high_contrast),
                );
                let _ = windows::Win32::Graphics::Gdi::SetBkColor(
                    hdc,
                    COLORREF(GetSysColor(COLOR_WINDOW)),
                );
                return LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize);
            }
        }
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn handle_close(window: HWND) -> LRESULT {
    let (running, terminal, critical, diagnostic, watchdog) = with_state(window, |app| {
        (
            app.is_running(),
            app.child.is_some(),
            app.critical_operation,
            matches!(app.native_operation, Some(NativeOperation::Diagnostic)),
            app.elevation_watchdog.is_some(),
        )
    })
    .unwrap_or((false, false, false, false, false));
    if watchdog {
        unsafe {
            MessageBoxW(
                Some(window),
                w!(
                    "The unelevated GUI must remain open while the elevated GUI runs so it can retain the authenticated package capability. It will return automatically when the elevated GUI exits."
                ),
                w!("frametime.cfg elevation watchdog active"),
                MB_OK | MB_ICONWARNING,
            );
        }
        return LRESULT(0);
    }
    if terminal {
        unsafe {
            MessageBoxW(
                Some(window),
                w!(
                    "The GUI must remain open while the authenticated native CLI runs so it can retain the package capability. Use Ctrl-Break and wait for the terminal result."
                ),
                w!("frametime.cfg authenticated CLI active"),
                MB_OK | MB_ICONWARNING,
            );
        }
        return LRESULT(0);
    }
    if critical {
        unsafe {
            MessageBoxW(
                Some(window),
                w!(
                    "A native write is still running. Closing is blocked until it completes so its transactional result can be shown."
                ),
                w!("frametime.cfg native operation in progress"),
                MB_OK | MB_ICONWARNING,
            );
        }
        return LRESULT(0);
    }
    if running && !confirm_close(window, diagnostic) {
        return LRESULT(0);
    }
    unsafe {
        DestroyWindow(window).expect("close native window");
    }
    LRESULT(0)
}

fn confirm_close(window: HWND, diagnostic: bool) -> bool {
    let message = if diagnostic {
        w!(
            "A bounded, read-only hardware diagnostic is still running. Choose OK to close the GUI; its in-process result will be discarded and no workflow progress or write is involved."
        )
    } else {
        w!(
            "The native CLI is still running. Cancel with Ctrl-Break and wait for its terminal result, or choose OK to close this GUI and leave the CLI running safely."
        )
    };
    unsafe {
        MessageBoxW(
            Some(window),
            message,
            w!("frametime.cfg operation still running"),
            MB_OKCANCEL | MB_ICONWARNING,
        ) != IDCANCEL
    }
}

fn handle_destroy(window: HWND) -> LRESULT {
    let _ = take_state(window); // Dropping Child only closes the handle; the native CLI remains detached.
    unsafe {
        let _ = KillTimer(Some(window), POLL_TIMER);
        PostQuitMessage(0);
    }
    LRESULT(0)
}

pub(super) fn configure_preference(window: HWND) {
    let Some((profile, dry_run)) = with_state(window, |app| (app.profile, app.dry_run)) else {
        return;
    };
    let selected =
        unsafe { SendMessageW(profile, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0 };
    if selected < 0 {
        update_status(
            window,
            StatusKind::Warning,
            "Choose one of the five supported profiles first.",
        );
        return;
    }
    let mut profile_text = vec![0_u16; 32];
    unsafe {
        SendMessageW(
            profile,
            CB_GETLBTEXT,
            Some(WPARAM(selected as usize)),
            Some(LPARAM(profile_text.as_mut_ptr() as isize)),
        );
    }
    let length = profile_text
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(0);
    let profile_name = String::from_utf16_lossy(&profile_text[..length]);
    if !model::profile_preference_is_valid(&profile_name) {
        update_status(
            window,
            StatusKind::Failed,
            "The selected profile is invalid; preference was not written.",
        );
        return;
    }
    let dry = unsafe {
        SendMessageW(dry_run, BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0
            == BST_CHECKED.0 as isize
    };
    let arguments = [
        "configure",
        profile_name.as_str(),
        "--dry-run",
        if dry { "true" } else { "false" },
    ];
    start_command(window, &arguments, true);
}
pub(super) fn restore_focus(window: HWND) {
    if let Some(last_focus) = with_state(window, |app| app.last_focus) {
        unsafe {
            let _ = SetFocus(Some(last_focus));
        }
    }
}
pub(super) fn take_state(window: HWND) -> Option<Box<AppState>> {
    let value = unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, 0) };
    (!value.eq(&0)).then(|| unsafe { Box::from_raw(value as *mut AppState) })
}
