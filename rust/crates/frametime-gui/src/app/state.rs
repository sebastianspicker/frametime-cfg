use super::*;

pub(super) struct RetainedChild {
    pub(super) child: Child,
}

impl RetainedChild {
    pub(super) fn new(child: Child) -> Self {
        Self { child }
    }
}

pub(super) enum NativeOperation {
    Recovery,
    VideoApply,
    NetworkApply,
    Diagnostic,
}

pub(super) enum NativeWorkerResult {
    Transaction(Result<String, String>),
    Diagnostic(DiagnosticPresentation),
}

pub(super) struct AppState {
    pub(super) area: Area,
    pub(super) nav: [HWND; 7],
    pub(super) heading: HWND,
    pub(super) description: HWND,
    pub(super) status: HWND,
    pub(super) action: HWND,
    pub(super) secondary: HWND,
    pub(super) tertiary: HWND,
    pub(super) quaternary: HWND,
    pub(super) quinary: HWND,
    pub(super) cancel: HWND,
    pub(super) table: HWND,
    pub(super) fps_label: HWND,
    pub(super) fps_input: HWND,
    pub(super) min_label: HWND,
    pub(super) min_input: HWND,
    pub(super) vprof_input: HWND,
    pub(super) profile: HWND,
    pub(super) dry_run: HWND,
    pub(super) filter_label: HWND,
    pub(super) catalog_filter: HWND,
    pub(super) video_root_label: HWND,
    pub(super) video_root: HWND,
    pub(super) video_tier_label: HWND,
    pub(super) video_tier: HWND,
    pub(super) video_preview: model::VideoPreview,
    pub(super) package: model::PackageAuthentication<frametime_windows::AuthenticatedPackage>,
    pub(super) child: Option<RetainedChild>,
    pub(super) elevation_watchdog: Option<ElevationWatchdog>,
    pub(super) native_result: Option<Receiver<NativeWorkerResult>>,
    pub(super) native_operation: Option<NativeOperation>,
    pub(super) critical_operation: bool,
    pub(super) diagnostics: DiagnosticPresentation,
    pub(super) operation: OperationState,
    pub(super) last_focus: HWND,
    pub(super) high_contrast: bool,
}

impl AppState {
    pub(super) fn is_running(&self) -> bool {
        self.child.is_some() || self.elevation_watchdog.is_some() || self.native_result.is_some()
    }

    pub(super) fn is_cancellable(&self) -> bool {
        self.child.is_some()
    }
}

pub(crate) fn safe_mode_active() -> bool {
    // SAFETY: GetSystemMetrics has no pointer arguments and is safe to call on the UI thread.
    unsafe { GetSystemMetrics(SM_CLEANBOOT) != 0 }
}
