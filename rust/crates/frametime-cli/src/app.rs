use std::process::ExitCode;

use clap::Parser;

use crate::{
    actions::{
        require_cleanup_confirmation, require_yes, run_baseline_benchmark, run_driver_plan,
        run_dry, run_fps_cap, run_hardware_diagnostic, run_prepare_nvidia,
    },
    cli::{Cli, Command, DriverCommand, FpsRequest, VprofBenchmarkRequest},
    console::{install_cancellation_handler, interactive_menu},
    error::AppError,
    package_auth::run_authentication_smoke,
    workflow::{run_final_benchmark, run_live},
};

pub(crate) fn main() -> ExitCode {
    install_cancellation_handler();
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
        Command::DryRun { branch } => run_dry(branch),
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
            reduction,
            minimum,
            label,
            copy,
            no_persist,
        } => run_fps_cap(FpsRequest {
            average: average_fps,
            text: vprof_text,
            file: vprof_file,
            clipboard,
            reduction,
            minimum,
            label,
            copy,
            no_persist,
        }),
        Command::BaselineBenchmark {
            vprof_text,
            vprof_file,
            clipboard,
        } => run_baseline_benchmark(VprofBenchmarkRequest {
            text: vprof_text,
            file: vprof_file,
            clipboard,
        }),
        Command::FinalBenchmark {
            vprof_text,
            vprof_file,
            clipboard,
        } => run_final_benchmark(VprofBenchmarkRequest {
            text: vprof_text,
            file: vprof_file,
            clipboard,
        }),
        Command::Driver {
            command: DriverCommand::Plan { input },
        } => run_driver_plan(&input),
        Command::Driver {
            command:
                DriverCommand::PrepareNvidia {
                    artifact_id,
                    artifact_file_name,
                    server_path,
                },
        } => run_prepare_nvidia(&artifact_id, &artifact_file_name, &server_path),
        Command::Hardware { command } => run_hardware_diagnostic(command),
        Command::Cleanup {
            mode,
            yes,
            acknowledge_irreversible,
        } => {
            require_cleanup_confirmation(mode, yes, acknowledge_irreversible)?;
            run_live(Command::Cleanup {
                mode,
                yes,
                acknowledge_irreversible,
            })
        }
        Command::BootSafeMode { yes } => {
            require_yes(yes, "boot-safe-mode")?;
            run_live(Command::BootSafeMode { yes })
        }
        command => run_live(command),
    }
}
