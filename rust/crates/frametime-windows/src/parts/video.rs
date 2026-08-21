/// A safe, display-only snapshot of discovered hardware.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HardwareInfo {
    pub display_adapters: Vec<String>,
    pub gpu_branch: Option<GpuBranch>,
}

/// Typed, read-only preview of the exact CS2 video settings managed by a
/// selected preset.  The `rows` collection is always the full 13-setting core
/// contract; an incomplete preset is treated as unavailable rather than being
/// partially displayed or applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoPreview {
    pub steam_root: PathBuf,
    pub video_path: PathBuf,
    pub requested_tier: VideoTier,
    pub resolved_tier: VideoTier,
    pub rows: Vec<VideoRow>,
}

/// Result of a readback-verified CS2 `video.txt` update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoApplyResult {
    pub preview: VideoPreview,
    pub backup_created: bool,
    pub bytes_written: usize,
}

/// Native controller for one operator-selected, trusted Steam root.
///
/// It does not consult the registry or search arbitrary drives.  Core owns
/// Steam-root/reparse validation, exact userdata path selection, one-time
/// backup creation, atomic replacement, and content readback; this adapter
/// supplies the Windows-only read-only attribute operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoController {
    steam_root: PathBuf,
    vendor: GpuVendor,
}

/// Resolve the video preset vendor from the same present, status-OK SetupAPI
/// evidence used by native device transactions. Auto selects the NVIDIA tier
/// only when every observed display adapter is NVIDIA; hybrid, mixed, empty,
/// or ambiguous inventories fail closed to the vendor-neutral tier.
pub fn detect_video_gpu_vendor() -> Result<GpuVendor, String> {
    #[cfg(windows)]
    {
        let devices = enumerate_present_status_ok_pci(&WindowsSetupApiEnumerator)
            .map_err(|error| format!("discover display adapters: {error}"))?;
        let vendors = devices
            .into_iter()
            .filter_map(|(class, binding)| {
                (class == PciDeviceClass::Display).then_some(binding.vendor_id)
            })
            .collect::<Vec<_>>();
        Ok(resolve_video_gpu_vendor(&vendors))
    }
    #[cfg(not(windows))]
    {
        Err("GPU vendor discovery is available only on supported Windows hosts".into())
    }
}

#[cfg(any(test, windows))]
fn resolve_video_gpu_vendor(vendors: &[u16]) -> GpuVendor {
    const NVIDIA_VENDOR_ID: u16 = 0x10de;
    if !vendors.is_empty() && vendors.iter().all(|vendor| *vendor == NVIDIA_VENDOR_ID) {
        GpuVendor::Nvidia
    } else {
        GpuVendor::Other
    }
}

impl VideoController {
    /// Bind this controller to an explicit Steam root.  The root must be an
    /// absolute real directory without a reparse hop, as checked by core on
    /// every preview and apply operation.
    pub fn new(steam_root: &Path, vendor: GpuVendor) -> Result<Self, String> {
        let controller = Self {
            steam_root: steam_root.to_path_buf(),
            vendor,
        };
        // Validate that the selected root currently contains an exact trusted
        // video file before exposing a controller that could later mutate it.
        controller.video_path()?;
        Ok(controller)
    }

    #[must_use]
    pub fn steam_root(&self) -> &Path {
        &self.steam_root
    }

    #[must_use]
    pub const fn vendor(&self) -> GpuVendor {
        self.vendor
    }

    /// Read the selected `video.txt` and return the complete preset diff.
    /// This performs no persistent write.
    pub fn preview(&self, tier: VideoTier) -> Result<VideoPreview, String> {
        let path = self.video_path()?;
        let document = read_trusted_video_document(&self.steam_root, &path)
            .map_err(|error| format!("read trusted CS2 video.txt: {error}"))?;
        self.preview_document(path, tier, &document)
    }

    /// Safely apply all 13 managed settings.  On Windows this first creates a
    /// one-time `.bak`, clears only the native read-only file attribute, then
    /// delegates atomic replace and exact readback to core.  Other platforms
    /// deliberately fail closed.
    pub fn apply(&self, tier: VideoTier) -> Result<VideoApplyResult, String> {
        #[cfg(windows)]
        {
            self.apply_with_platform(tier, &NativeVideoFilePlatform)
        }
        #[cfg(not(windows))]
        {
            let _ = tier;
            Err("CS2 video apply is available only on supported Windows hosts".into())
        }
    }

