use super::*;

pub(super) fn launch_or_act(window: HWND) {
    if safe_mode_active() && !model::gui_allows_phase_2_in_safe_mode() {
        update_status(
            window,
            StatusKind::Warning,
            "Safe Mode prohibition is active. The GUI does not execute phase work in Safe Mode.",
        );
        return;
    }
    let Some((is_running, action)) =
        with_state(window, |app| (app.is_running(), app.area.action()))
    else {
        return;
    };
    if is_running {
        update_status(
            window,
            StatusKind::Warning,
            "A terminal operation is already running. Cancel it before starting another.",
        );
        return;
    }
    match action {
        Action::Refresh => refresh_overview(window),
        Action::HardwareDoctor => start_diagnostic(window, DiagnosticAction::Doctor),
        Action::CalculateFpsCap => calculate_fps_cap(window),
        Action::NetworkApply => start_network_apply(window),
        Action::PhaseChoice => configure_preference(window),
        Action::VideoRefresh => refresh_video_preview(window),
        Action::ExportBackup => export_backup(window),
    }
}

pub(super) fn start_network_apply(window: HWND) {
    if !require_authenticated_package(window) {
        return;
    }
    if with_state(window, |app| app.is_running()).unwrap_or(true) {
        update_status(
            window,
            StatusKind::Warning,
            "Another operation is still running. Wait for its result before applying the NIC latency stack.",
        );
        return;
    }
    if !confirm(
        window,
        "Apply the P1:16 NIC latency stack now? This creates recovery data, changes the selected physical Ethernet adapter and QoS policy, and may require a reboot.",
        "Apply NIC latency stack",
    ) {
        return;
    }
    if !is_elevated() {
        match relaunch_elevated(window) {
            Ok(()) => return,
            Err(error) => update_status(
                window,
                StatusKind::Failed,
                &format!("Could not request administrator approval: {error}"),
            ),
        }
        return;
    }
    let Some(config) = with_state(window, |app| {
        app.package
            .package()
            .map(|package| package.config().clone())
    })
    .flatten() else {
        update_status(
            window,
            StatusKind::Warning,
            "Authenticated package authority is no longer available.",
        );
        return;
    };
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result =
            frametime_windows::run_network_stack_transaction(Path::new(WORK_DIR), true, config)
                .and_then(|report| model::format_network_apply_report(&report));
        let _ = sender.send(NativeWorkerResult::Transaction(result));
    });
    let _ = with_state(window, |app| {
        app.native_result = Some(receiver);
        app.native_operation = Some(NativeOperation::NetworkApply);
        app.critical_operation = true;
    });
    update_status(
        window,
        StatusKind::Running,
        "NIC latency stack is running in-process. Closing is blocked until the engine reports its verified result.",
    );
}

pub(super) fn start_command(window: HWND, arguments: &[&str], requires_elevation: bool) {
    if !require_authenticated_package(window) {
        return;
    }
    if requires_elevation && !is_elevated() {
        match relaunch_elevated(window) {
            Ok(()) => return,
            Err(error) => update_status(
                window,
                StatusKind::Failed,
                &format!("Could not request administrator approval: {error}"),
            ),
        }
        return;
    }
    match launch_terminal(window, arguments) {
        Ok(child) => {
            let _ = with_state(window, |app| app.child = Some(child));
            update_status(
                window,
                StatusKind::Running,
                "Native CLI terminal is running. It remains visible for prompts, reboot handoffs, and partial failures.",
            );
        }
        Err(error) => update_status(
            window,
            StatusKind::Failed,
            &format!("Could not start native terminal: {error}"),
        ),
    }
}

