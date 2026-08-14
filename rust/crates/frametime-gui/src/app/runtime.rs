use super::*;

pub(super) fn calculate_fps_cap(window: HWND) {
    let Some((fps_input, min_input)) = with_state(window, |app| (app.fps_input, app.min_input))
    else {
        return;
    };
    let average = control_text(fps_input);
    let minimum = control_text(min_input).trim().parse::<u32>().unwrap_or(0);
    match model::calculate_fps_cap(&average, minimum) {
        Ok(cap) => update_status(
            window,
            StatusKind::Complete,
            &format!(
                "Recommended fps_max: {cap}. Record this alongside a comparable benchmark run."
            ),
        ),
        Err(error) => update_status(window, StatusKind::Warning, error),
    }
}
pub(super) fn control_text(handle: HWND) -> String {
    unsafe {
        let length = GetWindowTextLengthW(handle);
        let mut buffer = vec![0_u16; length as usize + 1];
        GetWindowTextW(handle, &mut buffer);
        String::from_utf16_lossy(&buffer[..length as usize])
    }
}
pub(super) fn cancel(window: HWND) {
    let diagnostic_running = with_state(window, |app| {
        app.native_result.is_some()
            && matches!(app.native_operation, Some(NativeOperation::Diagnostic))
    })
    .unwrap_or(false);
    if diagnostic_running {
        update_status(
            window,
            StatusKind::Warning,
            "This bounded in-process diagnostic cannot be interrupted by the adapter. No write is in progress; wait for its typed result or close the GUI.",
        );
        return;
    }
    if with_state(window, |app| app.native_result.is_some()).unwrap_or(false) {
        update_status(
            window,
            StatusKind::Warning,
            "This native transaction has no unsafe forced-cancel path. Wait for its verified result.",
        );
        return;
    }
    let child_id = with_state(window, |app| {
        app.child.as_ref().map(|child| child.child.id())
    })
    .flatten();
    match child_id {
        Some(process_group) => match request_ctrl_break(process_group) {
            Ok(()) => update_status(
                window,
                StatusKind::Warning,
                &OperationState::cancellation_requested().detail,
            ),
            Err(error) => update_status(
                window,
                StatusKind::Failed,
                &format!("Could not request Ctrl-Break cancellation: {error}"),
            ),
        },
        None => update_status(
            window,
            StatusKind::Ready,
            "No terminal operation is running.",
        ),
    }
}
pub(super) fn request_ctrl_break(process_group: u32) -> Result<(), String> {
    // The child is a new console/process-group leader. Attach only long enough to target
    // its group, then detach without terminating or waiting for it.
    unsafe {
        AttachConsole(process_group).map_err(|error| error.to_string())?;
        let result = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, process_group)
            .map_err(|error| error.to_string());
        let _ = FreeConsole();
        result
    }
}
pub(super) fn poll_terminal(window: HWND) {
    let result = with_state(window, |app| {
        app.child.as_mut().and_then(|child| {
            child
                .child
                .try_wait()
                .ok()
                .flatten()
                .map(|status| status.code())
        })
    })
    .flatten();
    if let Some(code) = result {
        let _ = with_state(window, |app| app.child = None);
        let outcome = OperationState::terminal_result(code);
        update_status(window, outcome.status, &outcome.detail);
    }

    poll_elevation_watchdog(window);
    if let Some(native) = take_native_result(window) {
        present_native_result(window, native);
    }
}

fn poll_elevation_watchdog(window: HWND) {
    let elevation = with_state(window, |app| {
        app.elevation_watchdog
            .as_ref()
            .map(ElevationWatchdog::has_exited)
    })
    .flatten();
    match elevation {
        Some(Ok(true)) => {
            let _ = with_state(window, |app| app.elevation_watchdog = None);
            unsafe {
                let _ = ShowWindow(window, SW_RESTORE);
            }
            update_status(
                window,
                StatusKind::Ready,
                "The elevated GUI exited. The unelevated watchdog released the retained package capability.",
            );
        }
        Some(Err(error)) => update_status(
            window,
            StatusKind::Failed,
            &format!(
                "The hidden elevation watchdog retained the package capability but could not poll the elevated GUI: {error}"
            ),
        ),
        Some(Ok(false)) | None => {}
    }
}

