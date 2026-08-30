use std::path::PathBuf;

use frametime_domain::{OptionalCfgAsset, Profile};

#[derive(Debug, Clone, Copy)]
pub enum Branch {
    NvidiaRtx5000,
    Nvidia,
    Amd,
    IntelArc,
    All,
}

impl Branch {
    pub const fn number(self) -> Option<u8> {
        match self {
            Self::NvidiaRtx5000 => Some(1),
            Self::Nvidia => Some(2),
            Self::Amd => Some(3),
            Self::IntelArc => Some(4),
            Self::All => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CleanupMode {
    Quick,
    Full,
    Driver,
}

#[derive(Debug, Clone, Copy)]
pub enum FpsStrategyValue {
    Raw,
    Vrr,
}

#[derive(Debug, Clone)]
pub struct FpsRequest {
    pub average: Option<f64>,
    pub text: Option<String>,
    pub file: Option<PathBuf>,
    pub clipboard: bool,
    pub strategy: FpsStrategyValue,
    pub measured_cap: u32,
    pub refresh_hz: u32,
    pub ceiling_margin_hz: u32,
    pub label: String,
    pub copy: bool,
    pub no_persist: bool,
}

#[derive(Debug, Clone)]
pub struct VprofBenchmarkRequest {
    pub text: Option<String>,
    pub file: Option<PathBuf>,
    pub clipboard: bool,
}

#[derive(Debug, Clone)]
pub enum DriverCommand {
    Plan {
        input: PathBuf,
    },
    PrepareNvidia {
        artifact_id: String,
        artifact_file_name: String,
        server_path: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum HardwareCommand {
    Doctor,
    Cpu,
    Gpu,
    System,
    Whea { max_records: u16 },
    Frames { duration_ms: u32 },
}

/// Typed commands accepted by the shared application boundary. Paths are
/// intentionally absent from workflow mutations: state always lives below the
/// fixed trusted Windows work root.
#[derive(Debug, Clone)]
pub enum Command {
    DryRun {
        branch: Branch,
    },
    FpsCap(FpsRequest),
    BaselineBenchmark(VprofBenchmarkRequest),
    FinalBenchmark(VprofBenchmarkRequest),
    Driver(DriverCommand),
    Hardware(HardwareCommand),
    SmokeTest,
    PackageAuthSmoke,
    Exit,
    Optimize {
        yes: bool,
    },
    Configure {
        profile: Profile,
        dry_run: bool,
    },
    Cs2Cfg {
        assets: Vec<OptionalCfgAsset>,
        yes: bool,
    },
    BootSafeMode {
        yes: bool,
    },
    Phase2 {
        yes: bool,
    },
    Phase3 {
        yes: bool,
    },
    Phase3Handoff,
    Cleanup {
        mode: CleanupMode,
        yes: bool,
        acknowledge_irreversible: bool,
    },
    Verify,
    Restore {
        yes: bool,
    },
    BackupSummary,
    ResetProgress {
        yes: bool,
    },
    ShowLog,
}
