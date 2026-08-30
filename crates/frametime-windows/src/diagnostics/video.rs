use crate::*;
/// A safe, display-only snapshot of discovered hardware.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HardwareInfo {
    pub display_adapters: Vec<String>,
    pub gpu_branch: Option<GpuBranch>,
}

/// Typed, read-only preview of a discovered CS2 video document and one manual
/// UI-level display goal. The controller intentionally has no apply, backup,
/// or file-attribute mutation surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoPreview {
    pub steam_root: PathBuf,
    pub video_path: PathBuf,
    pub document_root: frametime_domain::VideoDocumentRoot,
    pub goal: frametime_domain::VideoGoal,
    pub rows: Vec<VideoRow>,
}

/// Performs trusted discovery and read-only parsing in one host-owned call.
pub fn preview_video(
    steam_root: &Path,
    goal: frametime_domain::VideoGoal,
) -> Result<Option<VideoPreview>, String> {
    let Some(path) = discover_video_txt(steam_root)? else {
        return Ok(None);
    };
    let document = read_trusted_video_document(steam_root, &path)
        .map_err(|error| format!("read trusted CS2 cs2_video.txt: {error}"))?;
    Ok(Some(VideoPreview {
        steam_root: steam_root.to_path_buf(),
        video_path: path,
        document_root: document.root(),
        goal,
        rows: document.rows(goal),
    }))
}