fn take_native_result(window: HWND) -> Option<(NativeOperation, NativeWorkerResult)> {
    with_state(window, |app| {
        let receiver = app.native_result.as_ref()?;
        match receiver.try_recv() {
            Ok(result) => {
                app.native_result = None;
                app.critical_operation = false;
                Some((
                    app.native_operation
                        .take()
                        .unwrap_or(NativeOperation::Recovery),
                    result,
                ))
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                app.native_result = None;
                app.critical_operation = false;
                Some((
                    app.native_operation
                        .take()
                        .unwrap_or(NativeOperation::Recovery),
                    NativeWorkerResult::Transaction(Err(
                        "Native worker ended without reporting a result.".into(),
                    )),
                ))
            }
        }
    })
    .flatten()
}

fn present_native_result(window: HWND, (operation, result): (NativeOperation, NativeWorkerResult)) {
    match (operation, result) {
        (NativeOperation::Recovery, NativeWorkerResult::Transaction(Ok(detail))) => {
            refresh_area_data(window, Area::Recovery);
            update_status(window, StatusKind::Complete, &detail);
        }
        (NativeOperation::VideoApply, NativeWorkerResult::Transaction(Ok(detail))) => {
            refresh_video_preview(window);
            update_status(window, StatusKind::Complete, &detail);
        }
        (NativeOperation::NetworkApply, NativeWorkerResult::Transaction(Ok(detail))) => {
            refresh_area_data(window, Area::Network);
            update_status(window, StatusKind::Complete, &detail);
        }
        (NativeOperation::Recovery, NativeWorkerResult::Transaction(Err(error))) => update_status(
            window,
            StatusKind::Warning,
            &format!("Native recovery operation did not fully complete: {error}"),
        ),
        (NativeOperation::VideoApply, NativeWorkerResult::Transaction(Err(error))) => {
            update_status(
                window,
                StatusKind::Warning,
                &format!(
                    "Native video apply did not fully complete; backup and partial state were retained: {error}"
                ),
            )
        }
        (NativeOperation::NetworkApply, NativeWorkerResult::Transaction(Err(error))) => {
            refresh_area_data(window, Area::Network);
            update_status(
                window,
                StatusKind::Warning,
                &format!(
                    "NIC latency stack did not fully complete; recovery data and partial state were retained: {error}"
                ),
            )
        }
        (NativeOperation::Diagnostic, NativeWorkerResult::Diagnostic(presentation)) => {
            let area = if presentation.belongs_to_benchmark() {
                Area::Benchmark
            } else {
                Area::Assess
            };
            let kind = match presentation.kind {
                DiagnosticPresentationKind::Complete => StatusKind::Complete,
                DiagnosticPresentationKind::Warning => StatusKind::Warning,
                DiagnosticPresentationKind::Failed => StatusKind::Failed,
            };
            let detail = presentation.detail.clone();
            let _ = with_state(window, |app| app.diagnostics = presentation);
            refresh_area_data(window, area);
            update_status(window, kind, &detail);
        }
        (_, NativeWorkerResult::Diagnostic(_))
        | (NativeOperation::Diagnostic, NativeWorkerResult::Transaction(_)) => update_status(
            window,
            StatusKind::Failed,
            "Native worker result did not match its operation. No workflow progress was recorded.",
        ),
    }
}

pub(super) fn high_contrast_enabled() -> bool {
    let mut setting = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            setting.cbSize,
            Some((&mut setting as *mut HIGHCONTRASTW).cast::<c_void>()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok()
            && setting.dwFlags.0 & HCF_HIGHCONTRASTON.0 != 0
    }
}
pub(super) fn status_color(kind: StatusKind, high_contrast: bool) -> COLORREF {
    if high_contrast {
        unsafe {
            return COLORREF(GetSysColor(COLOR_WINDOWTEXT));
        }
    }
    match kind {
        StatusKind::Ready | StatusKind::Complete => COLORREF(0x007000),
        StatusKind::Running => COLORREF(0x9A6500),
        StatusKind::Warning => COLORREF(0x0050B0),
        StatusKind::Failed => COLORREF(0x0000C0),
    }
}
