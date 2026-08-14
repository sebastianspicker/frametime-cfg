use std::{fs, path::Path};

use frametime_core::{
    Config, Engine, Event, Phase, Profile, Progress, VerificationItem, VerificationStatus,
    requires_irreversible_acknowledgement, step_catalog,
};
use frametime_driver::{DriverPlanInput, generate_dry_run_plan};
use frametime_hardware::{
    DiagnosticCommand, DiagnosticStatus, EtwFrameCaptureRequest, WheaEventsRequest,
};
use frametime_hardware_windows::WindowsHardwareDiagnostics;
use frametime_windows::{
    AuthenticatedPackage, FinalBenchmarkStatus, PlannerBackend, WINDOWS_WORK_DIR,
    arm_driver_cleanup, cleanup_full, cleanup_quick, final_benchmark_status, load_progress,
    load_state, verify_settings,
};
use serde_json::json;

use crate::{
    cli::{Branch, CleanupMode, HardwareCommand},
    error::AppError,
    package_auth::require_authenticated_package,
};

pub(crate) use crate::benchmark::{
    persist_final_benchmark_capture, read_final_benchmark_capture, run_baseline_benchmark,
    run_fps_cap,
};

pub(crate) fn run_dry(branch: Branch) -> Result<(), AppError> {
    let _config = load_config()?;
    let mut preview_failures = 0_usize;
    let branches = branch
        .number()
        .map_or_else(|| vec![1, 2, 3, 4], |value| vec![value]);
    for (index, gpu) in branches.iter().enumerate() {
        if branches.len() == 4 {
            println!("===== GPU BRANCH {} OF 4 =====", index + 1);
        }
        let mut engine = Engine::new(PlannerBackend::new(*gpu), Progress::default());
        for phase in [Phase::One, Phase::Two, Phase::Three] {
            let steps = step_catalog()
                .iter()
                .filter(|step| step.phase == phase)
                .copied()
                .collect::<Vec<_>>();
            let report = engine
                .run(&steps, Profile::Custom)
                .map_err(|error| AppError::failed(format!("preview issue (DRY-RUN): {error}")))?;
            preview_failures += report.failed;
            for event in report.events {
                if let Event::Plan(line) = event {
                    println!("[DRY-RUN] {line}");
                }
            }
            println!("PHASE {} PREVIEW COMPLETE", phase as u8);
        }
        println!("ALL 3 PHASES PREVIEW COMPLETE");
    }
    if branches.len() == 4 {
        println!("ALL FOUR GPU BRANCH PREVIEWS COMPLETE");
    }
    if preview_failures == 0 {
        Ok(())
    } else {
        Err(AppError::Failed(format!(
            "dry-run completed with {preview_failures} unsupported action preview(s)"
        )))
    }
}

pub(crate) fn run_driver_plan(input: &Path) -> Result<(), AppError> {
    let bytes = fs::read(input)
        .map_err(|error| AppError::failed(format!("read driver plan input: {error}")))?;
    let request: DriverPlanInput = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Invalid(format!("invalid driver plan input JSON: {error}")))?;
    let plan = generate_dry_run_plan(&request)
        .map_err(|error| AppError::Invalid(format!("invalid driver evidence: {error}")))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&plan)
            .map_err(|error| AppError::failed(format!("serialize driver plan: {error}")))?
    );
    Ok(())
}

/// Keep orchestration intentionally thin: the Windows boundary owns the fixed
/// host policy, retained capability, signature verification, and durable
/// readback transaction.
pub(crate) fn run_prepare_nvidia(
    artifact_id: &str,
    artifact_file_name: &str,
    server_path: &str,
) -> Result<(), AppError> {
    let _package = require_authenticated_package()?;
    let transaction = frametime_windows::prepare_nvidia_driver(
        Path::new(WINDOWS_WORK_DIR),
        artifact_id.into(),
        artifact_file_name.into(),
        server_path.into(),
    )
    .map_err(AppError::failed)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&transaction)
            .map_err(|error| AppError::failed(format!("serialize driver transaction: {error}")))?
    );
    Ok(())
}

pub(crate) fn run_hardware_diagnostic(command: HardwareCommand) -> Result<(), AppError> {
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
    println!(
        "{}",
        serde_json::to_string_pretty(&envelope)
            .map_err(|error| AppError::failed(format!("serialize hardware diagnostic: {error}")))?
    );
    if envelope.status == DiagnosticStatus::Success {
        Ok(())
    } else {
        Err(AppError::failed(envelope.error.map_or_else(
            || "hardware diagnostic failed".into(),
            |error| error.message,
        )))
    }
}

fn load_config() -> Result<Config, AppError> {
    let executable = std::env::current_exe()
        .map_err(|error| AppError::failed(format!("resolve executable: {error}")))?;
    let adjacent = executable
        .parent()
        .ok_or_else(|| AppError::failed("executable has no parent directory"))?
        .join("frametime.toml");
    if adjacent.exists() {
        return Config::load(adjacent).map_err(|error| AppError::failed(error.to_string()));
    }
    #[cfg(debug_assertions)]
    {
        let development = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../frametime.toml");
        Config::load(development).map_err(|error| AppError::failed(error.to_string()))
    }
    #[cfg(not(debug_assertions))]
    Err(AppError::failed(
        "required frametime.toml was not found beside frametime.exe",
    ))
}

