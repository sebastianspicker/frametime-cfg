use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use frametime_core::Profile;

#[derive(Debug, Parser)]
#[command(name = "frametime", version, about = "Native frametime.cfg workflow")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Start or resume native Phase 1. Prompted profile steps need explicit consent.
    Optimize {
        #[arg(long)]
        yes: bool,
    },
    /// Strict zero-persistence preview for one GPU branch or all branches.
    DryRun {
        #[arg(value_enum, default_value = "2")]
        branch: Branch,
    },
    /// Persist the selected profile and GUI preview preference.
    Configure {
        #[arg(value_enum)]
        profile: ProfileValue,
        #[arg(long, action = ArgAction::Set, default_value_t = false)]
        dry_run: bool,
    },
    /// Run the guarded Safe Mode phase after a verified Phase 1 handoff.
    Phase2 {
        #[arg(long)]
        yes: bool,
    },
    /// Run guarded normal-boot Phase 3 after Phase 2 finishes.
    Phase3 {
        #[arg(long)]
        yes: bool,
    },
    /// Internal same-user handoff entrypoint for the verified selected runtime.
    #[command(hide = true)]
    Phase3Handoff,
    /// Arm the bounded Phase 2 Safe Mode handoff after Phase 1 is complete.
    BootSafeMode {
        #[arg(long)]
        yes: bool,
    },
    /// Request a bounded standalone cleanup with explicit destructive acknowledgement.
    Cleanup {
        #[arg(value_enum, default_value = "quick")]
        mode: CleanupMode,
        #[arg(long)]
        yes: bool,
        /// Acknowledge Full cleanup's irreversible resets and record deletion.
        #[arg(long)]
        acknowledge_irreversible: bool,
    },
    /// Calculate and optionally persist a cap from a manual average or VProf result text/file.
    FpsCap {
        #[arg(value_name = "AVERAGE_FPS", conflicts_with_all = ["vprof_text", "vprof_file", "clipboard"])]
        average_fps: Option<f64>,
        #[arg(long, conflicts_with_all = ["average_fps", "vprof_file", "clipboard"])]
        vprof_text: Option<String>,
        #[arg(long, conflicts_with_all = ["average_fps", "vprof_text", "clipboard"])]
        vprof_file: Option<PathBuf>,
        /// Read VProf text from the native clipboard. Fails closed until the Windows adapter is available.
        #[arg(long, conflicts_with_all = ["average_fps", "vprof_text", "vprof_file"])]
        clipboard: bool,
        #[arg(long, default_value_t = 0.09)]
        reduction: f64,
        #[arg(long, default_value_t = 60)]
        minimum: u32,
        #[arg(long, default_value = "Manual benchmark")]
        label: String,
        #[arg(long)]
        copy: bool,
        /// Do not write state.json or benchmark_history.json on Windows.
        #[arg(long)]
        no_persist: bool,
    },
    /// Persist the P1:17 baseline from a complete VProf capture before optimizations.
    BaselineBenchmark {
        #[arg(long, required_unless_present_any = ["vprof_file", "clipboard"], conflicts_with_all = ["vprof_file", "clipboard"])]
        vprof_text: Option<String>,
        #[arg(long, required_unless_present_any = ["vprof_text", "clipboard"], conflicts_with_all = ["vprof_text", "clipboard"])]
        vprof_file: Option<PathBuf>,
        /// Read VProf text from the native clipboard on Windows.
        #[arg(long, required_unless_present_any = ["vprof_text", "vprof_file"], conflicts_with_all = ["vprof_text", "vprof_file"])]
        clipboard: bool,
    },
    /// Persist the transaction-bound P3:13 final benchmark from a complete VProf capture.
    FinalBenchmark {
        #[arg(long, required_unless_present_any = ["vprof_file", "clipboard"], conflicts_with_all = ["vprof_file", "clipboard"])]
        vprof_text: Option<String>,
        #[arg(long, required_unless_present_any = ["vprof_text", "clipboard"], conflicts_with_all = ["vprof_text", "clipboard"])]
        vprof_file: Option<PathBuf>,
        /// Read VProf text from the native clipboard on Windows.
        #[arg(long, required_unless_present_any = ["vprof_text", "vprof_file"], conflicts_with_all = ["vprof_text", "vprof_file"])]
        clipboard: bool,
    },
    /// Inspect Driver Foundry domain evidence without acquiring or mutating drivers.
    Driver {
        #[command(subcommand)]
        command: DriverCommand,
    },
    /// Run bounded, native Northclock-derived hardware diagnostics.
    Hardware {
        #[command(subcommand)]
        command: HardwareCommand,
    },
    /// Print a structured, read-only snapshot of persisted workflow state.
    Verify,
    /// Restore all supported backup entries in reverse order.
    Restore {
        #[arg(long)]
        yes: bool,
    },
    /// Print backup entry counts by type.
    BackupSummary,
    /// Clear only phase progress after confirmation.
    ResetProgress {
        #[arg(long)]
        yes: bool,
    },
    /// Print the current log.
    ShowLog,
    /// Load the public entrypoint without initialization or elevation.
    SmokeTest,
    /// Verify the complete signed package, catalog, retained identities, and publisher pin.
    #[command(hide = true)]
    PackageAuthSmoke,
    #[command(hide = true)]
    Exit,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DriverCommand {
    /// Validate exact GPU, OEM package, digest, and Authenticode evidence and print a read-only plan.
    Plan {
        /// JSON file containing a frametime-driver DriverPlanInput record.
        #[arg(long)]
        input: PathBuf,
    },
    /// Acquire one NVIDIA artifact from the compiled NVIDIA host policy,
    /// verify its retained file capability, and persist P1:18/P1:19 evidence.
    PrepareNvidia {
        /// Stable label used only to identify this prepared transaction.
        #[arg(long)]
        artifact_id: String,
        /// One safe installer leaf. It is retained only below the protected
        /// driver-artifacts directory.
        #[arg(long)]
        artifact_file_name: String,
        /// Opaque slash-normalized server path below the fixed NVIDIA CDN.
        #[arg(long)]
        server_path: String,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum HardwareCommand {
    /// Report native diagnostic capabilities without changing system state.
    Doctor,
    /// Read exact CPU identity and topology evidence.
    Cpu,
    /// Enumerate display adapters through DXGI.
    Gpu,
    /// Read bounded native system status.
    System,
    /// Read a bounded number of WHEA Event Log records.
    Whea {
        #[arg(long, default_value_t = 32, value_parser = clap::value_parser!(u16).range(1..=128))]
        max_records: u16,
    },
    /// Capture bounded DxgKrnl ETW present-start samples.
    Frames {
        #[arg(long, default_value_t = 10_000, value_parser = clap::value_parser!(u32).range(1..=60_000))]
        duration_ms: u32,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Branch {
    #[value(name = "1")]
    NvidiaRtx5000,
    #[value(name = "2")]
    Nvidia,
    #[value(name = "3")]
    Amd,
    #[value(name = "4")]
    IntelArc,
    All,
}

impl Branch {
    pub(crate) const fn number(self) -> Option<u8> {
        match self {
            Self::NvidiaRtx5000 => Some(1),
            Self::Nvidia => Some(2),
            Self::Amd => Some(3),
            Self::IntelArc => Some(4),
            Self::All => None,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum CleanupMode {
    Quick,
    Full,
    Driver,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum ProfileValue {
    Safe,
    Recommended,
    Competitive,
    Custom,
    Yolo,
}

impl From<ProfileValue> for Profile {
    fn from(value: ProfileValue) -> Self {
        match value {
            ProfileValue::Safe => Self::Safe,
            ProfileValue::Recommended => Self::Recommended,
            ProfileValue::Competitive => Self::Competitive,
            ProfileValue::Custom => Self::Custom,
            ProfileValue::Yolo => Self::Yolo,
        }
    }
}

pub(crate) struct FpsRequest {
    pub(crate) average: Option<f64>,
    pub(crate) text: Option<String>,
    pub(crate) file: Option<PathBuf>,
    pub(crate) clipboard: bool,
    pub(crate) reduction: f64,
    pub(crate) minimum: u32,
    pub(crate) label: String,
    pub(crate) copy: bool,
    pub(crate) no_persist: bool,
}

pub(crate) struct VprofBenchmarkRequest {
    pub(crate) text: Option<String>,
    pub(crate) file: Option<PathBuf>,
    pub(crate) clipboard: bool,
}
