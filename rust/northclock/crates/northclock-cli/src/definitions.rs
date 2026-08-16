use clap::{Args, Parser, Subcommand};
use northclock_core::OperationTarget;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "northclock",
    version,
    about = "Windows hardware diagnostics with explicit capability reporting"
)]
pub(crate) struct Cli {
    /// Emit the versioned command envelope as JSON.
    #[arg(long, global = true)]
    pub(crate) json: bool,

    /// Enable the session's experimental hardware-write controls.
    #[arg(long, global = true)]
    pub(crate) experimental: bool,

    /// Confirm that an operation plan should be applied.
    #[arg(long, global = true)]
    pub(crate) apply: bool,

    /// Fixed acknowledgement required by every hardware write.
    #[arg(long, global = true)]
    pub(crate) risk_acknowledgement: Option<String>,

    #[command(subcommand)]
    pub(crate) command: Option<Command>,

    #[arg(long, hide = true)]
    pub(crate) vendor: bool,
    #[arg(long, hide = true)]
    pub(crate) gpu_native: bool,
    #[arg(long, hide = true)]
    pub(crate) vanta: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Report exact backend and verification states.
    Doctor,
    /// CPU identity, measurements, and readback-driven tuning workflows.
    Cpu {
        #[command(subcommand)]
        command: CpuCommand,
    },
    /// GPU inventory and vendor-backed measurements.
    Gpu {
        #[command(subcommand)]
        command: GpuCommand,
    },
    /// Bounded system-memory and isolated VRAM tests.
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },
    /// Windows power-plan inventory.
    Power {
        #[command(subcommand)]
        command: PowerCommand,
    },
    /// Read-only Windows task, security, and conflict observations.
    System {
        #[command(subcommand)]
        command: SystemCommand,
    },
    /// Strict Windows process controls.
    Process {
        #[command(subcommand)]
        command: ProcessCommand,
    },
    /// Native Windows hardware-event observation.
    Events {
        #[command(subcommand)]
        command: EventsCommand,
    },
    /// Versioned settings stored under the local application-data directory.
    Settings {
        #[command(subcommand)]
        command: SettingsCommand,
    },
    /// Versioned profiles and one-time legacy INI import.
    Profiles {
        #[command(subcommand)]
        command: ProfilesCommand,
    },
    /// ETW frame capture.
    Frames {
        #[command(subcommand)]
        command: FramesCommand,
    },
    /// Read-only firmware inspection.
    Rom {
        #[command(subcommand)]
        command: RomCommand,
    },
    /// Preview, apply, or roll back a bounded hardware operation.
    Operation {
        #[command(subcommand)]
        command: OperationCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum CpuCommand {
    Identity,
    Measure,
    Workload {
        #[arg(long, default_value_t = 10_000)]
        duration_ms: u64,
        #[arg(long, default_value_t = 1)]
        threads: usize,
    },
    CurveOptimizerPreview {
        #[arg(long, default_value_t = -10)]
        offset: i64,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum GpuCommand {
    List,
    Measure {
        #[arg(long)]
        device: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum MemoryCommand {
    SystemTest {
        #[arg(long, default_value_t = 64 * 1024 * 1024)]
        bytes: usize,
        #[arg(long, default_value_t = 2)]
        passes: u32,
        #[arg(long, default_value_t = 30_000)]
        timeout_ms: u64,
    },
    #[command(visible_alias = "vanta")]
    VramTest {
        #[arg(long)]
        adapter: Option<String>,
        #[arg(long, default_value_t = 256 * 1024 * 1024)]
        bytes: u64,
        #[arg(long, default_value_t = 30_000)]
        timeout_ms: u64,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum PowerCommand {
    List,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SystemCommand {
    Status,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProcessCommand {
    Affinity {
        #[command(subcommand)]
        command: AffinityCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AffinityCommand {
    Preview {
        #[arg(long)]
        pid: u32,
        #[arg(long, value_parser = parse_affinity_mask)]
        mask: u64,
    },
    Apply {
        plan: PathBuf,
    },
    Rollback {
        receipt: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum EventsCommand {
    Whea {
        #[arg(long, default_value_t = 60_000)]
        duration_ms: u64,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum SettingsCommand {
    Show,
    Set {
        #[arg(long, default_value_t = 1_000)]
        measurement_interval_ms: u64,
        #[arg(long)]
        profile: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProfilesCommand {
    List,
    ImportIni { path: PathBuf },
}

#[derive(Debug, Subcommand)]
pub(crate) enum FramesCommand {
    Capture {
        #[arg(long, default_value_t = 10_000)]
        duration_ms: u64,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum RomCommand {
    Inspect { path: PathBuf },
}

#[derive(Debug, Subcommand)]
pub(crate) enum OperationCommand {
    Preview(OperationPreviewArgs),
    Apply { plan: PathBuf },
    Rollback { receipt: PathBuf },
}

#[derive(Debug, Args)]
pub(crate) struct OperationPreviewArgs {
    #[arg(long)]
    pub(crate) target: OperationTarget,
    #[arg(long = "change", value_parser = parse_change, required = true)]
    pub(crate) changes: Vec<(String, i64)>,
}

pub(crate) fn parse_change(value: &str) -> std::result::Result<(String, i64), String> {
    let (name, value) = value
        .split_once('=')
        .ok_or_else(|| "change must have the form name=value".to_string())?;
    if name.is_empty() {
        return Err("change name must not be empty".into());
    }
    let value = value
        .parse::<i64>()
        .map_err(|error| format!("invalid integer change value: {error}"))?;
    Ok((name.to_string(), value))
}

pub(crate) fn parse_affinity_mask(value: &str) -> std::result::Result<u64, String> {
    let parsed = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16)
    } else {
        value.parse::<u64>()
    };
    parsed
        .map_err(|error| format!("invalid affinity mask: {error}"))
        .and_then(|mask| {
            (mask != 0)
                .then_some(mask)
                .ok_or_else(|| "affinity mask must be non-zero".to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typed_change() {
        assert_eq!(
            parse_change("curve_optimizer=-12"),
            Ok(("curve_optimizer".into(), -12))
        );
        assert!(parse_change("curve_optimizer").is_err());
    }

    #[test]
    fn parses_decimal_and_hex_affinity_masks() {
        assert_eq!(parse_affinity_mask("3"), Ok(3));
        assert_eq!(parse_affinity_mask("0x10"), Ok(16));
        assert!(parse_affinity_mask("0").is_err());
    }
}
