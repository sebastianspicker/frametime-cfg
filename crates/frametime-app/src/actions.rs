use std::{fs, path::Path};

use frametime_domain::driver::{DriverPlanInput, generate_dry_run_plan};
use frametime_domain::hardware::{
    DiagnosticCommand, DiagnosticStatus, EtwFrameCaptureRequest, WheaEventsRequest,
};
use frametime_domain::{
    catalog::{Phase, step_catalog},
    cleanup::requires_irreversible_acknowledgement,
    engine::{Engine, Event},
    policy::Profile,
    state::Progress,
    verification::{VerificationItem, VerificationStatus},
};
use frametime_windows::WindowsHardwareDiagnostics;
use frametime_windows::{
    AuthenticatedPackage, FinalBenchmarkStatus, PlannerBackend, WINDOWS_WORK_DIR,
    arm_driver_cleanup, cleanup_full, cleanup_quick, final_benchmark_status, load_progress,
    load_state, verify_settings,
};

use crate::commands::{
    Branch, BuildNvidiaLabRequest, CleanupMode, HardwareCommand, NvidiaInstallerSource,
    PrepareNvidiaRequest,
};
use crate::{
    ApplicationError, CleanupSummary, DriverPlanOutcome, DriverPreparationOutcome, DryRunSummary,
    HardwareDiagnosticOutcome, VerificationSummary, require_authenticated_package,
};

pub(crate) use crate::benchmark::{persist_final_benchmark_capture, read_final_benchmark_capture};

pub fn run_dry(branch: Branch) -> Result<DryRunSummary, ApplicationError> {
    let mut preview_failures = 0_usize;
    let mut lines = Vec::new();
    let branches = branch
        .number()
        .map_or_else(|| vec![1, 2, 3, 4], |value| vec![value]);
    for (index, gpu) in branches.iter().enumerate() {
        if branches.len() == 4 {
            lines.push(format!("===== GPU BRANCH {} OF 4 =====", index + 1));
        }
        let mut engine = Engine::new(PlannerBackend::new(*gpu), Progress::default());
        for phase in [Phase::One, Phase::Two, Phase::Three] {
            let steps = step_catalog()
                .iter()
                .filter(|step| step.id.phase == phase)
                .copied()
                .collect::<Vec<_>>();
            let report = engine.run(&steps, Profile::Custom).map_err(|error| {
                ApplicationError::failed(format!("preview issue (DRY-RUN): {error}"))
            })?;
            preview_failures += report.failed;
            for event in report.events {
                if let Event::Plan(line) = event {
                    lines.push(format!("[DRY-RUN] {line}"));
                }
            }
            lines.push(format!("PHASE {} PREVIEW COMPLETE", phase as u8));
        }
        lines.push("ALL 3 PHASES PREVIEW COMPLETE".into());
    }
    if branches.len() == 4 {
        lines.push("ALL FOUR GPU BRANCH PREVIEWS COMPLETE".into());
    }
    if preview_failures == 0 {
        Ok(DryRunSummary {
            lines,
            preview_failures,
        })
    } else {
        Err(ApplicationError::Failed(format!(
            "dry-run completed with {preview_failures} unsupported action preview(s)"
        )))
    }
}

pub fn run_driver_plan(input: &Path) -> Result<DriverPlanOutcome, ApplicationError> {
    let bytes = fs::read(input)
        .map_err(|error| ApplicationError::failed(format!("read driver plan input: {error}")))?;
    let request: DriverPlanInput = serde_json::from_slice(&bytes).map_err(|error| {
        ApplicationError::Invalid(format!("invalid driver plan input JSON: {error}"))
    })?;
    let plan = generate_dry_run_plan(&request)
        .map_err(|error| ApplicationError::Invalid(format!("invalid driver evidence: {error}")))?;
    Ok(DriverPlanOutcome {
        json: serde_json::to_string_pretty(&plan)
            .map_err(|error| ApplicationError::failed(format!("serialize driver plan: {error}")))?,
    })
}

