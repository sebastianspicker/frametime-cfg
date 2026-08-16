use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::Cs2ConfigFs;

#[derive(Debug, Default)]
pub struct NativeCs2ConfigFs;

impl Cs2ConfigFs for NativeCs2ConfigFs {
    fn create_directory(&mut self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn read_file(&mut self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn create_file_new(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        output.write_all(bytes)?;
        output.sync_all()
    }

    fn atomic_replace(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("CS2 CFG target has no parent"))?;
        let temporary = temporary_path(path);
        let result = (|| {
            let mut output = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            output.write_all(bytes)?;
            output.sync_all()?;
            replace_file(&temporary, path)?;
            sync_parent(parent)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(format!(".tmp.{}.{}", std::process::id(), nonce));
    PathBuf::from(temporary)
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
