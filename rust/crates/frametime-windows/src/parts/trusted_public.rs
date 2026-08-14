fn backup_entry_kind(entry: &BackupEntry) -> &'static str {
    match entry {
        BackupEntry::Registry { .. } => "registry",
        BackupEntry::Hags { .. } => "hags",
        BackupEntry::Service { .. } => "service",
        BackupEntry::Powerplan { .. } => "powerplan",
        BackupEntry::Bootconfig { .. } => "bootconfig",
        BackupEntry::Scheduledtask { .. } => "scheduledtask",
        BackupEntry::NicAdapter { .. } => "nic_adapter",
        BackupEntry::QosUro { .. } => "qos_uro",
        BackupEntry::Defender { .. } => "defender",
        BackupEntry::Cs2ConfigTransaction { .. } => "cs2_config_transaction",
        BackupEntry::Pagefile { .. } => "pagefile",
        BackupEntry::PagefileTransaction { .. } => "pagefile_transaction",
        BackupEntry::Dns { .. } => "dns",
        BackupEntry::InterruptPolicy { .. } => "interrupt_policy",
        BackupEntry::NetworkStackTransaction { .. } => "network_stack_transaction",
        BackupEntry::Drs { .. } => "drs",
        BackupEntry::Unknown(_) => "unknown",
    }
}
/// A verified, handle-backed capability for the suite root.  It is deliberately
/// non-cloneable so callers keep the validated root handle alive while they use
/// any path derived from this fixed directory.
#[derive(Debug)]
pub struct TrustedWorkDir {
    path: PathBuf,
    #[cfg(windows)]
    _handle: trusted_work_dir::RootHandle,
}

impl TrustedWorkDir {
    /// Acquire the only supported suite root without accepting a caller path.
    pub(crate) fn acquire_fixed() -> Result<Self, String> {
        Self::acquire(Path::new(WINDOWS_WORK_DIR))
    }

    /// Open the fixed suite root, creating it with the protected suite DACL
    /// only when it does not already exist.  Existing roots are never silently
    /// repaired: their exact protected DACL must already be present.
    pub fn acquire(work_dir: &Path) -> Result<Self, String> {
        if !platform_is_supported() {
            return Err("the live backend is supported only on supported Windows x64".into());
        }
        if !requested_root_is_exact(work_dir) {
            return Err(format!(
                "live backend persistence is restricted to {WINDOWS_WORK_DIR}"
            ));
        }
        #[cfg(windows)]
        {
            let (path, handle) = trusted_work_dir::open_or_create(work_dir)?;
            Ok(Self {
                path,
                _handle: handle,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = work_dir;
            Err("the live backend is supported only on supported Windows x64".into())
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[cfg(windows)]
    pub(crate) fn root_handle(&self) -> windows::Win32::Foundation::HANDLE {
        self._handle.raw()
    }

    /// Open the only non-JSON child directory admitted for retained driver
    /// artifacts. No caller can select a sibling or arbitrary descendant.
    #[cfg(windows)]
    pub(crate) fn driver_artifact_directory_handle(
        &self,
    ) -> Result<trusted_work_dir::RootHandle, String> {
        trusted_work_dir::open_driver_artifact_directory(self.root_handle())
    }
}

fn requested_root_is_exact(work_dir: &Path) -> bool {
    let mut requested = work_dir.to_string_lossy().replace('/', "\\");
    while requested.ends_with('\\') && requested.len() > 3 {
        requested.pop();
    }
    requested.eq_ignore_ascii_case(WINDOWS_WORK_DIR)
}
fn timestamp() -> String {
    frametime_core::logging::legacy_timestamp_now()
}

fn write_json_atomic_trusted<T: serde::Serialize>(
    trusted: &TrustedWorkDir,
    name: &str,
    value: &T,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        trusted_json_windows::write_json_atomic(trusted, name, value)
    }
    #[cfg(not(windows))]
    {
        let _ = (trusted, name, value);
        Err("the live backend is supported only on supported Windows x64".into())
    }
}

fn read_json_trusted<T: serde::de::DeserializeOwned>(
    trusted: &TrustedWorkDir,
    name: &str,
) -> Result<T, String> {
    #[cfg(windows)]
    {
        trusted_json_windows::read_json(trusted, name)
    }
    #[cfg(not(windows))]
    {
        let _ = (trusted, name);
        Err("the live backend is supported only on supported Windows x64".into())
    }
}
