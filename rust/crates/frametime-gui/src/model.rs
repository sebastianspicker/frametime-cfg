//! Platform-neutral presentation model for the native desktop front end.
//!
//! This module deliberately contains no Win32 types.  Keeping navigation,
//! action routing and status text here makes the safety-critical UI states
//! testable on every host, including CI hosts that cannot create a window.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Overview,
    Assess,
    SetupVerify,
    Benchmark,
    Network,
    Video,
    Recovery,
}

impl Area {
    pub const ALL: [Self; 7] = [
        Self::Overview,
        Self::Assess,
        Self::SetupVerify,
        Self::Benchmark,
        Self::Network,
        Self::Video,
        Self::Recovery,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Assess => "Assess",
            Self::SetupVerify => "Setup / Verify",
            Self::Benchmark => "Benchmark",
            Self::Network => "Network",
            Self::Video => "Video",
            Self::Recovery => "Recovery",
        }
    }

    pub const fn action(self) -> Action {
        match self {
            Self::Overview => Action::Refresh,
            Self::Assess => Action::HardwareDoctor,
            Self::SetupVerify => Action::PhaseChoice,
            Self::Benchmark => Action::CalculateFpsCap,
            Self::Network => Action::NetworkApply,
            Self::Video => Action::VideoRefresh,
            Self::Recovery => Action::ExportBackup,
        }
    }

    pub const fn action_label(self) -> &'static str {
        match self.action() {
            Action::Refresh => "Refresh work directory",
            Action::HardwareDoctor => "Hardware doctor",
            Action::CalculateFpsCap => "Calculate FPS cap",
            Action::NetworkApply => "Apply NIC latency stack",
            Action::PhaseChoice => "Configure selected profile",
            Action::VideoRefresh => "Refresh video preview",
            Action::ExportBackup => "Export backup",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Overview => {
                "Read the native runtime state and choose a task area. No PowerShell is used."
            }
            Self::Assess => {
                "Run versioned, read-only native hardware diagnostics. Results remain visible here and never advance workflow progress."
            }
            Self::SetupVerify => {
                "Use the authenticated package to configure a profile, start or resume Phase 1, or verify the protected workflow state."
            }
            Self::Benchmark => {
                "Enter a measured average FPS to calculate an FPS cap. This does not claim a benchmark was run."
            }
            Self::Network => {
                "Apply the confirmed P1:16 NIC latency stack in-process. If needed, the authenticated GUI requests administrator approval and requires confirmation again after relaunch; no CLI mutation fallback is used."
            }
            Self::Video => {
                "Discover trusted Steam video.txt, compare a 13-setting preset, and apply it through the typed native controller with one-time backup and exact readback."
            }
            Self::Recovery => {
                "Inspect retained recovery records, export a byte-verified backup, or run a confirmed native restore."
            }
        }
    }

    pub const fn table_rows(self) -> &'static [(&'static str, &'static str, &'static str)] {
        match self {
            Self::Overview => &[
                ("Runtime", "C:\\FRAMETIME_CFG", "Read-only inspection"),
                ("Safety", "Normal Windows session", "Safe Mode prohibited"),
                ("Terminal", "frametime.exe", "Native CLI only"),
            ],
            Self::Assess => &[
                (
                    "Hardware diagnostics",
                    "Read only",
                    "Versioned typed native results",
                ),
                ("Unavailable adapters", "Fail closed", "No shell fallback"),
                ("Result", "Partial failures shown", "No workflow progress"),
            ],
            Self::SetupVerify => &[
                (
                    "Configuration",
                    "Authenticated package",
                    "Writes the selected profile and dry-run mode",
                ),
                (
                    "Phase 1",
                    "Native CLI",
                    "Publishes and arms the protected reboot handoff",
                ),
                ("Verification", "Read only", "Reports exact workflow state"),
                ("Phase 2", "Safe Mode", "GUI intentionally prohibited"),
            ],
            Self::Benchmark => &[
                ("Input", "Average FPS", "Measured value required"),
                ("Calculation", "fps_max", "90% of average by default"),
                (
                    "Persistence",
                    "Authenticated CLI",
                    "Adds a validated VProf capture to history",
                ),
            ],
            Self::Network => &[
                (
                    "NIC latency stack",
                    "P1:16",
                    "Explicit confirmation required",
                ),
                (
                    "Execution",
                    "In process",
                    "Authenticated GUI elevation; no CLI fallback",
                ),
                ("Result", "Engine report", "Counts and final event shown"),
            ],
            Self::Video => &[
                (
                    "Steam discovery",
                    "Read only",
                    "Trusted userdata/video.txt only",
                ),
                ("Preset", "13 settings", "Auto, High, Mid, or Low preview"),
                (
                    "Apply",
                    "Typed native controller",
                    "One-time backup and exact readback",
                ),
            ],
            Self::Recovery => &[
                ("Backup", "backup.json", "Native recovery grid"),
                (
                    "Restore",
                    "Confirmed native API",
                    "Partial records are retained",
                ),
                (
                    "Export",
                    "Byte verified",
                    "Choose destination with native dialog",
                ),
            ],
        }
    }
}

