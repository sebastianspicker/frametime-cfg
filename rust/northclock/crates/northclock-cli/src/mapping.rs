use crate::definitions::{
    AffinityCommand, Cli, Command, CpuCommand, EventsCommand, FramesCommand, GpuCommand,
    MemoryCommand, OperationCommand, PowerCommand, ProcessCommand, ProfilesCommand, RomCommand,
    SettingsCommand, SystemCommand,
};
use northclock_core::{ApplicationCommand, MemoryTestConfig, NorthclockError, OperationRequest};
use std::fs;
use std::path::Path;

pub(crate) fn to_application_command(
    cli: &Cli,
) -> northclock_core::Result<(ApplicationCommand, Option<&'static str>)> {
    if cli.vendor || cli.gpu_native {
        return Ok((
            ApplicationCommand::Doctor,
            Some("--vendor and --gpu-native are deprecated; use `northclock doctor`"),
        ));
    }
    if cli.vanta {
        return Ok((
            ApplicationCommand::VramTest {
                adapter: None,
                bytes: 256 * 1024 * 1024,
                timeout_ms: 30_000,
            },
            Some("--vanta is deprecated; use `northclock memory vram-test`"),
        ));
    }
    let command = cli.command.as_ref().ok_or_else(|| {
        NorthclockError::InvalidUsage("a command is required when GUI launch is disabled".into())
    })?;
    let command = match command {
        Command::Doctor => ApplicationCommand::Doctor,
        Command::Cpu { command } => match command {
            CpuCommand::Identity => ApplicationCommand::CpuIdentity,
            CpuCommand::Measure => ApplicationCommand::CpuMeasurements,
            CpuCommand::Workload {
                duration_ms,
                threads,
            } => ApplicationCommand::CpuWorkload {
                duration_ms: *duration_ms,
                threads: *threads,
            },
            CpuCommand::CurveOptimizerPreview { offset } => curve_preview(*offset),
        },
        Command::Gpu { command } => match command {
            GpuCommand::List => ApplicationCommand::GpuDevices,
            GpuCommand::Measure { device } => ApplicationCommand::GpuMeasurements {
                stable_id: device.clone(),
            },
        },
        Command::Memory { command } => match command {
            MemoryCommand::SystemTest {
                bytes,
                passes,
                timeout_ms,
            } => ApplicationCommand::SystemMemoryTest(MemoryTestConfig {
                bytes: *bytes,
                passes: *passes,
                timeout_ms: *timeout_ms,
            }),
            MemoryCommand::VramTest {
                adapter,
                bytes,
                timeout_ms,
            } => ApplicationCommand::VramTest {
                adapter: adapter.clone(),
                bytes: *bytes,
                timeout_ms: *timeout_ms,
            },
        },
        Command::Power { command } => match command {
            PowerCommand::List => ApplicationCommand::PowerPlans,
        },
        Command::System { command } => match command {
            SystemCommand::Status => ApplicationCommand::SystemStatus,
        },
        Command::Process { command } => match command {
            ProcessCommand::Affinity { command } => match command {
                AffinityCommand::Preview { pid, mask } => {
                    ApplicationCommand::ProcessAffinityPreview {
                        process_id: *pid,
                        mask: *mask,
                    }
                }
                AffinityCommand::Apply { plan } => ApplicationCommand::ProcessAffinityApply {
                    plan: read_json(plan)?,
                    experimental: cli.experimental,
                    apply: cli.apply,
                    risk_acknowledgement: cli.risk_acknowledgement.clone(),
                },
                AffinityCommand::Rollback { receipt } => {
                    ApplicationCommand::ProcessAffinityRollback {
                        receipt: read_json(receipt)?,
                        experimental: cli.experimental,
                        apply: cli.apply,
                        risk_acknowledgement: cli.risk_acknowledgement.clone(),
                    }
                }
            },
        },
        Command::Events { command } => match command {
            EventsCommand::Whea { duration_ms } => ApplicationCommand::WheaEvents {
                duration_ms: *duration_ms,
            },
        },
        Command::Settings { command } => match command {
            SettingsCommand::Show => ApplicationCommand::SettingsShow,
            SettingsCommand::Set {
                measurement_interval_ms,
                profile,
            } => ApplicationCommand::SettingsSet {
                measurement_interval_ms: *measurement_interval_ms,
                selected_profile: profile.clone(),
            },
        },
        Command::Profiles { command } => match command {
            ProfilesCommand::List => ApplicationCommand::ProfilesList,
            ProfilesCommand::ImportIni { path } => {
                ApplicationCommand::ProfileImport { path: path.clone() }
            }
        },
        Command::Frames { command } => match command {
            FramesCommand::Capture { duration_ms } => ApplicationCommand::FrameCapture {
                duration_ms: *duration_ms,
            },
        },
        Command::Rom { command } => match command {
            RomCommand::Inspect { path } => ApplicationCommand::RomInspect { path: path.clone() },
        },
        Command::Operation { command } => match command {
            OperationCommand::Preview(arguments) => {
                ApplicationCommand::OperationPreview(OperationRequest {
                    target: arguments.target,
                    changes: arguments.changes.iter().cloned().collect(),
                })
            }
            OperationCommand::Apply { plan } => ApplicationCommand::OperationApply {
                plan: read_json(plan)?,
                experimental: cli.experimental,
                apply: cli.apply,
                risk_acknowledgement: cli.risk_acknowledgement.clone(),
            },
            OperationCommand::Rollback { receipt } => ApplicationCommand::OperationRollback {
                receipt: read_json(receipt)?,
                experimental: cli.experimental,
                apply: cli.apply,
                risk_acknowledgement: cli.risk_acknowledgement.clone(),
            },
        },
    };
    Ok((command, None))
}

fn curve_preview(offset: i64) -> ApplicationCommand {
    ApplicationCommand::OperationPreview(OperationRequest::cpu_curve_optimizer(offset))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> northclock_core::Result<T> {
    let bytes = fs::read(path).map_err(|error| {
        NorthclockError::InvalidUsage(format!("could not read {}: {error}", path.display()))
    })?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        NorthclockError::InvalidUsage(format!("invalid JSON in {}: {error}", path.display()))
    })?;
    serde_json::from_value(value.clone())
        .or_else(|direct_error| {
            value
                .get("data")
                .cloned()
                .ok_or(direct_error)
                .and_then(serde_json::from_value)
        })
        .map_err(|error| {
            NorthclockError::InvalidUsage(format!(
                "JSON in {} does not contain the expected artifact: {error}",
                path.display()
            ))
        })
}
