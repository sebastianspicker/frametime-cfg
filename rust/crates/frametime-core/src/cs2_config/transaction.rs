use std::path::{Path, PathBuf};

use super::{Cs2ConfigError, Cs2ConfigFs};

pub(super) fn write_and_verify(
    files: &mut dyn Cs2ConfigFs,
    path: &Path,
    expected: &[u8],
    stage: &'static str,
    recovery: Option<PathBuf>,
) -> Result<(), Cs2ConfigError> {
    files
        .atomic_replace(path, expected)
        .map_err(|source| Cs2ConfigError::Mutation {
            stage,
            recovery: recovery.clone(),
            source,
        })?;
    let actual = files
        .read_file(path)
        .map_err(|source| Cs2ConfigError::Mutation {
            stage: "written CFG readback",
            recovery: recovery.clone(),
            source,
        })?;
    if actual == expected {
        Ok(())
    } else {
        Err(Cs2ConfigError::ReadbackMismatch {
            target: path.to_path_buf(),
            recovery,
        })
    }
}

pub(super) fn verify_bytes(
    files: &mut dyn Cs2ConfigFs,
    path: &Path,
    expected: &[u8],
) -> Result<(), Cs2ConfigError> {
    let actual = files.read_file(path)?;
    if actual == expected {
        Ok(())
    } else {
        Err(Cs2ConfigError::ReadbackMismatch {
            target: path.to_path_buf(),
            recovery: None,
        })
    }
}
