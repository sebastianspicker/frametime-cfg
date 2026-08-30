use frametime_domain::{
    CleanupReport, FinalBenchmarkReceipt, VerificationReport, fps::BenchmarkCapture,
};

/// Text is a presentation concern; application services return these values
/// so terminal and native UI adapters can render them independently.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunSummary {
    pub messages: Vec<String>,
}

/// Closed set of outcomes for the stateful live-command surface. This is not
/// a catch-all result type: every variant has a stable, command-specific
/// read model for adapters to render.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandOutcome {
    Run(RunSummary),
    Cleanup(CleanupSummary),
    Verification(Box<VerificationSummary>),
    BackupSummary(BackupSummary),
    Log(LogReadModel),
}

impl RunSummary {
    pub fn message(value: impl Into<String>) -> Self {
        Self {
            messages: vec![value.into()],
        }
    }

    pub fn extend(&mut self, other: Self) {
        self.messages.extend(other.messages);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunSummary {
    pub lines: Vec<String>,
    pub preview_failures: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverPlanOutcome {
    pub json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverPreparationOutcome {
    pub json: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HardwareDiagnosticOutcome {
    pub envelope: frametime_domain::hardware::DiagnosticEnvelope,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FpsCapOutcome {
    pub cap: u32,
    pub capture: BenchmarkCapture,
    pub strategy: FpsCapStrategyLabel,
    pub copied_to_clipboard: bool,
    pub persistence: BenchmarkPersistence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsCapStrategyLabel {
    RawUncapped,
    RawMeasured,
    VrrCeiling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkPersistence {
    Persisted,
    Disabled,
    UnsupportedHost,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkOutcome {
    pub capture: BenchmarkCapture,
    pub receipt: Option<FinalBenchmarkReceipt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CleanupSummary {
    pub report: CleanupReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerificationSummary {
    pub state: frametime_domain::State,
    pub progress: frametime_domain::Progress,
    pub report: VerificationReport,
    pub work_dir: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupSummary {
    pub entries: Vec<BackupSummaryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupSummaryEntry {
    pub kind: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogReadModel {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoPreview {
    pub discovery: String,
    pub rows: Vec<VideoPreviewRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoPreviewRow {
    pub setting: String,
    pub current_and_recommended: String,
    pub status_and_note: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_summary_retains_presentation_neutral_messages() {
        let mut summary = RunSummary::message("first");
        summary.extend(RunSummary::message("second"));
        assert_eq!(summary.messages, ["first", "second"]);
    }

    #[test]
    fn backup_summary_represents_an_empty_recovery_grid_without_console_text() {
        assert!(
            BackupSummary {
                entries: Vec::new()
            }
            .entries
            .is_empty()
        );
    }
}
