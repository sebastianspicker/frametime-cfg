use crate::*;
use std::{cell::RefCell, marker::PhantomData};

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

#[cfg(not(windows))]
#[derive(Debug)]
pub(crate) struct LockHandle(PathBuf);
#[cfg(not(windows))]
impl Drop for LockHandle {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

thread_local! {
    // Lock files this thread holds, with the number of live guards for each.
    static HELD: RefCell<BTreeMap<PathBuf, (usize, LockHandle)>> =
        const { RefCell::new(BTreeMap::new()) };
}

/// The suite work lock. It is re-entrant on the thread that holds it, so an
/// engine run can hold it throughout while inner writers still take it; other
/// threads and processes fail to acquire it until the last guard drops.
#[derive(Debug)]
pub(crate) struct WorkLock {
    path: PathBuf,
    // The lock is tracked per thread, so a guard must not move to another one.
    _thread_bound: PhantomData<*const ()>,
}
impl WorkLock {
    pub(crate) fn acquire(work_dir: &Path) -> Result<Self, String> {
        let path = work_dir.join(LOCK_FILE);
        let nested = HELD
            .with_borrow_mut(|held| held.get_mut(&path).map(|(depth, _)| *depth += 1).is_some());
        if !nested {
            let handle = create_lock(work_dir, &path)?;
            HELD.with_borrow_mut(|held| held.insert(path.clone(), (1, handle)));
        }
        Ok(Self {
            path,
            _thread_bound: PhantomData,
        })
    }
}
impl Drop for WorkLock {
    fn drop(&mut self) {
        // During thread teardown the map is already gone and released every lock.
        let released = HELD
            .try_with(|held| {
                let mut held = held.borrow_mut();
                let (depth, _) = held.get_mut(&self.path)?;
                *depth -= 1;
                if *depth == 0 {
                    held.remove(&self.path)
                } else {
                    None
                }
            })
            .ok()
            .flatten();
        drop(released);
    }
}

#[cfg(windows)]
fn create_lock(_work_dir: &Path, path: &Path) -> Result<LockHandle, String> {
    use windows::{
        Win32::Storage::FileSystem::{
            CREATE_NEW, CreateFileW, DELETE, FILE_FLAG_DELETE_ON_CLOSE,
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH, FILE_READ_ATTRIBUTES,
            FILE_SHARE_MODE, FILE_WRITE_DATA, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
        },
        core::PCWSTR,
    };

    let path = crate::wide_path(path);
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
    Ok(lock)
}

#[cfg(not(windows))]
fn create_lock(work_dir: &Path, path: &Path) -> Result<LockHandle, String> {
    fs::create_dir_all(work_dir).map_err(|error| error.to_string())?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("live transaction lock unavailable: {error}"))?;
    Ok(LockHandle(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Runs against the real create_lock on every OS, always in a tempdir. On Windows that
    // includes the protected-DACL hardening, which needs an elevated runner.

    fn acquire_on_other_thread(work_dir: &Path) -> Result<(), String> {
        let work_dir = work_dir.to_path_buf();
        std::thread::spawn(move || WorkLock::acquire(&work_dir).map(drop))
            .join()
            .expect("thread")
    }

    #[test]
    fn nested_acquire_on_the_same_thread_holds_the_file_until_the_last_guard() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_file = dir.path().join(LOCK_FILE);
        let outer = WorkLock::acquire(dir.path()).expect("outer");
        let inner = WorkLock::acquire(dir.path()).expect("nested");
        drop(inner);
        assert!(lock_file.exists());
        drop(outer);
        assert!(!lock_file.exists());
        drop(WorkLock::acquire(dir.path()).expect("fresh"));
    }

    #[test]
    fn another_thread_is_refused_while_the_lock_is_held() {
        let dir = tempfile::tempdir().expect("tempdir");
        let held = WorkLock::acquire(dir.path()).expect("held");
        let error = acquire_on_other_thread(dir.path()).expect_err("refused");
        assert!(
            error.contains("live transaction lock unavailable"),
            "{error}"
        );
        drop(held);
        acquire_on_other_thread(dir.path()).expect("released");
    }

    #[test]
    fn dropping_the_outer_guard_first_still_releases_after_the_inner_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_file = dir.path().join(LOCK_FILE);
        let outer = WorkLock::acquire(dir.path()).expect("outer");
        let inner = WorkLock::acquire(dir.path()).expect("nested");
        drop(outer);
        assert!(lock_file.exists());
        assert!(acquire_on_other_thread(dir.path()).is_err());
        drop(inner);
        assert!(!lock_file.exists());
        acquire_on_other_thread(dir.path()).expect("released");
    }
}
