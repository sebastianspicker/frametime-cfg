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
        Action::Cs2CfgApply => install_selected_cs2_cfg(window),
        Action::ExportBackup => export_backup(window),
    }
}

pub(super) fn install_selected_cs2_cfg(window: HWND) {
    let Some(asset_control) = with_state(window, |app| app.cs2_cfg_asset) else {
        return;
    };
    let asset = selected_cs2_cfg_asset(asset_control);
    start_native_recovery(window, true, move |package| {
        let written = frametime_app::deploy_optional_cs2_cfgs(package, &[asset])
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "Deployed and verified: {}. Original bytes remain in backup.json for native recovery.",
            written.first().map_or_else(
                || "no optional CFG".into(),
                |path| path.display().to_string()
            )
        ))
    });
}

fn selected_cs2_cfg_asset(control: HWND) -> frametime_domain::OptionalCfgAsset {
    let index = unsafe { SendMessageW(control, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0 };
    usize::try_from(index)
        .ok()
        .and_then(|index| frametime_domain::OptionalCfgAsset::ALL.get(index).copied())
        .unwrap_or(frametime_domain::OptionalCfgAsset::NetStable)
}

pub(super) fn start_network_apply(window: HWND) {
    if !require_authenticated_package(window) {
        return;
    }
    if with_state(window, |app| app.is_running()).unwrap_or(true) {
        update_status(
            window,
            StatusKind::Warning,
            "Another operation is still running. Wait for its result before enabling Ethernet RSS.",
        );
        return;
    }
    if !confirm(
        window,
        "Enable the supported RSS master state on the selected physical Ethernet adapter? Queue, processor, offload, QoS, and vendor-specific settings remain unchanged.",
        "Enable Ethernet RSS",
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
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = frametime_app::authenticate_package()
            .map_err(|error| error.to_string())
            .and_then(|package| {
                frametime_app::run_network_stack(&package).map_err(|error| error.to_string())
            })
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
        "The Ethernet RSS transaction is running in-process. Closing is blocked until the engine reports its verified result.",
    );
}

pub(super) fn configure_profile(window: HWND, profile: &str, dry_run: bool) {
    let Some(profile) = profile_from_name(profile) else {
        update_status(
            window,
            StatusKind::Failed,
            "The selected profile is invalid.",
        );
        return;
    };
    start_native_recovery(window, true, move |package| {
        frametime_app::configure_profile(package, profile, dry_run)
            .map_err(|error| error.to_string())
            .map(|state| {
                format!(
                    "Profile: {:?}; mode: {}; GUI preview preference: {dry_run}",
                    state.profile, state.mode
                )
            })
    });
}

fn profile_from_name(value: &str) -> Option<frametime_domain::Profile> {
    match value {
        "safe" => Some(frametime_domain::Profile::Safe),
        "recommended" => Some(frametime_domain::Profile::Recommended),
        "competitive" => Some(frametime_domain::Profile::Competitive),
        "custom" => Some(frametime_domain::Profile::Custom),
        "yolo" => Some(frametime_domain::Profile::Yolo),
        _ => None,
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
        (Area::SetupVerify, false) => start_native_recovery(window, true, |_| {
            frametime_app::run_live(frametime_app::Command::Optimize { yes: true })
                .map_err(|error| error.to_string())
                .map(|_| {
                    "Phase 1 completed or handed off through the shared application service.".into()
                })
        }),
        (Area::SetupVerify, true) => start_native_recovery(window, false, |_| {
            frametime_app::run_live(frametime_app::Command::Verify)
                .map_err(|error| error.to_string())
                .map(|_| {
                    "Read-only verification completed through the shared application service."
                        .into()
                })
        }),
        (Area::Benchmark, false) => parse_vprof_in_place(window),
        (Area::Benchmark, true) => add_vprof_with_application(window),
        (Area::Recovery, false) => {
            if confirm(
                window,
                "Restore every supported backup entry? Failed records stay in backup.json for retry.",
                "Restore all backups",
            ) {
                start_native_recovery(window, true, |package| {
                    frametime_app::restore_all_from_package(package)
                        .map_err(|error| error.to_string())
                        .map(|()| "Recovery completed. Retained entries, if any, are shown in the refreshed grid.".into())
                });
            }
        }
        (Area::Recovery, true) => restore_selected(window),
        (Area::Video, _) => update_status(
            window,
            StatusKind::Warning,
            "Video discovery and display-goal guidance are read only. CS2 video files cannot be applied from this GUI.",
        ),
        (Area::Cs2Cfg, _) => update_status(
            window,
            StatusKind::Warning,
            "Choose Install selected CS2 CFG to run the authenticated elevated CLI command.",
        ),
        (Area::Network, _) => update_status(
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
            start_native_recovery(window, true, |package| {
                frametime_app::clear_backup(package)
                    .map_err(|error| error.to_string())
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
        let envelope = frametime_app::diagnose(action.command());
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
        start_native_recovery(window, true, move |package| {
            frametime_app::restore_selected_from_package(package, &step)
                .map_err(|error| error.to_string())
                .map(|()| {
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
    operation: impl FnOnce(&frametime_app::AuthenticatedPackage) -> Result<String, String>
    + Send
    + 'static,
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
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = frametime_app::authenticate_package()
            .map_err(|error| error.to_string())
            .and_then(|package| operation(&package));
        let _ = sender.send(NativeWorkerResult::Transaction(result));
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
        frametime_app::export_backup(&destination)
            .map_err(|error| error.to_string())
            .map(|()| {
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
    match frametime_domain::fps::parse_vprof_output(&raw) {
        Some(capture) => {
            update_status(
                window,
                StatusKind::Complete,
                &format!(
                    "VProf parsed: {} run(s), avg {:.1}, P1 {:.1}. Choose raw uncapped/measured or a VRR ceiling; no percentage cap is inferred.",
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

pub(super) fn add_vprof_with_application(window: HWND) {
    if !require_authenticated_package(window) {
        return;
    }
    let Some(input) = with_state(window, |app| app.vprof_input) else {
        return;
    };
    let raw = control_text(input);
    if frametime_domain::fps::parse_vprof_output(&raw).is_none() {
        update_status(
            window,
            StatusKind::Warning,
            "No valid VProf result is available to add.",
        );
        return;
    }
    start_native_recovery(window, false, move |_| {
        frametime_app::run_fps_cap(frametime_app::FpsRequest {
            average: None,
            text: Some(raw),
            file: None,
            clipboard: false,
            strategy: frametime_app::FpsStrategyValue::Raw,
            measured_cap: 0,
            refresh_hz: 0,
            ceiling_margin_hz: 3,
            label: "VProf capture".into(),
            copy: false,
            no_persist: false,
        })
        .map_err(|error| error.to_string())
        .map(|_| "VProf capture persisted through the shared application service.".into())
    });
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