pub(super) fn secondary_action(window: HWND, tertiary: bool) {
    let Some(area) = with_state(window, |app| app.area) else {
        return;
    };
    match (area, tertiary) {
        (Area::Overview, false) => update_area(window, Area::Assess),
        (Area::Overview, true) => update_area(window, Area::Recovery),
        (Area::Assess, false) => start_diagnostic(window, DiagnosticAction::Cpu),
        (Area::Assess, true) => start_diagnostic(window, DiagnosticAction::Gpu),
        (Area::SetupVerify, false) => {
            start_command(window, &model::SETUP_PHASE_ONE_ARGUMENTS, true)
        }
        (Area::SetupVerify, true) => start_command(window, &model::SETUP_VERIFY_ARGUMENTS, false),
        (Area::Benchmark, false) => parse_vprof_in_place(window),
        (Area::Benchmark, true) => add_vprof_with_cli(window),
        (Area::Recovery, false) => {
            if confirm(
                window,
                "Restore every supported backup entry? Failed records stay in backup.json for retry.",
                "Restore all backups",
            ) {
                start_native_recovery(window, true, |config| {
                    frametime_windows::restore_all(Path::new(WORK_DIR), &config).map(|()| "Recovery completed. Retained entries, if any, are shown in the refreshed grid.".into())
                });
            }
        }
        (Area::Recovery, true) => restore_selected(window),
        (Area::Video, false) => apply_video_preset(window),
        (Area::Network, _) | (Area::Video, true) => update_status(
            window,
            StatusKind::Warning,
            "This area has no native action in the current batch.",
        ),
    }
}

pub(super) fn quaternary_action(window: HWND) {
    let Some(area) = with_state(window, |app| app.area) else {
        return;
    };
    match area {
        Area::Assess => start_diagnostic(window, DiagnosticAction::System),
        Area::Benchmark => start_diagnostic(window, DiagnosticAction::EtwFrames),
        Area::Recovery
            if confirm(
                window,
                "Clear every recovery record? This cannot restore settings and requires a separate backup export first.",
                "Clear backup records",
            ) =>
        {
            start_native_recovery(window, true, |_| {
                frametime_windows::clear_backup(Path::new(WORK_DIR))
                    .map(|()| "Backup records cleared after explicit confirmation.".into())
            })
        }
        _ => {}
    }
}

pub(super) fn quinary_action(window: HWND) {
    if with_state(window, |app| app.area) == Some(Area::Assess) {
        start_diagnostic(window, DiagnosticAction::Whea);
    }
}

pub(super) fn start_diagnostic(window: HWND, action: DiagnosticAction) {
    if with_state(window, |app| app.is_running()).unwrap_or(true) {
        update_status(
            window,
            StatusKind::Warning,
            "Another operation is still running. Wait for its result before starting a diagnostic.",
        );
        return;
    }
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let envelope = WindowsHardwareDiagnostics::new().execute(action.command());
        let _ = sender.send(NativeWorkerResult::Diagnostic(
            DiagnosticPresentation::from_envelope(action, envelope),
        ));
    });
    let _ = with_state(window, |app| {
        app.native_result = Some(receiver);
        app.native_operation = Some(NativeOperation::Diagnostic);
        app.critical_operation = false;
    });
    update_status(
        window,
        StatusKind::Running,
        &format!(
            "{} is running in-process through the read-only Windows adapter. No workflow progress or write is being performed.",
            action.label()
        ),
    );
}