/// Keep orchestration intentionally thin: the Windows boundary owns the fixed
/// host policy, retained capability, signature verification, and durable
/// readback transaction.
pub fn run_prepare_nvidia(
    artifact_id: &str,
    artifact_file_name: &str,
    server_path: &str,
) -> Result<DriverPreparationOutcome, ApplicationError> {
    let package = require_authenticated_package()?;
    let transaction = frametime_windows::prepare_nvidia_driver(
        &package,
        artifact_id.into(),
        artifact_file_name.into(),
        server_path.into(),
    )
    .map_err(ApplicationError::failed)?;
    Ok(DriverPreparationOutcome {
        json: serde_json::to_string_pretty(&transaction).map_err(|error| {
            ApplicationError::failed(format!("serialize driver transaction: {error}"))
        })?,
    })
}

pub fn run_prepare_nvidia_request(
    request: PrepareNvidiaRequest,
) -> Result<DriverPreparationOutcome, ApplicationError> {
    let package = require_authenticated_package()?;
    let source = match request.source {
        NvidiaInstallerSource::OfficialUrl(value) => {
            frametime_windows::NvidiaInstallerSource::OfficialUrl(value)
        }
        NvidiaInstallerSource::LocalInstaller(value) => {
            frametime_windows::NvidiaInstallerSource::LocalInstaller(value)
        }
    };
    let transaction = frametime_windows::prepare_nvidia_driver_with_options(
        &package,
        frametime_windows::NvidiaPreparationRequest {
            source,
            preset: request.preset,
            select: request.select,
            deselect: request.deselect,
            artifact_id: request.deprecated_artifact_id,
            artifact_file_name: request.deprecated_artifact_file_name,
        },
    )
    .map_err(ApplicationError::failed)?;
    driver_json_outcome(&transaction)
}

pub fn run_driver_inspect() -> Result<DriverPreparationOutcome, ApplicationError> {
    let status = frametime_windows::inspect_driver_status().map_err(ApplicationError::failed)?;
    driver_json_outcome(&status)
}

pub fn run_reconcile_nvidia_profiles(
    accept_driver_incompatible_profile_items: bool,
    yes: bool,
) -> Result<DriverPreparationOutcome, ApplicationError> {
    let transaction =
        frametime_windows::reconcile_nvidia_profiles(accept_driver_incompatible_profile_items, yes)
            .map_err(ApplicationError::failed)?;
    driver_json_outcome(&transaction)
}

pub fn run_build_nvidia_lab(
    request: BuildNvidiaLabRequest,
) -> Result<DriverPreparationOutcome, ApplicationError> {
    let manifest = frametime_windows::build_nvidia_lab(&frametime_windows::NvidiaLabBuildRequest {
        output: request.output,
        deep_inf: request.deep_inf,
        test_certificate_sha256: request.test_certificate_sha256,
        acknowledge_unqualified_driver: request.acknowledge_unqualified_driver,
    })
    .map_err(ApplicationError::failed)?;
    driver_json_outcome(&manifest)
}

fn driver_json_outcome(
    value: &impl serde::Serialize,
) -> Result<DriverPreparationOutcome, ApplicationError> {
    Ok(DriverPreparationOutcome {
        json: serde_json::to_string_pretty(value).map_err(|error| {
            ApplicationError::failed(format!("serialize driver result: {error}"))
        })?,
    })
}

pub fn run_hardware_diagnostic(
    command: HardwareCommand,
) -> Result<HardwareDiagnosticOutcome, ApplicationError> {
    let command = match command {
        HardwareCommand::Doctor => DiagnosticCommand::Doctor,
        HardwareCommand::Cpu => DiagnosticCommand::CpuIdentity,
        HardwareCommand::Gpu => DiagnosticCommand::GpuInventory,
        HardwareCommand::System => DiagnosticCommand::SystemStatus,
        HardwareCommand::Whea { max_records } => {
            DiagnosticCommand::WheaEvents(WheaEventsRequest { max_records })
        }
        HardwareCommand::Frames { duration_ms } => {
            DiagnosticCommand::EtwFrameCapture(EtwFrameCaptureRequest { duration_ms })
        }
    };
    let envelope = WindowsHardwareDiagnostics::new().execute(command);
    if envelope.status == DiagnosticStatus::Success {
        Ok(HardwareDiagnosticOutcome { envelope })
    } else {
        Err(ApplicationError::failed(envelope.error.map_or_else(
            || "hardware diagnostic failed".into(),
            |error| error.message,
        )))
    }
}