    fn video_path(&self) -> Result<PathBuf, String> {
        discover_video_txt(&self.steam_root)
            .map_err(|error| format!("validate selected Steam root: {error}"))?
            .ok_or_else(|| {
                "no trusted CS2 userdata video.txt exists under the selected Steam root".into()
            })
    }

    fn preview_document(
        &self,
        video_path: PathBuf,
        tier: VideoTier,
        document: &VideoDocument,
    ) -> Result<VideoPreview, String> {
        let resolved_tier = resolve_video_tier(tier, self.vendor);
        let rows = document.rows(tier, self.vendor);
        require_complete_video_rows(&rows)?;
        Ok(VideoPreview {
            steam_root: self.steam_root.clone(),
            video_path,
            requested_tier: tier,
            resolved_tier,
            rows,
        })
    }

    #[cfg(windows)]
    fn apply_with_platform(
        &self,
        tier: VideoTier,
        platform: &dyn VideoFilePlatform,
    ) -> Result<VideoApplyResult, String> {
        let path = self.video_path()?;
        let document = read_trusted_video_document(&self.steam_root, &path)
            .map_err(|error| format!("capture trusted CS2 video.txt before mutation: {error}"))?;
        self.preview_document(path.clone(), tier, &document)?;
        let VideoWriteReport {
            backup_created,
            bytes_written,
            ..
        } = write_trusted_video_config(
            &self.steam_root,
            &path,
            &document,
            tier,
            self.vendor,
            platform,
        )
        .map_err(|error| format!("apply trusted CS2 video preset: {error}"))?;
        let persisted = read_trusted_video_document(&self.steam_root, &path)
            .map_err(|error| format!("read back trusted CS2 video.txt: {error}"))?;
        let persisted_preview = self.preview_document(path, tier, &persisted)?;
        if persisted_preview
            .rows
            .iter()
            .any(|row| !matches!(row.status, frametime_core::VideoStatus::Ok))
        {
            return Err("CS2 video preset readback does not satisfy all 13 settings".into());
        }
        Ok(VideoApplyResult {
            preview: persisted_preview,
            backup_created,
            bytes_written,
        })
    }
}

fn require_complete_video_rows(rows: &[VideoRow]) -> Result<(), String> {
    const MANAGED_SETTINGS: usize = 13;
    if rows.len() != MANAGED_SETTINGS {
        return Err(format!(
            "CS2 video preset is incomplete: expected {MANAGED_SETTINGS} settings, found {}",
            rows.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod video_vendor_tests {
    use super::{GpuVendor, resolve_video_gpu_vendor};

    #[test]
    fn auto_video_vendor_requires_an_unambiguous_nvidia_inventory() {
        assert_eq!(resolve_video_gpu_vendor(&[0x10de]), GpuVendor::Nvidia);
        assert_eq!(
            resolve_video_gpu_vendor(&[0x10de, 0x10de]),
            GpuVendor::Nvidia
        );
        assert_eq!(resolve_video_gpu_vendor(&[]), GpuVendor::Other);
        assert_eq!(resolve_video_gpu_vendor(&[0x1002]), GpuVendor::Other);
        assert_eq!(
            resolve_video_gpu_vendor(&[0x10de, 0x8086]),
            GpuVendor::Other
        );
    }
}

#[cfg(windows)]
struct NativeVideoFilePlatform;

#[cfg(windows)]
impl VideoFilePlatform for NativeVideoFilePlatform {
    fn clear_read_only(&self, path: &Path) -> std::io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_READONLY, FILE_FLAGS_AND_ATTRIBUTES, GetFileAttributesW,
                INVALID_FILE_ATTRIBUTES, SetFileAttributesW,
            },
            core::PCWSTR,
        };

        let path = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let attributes = unsafe { GetFileAttributesW(PCWSTR(path.as_ptr())) };
        if attributes == INVALID_FILE_ATTRIBUTES {
            return Err(std::io::Error::last_os_error());
        }
        if attributes & FILE_ATTRIBUTE_READONLY.0 == 0 {
            return Ok(());
        }
        unsafe {
            SetFileAttributesW(
                PCWSTR(path.as_ptr()),
                FILE_FLAGS_AND_ATTRIBUTES(attributes & !FILE_ATTRIBUTE_READONLY.0),
            )
            .map_err(std::io::Error::other)
        }
    }
}