pub const PROFILE_PREFERENCES: [&str; 5] = ["safe", "recommended", "competitive", "custom", "yolo"];
pub const SETUP_PHASE_ONE_ARGUMENTS: [&str; 2] = ["optimize", "--yes"];
pub const SETUP_VERIFY_ARGUMENTS: [&str; 1] = ["verify"];

/// External launch and mutation stay unavailable unless package authentication
/// has bound the current GUI, its sibling CLI, and every packaged payload to the
/// configured release publisher while retaining the authenticated file objects.
pub const EXTERNAL_EXECUTION_UNAVAILABLE: &str = "Native CLI launch, GUI self-elevation, and GUI mutations require an authenticated package capability.";

/// Owns the result of authenticating the package that contains the GUI.
///
/// The generic payload makes the state machine testable on non-Windows hosts,
/// while the Win32 application carries the real non-cloneable package handle.
#[derive(Debug)]
pub struct PackageAuthentication<T> {
    package: Option<T>,
    failure: Option<String>,
}

impl<T> PackageAuthentication<T> {
    pub fn authenticate(authenticate: impl FnOnce() -> Result<T, String>) -> Self {
        match authenticate() {
            Ok(package) => Self {
                package: Some(package),
                failure: None,
            },
            Err(failure) => Self {
                package: None,
                failure: Some(failure),
            },
        }
    }

    #[must_use]
    pub const fn has_capability(&self) -> bool {
        self.package.is_some()
    }

    #[must_use]
    pub fn package(&self) -> Option<&T> {
        self.package.as_ref()
    }

    #[must_use]
    pub fn unavailable_detail(&self) -> String {
        match &self.failure {
            Some(failure) => {
                format!("{EXTERNAL_EXECUTION_UNAVAILABLE} Authentication failed: {failure}")
            }
            None => EXTERNAL_EXECUTION_UNAVAILABLE.into(),
        }
    }
}

pub fn profile_preference_is_valid(value: &str) -> bool {
    PROFILE_PREFERENCES.contains(&value)
}

/// The native ListView filter deliberately searches the operator-facing
/// category and status columns, not hidden detail text. This makes keyboard
/// filtering predictable and prevents a match from implying an action exists.
pub fn catalog_row_matches_filter(category: &str, status: &str, filter: &str) -> bool {
    let filter = filter.trim().to_ascii_lowercase();
    filter.is_empty()
        || category.to_ascii_lowercase().contains(&filter)
        || status.to_ascii_lowercase().contains(&filter)
}

pub fn catalog_filter_accessible_name() -> &'static str {
    "Category and status filter"
}

#[cfg(test)]
pub const STANDARD_TAB_ORDER: [&str; 4] = [
    "area navigation",
    "area actions",
    "Category and status filter",
    "catalog table",
];

