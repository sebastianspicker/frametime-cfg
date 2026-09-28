use std::process::ExitCode;

use clap::Parser;

use crate::{
    cli::{
        Branch, CleanupMode, Cli, Command, DriverCommand, FpsRequest, FpsStrategyValue,
        HardwareCommand, VprofBenchmarkRequest,
    },
    console::{install_cancellation_handler, interactive_menu},
    error::AppError,
    package_auth::run_authentication_smoke,
    render, site_register,
};

pub(crate) fn main() -> ExitCode {
    install_cancellation_handler();
    let _ = frametime_app::install_interaction(frametime_app::Interaction {
        prompt_for_step: crate::console::prompt_for_step,
        cancellation_requested: crate::console::cancellation_requested,
    });
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            match error {
                AppError::Invalid(_) => ExitCode::from(2),
                AppError::Failed(_) => ExitCode::FAILURE,
            }
        }
    }
}

fn run(mut cli: Cli) -> Result<(), AppError> {
    if cli.command.is_none() {
        cli.command = Some(interactive_menu()?);
    }
    match cli.command.expect("set above") {
        Command::DryRun { branch } => {
            present(frametime_app::run_dry(map_branch(branch)), render::dry_run)
        }
        Command::SiteRegister => site_register::print().map_err(AppError::Failed),
        Command::SmokeTest => {
            println!("SMOKE TEST OK: frametime");
            Ok(())
        }
        Command::PackageAuthSmoke => run_authentication_smoke(),
        Command::Exit => Ok(()),
        Command::FpsCap {
            average_fps,
            vprof_text,
            vprof_file,
            clipboard,
            strategy,
            measured_cap,
            refresh_hz,
            ceiling_margin_hz,
            label,
            copy,
            no_persist,
        } => present(
            frametime_app::run_fps_cap(map_fps(FpsRequest {
                average: average_fps,
                text: vprof_text,
                file: vprof_file,
                clipboard,
                strategy,
                measured_cap,
                refresh_hz,
                ceiling_margin_hz,
                label,
                copy,
                no_persist,
            })),
            render::fps_cap,
        ),
        Command::BaselineBenchmark {
            vprof_text,
            vprof_file,
            clipboard,
        } => present(
            frametime_app::run_baseline_benchmark(map_vprof(VprofBenchmarkRequest {
                text: vprof_text,
                file: vprof_file,
                clipboard,
            })),
            |outcome| render::benchmark(outcome, true),
        ),
        Command::FinalBenchmark {
            vprof_text,
            vprof_file,
            clipboard,
        } => present(
            frametime_app::run_final_benchmark(map_vprof(VprofBenchmarkRequest {
                text: vprof_text,
                file: vprof_file,
                clipboard,
            })),
            |outcome| render::benchmark(outcome, false),
        ),
        Command::Driver { command } => run_driver(command),
        Command::Hardware { command } => present(
            frametime_app::run_hardware_diagnostic(map_hardware(command)),
            |outcome| render::hardware(outcome).expect("serialize diagnostic"),
        ),
        command => present(
            frametime_app::run_live(map_live_command(command)),
            |outcome| render::command(outcome).expect("serialize verification"),
        ),
    }
}

fn run_driver(command: DriverCommand) -> Result<(), AppError> {
    match command {
        DriverCommand::Plan { input } => {
            present(frametime_app::run_driver_plan(&input), |outcome| {
                render::driver_json(outcome.json)
            })
        }
        DriverCommand::Inspect | DriverCommand::Status => {
            present(frametime_app::run_driver_inspect(), |outcome| {
                render::driver_json(outcome.json)
            })
        }
        DriverCommand::PrepareNvidia {
            official_url,
            local_installer,
            preset,
            select,
            deselect,
            artifact_id,
            artifact_file_name,
            server_path,
        } => run_prepare_nvidia(PrepareNvidiaArgs {
            official_url,
            local_installer,
            preset,
            select,
            deselect,
            artifact_id,
            artifact_file_name,
            server_path,
        }),
        DriverCommand::ReconcileProfiles {
            accept_driver_incompatible_profile_items,
            yes,
        } => present(
            frametime_app::run_reconcile_nvidia_profiles(
                accept_driver_incompatible_profile_items,
                yes,
            ),
            |outcome| render::driver_json(outcome.json),
        ),
        DriverCommand::BuildNvidiaLab {
            output,
            deep_inf,
            test_certificate_sha256,
            acknowledge_unqualified_driver,
        } => present(
            frametime_app::run_build_nvidia_lab(frametime_app::BuildNvidiaLabRequest {
                output,
                deep_inf,
                test_certificate_sha256,
                acknowledge_unqualified_driver,
            }),
            |outcome| render::driver_json(outcome.json),
        ),
    }
}

