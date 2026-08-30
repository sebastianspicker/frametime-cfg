//! Read-only CS2 video-file discovery and UI-level guidance.
//!
//! CS2 owns its video configuration format. This module deliberately does not
//! insert keys, clear attributes, create backups, or write game files.

use std::collections::BTreeMap;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VideoError {
    #[error("invalid CS2 video document: {0}")]
    Parse(String),
}

/// A manual in-game UI goal. It is not a raw file preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoGoal {
    RawLatency,
    NvidiaVrr,
    AmdFreeSync,
    GpuConstrained,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoStatus {
    Guidance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoRow {
    pub setting: String,
    pub current: Option<String>,
    pub recommended: String,
    pub status: VideoStatus,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoDocumentRoot {
    ObservedVideoConfig,
    ObservedVideoCfg,
}

impl VideoDocumentRoot {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ObservedVideoConfig => "VideoConfig (observed, non-authoritative)",
            Self::ObservedVideoCfg => "video.cfg (observed, non-authoritative)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoDocument {
    root: VideoDocumentRoot,
    values: BTreeMap<String, String>,
}

/// Returns operator guidance for a named display goal. Values are intentionally
/// represented only as UI choices, never as raw configuration-file assignments.
#[must_use]
pub fn video_guidance(goal: VideoGoal) -> Vec<VideoRow> {
    let (recommended, note) = match goal {
        VideoGoal::RawLatency => (
            "Raw latency",
            "Use the CS2 Video UI to select fullscreen, refresh, and V-Sync intentionally. Compare tearing and latency with a repeatable workload; the generated recovery baseline supplies fps_max 400 and the uncapped path is an explicit add-on.",
        ),
        VideoGoal::NvidiaVrr => (
            "NVIDIA VRR",
            "Configure G-SYNC/VRR, V-Sync, Reflex, and an explicit frame ceiling in their supported NVIDIA and CS2 UI surfaces. Validate the combination on the target display.",
        ),
        VideoGoal::AmdFreeSync => (
            "AMD FreeSync",
            "Configure FreeSync/VRR and Anti-Lag only through supported AMD and CS2 UI surfaces. Check current game and anti-cheat compatibility before enabling driver features.",
        ),
        VideoGoal::GpuConstrained => (
            "GPU-constrained",
            "Use the CS2 Video UI to reduce the measured GPU bottleneck one setting at a time, retaining gameplay-critical visibility options and comparing the same map and workload.",
        ),
    };
    vec![VideoRow {
        setting: recommended.into(),
        current: None,
        recommended: "Manual CS2 / driver UI review".into(),
        status: VideoStatus::Guidance,
        note: note.into(),
    }]
}

/// Parses a read-only CS2 video document. Observed `VideoConfig` and
/// `video.cfg` roots are accepted for inspection only; neither is a schema
/// contract or discovery target.
pub fn parse_video_document(text: &str) -> Result<VideoDocument, VideoError> {
    let mut quoted = text
        .lines()
        .map(str::trim)
        .map(|line| line.trim_start_matches('\u{feff}'))
        .filter(|line| line.starts_with('"'));
    let root = match quoted.next() {
        Some("\"VideoConfig\"") => VideoDocumentRoot::ObservedVideoConfig,
        Some("\"video.cfg\"") => VideoDocumentRoot::ObservedVideoCfg,
        _ => {
            return Err(VideoError::Parse(
                "missing VideoConfig or observed video.cfg root".into(),
            ));
        }
    };
    if !text.lines().any(|line| line.trim() == "{") || !text.lines().any(|line| line.trim() == "}")
    {
        return Err(VideoError::Parse("unbalanced video document braces".into()));
    }
    let values = text.lines().filter_map(parse_assignment).collect();
    Ok(VideoDocument { root, values })
}

impl VideoDocument {
    #[must_use]
    pub const fn root(&self) -> VideoDocumentRoot {
        self.root
    }

    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }

    #[must_use]
    pub fn rows(&self, goal: VideoGoal) -> Vec<VideoRow> {
        video_guidance(goal)
    }
}

fn parse_assignment(line: &str) -> Option<(String, String)> {
    let line = line.trim_start().trim_start_matches('\u{feff}');
    let (key, rest) = quoted_value(line)?;
    let (value, rest) = quoted_value(rest.trim_start())?;
    (rest.trim().is_empty() || rest.trim_start().starts_with("//"))
        .then(|| (key.to_owned(), value.to_owned()))
}

fn quoted_value(value: &str) -> Option<(&str, &str)> {
    let value = value.strip_prefix('"')?;
    let end = value.find('"')?;
    Some((&value[..end], &value[end + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guidance_has_one_explicit_ui_goal_per_choice() {
        for goal in [
            VideoGoal::RawLatency,
            VideoGoal::NvidiaVrr,
            VideoGoal::AmdFreeSync,
            VideoGoal::GpuConstrained,
        ] {
            assert_eq!(video_guidance(goal).len(), 1);
        }
    }

    #[test]
    fn inspection_accepts_observed_roots_without_authorizing_writes() {
        assert_eq!(
            parse_video_document("\"VideoConfig\"\n{\n}\n")
                .expect("CS2 root")
                .root(),
            VideoDocumentRoot::ObservedVideoConfig
        );
        assert!(
            parse_video_document("\"VideoConfig\"\n{\n}\n")
                .expect("VideoConfig root")
                .root()
                .label()
                .contains("non-authoritative")
        );
        let observed = parse_video_document("\"video.cfg\"\n{\n}\n").expect("observed root");
        assert_eq!(observed.root(), VideoDocumentRoot::ObservedVideoCfg);
        assert!(observed.root().label().contains("non-authoritative"));
    }
}