pub(crate) fn cleanup(
    mode: CleanupMode,
    yes: bool,
    acknowledge_irreversible: bool,
    package: &AuthenticatedPackage,
) -> Result<CleanupSummary, ApplicationError> {
    require_cleanup_confirmation(mode, yes, acknowledge_irreversible)?;
    let report = match mode {
        CleanupMode::Quick => cleanup_quick(package),
        CleanupMode::Full => cleanup_full(package),
        CleanupMode::Driver => arm_driver_cleanup(package),
    }
    .map_err(ApplicationError::failed)?;
    Ok(CleanupSummary { report })
}

pub(crate) fn require_cleanup_confirmation(
    mode: CleanupMode,
    yes: bool,
    acknowledge_irreversible: bool,
) -> Result<(), ApplicationError> {
    require_yes(yes, "cleanup")?;
    let contract_mode = match mode {
        CleanupMode::Quick => frametime_domain::cleanup::CleanupMode::Quick,
        CleanupMode::Full => frametime_domain::cleanup::CleanupMode::Full,
        CleanupMode::Driver => frametime_domain::cleanup::CleanupMode::Driver,
    };
    if requires_irreversible_acknowledgement(contract_mode) && !acknowledge_irreversible {
        return Err(ApplicationError::Invalid(
            "cleanup --mode full requires --acknowledge-irreversible after --yes".into(),
        ));
    }
    Ok(())
}

pub(crate) fn require_yes(yes: bool, command: &str) -> Result<(), ApplicationError> {
    if yes {
        Ok(())
    } else {
        Err(ApplicationError::Invalid(format!(
            "{command} requires --yes"
        )))
    }
}

pub(crate) fn verify_snapshot() -> Result<VerificationSummary, ApplicationError> {
    let state = load_state().map_err(ApplicationError::failed)?;
    let progress = load_progress().map_err(ApplicationError::failed)?;
    let report = verify_settings().map_err(ApplicationError::failed)?;
    let receipt_status = final_benchmark_status().map_err(ApplicationError::failed)?;
    let mut report = report;
    report
        .items
        .push(final_benchmark_verification_item(receipt_status));
    if report.has_drift() {
        Err(ApplicationError::failed(
            "verification found missing or changed items",
        ))
    } else {
        Ok(VerificationSummary {
            state,
            progress,
            report,
            work_dir: WINDOWS_WORK_DIR,
        })
    }
}

fn final_benchmark_verification_item(status: FinalBenchmarkStatus) -> VerificationItem {
    match status {
        FinalBenchmarkStatus::Absent => VerificationItem {
            status: VerificationStatus::Info,
            name: "P3:13 final benchmark".into(),
            detail: "no final benchmark receipt has been persisted".into(),
        },
        FinalBenchmarkStatus::Coherent(receipt) => VerificationItem {
            status: VerificationStatus::Ok,
            name: "P3:13 final benchmark".into(),
            detail: format!("coherent final benchmark receipt {}", receipt.receipt_id),
        },
        FinalBenchmarkStatus::Incoherent(error) => VerificationItem {
            status: VerificationStatus::Changed,
            name: "P3:13 final benchmark".into(),
            detail: format!("incoherent final benchmark receipt: {error}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_confirmation_requires_an_extra_acknowledgement_only_for_full() {
        assert!(require_cleanup_confirmation(CleanupMode::Quick, true, false).is_ok());
        assert!(require_cleanup_confirmation(CleanupMode::Driver, true, false).is_ok());
        assert!(require_cleanup_confirmation(CleanupMode::Full, true, false).is_err());
        assert!(require_cleanup_confirmation(CleanupMode::Full, true, true).is_ok());
        assert!(require_cleanup_confirmation(CleanupMode::Driver, false, true).is_err());
    }

    #[test]
    fn final_benchmark_verification_maps_absent_coherent_and_incoherent_receipts() {
        assert_eq!(
            final_benchmark_verification_item(FinalBenchmarkStatus::Absent).status,
            VerificationStatus::Info
        );
        assert_eq!(
            final_benchmark_verification_item(FinalBenchmarkStatus::Coherent(
                crate::test_support::final_benchmark_receipt(),
            ))
            .status,
            VerificationStatus::Ok
        );
        assert_eq!(
            final_benchmark_verification_item(FinalBenchmarkStatus::Incoherent("prefix".into()))
                .status,
            VerificationStatus::Changed
        );
    }
}
