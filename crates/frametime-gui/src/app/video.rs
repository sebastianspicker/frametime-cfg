use super::*;

pub(super) fn refresh_video_preview(window: HWND) {
    refresh_area_data(window, Area::Video);
}

pub(super) fn selected_video_tier(control: HWND) -> model::VideoPresetTier {
    let index = unsafe { SendMessageW(control, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0 };
    selected_or_default(
        index,
        &model::VideoPresetTier::ALL,
        model::VideoPresetTier::RawLatency,
    )
}

pub(super) fn core_video_goal(tier: model::VideoPresetTier) -> frametime_domain::video::VideoGoal {
    match tier {
        model::VideoPresetTier::RawLatency => frametime_domain::video::VideoGoal::RawLatency,
        model::VideoPresetTier::NvidiaVrr => frametime_domain::video::VideoGoal::NvidiaVrr,
        model::VideoPresetTier::AmdFreeSync => frametime_domain::video::VideoGoal::AmdFreeSync,
        model::VideoPresetTier::GpuConstrained => {
            frametime_domain::video::VideoGoal::GpuConstrained
        }
    }
}
