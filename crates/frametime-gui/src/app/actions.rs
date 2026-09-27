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
        Action::DriverInspect => refresh_area_data(window, Area::Drivers),
        Action::ExportBackup => export_backup(window),
    }
}

pub(super) fn install_selected_cs2_cfg(window: HWND) {
    let Some(asset_control) = with_state(window, |app| app.cs2_cfg_asset) else {
        return;
    };
    let asset = selected_cs2_cfg_asset(asset_control);
    start_native_recovery(window, model::NativeAccess::ElevatedWrite, move |package| {
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

fn selected_cs2_cfg_asset(control: HWND) -> frametime_domain::cs2_config::OptionalCfgAsset {
    let index = unsafe { SendMessageW(control, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0 };
    selected_or_default(
        index,
        &frametime_domain::cs2_config::OptionalCfgAsset::ALL,
        frametime_domain::cs2_config::OptionalCfgAsset::NetStable,
    )
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
    start_native_recovery(window, model::NativeAccess::ElevatedWrite, move |package| {
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

fn profile_from_name(value: &str) -> Option<frametime_domain::policy::Profile> {
    match value {
        "safe" => Some(frametime_domain::policy::Profile::Safe),
        "recommended" => Some(frametime_domain::policy::Profile::Recommended),
        "competitive" => Some(frametime_domain::policy::Profile::Competitive),
        "custom" => Some(frametime_domain::policy::Profile::Custom),
        "yolo" => Some(frametime_domain::policy::Profile::Yolo),
        _ => None,
    }
}

pub(super) fn secondary_action(window: HWND, tertiary: bool) {
    let Some(area) = with_state(window, |app| app.area) else {
        return;
    };
    match secondary_command(area, tertiary) {
        SecondaryCommand::Navigate(area) => update_area(window, area),
        SecondaryCommand::Diagnostic(action) => start_diagnostic(window, action),
        SecondaryCommand::PhaseOne => start_phase_one(window),
        SecondaryCommand::Verify => start_verification(window),
        SecondaryCommand::ParseVprof => parse_vprof_in_place(window),
        SecondaryCommand::PersistVprof => add_vprof_with_application(window),
        SecondaryCommand::RestoreAll => restore_all(window),
        SecondaryCommand::RestoreSelected => restore_selected(window),
        SecondaryCommand::Warning(detail) => update_status(window, StatusKind::Warning, detail),
    }
}

enum SecondaryCommand {
    Navigate(Area),
    Diagnostic(DiagnosticAction),
    PhaseOne,
    Verify,
    ParseVprof,
    PersistVprof,
    RestoreAll,
    RestoreSelected,
    Warning(&'static str),
}

fn secondary_command(area: Area, tertiary: bool) -> SecondaryCommand {
    match (area, tertiary) {
        (Area::Overview, false) => SecondaryCommand::Navigate(Area::Assess),
        (Area::Overview, true) => SecondaryCommand::Navigate(Area::Recovery),
        (Area::Assess, false) => SecondaryCommand::Diagnostic(DiagnosticAction::Cpu),
        (Area::Assess, true) => SecondaryCommand::Diagnostic(DiagnosticAction::Gpu),
        (Area::SetupVerify, false) => SecondaryCommand::PhaseOne,
        (Area::SetupVerify, true) => SecondaryCommand::Verify,
        (Area::Benchmark, false) => SecondaryCommand::ParseVprof,
        (Area::Benchmark, true) => SecondaryCommand::PersistVprof,
        (Area::Recovery, false) => SecondaryCommand::RestoreAll,
        (Area::Recovery, true) => SecondaryCommand::RestoreSelected,
        (Area::Video, _) => SecondaryCommand::Warning(
            "Video discovery and display-goal guidance are read only. CS2 video files cannot be applied from this GUI.",
        ),
        (Area::Cs2Cfg, _) => SecondaryCommand::Warning(
            "Choose Install selected CS2 CFG to run the authenticated elevated CLI command.",
        ),
        (Area::Network, _) => {
            SecondaryCommand::Warning("This area has no native action in the current batch.")
        }
        (Area::Drivers, _) => SecondaryCommand::Warning(
            "Driver lifecycle mutation and Safe Mode execution remain CLI-only.",
        ),
    }
}

fn start_phase_one(window: HWND) {
    start_native_recovery(window, model::NativeAccess::ElevatedWrite, |_| {
        frametime_app::run_live(frametime_app::Command::Optimize { yes: true })
            .map_err(|error| error.to_string())
            .map(|_| {
                "Phase 1 completed or handed off through the shared application service.".into()
            })
    });
}

fn start_verification(window: HWND) {
    start_native_recovery(window, model::NativeAccess::ReadOnly, |_| {
        frametime_app::run_live(frametime_app::Command::Verify)
            .map_err(|error| error.to_string())
            .map(|_| {
                "Read-only verification completed through the shared application service.".into()
            })
    });
}

fn restore_all(window: HWND) {
    if confirm(
        window,
        "Restore every supported backup entry? Failed records stay in backup.json for retry.",
        "Restore all backups",
    ) {
        start_native_recovery(window, model::NativeAccess::ElevatedWrite, |package| {
            frametime_app::restore_all_from_package(package)
                .map_err(|error| error.to_string())
                .map(|()| {
                    "Recovery completed. Retained entries, if any, are shown in the refreshed grid."
                        .into()
                })
        });
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
            start_native_recovery(window, model::NativeAccess::ElevatedWrite, |package| {
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
            "Select a recognized recovery entry in a Ready snapshot. Press F5 if the snapshot is stale or failed.",
        );
        return;
    };
    if confirm(
        window,
        &format!("Restore only the selected entry: {step}? Other entries remain for later retry."),
        "Restore selected backup",
    ) {
        start_native_recovery(window, model::NativeAccess::ElevatedWrite, move |package| {
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
    let selected = usize::try_from(selected).ok()?;
    with_state(window, |app| {
        app.reads
            .snapshot(model::snapshots::Resource::Recovery)?
            .selected_data_key(selected, &control_text(app.catalog_filter))
    })
    .flatten()
}

pub(super) fn start_native_recovery(
    window: HWND,
    access: model::NativeAccess,
    operation: impl FnOnce(&frametime_app::AuthenticatedPackage) -> Result<String, String>
    + Send
    + 'static,
) {
    if !require_authenticated_package(window) {
        return;
    }
    if access.requires_elevation() && !is_elevated() {
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
        app.critical_operation = access.blocks_close();
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
    start_native_recovery(window, model::NativeAccess::Write, move |_| {
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
        lStructSize: u32::try_from(std::mem::size_of::<OPENFILENAMEW>())
            .expect("OPENFILENAMEW size fits in u32"),
        hwndOwner: window,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: windows::core::PWSTR(buffer.as_mut_ptr()),
        nMaxFile: u32::try_from(buffer.len()).expect("fixed save buffer length fits in u32"),
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
    let Some((input, target)) = with_state(window, |app| {
        (
            app.vprof_input,
            control_text(app.min_input)
                .trim()
                .parse::<u32>()
                .unwrap_or(0),
        )
    }) else {
        return;
    };
    let Some(raw) = vprof_control_text(window, input) else {
        return;
    };
    match frametime_domain::fps::parse_vprof_output_detailed(&raw) {
        Ok(capture) => {
            let _ = with_state(window, |app| {
                app.benchmark_preview = model::capture_rows(&capture, target)
            });
            render_catalog(window, Area::Benchmark);
            update_status(
                window,
                StatusKind::Complete,
                "VProf parsed. Individual unrounded results, P1 range, and supporting run count are shown in the table.",
            );
        }
        Err(error) => update_status(window, StatusKind::Warning, &error.to_string()),
    }
}

pub(super) fn add_vprof_with_application(window: HWND) {
    if !require_authenticated_package(window) {
        return;
    }
    let Some(input) = with_state(window, |app| app.vprof_input) else {
        return;
    };
    let Some(raw) = vprof_control_text(window, input) else {
        return;
    };
    let capture = match frametime_domain::fps::parse_vprof_output_detailed(&raw) {
        Ok(capture) => capture,
        Err(error) => {
            update_status(window, StatusKind::Warning, &error.to_string());
            return;
        }
    };
    let _ = with_state(window, |app| {
        app.benchmark_preview = model::capture_rows(&capture, 0)
    });
    render_catalog(window, Area::Benchmark);
    start_native_recovery(window, model::NativeAccess::Write, move |_| {
        frametime_app::evaluate_fps_cap(frametime_app::ValidatedFpsRequest {
            capture,
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
