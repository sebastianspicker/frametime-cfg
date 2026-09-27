use super::*;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST,
};

pub(in crate::app) fn command(window: HWND, id: usize, notification: usize) {
    if with_state(window, |app| app.is_running()).unwrap_or(true) {
        return;
    }
    match id {
        OPEN => choose_file(window),
        PASTE => {
            let input = with_state(window, |app| {
                app.fps_session.invalidate_source();
                app.fps_source = "Pasted text".into();
                app.vprof_input
            });
            if let Some(input) = input {
                set_text(input, "");
                unsafe {
                    let _ = SetFocus(Some(input));
                }
            }
            render(window);
        }
        NEXT => next(window),
        BACK => back(window),
        HISTORY => toggle_history(window),
        ETW => start_diagnostic(window, DiagnosticAction::EtwFrames),
        ELEVATE => {
            if require_authenticated_package(window)
                && let Err(error) = relaunch_elevated(window)
            {
                update_status(window, StatusKind::Failed, &error);
            }
        }
        STRATEGY if notification == CBN_SELCHANGE as usize => input_changed(window, false),
        REFRESH_RATE | MARGIN if notification == EN_CHANGE as usize => input_changed(window, false),
        _ => {}
    }
}

pub(in crate::app) fn input_changed(window: HWND, source: bool) {
    let _ = with_state(window, |app| {
        if app.is_running() {
            return;
        }
        if source {
            app.fps_session.invalidate_source();
            app.fps_source = "Pasted text".into();
        } else {
            app.fps_session.invalidate();
        }
    });
    render(window);
}

fn next(window: HWND) {
    let Some((history, stage, has_capture, has_outcome)) = with_state(window, |app| {
        (
            app.fps_history,
            app.fps_session.stage(),
            app.fps_session.capture().is_some(),
            app.fps_session.outcome().is_some(),
        )
    }) else {
        return;
    };
    if history {
        toggle_history(window);
        return;
    }
    match stage {
        FpsStage::Import if !has_capture => {
            let Some(input) = with_state(window, |app| app.vprof_input) else {
                return;
            };
            let Some(text) = vprof_control_text(window, input) else {
                return;
            };
            read_source(
                window,
                frametime_app::VprofBenchmarkRequest {
                    text: Some(text),
                    file: None,
                    clipboard: false,
                },
            );
        }
        FpsStage::Import | FpsStage::Evaluate if !has_outcome => evaluate(window),
        FpsStage::Evaluate => {
            let result = with_state(window, |app| app.fps_session.begin_review());
            report(window, result);
            render(window);
            focus_next(window);
        }
        FpsStage::Review => save(window),
        FpsStage::Save | FpsStage::Saved => {
            let _ = with_state(window, |app| {
                app.fps_session.reset();
                app.fps_source = "Pasted text".into();
            });
            if let Some(input) = with_state(window, |app| app.vprof_input) {
                set_text(input, "");
            }
            update_status(
                window,
                StatusKind::Ready,
                "Start a new calculation. Previously saved evidence is retained.",
            );
            render(window);
        }
        _ => {}
    }
}

fn evaluate(window: HWND) {
    let result = with_state(window, |app| {
        let strategy = if unsafe { SendMessageW(app.fps.strategy, CB_GETCURSEL, None, None).0 } == 1
        {
            frametime_app::FpsStrategyValue::Vrr
        } else {
            frametime_app::FpsStrategyValue::Raw
        };
        app.fps_session
            .evaluate_text(
                strategy,
                &control_text(app.min_input),
                &control_text(app.fps.refresh),
                &control_text(app.fps.margin),
            )
            .map(|_| ())
    });
    report(window, result);
    render(window);
    focus_next(window);
}

fn report(window: HWND, result: Option<Result<(), String>>) {
    match result {
        Some(Ok(())) => update_status(
            window,
            StatusKind::Ready,
            "Analysis only - Nothing saved. Game configuration is unchanged.",
        ),
        Some(Err(error)) => update_status(window, StatusKind::Warning, &error),
        None => {}
    }
}

fn back(window: HWND) {
    let _ = with_state(window, |app| match app.fps_session.stage() {
        FpsStage::Review => app.fps_session.invalidate(),
        FpsStage::Evaluate => {
            let _ = app.fps_session.return_to_import();
        }
        _ => {}
    });
    render(window);
    focus_next(window);
}

fn toggle_history(window: HWND) {
    let _ = with_state(window, |app| app.fps_history = !app.fps_history);
    render(window);
    refresh_area_data(window, Area::Benchmark);
}