struct PrepareNvidiaArgs {
    official_url: Option<String>,
    local_installer: Option<std::path::PathBuf>,
    preset: crate::cli::NvidiaPresetValue,
    select: Vec<String>,
    deselect: Vec<String>,
    artifact_id: Option<String>,
    artifact_file_name: Option<String>,
    server_path: Option<String>,
}

enum NvidiaSourceResolution {
    Current(frametime_app::NvidiaInstallerSource),
    Deprecated {
        artifact_id: String,
        artifact_file_name: String,
        server_path: String,
    },
}

fn resolve_nvidia_source(args: &PrepareNvidiaArgs) -> Result<NvidiaSourceResolution, AppError> {
    match (&args.official_url, &args.local_installer, &args.server_path) {
        (Some(url), None, None) => Ok(NvidiaSourceResolution::Current(
            frametime_app::NvidiaInstallerSource::OfficialUrl(url.clone()),
        )),
        (None, Some(path), None) => Ok(NvidiaSourceResolution::Current(
            frametime_app::NvidiaInstallerSource::LocalInstaller(path.clone()),
        )),
        (None, None, Some(server_path)) => Ok(NvidiaSourceResolution::Deprecated {
            artifact_id: args.artifact_id.clone().ok_or_else(|| {
                AppError::Invalid("deprecated --server-path requires --artifact-id".into())
            })?,
            artifact_file_name: args.artifact_file_name.clone().ok_or_else(|| {
                AppError::Invalid("deprecated --server-path requires --artifact-file-name".into())
            })?,
            server_path: server_path.clone(),
        }),
        _ => Err(AppError::Invalid(
            "prepare-nvidia requires exactly one of --official-url or --local-installer".into(),
        )),
    }
}

fn run_prepare_nvidia(args: PrepareNvidiaArgs) -> Result<(), AppError> {
    match resolve_nvidia_source(&args)? {
        NvidiaSourceResolution::Deprecated {
            artifact_id,
            artifact_file_name,
            server_path,
        } => present(
            frametime_app::run_prepare_nvidia(&artifact_id, &artifact_file_name, &server_path),
            |outcome| render::driver_json(outcome.json),
        ),
        NvidiaSourceResolution::Current(source) => present(
            frametime_app::run_prepare_nvidia_request(frametime_app::PrepareNvidiaRequest {
                source,
                preset: args.preset.into(),
                select: args.select,
                deselect: args.deselect,
                deprecated_artifact_id: args.artifact_id,
                deprecated_artifact_file_name: args.artifact_file_name,
            }),
            |outcome| render::driver_json(outcome.json),
        ),
    }
}

fn app_result<T>(result: Result<T, frametime_app::ApplicationError>) -> Result<T, AppError> {
    result.map_err(|error| match error {
        frametime_app::ApplicationError::Invalid(message) => AppError::Invalid(message),
        frametime_app::ApplicationError::Failed(message) => AppError::Failed(message),
    })
}

fn present<T>(
    result: Result<T, frametime_app::ApplicationError>,
    render: impl FnOnce(T),
) -> Result<(), AppError> {
    render(app_result(result)?);
    Ok(())
}

