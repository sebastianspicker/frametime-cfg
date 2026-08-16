//! PE detection and setup argument construction.

use std::fs;
use std::path::Path;

#[cfg(test)]
use crate::InstallError;

/// True when file starts with MZ (Windows PE).
pub fn looks_like_windows_pe(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    bytes.len() >= 2 && bytes[0] == b'M' && bytes[1] == b'Z'
}

/// Create a minimal MZ stub for tests. It is intentionally never launch authorization.
#[cfg(test)]
pub(crate) fn write_mz_stub(path: &Path) -> Result<(), InstallError> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    // Minimal MZ header + padding so CreateProcess can fail gracefully or PE check passes
    let mut buf = vec![0u8; 128];
    buf[0] = b'M';
    buf[1] = b'Z';
    fs::write(path, buf)?;
    Ok(())
}

/// Build the default silent setup arguments for vendor packages.
pub fn default_setup_args(clean: bool) -> Vec<String> {
    let mut a = vec!["-s".into(), "-noreboot".into()];
    if clean {
        a.push("-clean".into());
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn uniq() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("dfoundry-launch-{n}"))
    }

    #[test]
    fn pe_check_rejects_text() {
        let dir = uniq();
        fs::create_dir_all(&dir).unwrap();
        let setup = dir.join("setup.exe");
        fs::write(&setup, "fake-setup-placeholder").unwrap();
        assert!(!looks_like_windows_pe(&setup));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pe_check_accepts_mz() {
        let dir = uniq();
        let setup = dir.join("setup.exe");
        write_mz_stub(&setup).unwrap();
        assert!(looks_like_windows_pe(&setup));
        let _ = fs::remove_dir_all(&dir);
    }
}
