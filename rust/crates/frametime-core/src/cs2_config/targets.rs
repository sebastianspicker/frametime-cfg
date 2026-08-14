use serde::{Deserialize, Serialize};

use super::{AUTOEXEC_FILE, Cs2ConfigRequest, OPTIMIZATION_FILE, OptionalCfgAsset};

/// Closed CS2 CFG targets. Serialized backups name these logical targets, not
/// filesystem paths, so a later restore implementation must re-bind them to a
/// freshly trusted install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cs2ConfigTarget {
    Optimization,
    Autoexec,
    NetStable,
    NetHighPing,
    NetUnstable,
    NetBad,
    DebugHud,
    DebugHudOff,
    AudioStable,
    AudioLowLatency025,
    AudioLowLatency001,
}

impl Cs2ConfigTarget {
    /// Returns the complete, deterministic write set for one request.
    #[must_use]
    pub fn for_request(request: &Cs2ConfigRequest) -> Vec<Self> {
        let mut targets = vec![Self::Optimization, Self::Autoexec];
        targets.extend(request.optional_assets().iter().map(|asset| match asset {
            OptionalCfgAsset::NetStable => Self::NetStable,
            OptionalCfgAsset::NetHighPing => Self::NetHighPing,
            OptionalCfgAsset::NetUnstable => Self::NetUnstable,
            OptionalCfgAsset::NetBad => Self::NetBad,
            OptionalCfgAsset::DebugHud => Self::DebugHud,
            OptionalCfgAsset::DebugHudOff => Self::DebugHudOff,
            OptionalCfgAsset::AudioStable => Self::AudioStable,
            OptionalCfgAsset::AudioLowLatency025 => Self::AudioLowLatency025,
            OptionalCfgAsset::AudioLowLatency001 => Self::AudioLowLatency001,
        }));
        targets
    }

    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Optimization => OPTIMIZATION_FILE,
            Self::Autoexec => AUTOEXEC_FILE,
            Self::NetStable => "net_stable.cfg",
            Self::NetHighPing => "net_highping.cfg",
            Self::NetUnstable => "net_unstable.cfg",
            Self::NetBad => "net_bad.cfg",
            Self::DebugHud => "debug_hud.cfg",
            Self::DebugHudOff => "debug_hud_off.cfg",
            Self::AudioStable => "audio_stable.cfg",
            Self::AudioLowLatency025 => "audio_lowlatency_025.cfg",
            Self::AudioLowLatency001 => "audio_lowlatency_001.cfg",
        }
    }
}
