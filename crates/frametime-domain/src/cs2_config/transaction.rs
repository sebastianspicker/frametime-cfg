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

/// Drops the `// Generated: <minute>` line, which differs on every render.
fn without_generated_line(bytes: &[u8]) -> Vec<u8> {
    bytes
        .split_inclusive(|byte| *byte == b'\n')
        .filter(|line| !line.starts_with(b"// Generated:"))
        .flatten()
        .copied()
        .collect()
}

/// Like `verify_bytes`, but a different generation timestamp is not a mismatch.
pub(super) fn verify_bytes_ignoring_generated_line(
    files: &mut dyn Cs2ConfigFs,
    path: &Path,
    expected: &[u8],
) -> Result<(), Cs2ConfigError> {
    let actual = files.read_file(path)?;
    if without_generated_line(&actual) == without_generated_line(expected) {
        Ok(())
    } else {
        Err(Cs2ConfigError::ReadbackMismatch {
            target: path.to_path_buf(),
            recovery: None,
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

#[cfg(test)]
mod generated_line_tests {
    use super::without_generated_line;

    #[test]
    fn only_the_generated_line_is_ignored() {
        let a = b"// head\n// Generated: 2026-08-23 12:34\nexec x\n";
        let b = b"// head\n// Generated: 2026-08-23 12:35\nexec x\n";
        let c = b"// head\n// Generated: 2026-08-23 12:35\nexec y\n";
        assert_eq!(without_generated_line(a), without_generated_line(b));
        assert_ne!(without_generated_line(a), without_generated_line(c));
    }
}
