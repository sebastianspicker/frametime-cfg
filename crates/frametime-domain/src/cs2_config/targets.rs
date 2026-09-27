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
    FrameBaseline400,
    FrameRawUncapped,
    AudioEqCrisp,
    AudioEqSmooth,
    AudioLegacyLrIsolation,
    Competitive2026,
}

impl Cs2ConfigTarget {
    /// Returns the complete, deterministic write set for one request.
    #[must_use]
    pub fn for_request(request: &Cs2ConfigRequest) -> Vec<Self> {
        let mut targets = vec![Self::Optimization];
        if request.bootstraps_autoexec() {
            targets.push(Self::Autoexec);
        }
        targets.extend(Self::for_optional_assets(request.optional_assets()));
        targets
    }

    #[must_use]
    pub(crate) fn for_optional_assets(
        assets: &std::collections::BTreeSet<OptionalCfgAsset>,
    ) -> Vec<Self> {
        assets
            .iter()
            .copied()
            .map(Self::from_optional_asset)
            .collect()
    }

    pub(crate) const fn from_optional_asset(asset: OptionalCfgAsset) -> Self {
        match asset {
            OptionalCfgAsset::NetStable => Self::NetStable,
            OptionalCfgAsset::NetHighPing => Self::NetHighPing,
            OptionalCfgAsset::NetUnstable => Self::NetUnstable,
            OptionalCfgAsset::NetBad => Self::NetBad,
            OptionalCfgAsset::DebugHud => Self::DebugHud,
            OptionalCfgAsset::DebugHudOff => Self::DebugHudOff,
            OptionalCfgAsset::AudioStable => Self::AudioStable,
            OptionalCfgAsset::AudioLowLatency025 => Self::AudioLowLatency025,
            OptionalCfgAsset::AudioLowLatency001 => Self::AudioLowLatency001,
            OptionalCfgAsset::FrameBaseline400 => Self::FrameBaseline400,
            OptionalCfgAsset::FrameRawUncapped => Self::FrameRawUncapped,
            OptionalCfgAsset::AudioEqCrisp => Self::AudioEqCrisp,
            OptionalCfgAsset::AudioEqSmooth => Self::AudioEqSmooth,
            OptionalCfgAsset::AudioLegacyLrIsolation => Self::AudioLegacyLrIsolation,
            OptionalCfgAsset::Competitive2026 => Self::Competitive2026,
        }
    }

    pub(crate) const fn optional_asset(self) -> Option<OptionalCfgAsset> {
        match self {
            Self::Optimization | Self::Autoexec => None,
            Self::NetStable => Some(OptionalCfgAsset::NetStable),
            Self::NetHighPing => Some(OptionalCfgAsset::NetHighPing),
            Self::NetUnstable => Some(OptionalCfgAsset::NetUnstable),
            Self::NetBad => Some(OptionalCfgAsset::NetBad),
            Self::DebugHud => Some(OptionalCfgAsset::DebugHud),
            Self::DebugHudOff => Some(OptionalCfgAsset::DebugHudOff),
            Self::AudioStable => Some(OptionalCfgAsset::AudioStable),
            Self::AudioLowLatency025 => Some(OptionalCfgAsset::AudioLowLatency025),
            Self::AudioLowLatency001 => Some(OptionalCfgAsset::AudioLowLatency001),
            Self::FrameBaseline400 => Some(OptionalCfgAsset::FrameBaseline400),
            Self::FrameRawUncapped => Some(OptionalCfgAsset::FrameRawUncapped),
            Self::AudioEqCrisp => Some(OptionalCfgAsset::AudioEqCrisp),
            Self::AudioEqSmooth => Some(OptionalCfgAsset::AudioEqSmooth),
            Self::AudioLegacyLrIsolation => Some(OptionalCfgAsset::AudioLegacyLrIsolation),
            Self::Competitive2026 => Some(OptionalCfgAsset::Competitive2026),
        }
    }

    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Optimization => OPTIMIZATION_FILE,
            Self::Autoexec => AUTOEXEC_FILE,
            target => target
                .optional_asset()
                .expect("non-core target has a matching optional asset")
                .file_name(),
        }
    }
}
