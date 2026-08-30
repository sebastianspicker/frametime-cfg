//! Shared application/use-case boundary for the authenticated frametime
//! workflow.  Frontends parse and render; this crate coordinates domain policy
//! with the Windows adapter and never accepts a caller-selected persistence
//! root.

mod actions;
mod benchmark;
pub mod commands;
mod outcomes;
mod workflow;

use std::{path::Path, sync::OnceLock};

use frametime_domain::{
    OptionalCfgAsset, Profile, Step,
    hardware::{DiagnosticCommand, DiagnosticEnvelope},
};
pub use frametime_windows::AuthenticatedPackage;
use frametime_windows::authenticate_current_package;

pub use actions::{run_driver_plan, run_dry, run_hardware_diagnostic, run_prepare_nvidia};
pub use benchmark::{run_baseline_benchmark, run_fps_cap};
pub use commands::{
    Branch, CleanupMode, Command, DriverCommand, FpsRequest, FpsStrategyValue, HardwareCommand,
    VprofBenchmarkRequest,
};
pub use outcomes::{
    BackupSummary, BackupSummaryEntry, BenchmarkOutcome, BenchmarkPersistence, CleanupSummary,
    CommandOutcome, DriverPlanOutcome, DriverPreparationOutcome, DryRunSummary, FpsCapOutcome,
    FpsCapStrategyLabel, HardwareDiagnosticOutcome, LogReadModel, RunSummary, VerificationSummary,
    VideoPreview, VideoPreviewRow,
};
pub use workflow::{run_final_benchmark, run_live};

#[derive(Debug, Clone, Default)]
pub struct OverviewReadModel {
    pub progress: frametime_domain::Progress,
    pub state: frametime_domain::State,
    pub history: Vec<frametime_domain::benchmark::BenchmarkRecord>,
}

/// Frontend-safe read model. The Windows adapter owns all filesystem access.
pub fn read_overview() -> Result<OverviewReadModel, ApplicationError> {
    Ok(OverviewReadModel {
        progress: frametime_windows::load_progress().map_err(ApplicationError::failed)?,
        state: frametime_windows::load_state().map_err(ApplicationError::failed)?,
        history: frametime_windows::load_benchmark_history().map_err(ApplicationError::failed)?,
    })
}

pub fn read_recovery() -> Result<frametime_domain::BackupFile, ApplicationError> {
    frametime_windows::load_backup().map_err(ApplicationError::failed)
}

pub fn read_backup_summary() -> Result<BackupSummary, ApplicationError> {
    let summary = frametime_windows::read_backup_summary().map_err(ApplicationError::failed)?;
    Ok(BackupSummary {
        entries: summary
            .entries
            .into_iter()
            .map(|entry| BackupSummaryEntry {
                kind: entry.kind,
                count: entry.count,
            })
            .collect(),
    })
}

pub fn read_log() -> Result<LogReadModel, ApplicationError> {
    Ok(LogReadModel {
        text: frametime_windows::read_log().map_err(ApplicationError::failed)?,
    })
}

pub fn preview_video(root: &Path, goal: frametime_domain::VideoGoal) -> VideoPreview {
    match frametime_windows::preview_video(root, goal) {
        Ok(Some(preview)) => VideoPreview {
            discovery: format!(
                "Trusted cs2_video.txt discovered at {} with {} root. {} UI guidance item(s) shown; no file can be changed.",
                preview.video_path.display(),
                preview.document_root.label(),
                preview.rows.len()
            ),
            rows: preview
                .rows
                .into_iter()
                .map(|row| VideoPreviewRow {
                    setting: row.setting,
                    current_and_recommended: row.recommended,
                    status_and_note: format!("{:?}: {}", row.status, row.note),
                })
                .collect(),
        },
        Ok(None) => VideoPreview {
            discovery: format!(
                "No trusted numeric-userdata cs2_video.txt was found below {}. {} UI guidance item(s) are shown without a mutation path.",
                root.display(),
                frametime_domain::video_guidance(goal).len()
            ),
            rows: frametime_domain::video_guidance(goal)
                .into_iter()
                .map(|row| VideoPreviewRow {
                    setting: row.setting,
                    current_and_recommended: row.recommended,
                    status_and_note: format!("Guidance: {}", row.note),
                })
                .collect(),
        },
        Err(error) => VideoPreview {
            discovery: format!(
                "Steam discovery refused {}: {error}. No file was changed.",
                root.display()
            ),
            rows: Vec::new(),
        },
    }
}

