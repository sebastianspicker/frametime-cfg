//! Trusted, install-bound persistence for the CS2 CFG portion of Step 34.

use std::{io, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Cs2ConfigError {
    #[error("CS2 install binding is not the exact discovered Steam layout")]
    InvalidBinding,
    #[error("CS2 CFG request timestamp must be yyyy-mm-dd hh:mm")]
    InvalidTimestamp,
    #[error("at least one closed optional CS2 CFG asset must be selected")]
    EmptyOptionalSelection,
    #[error("CS2 CFG target changed after backup capture: {0}")]
    PreconditionMismatch(PathBuf),
    #[error("CS2 CFG path is not a trusted real path: {0}")]
    UntrustedPath(PathBuf),
    #[error("existing autoexec.cfg is not valid UTF-8; refusing to rewrite user content")]
    AutoexecNotUtf8,
    #[error("CS2 CFG I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("{stage} failed; recovery evidence retained at {recovery:?}: {source}")]
    Mutation {
        stage: &'static str,
        recovery: Option<PathBuf>,
        #[source]
        source: io::Error,
    },
    #[error("exact readback failed for {target}; recovery evidence retained at {recovery:?}")]
    ReadbackMismatch {
        target: PathBuf,
        recovery: Option<PathBuf>,
    },
}

/// Fixed assets only: no arbitrary path or byte input can cross this boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OptionalCfgAsset {
    NetStable,
    NetHighPing,
    NetUnstable,
    NetBad,
    DebugHud,
    DebugHudOff,
    AudioStable,
    AudioLowLatency025,
    AudioLowLatency001,
    FrameBaseline400,
    FrameRawUncapped,
    AudioEqCrisp,
    AudioEqSmooth,
    AudioLegacyLrIsolation,
    Competitive2026,
}

impl OptionalCfgAsset {
    pub const ALL: [Self; 15] = [
        Self::NetStable,
        Self::NetHighPing,
        Self::NetUnstable,
        Self::NetBad,
        Self::DebugHud,
        Self::DebugHudOff,
        Self::AudioStable,
        Self::AudioLowLatency025,
        Self::AudioLowLatency001,
        Self::FrameBaseline400,
        Self::FrameRawUncapped,
        Self::AudioEqCrisp,
        Self::AudioEqSmooth,
        Self::AudioLegacyLrIsolation,
        Self::Competitive2026,
    ];

    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::NetStable => "net_stable.cfg",
            Self::NetHighPing => "net_highping.cfg",
            Self::NetUnstable => "net_unstable.cfg",
            Self::NetBad => "net_bad.cfg",
            Self::DebugHud => "debug_hud.cfg",
            Self::DebugHudOff => "debug_hud_off.cfg",
            Self::AudioStable => "audio_stable.cfg",
            Self::AudioLowLatency025 => "audio_lowlatency_025.cfg",
            Self::AudioLowLatency001 => "audio_lowlatency_001.cfg",
            Self::FrameBaseline400 => "frame_baseline_400.cfg",
            Self::FrameRawUncapped => "frame_raw_uncapped.cfg",
            Self::AudioEqCrisp => "audio_eq_crisp.cfg",
            Self::AudioEqSmooth => "audio_eq_smooth.cfg",
            Self::AudioLegacyLrIsolation => "audio_legacy_lr_isolation.cfg",
            Self::Competitive2026 => "competitive_2026.cfg",
        }
    }

    #[must_use]
    pub const fn cli_token(self) -> &'static str {
        match self {
            Self::NetStable => "net-stable",
            Self::NetHighPing => "net-highping",
            Self::NetUnstable => "net-unstable",
            Self::NetBad => "net-bad",
            Self::DebugHud => "debug-hud",
            Self::DebugHudOff => "debug-hud-off",
            Self::AudioStable => "audio-stable",
            Self::AudioLowLatency025 => "audio-lowlatency-025",
            Self::AudioLowLatency001 => "audio-lowlatency-001",
            Self::FrameBaseline400 => "frame-baseline-400",
            Self::FrameRawUncapped => "frame-raw-uncapped",
            Self::AudioEqCrisp => "audio-eq-crisp",
            Self::AudioEqSmooth => "audio-eq-smooth",
            Self::AudioLegacyLrIsolation => "audio-legacy-lr-isolation",
            Self::Competitive2026 => "competitive-2026",
        }
    }

    #[must_use]
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::NetStable => "Network stable",
            Self::NetHighPing => "Network high ping",
            Self::NetUnstable => "Network unstable",
            Self::NetBad => "Network loss diagnostics",
            Self::DebugHud => "Debug HUD on",
            Self::DebugHudOff => "Debug HUD off",
            Self::AudioStable => "Audio stable",
            Self::AudioLowLatency025 => "Audio low latency 0.025",
            Self::AudioLowLatency001 => "Audio auto latency / 0.001 alias",
            Self::FrameBaseline400 => "Frame baseline 400",
            Self::FrameRawUncapped => "Frame raw uncapped",
            Self::AudioEqCrisp => "Audio EQ crisp",
            Self::AudioEqSmooth => "Audio EQ smooth",
            Self::AudioLegacyLrIsolation => "Audio legacy L/R isolation",
            Self::Competitive2026 => "Competitive 2026",
        }
    }

    #[must_use]
    pub fn from_cli_token(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|asset| asset.cli_token() == value)
    }
    #[must_use]
    pub const fn bytes(self) -> &'static [u8] {
        match self {
            Self::NetStable => include_bytes!("../../../../assets/cfgs/net_stable.cfg"),
            Self::NetHighPing => include_bytes!("../../../../assets/cfgs/net_highping.cfg"),
            Self::NetUnstable => include_bytes!("../../../../assets/cfgs/net_unstable.cfg"),
            Self::NetBad => include_bytes!("../../../../assets/cfgs/net_bad.cfg"),
            Self::DebugHud => include_bytes!("../../../../assets/cfgs/debug_hud.cfg"),
            Self::DebugHudOff => include_bytes!("../../../../assets/cfgs/debug_hud_off.cfg"),
            Self::AudioStable => include_bytes!("../../../../assets/cfgs/audio_stable.cfg"),
            Self::AudioLowLatency025 => {
                include_bytes!("../../../../assets/cfgs/audio_lowlatency_025.cfg")
            }
            Self::AudioLowLatency001 => {
                include_bytes!("../../../../assets/cfgs/audio_lowlatency_001.cfg")
            }
            Self::FrameBaseline400 => {
                include_bytes!("../../../../assets/cfgs/frame_baseline_400.cfg")
            }
            Self::FrameRawUncapped => {
                include_bytes!("../../../../assets/cfgs/frame_raw_uncapped.cfg")
            }
            Self::AudioEqCrisp => include_bytes!("../../../../assets/cfgs/audio_eq_crisp.cfg"),
            Self::AudioEqSmooth => include_bytes!("../../../../assets/cfgs/audio_eq_smooth.cfg"),
            Self::AudioLegacyLrIsolation => {
                include_bytes!("../../../../assets/cfgs/audio_legacy_lr_isolation.cfg")
            }
            Self::Competitive2026 => include_bytes!("../../../../assets/cfgs/competitive_2026.cfg"),
        }
    }
}
