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
    if let Some(legacy_command) = legacy_command(cli) {
        return Ok(legacy_command);
    }
    let command = cli.command.as_ref().ok_or_else(|| {
        NorthclockError::InvalidUsage("a command is required when GUI launch is disabled".into())
    })?;
    Ok((map_command(command, cli)?, None))
}

fn legacy_command(cli: &Cli) -> Option<(ApplicationCommand, Option<&'static str>)> {
    if cli.vendor || cli.gpu_native {
        return Some((
            ApplicationCommand::Doctor,
            Some("--vendor and --gpu-native are deprecated; use `northclock doctor`"),
        ));
    }
    if cli.vanta {
        return Some((
            ApplicationCommand::VramTest {
                adapter: None,
                bytes: 256 * 1024 * 1024,
                timeout_ms: 30_000,
            },
            Some("--vanta is deprecated; use `northclock memory vram-test`"),
        ));
    }
    None
}

fn map_command(command: &Command, cli: &Cli) -> northclock_core::Result<ApplicationCommand> {
    let mapped = match command {
        Command::Doctor => ApplicationCommand::Doctor,
        Command::Cpu { command } => map_cpu_command(command),
        Command::Gpu { command } => map_gpu_command(command),
        Command::Memory { command } => map_memory_command(command),
        Command::Power { command } => map_power_command(command),
        Command::System { command } => map_system_command(command),
        Command::Process { command } => map_process_command(command, cli)?,
        Command::Events { command } => map_events_command(command),
        Command::Settings { command } => map_settings_command(command),
        Command::Profiles { command } => map_profiles_command(command),
        Command::Frames { command } => map_frames_command(command),
        Command::Rom { command } => map_rom_command(command),
        Command::Operation { command } => map_operation_command(command, cli)?,
    };
    Ok(mapped)
}

fn map_cpu_command(command: &CpuCommand) -> ApplicationCommand {
    match command {
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
    }
}

fn map_gpu_command(command: &GpuCommand) -> ApplicationCommand {
    match command {
        GpuCommand::List => ApplicationCommand::GpuDevices,
        GpuCommand::Measure { device } => ApplicationCommand::GpuMeasurements {
            stable_id: device.clone(),
        },
    }
}

fn map_memory_command(command: &MemoryCommand) -> ApplicationCommand {
    match command {
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
    }
}

fn map_power_command(command: &PowerCommand) -> ApplicationCommand {
    match command {
        PowerCommand::List => ApplicationCommand::PowerPlans,
    }
}

fn map_system_command(command: &SystemCommand) -> ApplicationCommand {
    match command {
        SystemCommand::Status => ApplicationCommand::SystemStatus,
    }
}

fn map_process_command(
    command: &ProcessCommand,
    cli: &Cli,
) -> northclock_core::Result<ApplicationCommand> {
    match command {
        ProcessCommand::Affinity { command } => match command {
            AffinityCommand::Preview { pid, mask } => {
                Ok(ApplicationCommand::ProcessAffinityPreview {
                    process_id: *pid,
                    mask: *mask,
                })
            }
            AffinityCommand::Apply { plan } => Ok(ApplicationCommand::ProcessAffinityApply {
                plan: read_json(plan)?,
                experimental: cli.experimental,
                apply: cli.apply,
                risk_acknowledgement: cli.risk_acknowledgement.clone(),
            }),
            AffinityCommand::Rollback { receipt } => {
                Ok(ApplicationCommand::ProcessAffinityRollback {
                    receipt: read_json(receipt)?,
                    experimental: cli.experimental,
                    apply: cli.apply,
                    risk_acknowledgement: cli.risk_acknowledgement.clone(),
                })
            }
        },
    }
}

fn map_events_command(command: &EventsCommand) -> ApplicationCommand {
    match command {
        EventsCommand::Whea { duration_ms } => ApplicationCommand::WheaEvents {
            duration_ms: *duration_ms,
        },
    }
}

fn map_settings_command(command: &SettingsCommand) -> ApplicationCommand {
    match command {
        SettingsCommand::Show => ApplicationCommand::SettingsShow,
        SettingsCommand::Set {
            measurement_interval_ms,
            profile,
        } => ApplicationCommand::SettingsSet {
            measurement_interval_ms: *measurement_interval_ms,
            selected_profile: profile.clone(),
        },
    }
}

fn map_profiles_command(command: &ProfilesCommand) -> ApplicationCommand {
    match command {
        ProfilesCommand::List => ApplicationCommand::ProfilesList,
        ProfilesCommand::ImportIni { path } => {
            ApplicationCommand::ProfileImport { path: path.clone() }
        }
    }
}

fn map_frames_command(command: &FramesCommand) -> ApplicationCommand {
    match command {
        FramesCommand::Capture { duration_ms } => ApplicationCommand::FrameCapture {
            duration_ms: *duration_ms,
        },
    }
}

fn map_rom_command(command: &RomCommand) -> ApplicationCommand {
    match command {
        RomCommand::Inspect { path } => ApplicationCommand::RomInspect { path: path.clone() },
    }
}

fn map_operation_command(
    command: &OperationCommand,
    cli: &Cli,
) -> northclock_core::Result<ApplicationCommand> {
    match command {
        OperationCommand::Preview(arguments) => {
            Ok(ApplicationCommand::OperationPreview(OperationRequest {
                target: arguments.target,
                changes: arguments.changes.iter().cloned().collect(),
            }))
        }
        OperationCommand::Apply { plan } => Ok(ApplicationCommand::OperationApply {
            plan: read_json(plan)?,
            experimental: cli.experimental,
            apply: cli.apply,
            risk_acknowledgement: cli.risk_acknowledgement.clone(),
        }),
        OperationCommand::Rollback { receipt } => Ok(ApplicationCommand::OperationRollback {
            receipt: read_json(receipt)?,
            experimental: cli.experimental,
            apply: cli.apply,
            risk_acknowledgement: cli.risk_acknowledgement.clone(),
        }),
    }
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