pub fn is_supported_windows_host() -> bool {
    frametime_windows::platform_is_supported()
}

#[derive(Debug)]
pub enum ApplicationError {
    Invalid(String),
    Failed(String),
}

impl ApplicationError {
    pub fn failed(value: impl Into<String>) -> Self {
        Self::Failed(value.into())
    }
}

impl std::fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(value) | Self::Failed(value) => formatter.write_str(value),
        }
    }
}

impl std::error::Error for ApplicationError {}

/// Frontends install their terminal/UI-owned consent and cancellation hooks
/// once during startup.  The business boundary owns the resulting phase
/// coordination, trusted root, and durable handoff semantics.
#[derive(Clone, Copy)]
pub struct Interaction {
    pub prompt_for_step: fn(&Step) -> bool,
    pub cancellation_requested: fn() -> bool,
}

static INTERACTION: OnceLock<Interaction> = OnceLock::new();

pub fn install_interaction(interaction: Interaction) -> Result<(), ApplicationError> {
    INTERACTION
        .set(interaction)
        .map_err(|_| ApplicationError::failed("application interaction is already installed"))
}

pub(crate) fn prompt_for_step(step: &Step) -> bool {
    INTERACTION
        .get()
        .is_some_and(|hooks| (hooks.prompt_for_step)(step))
}

pub(crate) fn cancellation_requested() -> bool {
    INTERACTION
        .get()
        .is_some_and(|hooks| (hooks.cancellation_requested)())
}

/// Authentication remains a frontend startup concern.  The returned
/// non-cloneable capability is retained only for the operation lifetime.
pub fn authenticate_package() -> Result<AuthenticatedPackage, ApplicationError> {
    authenticate_current_package().map_err(|error| {
        ApplicationError::failed(format!(
            "authenticated release package is required: {error}"
        ))
    })
}

pub(crate) fn require_authenticated_package() -> Result<AuthenticatedPackage, ApplicationError> {
    authenticate_package()
}

pub fn run_network_stack(
    package: &AuthenticatedPackage,
) -> Result<frametime_domain::RunReport, ApplicationError> {
    frametime_windows::run_network_stack_transaction(package).map_err(ApplicationError::failed)
}

pub fn restore_all_from_package(package: &AuthenticatedPackage) -> Result<(), ApplicationError> {
    frametime_windows::restore_all_from_package(package).map_err(ApplicationError::failed)
}

pub fn restore_selected_from_package(
    package: &AuthenticatedPackage,
    step: &str,
) -> Result<(), ApplicationError> {
    frametime_windows::restore_selected_from_package(package, step)
        .map_err(ApplicationError::failed)
}

pub fn clear_backup(package: &AuthenticatedPackage) -> Result<(), ApplicationError> {
    frametime_windows::clear_backup(package).map_err(ApplicationError::failed)
}

pub fn export_backup(destination: &Path) -> Result<(), ApplicationError> {
    frametime_windows::export_backup(destination).map_err(ApplicationError::failed)
}

pub fn diagnose(command: DiagnosticCommand) -> DiagnosticEnvelope {
    frametime_windows::WindowsHardwareDiagnostics::new().execute(command)
}

pub fn deploy_optional_cs2_cfgs(
    package: &AuthenticatedPackage,
    assets: &[OptionalCfgAsset],
) -> Result<Vec<std::path::PathBuf>, ApplicationError> {
    frametime_windows::deploy_optional_cs2_cfgs(package, assets).map_err(ApplicationError::failed)
}

pub fn configure_profile(
    package: &AuthenticatedPackage,
    profile: Profile,
    dry_run: bool,
) -> Result<frametime_domain::State, ApplicationError> {
    frametime_windows::configure_profile(package, profile, dry_run)
        .map_err(ApplicationError::failed)
}
