use crate::*;
#[cfg(windows)]
#[derive(Debug)]
pub(crate) struct LockHandle(windows::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Drop for LockHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[derive(Debug)]
pub(crate) struct WorkLock {
    #[cfg(windows)]
    pub(crate) _handle: LockHandle,
    #[cfg(not(windows))]
    pub(crate) path: PathBuf,
}
impl WorkLock {
    #[cfg(windows)]
    pub(crate) fn acquire(work_dir: &Path) -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::Storage::FileSystem::{
                CREATE_NEW, CreateFileW, DELETE, FILE_FLAG_DELETE_ON_CLOSE,
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH, FILE_READ_ATTRIBUTES,
                FILE_SHARE_MODE, FILE_WRITE_DATA, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
            },
            core::PCWSTR,
        };

        let path = work_dir
            .join(LOCK_FILE)
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let handle = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                DELETE.0
                    | FILE_WRITE_DATA.0
                    | FILE_READ_ATTRIBUTES.0
                    | READ_CONTROL.0
                    | WRITE_DAC.0
                    | WRITE_OWNER.0,
                FILE_SHARE_MODE(0),
                None,
                CREATE_NEW,
                FILE_FLAG_DELETE_ON_CLOSE | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH,
                None,
            )
        }
        .map_err(|error| format!("live transaction lock unavailable: {error}"))?;
        let lock = LockHandle(handle);
        trusted_work_dir::harden_created_child(lock.0)
            .map_err(|error| format!("harden live transaction lock: {error}"))?;
        Ok(Self { _handle: lock })
    }

    #[cfg(not(windows))]
    pub(crate) fn acquire(work_dir: &Path) -> Result<Self, String> {
        let path = work_dir.join(LOCK_FILE);
        fs::create_dir_all(work_dir).map_err(|error| error.to_string())?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("live transaction lock unavailable: {error}"))?;
        Ok(Self { path })
    }
}
#[cfg(not(windows))]
impl Drop for WorkLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