pub(crate) fn cleanup(
    mode: CleanupMode,
    yes: bool,
    acknowledge_irreversible: bool,
    package: &AuthenticatedPackage,
) -> Result<(), AppError> {
    require_cleanup_confirmation(mode, yes, acknowledge_irreversible)?;
    let work_dir = Path::new(WINDOWS_WORK_DIR);
    let report = match mode {
        CleanupMode::Quick => cleanup_quick(work_dir, package),
        CleanupMode::Full => cleanup_full(work_dir, package),
        CleanupMode::Driver => arm_driver_cleanup(work_dir, package),
    }
    .map_err(AppError::failed)?;
    for result in &report.action_results {
        let (is_failure, line) = cleanup_result_line(result);
        if is_failure {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
    println!("Affected items: {}", report.affected_items());
    if report.restart_required {
        println!("Restart required: yes (Winsock catalog reset).");
    }
    if report.is_complete() {
        Ok(())
    } else {
        Err(AppError::failed(
            "cleanup completed partially; failed or deferred actions remain unresolved",
        ))
    }
}

fn cleanup_result_line(result: &frametime_core::CleanupActionResult) -> (bool, String) {
    let line = match &result.outcome {
        frametime_core::CleanupActionOutcome::Completed { affected_items } => {
            format!("COMPLETED: {:?} ({affected_items} items)", result.action)
        }
        frametime_core::CleanupActionOutcome::Inapplicable { reason } => {
            format!("INAPPLICABLE: {:?}: {reason}", result.action)
        }
        frametime_core::CleanupActionOutcome::Deferred { reason } => {
            format!("DEFERRED: {:?}: {reason}", result.action)
        }
        frametime_core::CleanupActionOutcome::Skipped { reason } => {
            format!("SKIPPED: {:?}: {reason}", result.action)
        }
        frametime_core::CleanupActionOutcome::Failed { reason } => {
            return (true, format!("FAILED: {:?}: {reason}", result.action));
        }
    };
    (false, line)
}

pub(crate) fn require_cleanup_confirmation(
    mode: CleanupMode,
    yes: bool,
    acknowledge_irreversible: bool,
) -> Result<(), AppError> {
    require_yes(yes, "cleanup")?;
    let contract_mode = match mode {
        CleanupMode::Quick => frametime_core::CleanupMode::Quick,
        CleanupMode::Full => frametime_core::CleanupMode::Full,
        CleanupMode::Driver => frametime_core::CleanupMode::Driver,
    };
    if requires_irreversible_acknowledgement(contract_mode) && !acknowledge_irreversible {
        return Err(AppError::Invalid(
            "cleanup --mode full requires --acknowledge-irreversible after --yes".into(),
        ));
    }
    Ok(())
}

pub(crate) fn require_yes(yes: bool, command: &str) -> Result<(), AppError> {
    if yes {
        Ok(())
    } else {
        Err(AppError::Invalid(format!("{command} requires --yes")))
    }
}

pub(crate) fn verify_snapshot(work_dir: &Path) -> Result<(), AppError> {
    let state = load_state(work_dir).map_err(AppError::failed)?;
    let progress = load_progress(work_dir).map_err(AppError::failed)?;
    let report = verify_settings(work_dir).map_err(AppError::failed)?;
    let receipt_status = final_benchmark_status(work_dir).map_err(AppError::failed)?;
    let mut report = report;
    report
        .items
        .push(final_benchmark_verification_item(receipt_status));
    let items = report
        .items
        .iter()
        .map(|item| json!({ "status": item.status.label(), "name": item.name, "detail": item.detail }))
        .collect::<Vec<_>>();
    println!("{}", serde_json::to_string_pretty(&json!({ "readOnly": true, "workDir": WINDOWS_WORK_DIR, "state": state, "progress": progress, "items": items })).map_err(|error| AppError::failed(error.to_string()))?);
    if report.has_drift() {
        Err(AppError::failed(
            "verification found missing or changed items",
        ))
    } else {
        Ok(())
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
    fn cleanup_renderer_keeps_deferred_and_inapplicable_distinct_from_failures() {
        let deferred = frametime_core::CleanupActionResult {
            action: frametime_core::CleanupAction::ResetWinsockCatalog,
            outcome: frametime_core::CleanupActionOutcome::Deferred {
                reason: "native API unavailable".into(),
            },
        };
        let inapplicable = frametime_core::CleanupActionResult {
            action: frametime_core::CleanupAction::ClearAmdDxShaderCache,
            outcome: frametime_core::CleanupActionOutcome::Inapplicable {
                reason: "AMD absent".into(),
            },
        };
        assert_eq!(
            cleanup_result_line(&deferred),
            (
                false,
                "DEFERRED: ResetWinsockCatalog: native API unavailable".into()
            )
        );
        assert_eq!(
            cleanup_result_line(&inapplicable),
            (
                false,
                "INAPPLICABLE: ClearAmdDxShaderCache: AMD absent".into()
            )
        );
    }

    fn final_receipt() -> frametime_core::FinalBenchmarkReceipt {
        frametime_core::FinalBenchmarkReceipt {
            schema_version: 1,
            receipt_id: frametime_core::TransactionId::parse("fedcba9876543210fedcba9876543210")
                .expect("receipt id"),
            transaction_id: frametime_core::TransactionId::parse(
                "0123456789abcdef0123456789abcdef",
            )
            .expect("transaction id"),
            captured_utc: "2026-08-10 12:34:56".into(),
            avg_fps: 300.0,
            p1_fps: 180.0,
            runs: 3,
            fps_cap: 270,
            label: "After all optimizations".into(),
            unknown: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn final_benchmark_verification_maps_absent_coherent_and_incoherent_receipts() {
        assert_eq!(
            final_benchmark_verification_item(FinalBenchmarkStatus::Absent).status,
            VerificationStatus::Info
        );
        assert_eq!(
            final_benchmark_verification_item(FinalBenchmarkStatus::Coherent(final_receipt()))
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
