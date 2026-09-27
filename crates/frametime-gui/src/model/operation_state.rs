#[derive(Debug, Clone, Copy)]
pub enum NativeAccess {
    ReadOnly,
    Write,
    ElevatedWrite,
}
impl NativeAccess {
    pub const fn requires_elevation(self) -> bool {
        matches!(self, Self::ElevatedWrite)
    }
    pub const fn blocks_close(self) -> bool {
        !matches!(self, Self::ReadOnly)
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
        match exit_code { Some(0) => Self { status: StatusKind::Complete, detail: "Terminal command completed. Review its visible output for skipped or unsupported steps.".into(), cancellable: false }, Some(code) => Self { status: StatusKind::Warning, detail: format!("Terminal command exited with {code}. Partial backup and recovery data were retained; review the terminal."), cancellable: false }, None => Self { status: StatusKind::Failed, detail: "Terminal process ended without an exit code.".into(), cancellable: false } }
    }
    pub fn cancellation_requested() -> Self {
        Self { status: StatusKind::Warning, detail: "Ctrl-Break cancellation requested. The native CLI is still running while it reaches a safe engine boundary.".into(), cancellable: true }
    }
}
#[cfg(test)]
pub fn calculate_fps_cap(input: &str, target: u32) -> Result<u32, String> {
    if target == 0 {
        return Ok(0);
    }
    let capture = frametime_domain::fps::parse_vprof_output_detailed(input)
        .map_err(|error| error.to_string())?;
    calculate_from_capture(&capture, target)
}

#[cfg(test)]
pub fn calculate_from_capture(
    capture: &frametime_domain::fps::ValidatedBenchmarkCapture,
    target: u32,
) -> Result<u32, String> {
    if target == 0 {
        return Ok(0);
    }
    if !(30..=1000).contains(&target) {
        return Err("Target FPS cap must be zero or between 30 and 1000.".into());
    }
    frametime_domain::fps::measured_fps_cap(
        frametime_domain::fps::FpsCapStrategy::RawLatency { measured_cap: Some(target) }, capture,
    ).ok_or_else(|| format!("Cap {target} rejected: {} of {} runs fail the requested cap; at least five runs and every unrounded P1 must support it.", capture.failing_runs(target), capture.observations().len()))
}

pub fn capture_rows(
    capture: &frametime_domain::fps::ValidatedBenchmarkCapture,
    target: u32,
) -> super::snapshots::Rows {
    let (minimum, maximum) = capture.p1_range();
    let mut rows = vec![(
        "Parsed VProf evidence".into(),
        format!("P1 min {minimum} / max {maximum}"),
        format!(
            "{} / {} runs support cap {target}",
            capture.supporting_runs(target),
            capture.observations().len()
        ),
    )];
    rows.extend(capture.observations().iter().enumerate().map(|(i, run)| {
        (
            format!("Run {}", i + 1),
            format!("Avg {} / P1 {}", run.average_fps, run.p1_fps),
            if run.p1_fps > run.average_fps {
                "Invalid run: P1 exceeds average".into()
            } else if run.p1_fps >= f64::from(target) {
                "Supports requested cap".into()
            } else {
                "Below requested cap".into()
            },
        )
    }));
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn statuses_remain_explicit() {
        assert!(!NativeAccess::ReadOnly.blocks_close());
        assert!(!NativeAccess::Write.requires_elevation());
        assert!(NativeAccess::Write.blocks_close());
        assert!(NativeAccess::ElevatedWrite.blocks_close());
        assert!(NativeAccess::ElevatedWrite.requires_elevation());
        let failed = OperationState::terminal_result(Some(5));
        assert_eq!(failed.status, StatusKind::Warning);
        assert!(failed.detail.contains("Partial"));
        let ready = OperationState::ready("Awaiting an operator action.");
        assert_eq!(ready.status.text(), "Ready");
        assert_eq!(StatusKind::Running.text(), "Running");
        assert!(!ready.cancellable);
        let cancelling = OperationState::cancellation_requested();
        assert_eq!(cancelling.status, StatusKind::Warning);
        assert!(cancelling.cancellable);
        assert!(cancelling.detail.contains("Ctrl-Break"));
    }
    #[test]
    fn benchmark_rejects_unsound_input() {
        assert_eq!(calculate_fps_cap("", 0), Ok(0));
        assert!(calculate_fps_cap("[VProf] FPS: Avg=300, P1=240", 240).is_err());
        let five_runs = "[VProf] FPS: Avg=300, P1=245\n".repeat(5);
        assert_eq!(calculate_fps_cap(&five_runs, 240), Ok(240));
        let parsed = frametime_domain::fps::parse_vprof_output_detailed(&five_runs).unwrap();
        assert_eq!(capture_rows(&parsed, 240).len(), 6);
        let unsound = "[VProf] FPS: Avg=600, P1=100\n".repeat(4) + "[VProf] FPS: Avg=600, P1=600";
        assert!(
            calculate_fps_cap(&unsound, 180)
                .unwrap_err()
                .contains("4 of 5")
        );
    }
}
