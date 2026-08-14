use super::*;

pub(super) fn create_controls(
    parent: HWND,
    instance: HINSTANCE,
    package: model::PackageAuthentication<frametime_windows::AuthenticatedPackage>,
) -> windows::core::Result<()> {
    let nav = create_navigation(parent, instance)?;
    let standard = create_standard_controls(parent, instance)?;
    let benchmark = create_benchmark_controls(parent, instance)?;
    let preference = create_preference_controls(parent, instance)?;
    let catalog = create_catalog_controls(parent, instance)?;
    let video = create_video_controls(parent, instance)?;
    let table = create_table(parent, instance)?;
    install_state(
        parent,
        CreatedControls {
            nav,
            standard,
            benchmark,
            preference,
            catalog,
            video,
            table,
        },
        package,
    );
    unsafe {
        SetTimer(Some(parent), POLL_TIMER, 250, None);
    }
    update_area(parent, Area::Overview);
    Ok(())
}

struct StandardControls {
    heading: HWND,
    description: HWND,
    status: HWND,
    action: HWND,
    secondary: HWND,
    tertiary: HWND,
    quaternary: HWND,
    quinary: HWND,
    cancel: HWND,
}

struct BenchmarkControls {
    fps_label: HWND,
    fps_input: HWND,
    min_label: HWND,
    min_input: HWND,
    vprof_input: HWND,
}

struct PreferenceControls {
    profile: HWND,
    dry_run: HWND,
}

struct CatalogControls {
    filter_label: HWND,
    catalog_filter: HWND,
}

struct VideoControls {
    video_root_label: HWND,
    video_root: HWND,
    video_tier_label: HWND,
    video_tier: HWND,
}

struct CreatedControls {
    nav: [HWND; 7],
    standard: StandardControls,
    benchmark: BenchmarkControls,
    preference: PreferenceControls,
    catalog: CatalogControls,
    video: VideoControls,
    table: HWND,
}

fn create_navigation(parent: HWND, instance: HINSTANCE) -> windows::core::Result<[HWND; 7]> {
    let mut nav = [HWND::default(); 7];
    for (index, area) in Area::ALL.iter().enumerate() {
        nav[index] = create_button(parent, instance, area.title(), NAV_BASE + index)?;
    }
    Ok(nav)
}

fn create_standard_controls(
    parent: HWND,
    instance: HINSTANCE,
) -> windows::core::Result<StandardControls> {
    Ok(StandardControls {
        heading: create_static_text(parent, instance, "Overview", 0)?,
        description: create_static_text(parent, instance, "", 0)?,
        status: create_static_text(parent, instance, "", 0)?,
        action: create_button(parent, instance, "Refresh work directory", ACTION)?,
        secondary: create_button(parent, instance, "", SECONDARY)?,
        tertiary: create_button(parent, instance, "", TERTIARY)?,
        quaternary: create_button(parent, instance, "", QUATERNARY)?,
        quinary: create_button(parent, instance, "", QUINARY)?,
        cancel: create_button(parent, instance, "Cancel terminal operation", CANCEL)?,
    })
}

fn create_benchmark_controls(
    parent: HWND,
    instance: HINSTANCE,
) -> windows::core::Result<BenchmarkControls> {
    let fps_label = create_static_text(parent, instance, "Average FPS", 0)?;
    let fps_input = create_text_control(
        parent,
        instance,
        "EDIT",
        "",
        WS_CHILD | WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        FPS_INPUT,
    )?;
    let min_label = create_static_text(parent, instance, "Minimum cap", 0)?;
    let min_input = create_text_control(
        parent,
        instance,
        "EDIT",
        "60",
        WS_CHILD | WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        MIN_INPUT,
    )?;
    let vprof_input = create_text_control(
        parent,
        instance,
        "EDIT",
        "Paste [VProf] FPS: Avg=…, P1=… output here",
        WS_CHILD
            | WS_TABSTOP
            | WS_BORDER
            | WINDOW_STYLE(ES_MULTILINE as u32)
            | WINDOW_STYLE(ES_AUTOVSCROLL as u32)
            | WS_VSCROLL,
        VPROF_INPUT,
    )?;
    Ok(BenchmarkControls {
        fps_label,
        fps_input,
        min_label,
        min_input,
        vprof_input,
    })
}