pub(super) fn apply_video_preset(window: HWND) {
    if !require_authenticated_package(window) {
        return;
    }
    let Some((root_control, tier_control, available)) = with_state(window, |app| {
        (
            app.video_root,
            app.video_tier,
            app.video_preview.apply_available(),
        )
    }) else {
        return;
    };
    if !available {
        update_status(
            window,
            StatusKind::Warning,
            "No validated typed video controller is available for this Steam root. Refresh the trusted preview first.",
        );
        return;
    }
    let root = control_text(root_control).trim().to_owned();
    let tier = selected_video_tier(tier_control);
    if !confirm(
        window,
        &format!(
            "Apply all 13 managed CS2 video settings using the {} preset? The native controller creates video.txt.bak once and verifies readback.",
            tier.label()
        ),
        "Apply trusted CS2 video preset",
    ) {
        return;
    }
    let running = with_state(window, |app| app.is_running()).unwrap_or(true);
    if running {
        update_status(
            window,
            StatusKind::Warning,
            "Another operation is still running. Wait for it before applying the video preset.",
        );
        return;
    }
    let root = PathBuf::from(root);
    let tier = core_video_tier(tier);
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = frametime_windows::detect_video_gpu_vendor()
        .and_then(|vendor| frametime_windows::VideoController::new(&root, vendor))
        .and_then(|controller| controller.apply(tier))
        .map(|result| {
            format!(
                "Native video apply completed: {} settings read back as compliant; {}; {} bytes written.",
                result.preview.rows.len(),
                if result.backup_created {
                    "created video.txt.bak"
                } else {
                    "retained existing video.txt.bak"
                },
                result.bytes_written,
            )
        });
        let _ = sender.send(NativeWorkerResult::Transaction(result));
    });
    let _ = with_state(window, |app| {
        app.native_result = Some(receiver);
        app.native_operation = Some(NativeOperation::VideoApply);
        app.critical_operation = true;
    });
    update_status(
        window,
        StatusKind::Running,
        "Native video apply is running. Closing is blocked until the typed controller reports its verified result.",
    );
}

pub(super) fn restore_selected(window: HWND) {
    let Some(step) = selected_recovery_step(window) else {
        update_status(
            window,
            StatusKind::Warning,
            "Select a retained recovery entry before Restore selected.",
        );
        return;
    };
    if confirm(
        window,
        &format!("Restore only the selected entry: {step}? Other entries remain for later retry."),
        "Restore selected backup",
    ) {
        start_native_recovery(window, true, move |config| {
            frametime_windows::restore_selected(Path::new(WORK_DIR), &step, &config).map(|()| {
                "Selected recovery entries completed. Other records remain in the backup grid."
                    .into()
            })
        });
    }
}

pub(super) fn selected_recovery_step(window: HWND) -> Option<String> {
    let table = with_state(window, |app| app.table)?;
    let selected = unsafe {
        SendMessageW(
            table,
            LVM_GETNEXTITEM,
            Some(WPARAM(usize::MAX)),
            Some(LPARAM(LVNI_SELECTED as isize)),
        )
        .0
    };
    if selected < 0 {
        return None;
    }
    let mut text = vec![0_u16; 256];
    let mut item = LVITEMW {
        iSubItem: 0,
        cchTextMax: text.len() as i32,
        pszText: windows::core::PWSTR(text.as_mut_ptr()),
        ..Default::default()
    };
    unsafe {
        SendMessageW(
            table,
            LVM_GETITEMTEXTW,
            Some(WPARAM(selected as usize)),
            Some(LPARAM((&mut item as *mut LVITEMW).cast::<c_void>() as isize)),
        );
    }
    let length = text
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(text.len());
    let step = String::from_utf16_lossy(&text[..length]);
    (!step.is_empty() && step != "Backup grid").then_some(step)
}

pub(super) fn start_native_recovery(
    window: HWND,
    requires_elevation: bool,
    operation: impl FnOnce(frametime_windows::VerifiedConfig) -> Result<String, String> + Send + 'static,
) {
    if !require_authenticated_package(window) {
        return;
    }
    if requires_elevation && !is_elevated() {
        match relaunch_elevated(window) {
            Ok(()) => return,
            Err(error) => update_status(
                window,
                StatusKind::Failed,
                &format!("Could not request administrator approval: {error}"),
            ),
        }
        return;
    }
    let running = with_state(window, |app| app.is_running()).unwrap_or(true);
    if running {
        update_status(
            window,
            StatusKind::Warning,
            "Another operation is still running. Wait for it before changing recovery records.",
        );
        return;
    }
    let Some(config) = with_state(window, |app| {
        app.package
            .package()
            .map(|package| package.config().clone())
    })
    .flatten() else {
        update_status(
            window,
            StatusKind::Warning,
            "Authenticated package authority is no longer available.",
        );
        return;
    };
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(NativeWorkerResult::Transaction(operation(config)));
    });
    let _ = with_state(window, |app| {
        app.native_result = Some(receiver);
        app.native_operation = Some(NativeOperation::Recovery);
        app.critical_operation = requires_elevation;
    });
    update_status(
        window,
        StatusKind::Running,
        "Native recovery operation is running. Closing this GUI is blocked until it reaches a safe result.",
    );
}