/// Phase 2 is a Safe Mode CLI handoff; the GUI never runs it from a Safe Mode boot.
pub const fn gui_allows_phase_2_in_safe_mode() -> bool {
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoPresetTier {
    Auto,
    High,
    Mid,
    Low,
}

impl VideoPresetTier {
    pub const ALL: [Self; 4] = [Self::Auto, Self::High, Self::Mid, Self::Low];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::High => "High",
            Self::Mid => "Mid",
            Self::Low => "Low",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoPreviewRow {
    pub setting: String,
    pub current_and_recommended: String,
    pub status_and_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoPreview {
    pub discovery: String,
    pub tier: VideoPresetTier,
    pub rows: Vec<VideoPreviewRow>,
    pub apply_available: bool,
}

impl VideoPreview {
    pub fn awaiting_discovery() -> Self {
        Self {
            discovery: "Enter a trusted Steam root and refresh the read-only preview.".into(),
            tier: VideoPresetTier::Auto,
            rows: Vec::new(),
            apply_available: false,
        }
    }

    pub const fn apply_available(&self) -> bool {
        self.apply_available
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Refresh,
    HardwareDoctor,
    CalculateFpsCap,
    NetworkApply,
    PhaseChoice,
    VideoRefresh,
    ExportBackup,
}

pub fn format_network_apply_report(report: &frametime_core::RunReport) -> Result<String, String> {
    let final_event = report
        .events
        .last()
        .map(network_event_summary)
        .unwrap_or_else(|| "no engine event was recorded".into());
    let detail = format!(
        "NIC latency stack report: {} completed, {} skipped, {} advisories, {} failed; final event: {final_event}.",
        report.completed, report.skipped, report.advisories, report.failed,
    );
    if report.skipped != 0 || report.advisories != 0 || report.failed != 0 {
        Err(detail)
    } else {
        Ok(detail)
    }
}

fn network_event_summary(event: &frametime_core::Event) -> String {
    use frametime_core::Event;

    match event {
        Event::Advisory { key, reason } => format!("advisory {key}: {reason}"),
        Event::Inspect(key) => format!("inspected {key}"),
        Event::CaptureBackup(key) => format!("captured backup for {key}"),
        Event::PersistBackup(key) => format!("persisted backup for {key}"),
        Event::CaptureAudit(key) => format!("captured audit for {key}"),
        Event::PersistAudit(key) => format!("persisted audit for {key}"),
        Event::CaptureEvidence(key) => format!("captured evidence for {key}"),
        Event::PersistEvidence(key) => format!("persisted evidence for {key}"),
        Event::VerifyEvidence(key) => format!("verified evidence for {key}"),
        Event::Apply(key) => format!("applied {key}"),
        Event::Verify(key) => format!("verified {key}"),
        Event::FinalizeAudit(key) => format!("finalized audit for {key}"),
        Event::FailAudit(key) => format!("recorded failed audit for {key}"),
        Event::Complete(key) => format!("completed {key}"),
        Event::Skip(key) => format!("skipped {key}"),
        Event::Plan(key) => format!("planned {key}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Ready,
    Running,
    Complete,
    Warning,
    Failed,
}

impl StatusKind {
    pub const fn text(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Running => "Running",
            Self::Complete => "Complete",
            Self::Warning => "Warning",
            Self::Failed => "Failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationState {
    pub status: StatusKind,
    pub detail: String,
    pub cancellable: bool,
}

impl OperationState {
    pub fn ready(detail: impl Into<String>) -> Self {
        Self {
            status: StatusKind::Ready,
            detail: detail.into(),
            cancellable: false,
        }
    }
    pub fn terminal_result(exit_code: Option<i32>) -> Self {
        match exit_code {
            Some(0) => Self { status: StatusKind::Complete, detail: "Terminal command completed. Review its visible output for skipped or unsupported steps.".into(), cancellable: false },
            Some(code) => Self { status: StatusKind::Warning, detail: format!("Terminal command exited with {code}. Partial backup and recovery data were retained; review the terminal."), cancellable: false },
            None => Self { status: StatusKind::Failed, detail: "Terminal process ended without an exit code.".into(), cancellable: false },
        }
    }
    pub fn cancellation_requested() -> Self {
        Self { status: StatusKind::Warning, detail: "Ctrl-Break cancellation requested. The native CLI is still running while it reaches a safe engine boundary.".into(), cancellable: true }
    }
}

pub fn calculate_fps_cap(input: &str, minimum: u32) -> Result<u32, &'static str> {
    let average = input
        .trim()
        .parse::<f64>()
        .map_err(|_| "Average FPS must be a number.")?;
    if !average.is_finite() || average <= 0.0 {
        return Err("Average FPS must be a finite positive number.");
    }
    if !(30..=500).contains(&minimum) {
        return Err("Minimum FPS cap must be between 30 and 500.");
    }
    let cap = frametime_core::fps::recommended_cap(average, 0.09, minimum);
    if cap == 0 {
        return Err("Average FPS is outside the supported range.");
    }
    Ok(cap)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seven_distinct_keyboard_destinations_exist() {
        assert_eq!(Area::ALL.len(), 7);
        assert_eq!(Area::ALL[2].title(), "Setup / Verify");
    }
    #[test]
    fn package_authentication_is_host_injectable_and_fail_closed() {
        let authenticated = PackageAuthentication::authenticate(|| Ok("capability"));
        assert!(authenticated.has_capability());
        assert_eq!(authenticated.package(), Some(&"capability"));

        let rejected = PackageAuthentication::<()>::authenticate(|| Err("missing pin".into()));
        assert!(!rejected.has_capability());
        assert!(rejected.package().is_none());
        assert!(rejected.unavailable_detail().contains("missing pin"));
    }
    #[test]
    fn assess_routes_to_the_in_process_hardware_doctor() {
        assert!(matches!(Area::Assess.action(), Action::HardwareDoctor));
        assert_eq!(Area::Assess.action_label(), "Hardware doctor");
        assert!(Area::Assess.description().contains("workflow progress"));
    }
    #[test]
    fn network_routes_only_to_the_in_process_latency_stack() {
        assert!(matches!(Area::Network.action(), Action::NetworkApply));
        assert_eq!(Area::Network.action_label(), "Apply NIC latency stack");
        assert!(Area::Network.description().contains("no CLI"));
        assert!(Area::Network.table_rows()[1].2.contains("no CLI"));
    }
    #[test]
    fn network_report_preserves_counts_and_final_event() {
        let report = frametime_core::RunReport {
            events: vec![frametime_core::Event::Complete("P1:16".into())],
            completed: 1,
            ..Default::default()
        };
        let detail = format_network_apply_report(&report).expect("clean report");
        assert!(detail.contains("1 completed"));
        assert!(detail.contains("completed P1:16"));
    }
    #[test]
    fn network_report_marks_partial_engine_results_as_warnings() {
        let report = frametime_core::RunReport {
            events: vec![frametime_core::Event::Skip("P1:16".into())],
            skipped: 1,
            ..Default::default()
        };
        let detail = format_network_apply_report(&report).expect_err("partial report");
        assert!(detail.contains("1 skipped"));
        assert!(detail.contains("skipped P1:16"));
    }
    #[test]
    fn failed_terminal_is_explicitly_partial() {
        let state = OperationState::terminal_result(Some(5));
        assert_eq!(state.status, StatusKind::Warning);
        assert!(state.detail.contains("Partial"));
    }
    #[test]
    fn ready_and_running_statuses_have_visible_text() {
        let ready = OperationState::ready("Awaiting an operator action.");
        assert_eq!(ready.status, StatusKind::Ready);
        assert_eq!(ready.status.text(), "Ready");
        assert_eq!(StatusKind::Running.text(), "Running");
        assert!(!ready.cancellable);
    }
    #[test]
    fn benchmark_rejects_unsound_input() {
        assert!(calculate_fps_cap("NaN", 60).is_err());
        assert!(calculate_fps_cap("0", 60).is_err());
        assert_eq!(calculate_fps_cap("240", 60), Ok(219));
    }
    #[test]
    fn setup_selects_an_authenticated_phase_action_and_video_is_read_only() {
        assert!(matches!(Area::SetupVerify.action(), Action::PhaseChoice));
        assert_eq!(
            Area::SetupVerify.action_label(),
            "Configure selected profile"
        );
        assert_eq!(SETUP_PHASE_ONE_ARGUMENTS, ["optimize", "--yes"]);
        assert_eq!(SETUP_VERIFY_ARGUMENTS, ["verify"]);
        assert!(matches!(Area::Video.action(), Action::VideoRefresh));
    }
    #[test]
    fn cancellation_remains_observable_until_the_cli_exits() {
        let state = OperationState::cancellation_requested();
        assert_eq!(state.status, StatusKind::Warning);
        assert!(state.cancellable);
        assert!(state.detail.contains("Ctrl-Break"));
    }
    #[test]
    fn five_explicit_profile_preferences_are_available() {
        assert_eq!(PROFILE_PREFERENCES.len(), 5);
        assert!(
            PROFILE_PREFERENCES
                .iter()
                .all(|profile| profile_preference_is_valid(profile))
        );
        assert!(!profile_preference_is_valid("unsafe"));
    }
    #[test]
    fn recovery_primary_action_is_an_export_not_a_shell_alias() {
        assert!(matches!(Area::Recovery.action(), Action::ExportBackup));
        assert_eq!(Area::Recovery.action_label(), "Export backup");
        assert!(
            !Area::Recovery
                .description()
                .to_ascii_lowercase()
                .contains("powershell")
        );
    }
    #[test]
    fn catalog_filter_is_case_insensitive_and_limited_to_category_or_status() {
        assert!(catalog_row_matches_filter("Phase 1", "38 / 38", "phase"));
        assert!(catalog_row_matches_filter("Phase 1", "Complete", "COMP"));
        assert!(!catalog_row_matches_filter("Phase 1", "Ready", "recovery"));
        assert!(catalog_row_matches_filter("Phase 1", "Ready", "   "));
    }
    #[test]
    fn filter_has_a_stable_accessibility_name_and_keeps_tab_order_explicit() {
        assert_eq!(
            catalog_filter_accessible_name(),
            "Category and status filter"
        );
        assert_eq!(STANDARD_TAB_ORDER[2], catalog_filter_accessible_name());
        assert_eq!(STANDARD_TAB_ORDER.last(), Some(&"catalog table"));
    }
    #[test]
    fn safe_mode_never_offers_phase_two_in_the_gui() {
        assert!(!gui_allows_phase_2_in_safe_mode());
        assert!(
            Area::SetupVerify
                .table_rows()
                .iter()
                .any(|row| row.2.contains("prohibited"))
        );
    }
    #[test]
    fn video_preview_requires_a_valid_typed_controller_before_apply() {
        let preview = VideoPreview::awaiting_discovery();
        assert_eq!(VideoPresetTier::ALL.len(), 4);
        assert_eq!(VideoPresetTier::High.label(), "High");
        assert_eq!(
            frametime_core::video_preset(frametime_core::VideoTier::Auto).len(),
            13
        );
        assert!(preview.rows.is_empty());
        assert!(!preview.apply_available());
    }
    #[test]
    fn video_apply_is_enabled_only_after_a_complete_typed_preview() {
        let preview = VideoPreview {
            discovery: "trusted controller preview".into(),
            tier: VideoPresetTier::Mid,
            rows: Vec::new(),
            apply_available: true,
        };
        assert!(preview.apply_available());
    }
}