fn create_preference_controls(
    parent: HWND,
    instance: HINSTANCE,
) -> windows::core::Result<PreferenceControls> {
    let profile = create_text_control(
        parent,
        instance,
        "COMBOBOX",
        "",
        WS_CHILD | WS_TABSTOP | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        PROFILE,
    )?;
    for name in model::PROFILE_PREFERENCES {
        let name = utf16(name);
        unsafe {
            SendMessageW(
                profile,
                CB_ADDSTRING,
                Some(WPARAM(0)),
                Some(LPARAM(name.as_ptr() as isize)),
            );
        }
    }
    unsafe {
        SendMessageW(profile, CB_SETCURSEL, Some(WPARAM(1)), Some(LPARAM(0)));
    }
    let dry_run = create_text_control(
        parent,
        instance,
        "BUTTON",
        "Dry run preference",
        WS_CHILD | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        DRY_RUN,
    )?;
    Ok(PreferenceControls { profile, dry_run })
}

fn create_catalog_controls(
    parent: HWND,
    instance: HINSTANCE,
) -> windows::core::Result<CatalogControls> {
    Ok(CatalogControls {
        filter_label: create_static_text(
            parent,
            instance,
            model::catalog_filter_accessible_name(),
            0,
        )?,
        catalog_filter: create_text_control(
            parent,
            instance,
            "EDIT",
            "",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            CATALOG_FILTER,
        )?,
    })
}

fn create_video_controls(
    parent: HWND,
    instance: HINSTANCE,
) -> windows::core::Result<VideoControls> {
    let video_root_label = create_static_text(parent, instance, "Trusted Steam root", 0)?;
    let video_root = create_text_control(
        parent,
        instance,
        "EDIT",
        r"C:\Program Files (x86)\Steam",
        WS_CHILD | WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        VIDEO_ROOT,
    )?;
    let video_tier_label = create_static_text(parent, instance, "Preset tier", 0)?;
    let video_tier = create_text_control(
        parent,
        instance,
        "COMBOBOX",
        "",
        WS_CHILD | WS_TABSTOP | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        VIDEO_TIER,
    )?;
    for tier in model::VideoPresetTier::ALL {
        let label = utf16(tier.label());
        unsafe {
            SendMessageW(
                video_tier,
                CB_ADDSTRING,
                Some(WPARAM(0)),
                Some(LPARAM(label.as_ptr() as isize)),
            );
        }
    }
    unsafe {
        SendMessageW(video_tier, CB_SETCURSEL, Some(WPARAM(0)), Some(LPARAM(0)));
    }
    Ok(VideoControls {
        video_root_label,
        video_root,
        video_tier_label,
        video_tier,
    })
}

fn create_table(parent: HWND, instance: HINSTANCE) -> windows::core::Result<HWND> {
    // The adjacent STATIC label has the stable automation-facing name from the model;
    // the edit remains empty so filtering never changes data until the operator types.
    let table = unsafe {
        CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("SysListView32"),
            w!("Task detail table"),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WINDOW_STYLE(LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS),
            0,
            0,
            0,
            0,
            Some(parent),
            None,
            Some(instance),
            None,
        )?
    };
    insert_columns(table);
    Ok(table)
}

fn install_state(
    parent: HWND,
    controls: CreatedControls,
    package: model::PackageAuthentication<frametime_windows::AuthenticatedPackage>,
) {
    let operation = if package.has_capability() {
        OperationState::ready(
            "Authenticated package retained. Setup, persistence, elevation, and native mutation controls are available.",
        )
    } else {
        OperationState {
            status: StatusKind::Warning,
            detail: package.unavailable_detail(),
            cancellable: false,
        }
    };
    let mut state = Box::new(AppState {
        area: Area::Overview,
        nav: controls.nav,
        heading: controls.standard.heading,
        description: controls.standard.description,
        status: controls.standard.status,
        action: controls.standard.action,
        secondary: controls.standard.secondary,
        tertiary: controls.standard.tertiary,
        quaternary: controls.standard.quaternary,
        quinary: controls.standard.quinary,
        cancel: controls.standard.cancel,
        table: controls.table,
        fps_label: controls.benchmark.fps_label,
        fps_input: controls.benchmark.fps_input,
        min_label: controls.benchmark.min_label,
        min_input: controls.benchmark.min_input,
        vprof_input: controls.benchmark.vprof_input,
        profile: controls.preference.profile,
        dry_run: controls.preference.dry_run,
        filter_label: controls.catalog.filter_label,
        catalog_filter: controls.catalog.catalog_filter,
        video_root_label: controls.video.video_root_label,
        video_root: controls.video.video_root,
        video_tier_label: controls.video.video_tier_label,
        video_tier: controls.video.video_tier,
        video_preview: model::VideoPreview::awaiting_discovery(),
        package,
        child: None,
        elevation_watchdog: None,
        native_result: None,
        native_operation: None,
        critical_operation: false,
        diagnostics: DiagnosticPresentation::empty(),
        operation,
        last_focus: controls.standard.action,
        high_contrast: high_contrast_enabled(),
    });
    unsafe {
        let _ = SetWindowLongPtrW(
            parent,
            GWLP_USERDATA,
            (&mut *state as *mut AppState).cast::<c_void>() as isize,
        );
    }
    std::mem::forget(state);
}

