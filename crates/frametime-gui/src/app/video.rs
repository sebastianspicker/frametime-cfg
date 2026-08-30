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
        .unwrap_or(model::VideoPresetTier::RawLatency)
}

pub(super) fn core_video_goal(tier: model::VideoPresetTier) -> frametime_domain::VideoGoal {
    match tier {
        model::VideoPresetTier::RawLatency => frametime_domain::VideoGoal::RawLatency,
        model::VideoPresetTier::NvidiaVrr => frametime_domain::VideoGoal::NvidiaVrr,
        model::VideoPresetTier::AmdFreeSync => frametime_domain::VideoGoal::AmdFreeSync,
        model::VideoPresetTier::GpuConstrained => frametime_domain::VideoGoal::GpuConstrained,
    }
}

pub(super) fn build_video_preview(root: &str, tier: model::VideoPresetTier) -> model::VideoPreview {
    if root.is_empty() {
        return model::VideoPreview {
            discovery: "Steam root is required for trusted read-only discovery.".into(),
            tier,
            rows: Vec::new(),
        };
    }
    let root_path = Path::new(root);
    let goal = core_video_goal(tier);
    let preview = frametime_app::preview_video(root_path, goal);
    model::VideoPreview {
        discovery: preview.discovery,
        tier,
        rows: preview
            .rows
            .into_iter()
            .map(|row| model::VideoPreviewRow {
                setting: row.setting,
                current_and_recommended: row.current_and_recommended,
                status_and_note: row.status_and_note,
            })
            .collect(),
    }
}
