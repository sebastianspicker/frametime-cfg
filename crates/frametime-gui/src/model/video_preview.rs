#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoPresetTier {
    RawLatency,
    NvidiaVrr,
    AmdFreeSync,
    GpuConstrained,
}
impl VideoPresetTier {
    pub const ALL: [Self; 4] = [
        Self::RawLatency,
        Self::NvidiaVrr,
        Self::AmdFreeSync,
        Self::GpuConstrained,
    ];
    pub const fn label(self) -> &'static str {
        match self {
            Self::RawLatency => "Raw latency",
            Self::NvidiaVrr => "NVIDIA VRR",
            Self::AmdFreeSync => "AMD FreeSync",
            Self::GpuConstrained => "GPU-constrained",
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tier_labels_are_stable() {
        assert_eq!(VideoPresetTier::ALL.len(), 4);
        assert_eq!(VideoPresetTier::NvidiaVrr.label(), "NVIDIA VRR");
    }
}