pub(super) fn export_backup(window: HWND) {
    if !require_authenticated_package(window) {
        return;
    }
    let Some(destination) = choose_backup_destination(window) else {
        return;
    };
    start_native_recovery(window, false, move |_| {
        frametime_windows::export_backup(Path::new(WORK_DIR), &destination).map(|()| {
            format!(
                "Backup exported and byte-verified at {}.",
                destination.display()
            )
        })
    });
}

pub(super) fn choose_backup_destination(window: HWND) -> Option<PathBuf> {
    let mut buffer = vec![0_u16; 32_768];
    let filter = utf16("JSON files (*.json)\0*.json\0All files (*.*)\0*.*\0\0");
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: window,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: windows::core::PWSTR(buffer.as_mut_ptr()),
        nMaxFile: buffer.len() as u32,
        lpstrDefExt: w!("json"),
        Flags: OFN_OVERWRITEPROMPT,
        ..Default::default()
    };
    if unsafe { GetSaveFileNameW(&mut dialog) }.as_bool() {
        let length = buffer.iter().position(|value| *value == 0).unwrap_or(0);
        return Some(PathBuf::from(String::from_utf16_lossy(&buffer[..length])));
    }
    None
}

pub(super) fn parse_vprof_in_place(window: HWND) {
    let Some(input) = with_state(window, |app| app.vprof_input) else {
        return;
    };
    let raw = control_text(input);
    match frametime_core::fps::parse_vprof_output(&raw) {
        Some(capture) => {
            let cap = frametime_core::fps::recommended_cap(capture.average_fps, 0.09, 60);
            update_status(
                window,
                StatusKind::Complete,
                &format!(
                    "VProf parsed: {} run(s), avg {:.1}, P1 {:.1}, recommended fps_max {cap}.",
                    capture.runs, capture.average_fps, capture.p1_fps
                ),
            );
        }
        None => update_status(
            window,
            StatusKind::Warning,
            "No valid [VProf] FPS result was found. Paste one or more complete Avg/P1 lines.",
        ),
    }
}

pub(super) fn add_vprof_with_cli(window: HWND) {
    if !require_authenticated_package(window) {
        return;
    }
    let Some(input) = with_state(window, |app| app.vprof_input) else {
        return;
    };
    let raw = control_text(input);
    if frametime_core::fps::parse_vprof_output(&raw).is_none() {
        update_status(
            window,
            StatusKind::Warning,
            "No valid VProf result is available to add.",
        );
        return;
    }
    match launch_terminal(
        window,
        &[
            "fps-cap",
            "--vprof-text",
            raw.as_str(),
            "--label",
            "VProf capture",
        ],
    ) {
        Ok(child) => {
            let _ = with_state(window, |app| app.child = Some(child));
            update_status(
                window,
                StatusKind::Running,
                "VProf capture is being persisted by the native CLI with the VProf capture label.",
            );
        }
        Err(error) => update_status(
            window,
            StatusKind::Failed,
            &format!("Could not start native benchmark command: {error}"),
        ),
    }
}

pub(super) fn confirm(window: HWND, prompt: &str, caption: &str) -> bool {
    let prompt = utf16(prompt);
    let caption = utf16(caption);
    unsafe {
        MessageBoxW(
            Some(window),
            PCWSTR(prompt.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_YESNO | MB_ICONWARNING,
        ) == IDYES
    }
}

pub(super) fn is_elevated() -> bool {
    unsafe { IsUserAnAdmin().as_bool() }
}
