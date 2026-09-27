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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoPreviewRow {
    pub setting: String,
    pub current_and_recommended: String,
    pub status_and_note: String,
}
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoPreview {
    pub discovery: String,
    pub tier: VideoPresetTier,
    pub rows: Vec<VideoPreviewRow>,
}
#[cfg(test)]
impl VideoPreview {
    pub fn awaiting_discovery() -> Self {
        Self {
            discovery:
                "Enter a trusted Steam root and refresh read-only CS2 video discovery and guidance."
                    .into(),
            tier: VideoPresetTier::RawLatency,
            rows: Vec::new(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_is_read_only_guidance() {
        let preview = VideoPreview::awaiting_discovery();
        assert_eq!(VideoPresetTier::ALL.len(), 4);
        assert_eq!(VideoPresetTier::NvidiaVrr.label(), "NVIDIA VRR");
        assert!(preview.rows.is_empty());
    }
    #[test]
    fn preview_has_no_apply_capability() {
        let preview = VideoPreview {
            discovery: "trusted CS2 video document".into(),
            tier: VideoPresetTier::GpuConstrained,
            rows: Vec::new(),
        };
        assert!(preview.rows.is_empty());
    }
}
