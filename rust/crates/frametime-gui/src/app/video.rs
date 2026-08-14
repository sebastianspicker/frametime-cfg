use super::*;

pub(super) fn refresh_video_preview(window: HWND) {
    let Some((root_input, tier_input)) = with_state(window, |app| (app.video_root, app.video_tier))
    else {
        return;
    };
    let root = control_text(root_input).trim().to_owned();
    let tier = selected_video_tier(tier_input);
    let preview = build_video_preview(&root, tier);
    let detail = preview.discovery.clone();
    let kind = if preview.rows.is_empty() {
        StatusKind::Warning
    } else {
        StatusKind::Complete
    };
    let _ = with_state(window, |app| app.video_preview = preview);
    render_catalog(window, Area::Video);
    update_status(window, kind, &detail);
}

pub(super) fn selected_video_tier(control: HWND) -> model::VideoPresetTier {
    let index = unsafe { SendMessageW(control, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0 };
    usize::try_from(index)
        .ok()
        .and_then(|index| model::VideoPresetTier::ALL.get(index).copied())
        .unwrap_or(model::VideoPresetTier::Auto)
}

pub(super) fn core_video_tier(tier: model::VideoPresetTier) -> frametime_core::VideoTier {
    match tier {
        model::VideoPresetTier::Auto => frametime_core::VideoTier::Auto,
        model::VideoPresetTier::High => frametime_core::VideoTier::High,
        model::VideoPresetTier::Mid => frametime_core::VideoTier::Mid,
        model::VideoPresetTier::Low => frametime_core::VideoTier::Low,
    }
}

pub(super) fn build_video_preview(root: &str, tier: model::VideoPresetTier) -> model::VideoPreview {
    if root.is_empty() {
        return model::VideoPreview {
            discovery: "Steam root is required for trusted read-only discovery.".into(),
            tier,
            rows: Vec::new(),
            apply_available: false,
        };
    }
    let root_path = Path::new(root);
    let core_tier = core_video_tier(tier);
    let vendor =
        frametime_windows::detect_video_gpu_vendor().unwrap_or(frametime_core::GpuVendor::Other);
    let tier_detail = match (tier, vendor) {
        (model::VideoPresetTier::Auto, frametime_core::GpuVendor::Nvidia) => {
            " Auto resolved to High from an NVIDIA-only SetupAPI display inventory."
        }
        (model::VideoPresetTier::Auto, frametime_core::GpuVendor::Other) => {
            " Auto resolved conservatively to Mid because the display inventory was mixed, non-NVIDIA, or unavailable."
        }
        _ => "",
    };
    match frametime_core::discover_video_txt(root_path) {
        Ok(Some(path)) => match frametime_windows::VideoController::new(root_path, vendor)
            .and_then(|controller| controller.preview(core_tier))
        {
            Ok(preview) => {
                let rows = preview
                    .rows
                    .into_iter()
                    .map(|row| model::VideoPreviewRow {
                        setting: row.setting,
                        current_and_recommended: format!(
                            "{} -> {}",
                            row.current.unwrap_or_else(|| "missing".into()),
                            row.recommended
                        ),
                        status_and_note: format!("{:?}: {}", row.status, row.note),
                    })
                    .collect::<Vec<_>>();
                model::VideoPreview {
                    discovery: format!(
                        "Trusted video.txt discovered at {}. {} settings previewed; no file was changed.{tier_detail}",
                        path.display(),
                        rows.len()
                    ),
                    tier,
                    rows,
                    apply_available: true,
                }
            }
            Err(error) => model::VideoPreview {
                discovery: format!(
                    "Trusted video.txt could not be read: {error}. No file was changed."
                ),
                tier,
                rows: Vec::new(),
                apply_available: false,
            },
        },
        Ok(None) => {
            let rows = frametime_core::video_preset(core_tier)
                .into_iter()
                .map(|(setting, preset)| model::VideoPreviewRow {
                    setting: setting.trim_start_matches("setting.").into(),
                    current_and_recommended: format!("not discovered -> {}", preset.value),
                    status_and_note: format!("Not discovered: {}", preset.note),
                })
                .collect::<Vec<_>>();
            model::VideoPreview {
                discovery: format!(
                    "No trusted video.txt was found below {}. {} preset settings are shown without applying anything.{tier_detail}",
                    root_path.display(),
                    rows.len()
                ),
                tier,
                rows,
                apply_available: false,
            }
        }
        Err(error) => model::VideoPreview {
            discovery: format!(
                "Steam discovery refused {}: {error}. No file was changed.",
                root_path.display()
            ),
            tier,
            rows: Vec::new(),
            apply_available: false,
        },
    }
}