fn read_source(window: HWND, request: frametime_app::VprofBenchmarkRequest) {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = frametime_app::read_fps_capture(request).map_err(|error| error.to_string());
        let _ = sender.send(NativeWorkerResult::FpsRead(result));
    });
    let _ = with_state(window, |app| {
        app.native_result = Some(receiver);
        app.native_operation = Some(NativeOperation::FpsRead);
    });
    update_status(
        window,
        StatusKind::Running,
        "Reading and validating VProf source. Nothing is being saved.",
    );
    render(window);
}

fn choose_file(window: HWND) {
    let mut buffer = vec![0_u16; 32_768];
    let filter = utf16("VProf text (*.txt;*.log)\0*.txt;*.log\0All files (*.*)\0*.*\0\0");
    let mut dialog = OPENFILENAMEW {
        lStructSize: u32::try_from(std::mem::size_of::<OPENFILENAMEW>())
            .expect("dialog structure fits u32"),
        hwndOwner: window,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: windows::core::PWSTR(buffer.as_mut_ptr()),
        nMaxFile: u32::try_from(buffer.len()).expect("bounded path buffer fits u32"),
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
        ..Default::default()
    };
    if !unsafe { GetOpenFileNameW(&mut dialog) }.as_bool() {
        return;
    }
    let length = buffer.iter().position(|value| *value == 0).unwrap_or(0);
    let path = PathBuf::from(String::from_utf16_lossy(&buffer[..length]));
    let input = with_state(window, |app| app.vprof_input);
    if let Some(input) = input {
        set_text(input, "");
    }
    let _ = with_state(window, |app| {
        app.fps_session.invalidate_source();
        app.fps_source = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
    });
    read_source(
        window,
        frametime_app::VprofBenchmarkRequest {
            text: None,
            file: Some(path),
            clipboard: false,
        },
    );
}

fn save(window: HWND) {
    if !require_authenticated_package(window) || !is_elevated() {
        update_status(
            window,
            StatusKind::Warning,
            "An authenticated package and administrator access are required to save. Your analysis is retained.",
        );
        return;
    }
    let Some(request) = with_state(window, |app| {
        app.fps_session
            .persistence_request(control_text(app.fps.label))
    }) else {
        return;
    };
    let request = match request {
        Ok(request) => request,
        Err(error) => {
            update_status(window, StatusKind::Warning, &error);
            return;
        }
    };
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = frametime_app::evaluate_fps_cap(request).map_err(|error| error.to_string());
        let _ = sender.send(NativeWorkerResult::FpsSaved(result));
    });
    let _ = with_state(window, |app| {
        app.native_result = Some(receiver);
        app.native_operation = Some(NativeOperation::FpsSave);
        app.critical_operation = true;
    });
    update_status(
        window,
        StatusKind::Running,
        "Saving cap and evidence; waiting for state and history readback.",
    );
    render(window);
}

pub(in crate::app) fn present_result(window: HWND, result: NativeWorkerResult) {
    match result {
        NativeWorkerResult::FpsRead(Ok(capture)) => {
            let _ = with_state(window, |app| app.fps_session.set_capture(capture));
            evaluate(window);
        }
        NativeWorkerResult::FpsRead(Err(error)) => {
            update_status(window, StatusKind::Warning, &error);
            render(window);
        }
        NativeWorkerResult::FpsSaved(result) => {
            let result = result.map_err(frametime_app::ApplicationError::Failed);
            let finished = with_state(window, |app| app.fps_session.finish_save(result));
            match finished {
                Some(Ok(())) => update_status(
                    window,
                    StatusKind::Complete,
                    "Cap and evidence saved. State and history readback succeeded. Game configuration is unchanged.",
                ),
                Some(Err(error)) => update_status(
                    window,
                    StatusKind::Failed,
                    &format!(
                        "Save may be incomplete. Inspect saved state before retrying: {error}"
                    ),
                ),
                None => {}
            }
            render(window);
            focus_next(window);
        }
        _ => update_status(
            window,
            StatusKind::Failed,
            "FPS worker did not return the expected result. Inspect state before retrying a save.",
        ),
    }
}

fn focus_next(window: HWND) {
    if let Some(handle) = with_state(window, |app| {
        (app.area == Area::Benchmark).then_some(app.fps.next)
    })
    .flatten()
    {
        unsafe {
            let _ = SetFocus(Some(handle));
        }
    }
}