const fn map_branch(value: Branch) -> frametime_app::Branch {
    match value {
        Branch::NvidiaRtx5000 => frametime_app::Branch::NvidiaRtx5000,
        Branch::Nvidia => frametime_app::Branch::Nvidia,
        Branch::Amd => frametime_app::Branch::Amd,
        Branch::IntelArc => frametime_app::Branch::IntelArc,
        Branch::All => frametime_app::Branch::All,
    }
}

const fn map_cleanup(value: CleanupMode) -> frametime_app::CleanupMode {
    match value {
        CleanupMode::Quick => frametime_app::CleanupMode::Quick,
        CleanupMode::Full => frametime_app::CleanupMode::Full,
        CleanupMode::Driver => frametime_app::CleanupMode::Driver,
    }
}

const fn map_fps_strategy(value: FpsStrategyValue) -> frametime_app::FpsStrategyValue {
    match value {
        FpsStrategyValue::Raw => frametime_app::FpsStrategyValue::Raw,
        FpsStrategyValue::Vrr => frametime_app::FpsStrategyValue::Vrr,
    }
}

fn map_fps(value: FpsRequest) -> frametime_app::FpsRequest {
    frametime_app::FpsRequest {
        average: value.average,
        text: value.text,
        file: value.file,
        clipboard: value.clipboard,
        strategy: map_fps_strategy(value.strategy),
        measured_cap: value.measured_cap,
        refresh_hz: value.refresh_hz,
        ceiling_margin_hz: value.ceiling_margin_hz,
        label: value.label,
        copy: value.copy,
        no_persist: value.no_persist,
    }
}

fn map_vprof(value: VprofBenchmarkRequest) -> frametime_app::VprofBenchmarkRequest {
    frametime_app::VprofBenchmarkRequest {
        text: value.text,
        file: value.file,
        clipboard: value.clipboard,
    }
}

const fn map_hardware(value: HardwareCommand) -> frametime_app::HardwareCommand {
    match value {
        HardwareCommand::Doctor => frametime_app::HardwareCommand::Doctor,
        HardwareCommand::Cpu => frametime_app::HardwareCommand::Cpu,
        HardwareCommand::Gpu => frametime_app::HardwareCommand::Gpu,
        HardwareCommand::System => frametime_app::HardwareCommand::System,
        HardwareCommand::Whea { max_records } => {
            frametime_app::HardwareCommand::Whea { max_records }
        }
        HardwareCommand::Frames { duration_ms } => {
            frametime_app::HardwareCommand::Frames { duration_ms }
        }
    }
}

fn map_live_command(value: Command) -> frametime_app::Command {
    match value {
        Command::Optimize { yes } => frametime_app::Command::Optimize { yes },
        Command::Configure { profile, dry_run } => frametime_app::Command::Configure {
            profile: profile.into(),
            dry_run,
        },
        Command::Cs2Cfg { assets, yes } => frametime_app::Command::Cs2Cfg { assets, yes },
        Command::BootSafeMode { yes } => frametime_app::Command::BootSafeMode { yes },
        Command::Phase2 { yes } => frametime_app::Command::Phase2 { yes },
        Command::Phase3 { yes } => frametime_app::Command::Phase3 { yes },
        Command::Phase3Handoff => frametime_app::Command::Phase3Handoff,
        Command::Cleanup {
            mode,
            yes,
            acknowledge_irreversible,
        } => frametime_app::Command::Cleanup {
            mode: map_cleanup(mode),
            yes,
            acknowledge_irreversible,
        },
        Command::Verify => frametime_app::Command::Verify,
        Command::Restore { yes } => frametime_app::Command::Restore { yes },
        Command::BackupSummary => frametime_app::Command::BackupSummary,
        Command::ResetProgress { yes } => frametime_app::Command::ResetProgress { yes },
        Command::ShowLog => frametime_app::Command::ShowLog,
        Command::DryRun { .. }
        | Command::FpsCap { .. }
        | Command::BaselineBenchmark { .. }
        | Command::FinalBenchmark { .. }
        | Command::Driver { .. }
        | Command::Hardware { .. }
        | Command::SiteRegister
        | Command::SmokeTest
        | Command::PackageAuthSmoke
        | Command::Exit => unreachable!("handled before the live command mapping"),
    }
}
