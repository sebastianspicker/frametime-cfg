use std::{
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

pub fn safe_relative_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    if text.is_empty()
        || text.starts_with(['/', '\\'])
        || text.contains(['\0', ':'])
        || text
            .split(['/', '\\'])
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return false;
    }
    !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

pub fn read_json_tolerant<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

/// Reads JSON and preserves malformed bytes beside the source before returning
/// the parse error. The source itself is never replaced or deleted.
pub fn read_json_preserving_corrupt<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    let bytes = fs::read(path)?;
    match serde_json::from_slice(&bytes) {
        Ok(value) => Ok(value),
        Err(parse_error) => {
            let preserved = preserve_corrupt_bytes(path, &bytes)?;
            Err(io::Error::other(format!(
                "invalid JSON preserved at {}: {parse_error}",
                preserved.display()
            )))
        }
    }
}

pub fn preserve_corrupt_json(path: &Path) -> io::Result<PathBuf> {
    let bytes = fs::read(path)?;
    preserve_corrupt_bytes(path, &bytes)
}

fn preserve_corrupt_bytes(path: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let extension = format!("corrupt.{suffix}.json");
    let preserved = path.with_extension(extension);
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&preserved)?;
    output.write_all(bytes)?;
    output.sync_all()?;
    let verified = fs::read(&preserved)?;
    if Sha256::digest(&verified) != Sha256::digest(bytes) {
        return Err(io::Error::other("corrupt-file preservation hash mismatch"));
    }
    Ok(preserved)
}

pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = temporary_path(path);
    let mut expected = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    expected.push(b'\n');
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(&expected)?;
        file.sync_all()?;
        replace_file(&temporary, path)?;
        sync_parent(parent)?;
        let persisted = fs::read(path)?;
        if Sha256::digest(&persisted) != Sha256::digest(&expected) {
            return Err(io::Error::other("atomic JSON verification failed"));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(windows))]
fn sync_parent(parent: &Path) -> io::Result<()> {
    fs::File::open(parent)?.sync_all()
}

#[cfg(windows)]
const fn sync_parent(_: &Path) -> io::Result<()> {
    // MOVEFILE_WRITE_THROUGH provides the Windows durability boundary.
    Ok(())
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

fn temporary_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    value.push(format!(".tmp.{}.{nonce}", std::process::id()));
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_traversal_and_absolute_paths() {
        assert!(safe_relative_path(Path::new("cfgs/a.cfg")));
        assert!(!safe_relative_path(Path::new("../a.cfg")));
        assert!(!safe_relative_path(Path::new("/a.cfg")));
        assert!(!safe_relative_path(Path::new(r"C:\a.cfg")));
        assert!(!safe_relative_path(Path::new(r"..\a.cfg")));
        assert!(!safe_relative_path(Path::new(r"\\server\share\a.cfg")));
    }

    #[test]
    fn corrupt_input_is_hash_preserved_before_reset() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = temporary.path().join("state.json");
        fs::write(&source, b"not json {{{").expect("fixture");
        let preserved = preserve_corrupt_json(&source).expect("preserved");
        assert_eq!(
            fs::read(source).expect("source"),
            fs::read(preserved).expect("copy")
        );
    }

    #[test]
    fn tolerant_corrupt_read_reports_preserved_copy() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = temporary.path().join("progress.json");
        fs::write(&source, b"not json").expect("fixture");
        let error =
            read_json_preserving_corrupt::<serde_json::Value>(&source).expect_err("must preserve");
        assert!(error.to_string().contains("invalid JSON preserved at"));
        assert_eq!(
            fs::read_dir(temporary.path()).expect("directory").count(),
            2
        );
        assert_eq!(fs::read(source).expect("source"), b"not json");
    }
}