fn create_button(
    parent: HWND,
    instance: HINSTANCE,
    text: &str,
    id: usize,
) -> windows::core::Result<HWND> {
    create_text_control(
        parent,
        instance,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        id,
    )
}

fn create_static_text(
    parent: HWND,
    instance: HINSTANCE,
    text: &str,
    id: usize,
) -> windows::core::Result<HWND> {
    create_text_control(parent, instance, "STATIC", text, WS_CHILD | WS_VISIBLE, id)
}

pub(super) fn create_text_control(
    parent: HWND,
    instance: HINSTANCE,
    class: &str,
    text: &str,
    style: WINDOW_STYLE,
    id: usize,
) -> windows::core::Result<HWND> {
    let class = utf16(class);
    let text = utf16(text);
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR(class.as_ptr()),
            PCWSTR(text.as_ptr()),
            style,
            0,
            0,
            0,
            0,
            Some(parent),
            Some(HMENU(id as *mut c_void)),
            Some(instance),
            None,
        )
    }
}

pub(super) fn utf16(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub(super) fn with_state<R>(window: HWND, operation: impl FnOnce(&mut AppState) -> R) -> Option<R> {
    let value = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
    (!value.eq(&0)).then(|| unsafe { operation(&mut *(value as *mut AppState)) })
}
pub(super) fn set_text(handle: HWND, text: &str) {
    let text = utf16(text);
    unsafe {
        SetWindowTextW(handle, PCWSTR(text.as_ptr())).expect("standard control");
    }
}

pub(super) fn insert_columns(table: HWND) {
    for (index, (title, width)) in [("Item", 220), ("Value", 270), ("State", 250)]
        .iter()
        .enumerate()
    {
        let title = utf16(title);
        let mut column = LVCOLUMNW {
            mask: LVCF_TEXT | LVCF_WIDTH,
            cx: *width,
            pszText: windows::core::PWSTR(title.as_ptr().cast_mut()),
            ..Default::default()
        };
        unsafe {
            SendMessageW(
                table,
                LVM_INSERTCOLUMNW,
                Some(WPARAM(index)),
                Some(LPARAM(
                    (&mut column as *mut LVCOLUMNW).cast::<c_void>() as isize
                )),
            );
        }
    }
}
pub(super) fn populate_table(table: HWND, rows: &[(&str, &str, &str)]) {
    unsafe {
        SendMessageW(table, LVM_DELETEALLITEMS, Some(WPARAM(0)), Some(LPARAM(0)));
    }
    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, value) in [row.0, row.1, row.2].iter().enumerate() {
            let value = utf16(value);
            let mut item = LVITEMW {
                mask: LVIF_TEXT,
                iItem: row_index as i32,
                iSubItem: column_index as i32,
                pszText: windows::core::PWSTR(value.as_ptr().cast_mut()),
                ..Default::default()
            };
            let message = if column_index == 0 {
                LVM_INSERTITEMW
            } else {
                LVM_SETITEMTEXTW
            };
            unsafe {
                SendMessageW(
                    table,
                    message,
                    Some(WPARAM(row_index)),
                    Some(LPARAM((&mut item as *mut LVITEMW).cast::<c_void>() as isize)),
                );
            }
        }
    }
}
