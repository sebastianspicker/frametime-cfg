//! Counter-Strike video configuration parsing, policy, and safe persistence.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use thiserror::Error;

use crate::steam::{SteamError, trusted_directory, trusted_file_under};

const USERDATA_DIR: &str = "userdata";

#[derive(Debug, Error)]
pub enum VideoError {
    #[error(transparent)]
    Steam(#[from] SteamError),
    #[error("invalid video.txt: {0}")]
    Parse(String),
    #[error("video.txt is not a trusted Steam userdata file: {0}")]
    UntrustedPath(PathBuf),
    #[error("video.txt platform preparation failed: {0}")]
    Platform(#[source] io::Error),
    #[error("video.txt I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("video.txt readback did not match the requested content")]
    ReadbackMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoTier {
    Auto,
    High,
    Mid,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoStatus {
    Ok,
    Missing,
    Differs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoPreset {
    pub value: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoRow {
    pub setting: String,
    pub current: Option<String>,
    pub recommended: String,
    pub status: VideoStatus,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoDocument {
    lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoWriteReport {
    pub backup_created: bool,
    pub tier: VideoTier,
    pub bytes_written: usize,
}

/// Platform-specific mutation boundary. The Windows/UI layer can clear the
/// Steam Cloud read-only attribute here before the data layer replaces a file.
pub trait VideoFilePlatform {
    fn clear_read_only(&self, path: &Path) -> io::Result<()>;
}

#[must_use]
pub const fn resolve_video_tier(requested: VideoTier, vendor: GpuVendor) -> VideoTier {
    match requested {
        VideoTier::Auto if matches!(vendor, GpuVendor::Nvidia) => VideoTier::High,
        VideoTier::Auto => VideoTier::Mid,
        value => value,
    }
}

#[must_use]
pub fn video_preset(tier: VideoTier) -> BTreeMap<&'static str, VideoPreset> {
    let tier = resolve_video_tier(tier, GpuVendor::Other);
    let mut values = BTreeMap::new();
    let mut insert = |key, value, note| {
        values.insert(key, VideoPreset { value, note });
    };
    let msaa = if matches!(tier, VideoTier::Low) {
        "0"
    } else {
        "4"
    };
    let texture = if matches!(tier, VideoTier::Low) {
        "0"
    } else {
        "5"
    };
    let cmaa = if matches!(tier, VideoTier::Low) {
        "1"
    } else {
        "0"
    };
    insert(
        "setting.msaa_samples",
        msaa,
        if matches!(tier, VideoTier::High) {
            "4x MSAA; benchmark against 2x or CMAA2"
        } else if matches!(tier, VideoTier::Mid) {
            "4x; use 2x if the FPS budget is tight"
        } else {
            "MSAA disabled; CMAA2 enabled separately"
        },
    );
    insert(
        "setting.mat_vsync",
        "0",
        "VSync off in the repository preset",
    );
    insert(
        "setting.fullscreen",
        "1",
        "Exclusive fullscreen in the repository preset",
    );
    insert("setting.r_low_latency", "1", "NVIDIA Reflex On");
    insert("setting.r_csgo_fsr_upsample", "0", "FSR disabled");
    insert(
        "setting.shaderquality",
        if matches!(tier, VideoTier::High) {
            "1"
        } else {
            "0"
        },
        if matches!(tier, VideoTier::High) {
            "High shader quality"
        } else {
            "Low shader quality"
        },
    );
    insert(
        "setting.r_texturefilteringquality",
        texture,
        if matches!(tier, VideoTier::Low) {
            "Bilinear filtering"
        } else {
            "AF16x"
        },
    );
    insert(
        "setting.r_csgo_cmaa_enable",
        cmaa,
        if matches!(tier, VideoTier::Low) {
            "CMAA2 on; compare against MSAA with the same workload"
        } else {
            "Off; MSAA handles AA"
        },
    );
    insert("setting.r_aoproxy_enable", "0", "AO off");
    insert(
        "setting.sc_hdr_enabled_override",
        "3",
        "Performance; compare visually",
    );
    insert(
        "setting.r_particle_max_detail_level",
        "0",
        "Low particle detail",
    );
    insert("setting.csm_enabled", "1", "Shadows enabled");
    insert(
        "setting.videocfg_dynamic_shadows",
        "1",
        "Dynamic Shadows All",
    );
    values
}

pub fn parse_video_document(text: &str) -> Result<VideoDocument, VideoError> {
    let lines: Vec<String> = text.lines().map(ToOwned::to_owned).collect();
    if !lines
        .iter()
        .any(|line| line.trim().trim_start_matches('\u{feff}') == "\"VideoConfig\"")
    {
        return Err(VideoError::Parse("missing VideoConfig root".into()));
    }
    if !lines.iter().any(|line| line.trim() == "{") || !lines.iter().any(|line| line.trim() == "}")
    {
        return Err(VideoError::Parse("unbalanced VideoConfig braces".into()));
    }
    for line in &lines {
        let trimmed = line.trim_start().trim_start_matches('\u{feff}');
        if trimmed.starts_with('"')
            && trimmed != "\"VideoConfig\""
            && parse_assignment(line).is_none()
        {
            return Err(VideoError::Parse("malformed quoted setting".into()));
        }
    }
    Ok(VideoDocument { lines })
}

impl VideoDocument {
    #[must_use]
    pub fn values(&self) -> BTreeMap<String, String> {
        self.lines
            .iter()
            .filter_map(|line| parse_assignment(line))
            .collect()
    }

    #[must_use]
    pub fn rows(&self, tier: VideoTier, vendor: GpuVendor) -> Vec<VideoRow> {
        let current = self.values();
        video_preset(resolve_video_tier(tier, vendor))
            .into_iter()
            .map(|(setting, preset)| {
                let value = current.get(setting).cloned();
                let status = match value.as_deref() {
                    Some(value) if value == preset.value => VideoStatus::Ok,
                    Some(_) => VideoStatus::Differs,
                    None => VideoStatus::Missing,
                };
                VideoRow {
                    setting: setting.trim_start_matches("setting.").to_owned(),
                    current: value,
                    recommended: preset.value.to_owned(),
                    status,
                    note: preset.note.to_owned(),
                }
            })
            .collect()
    }

    #[must_use]
    pub fn with_preset(&self, tier: VideoTier, vendor: GpuVendor) -> Self {
        let tier = resolve_video_tier(tier, vendor);
        let managed = video_preset(tier);
        let mut seen = BTreeSet::new();
        let mut output = Vec::with_capacity(self.lines.len() + managed.len());
        for line in &self.lines {
            let Some((key, _, indent)) = parse_assignment_with_indent(line) else {
                output.push(line.clone());
                continue;
            };
            let Some(preset) = managed.get(key.as_str()) else {
                output.push(line.clone());
                continue;
            };
            if seen.insert(key.clone()) {
                output.push(format!(
                    "{indent}\"{key}\"\t\"{}\"{}",
                    preset.value,
                    trailing_comment(line)
                ));
            }
        }
        let closing = output
            .iter()
            .rposition(|line| line.trim() == "}")
            .unwrap_or(output.len());
        for (key, preset) in &managed {
            if !seen.contains(*key) {
                output.insert(closing, format!("    \"{key}\"\t\"{}\"", preset.value));
            }
        }
        Self { lines: output }
    }

    #[must_use]
    pub fn to_utf8(&self) -> String {
        let mut text = self.lines.join("\n");
        text.push('\n');
        text
    }
}

/// Finds the newest trusted video.txt in Steam userdata. Paths outside the
/// exact `userdata/{account-id}/730/local/cfg/video.txt` layout are never returned.
pub fn discover_video_txt(steam_root: &Path) -> Result<Option<PathBuf>, VideoError> {
    trusted_directory(steam_root)?;
    let userdata = steam_root.join(USERDATA_DIR);
    if !userdata.is_dir() {
        return Ok(None);
    }
    trusted_directory(&userdata)?;
    let mut candidates = Vec::new();
    for account in fs::read_dir(&userdata)? {
        let account = account?.path();
        if account
            .file_name()
            .is_none_or(|name| name.to_string_lossy().is_empty())
            || trusted_directory(&account).is_err()
        {
            continue;
        }
        let video = account
            .join("730")
            .join("local")
            .join("cfg")
            .join("video.txt");
        if video.is_file() && trusted_file_under(steam_root, &video).is_ok() {
            let modified = video.metadata()?.modified().unwrap_or(UNIX_EPOCH);
            candidates.push((modified, video));
        }
    }
    candidates.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    Ok(candidates.pop().map(|(_, path)| path))
}

pub fn read_trusted_video_document(
    steam_root: &Path,
    path: &Path,
) -> Result<VideoDocument, VideoError> {
    ensure_trusted_video_path(steam_root, path)?;
    parse_video_document(&fs::read_to_string(path)?)
}

/// Creates `video.txt.bak` exactly once, delegates read-only attribute removal,
/// writes a same-directory temporary file, atomically replaces and readbacks.
pub fn write_trusted_video_config(
    steam_root: &Path,
    path: &Path,
    document: &VideoDocument,
    tier: VideoTier,
    vendor: GpuVendor,
    platform: &dyn VideoFilePlatform,
) -> Result<VideoWriteReport, VideoError> {
    ensure_trusted_video_path(steam_root, path)?;
    let original = fs::read(path)?;
    let backup_created = create_backup_once(&path.with_extension("txt.bak"), &original)?;
    platform
        .clear_read_only(path)
        .map_err(VideoError::Platform)?;
    let resolved = resolve_video_tier(tier, vendor);
    let expected = document
        .with_preset(resolved, vendor)
        .to_utf8()
        .into_bytes();
    let parent = path
        .parent()
        .ok_or_else(|| VideoError::UntrustedPath(path.to_path_buf()))?;
    let temporary = temporary_path(path);
    let write_result = (|| -> Result<(), VideoError> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(&expected)?;
        file.sync_all()?;
        replace_file(&temporary, path)?;
        sync_parent(parent)?;
        if fs::read(path)? != expected {
            return Err(VideoError::ReadbackMismatch);
        }
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result?;
    Ok(VideoWriteReport {
        backup_created,
        tier: resolved,
        bytes_written: expected.len(),
    })
}

fn ensure_trusted_video_path(steam_root: &Path, path: &Path) -> Result<(), VideoError> {
    trusted_directory(steam_root)?;
    let relative = path
        .strip_prefix(steam_root)
        .map_err(|_| VideoError::UntrustedPath(path.to_path_buf()))?;
    let components: Vec<_> = relative.components().collect();
    let valid = components.len() == 6
        && components[0].as_os_str() == "userdata"
        && !components[1].as_os_str().is_empty()
        && components[2].as_os_str() == "730"
        && components[3].as_os_str() == "local"
        && components[4].as_os_str() == "cfg"
        && components[5].as_os_str() == "video.txt";
    if !valid || trusted_file_under(steam_root, path).is_err() {
        return Err(VideoError::UntrustedPath(path.to_path_buf()));
    }
    Ok(())
}

fn create_backup_once(path: &Path, original: &[u8]) -> io::Result<bool> {
    let mut backup = match fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error),
    };
    backup.write_all(original)?;
    backup.sync_all()?;
    Ok(true)
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    value.push(format!(".tmp.{}.{nonce}", std::process::id()));
    PathBuf::from(value)
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        core::PCWSTR,
    };
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(io::Error::other)
    }
}

#[cfg(not(windows))]
fn sync_parent(parent: &Path) -> io::Result<()> {
    fs::File::open(parent)?.sync_all()
}
#[cfg(windows)]
const fn sync_parent(_: &Path) -> io::Result<()> {
    Ok(())
}

fn parse_assignment(line: &str) -> Option<(String, String)> {
    parse_assignment_with_indent(line).map(|(key, value, _)| (key, value))
}
fn parse_assignment_with_indent(line: &str) -> Option<(String, String, String)> {
    let indent = line.len() - line.trim_start().len();
    let body = line.trim_start();
    let (key, rest) = quoted_value(body)?;
    let (value, rest) = quoted_value(rest.trim_start())?;
    if !rest.trim().is_empty() && !rest.trim_start().starts_with("//") {
        return None;
    }
    Some((key.to_owned(), value.to_owned(), line[..indent].to_owned()))
}

fn trailing_comment(line: &str) -> &str {
    let Some((_, rest)) = quoted_value(line.trim_start()) else {
        return "";
    };
    let Some((_, rest)) = quoted_value(rest.trim_start()) else {
        return "";
    };
    let rest = rest.trim_end();
    if rest.trim_start().starts_with("//") {
        rest
    } else {
        ""
    }
}
fn quoted_value(value: &str) -> Option<(&str, &str)> {
    let value = value.strip_prefix('"')?;
    let end = value.find('"')?;
    Some((&value[..end], &value[end + 1..]))
}
