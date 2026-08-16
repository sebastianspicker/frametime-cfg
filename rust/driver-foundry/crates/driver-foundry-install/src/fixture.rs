//! Synthetic NVIDIA-like package tree for dry-run pipeline tests.

use std::fs;
use std::path::Path;

use crate::InstallError;

/// Create package root with setup.exe stub, NVI2/, and one folder per component id.
pub(crate) fn create_synthetic_package<I, S>(
    root: &Path,
    component_ids: I,
) -> Result<(), InstallError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    crate::copy::create_new_directory(root)?;
    fs::write(root.join("setup.exe"), b"fake-setup-placeholder")?;
    fs::create_dir_all(root.join("NVI2"))?;
    fs::write(root.join("NVI2").join("setup.cfg"), b"synthetic")?;

    let payload: Vec<u8> = (0..2048u32).map(|i| (i & 0xff) as u8).collect();
    for id in component_ids {
        let id = id.as_ref();
        let dir = root.join(id);
        fs::create_dir_all(dir.join("sub"))?;
        fs::write(dir.join(format!("{id}.txt")), format!("component {id}"))?;
        fs::write(dir.join("sub").join("payload.bin"), &payload)?;
        // Sample INF so deep-inf / try-sign paths have real text to edit in fixtures.
        if id.eq_ignore_ascii_case("Display.Driver") {
            fs::write(
                dir.join("sample.inf"),
                "[Version]\r\n\
Signature=\"$WINDOWS NT$\"\r\n\
\r\n\
[SourceDisksFiles]\r\n\
nvlddmkm.sys=1\r\n\
\r\n\
[Telemetry.Services]\r\n\
AddService=NvTelemetry,,NvTelemetry_Service\r\n\
\r\n\
[GFExperience.CopyFiles]\r\n\
nvcontainer.exe\r\n\
\r\n\
[Strings]\r\n\
Vendor=\"NVIDIA\"\r\n",
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn creates_layout() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("dfoundry-fixture-{n}"));
        create_synthetic_package(&root, ["Display.Driver", "HDAudio"]).unwrap();
        assert!(root.join("setup.exe").is_file());
        assert!(root.join("Display.Driver").is_dir());
        assert!(root.join("NVI2").join("setup.cfg").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_to_replace_existing_fixture_root() {
        let root =
            std::env::temp_dir().join(format!("dfoundry-fixture-existing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("user-owned"), b"keep").unwrap();
        assert!(create_synthetic_package(&root, ["Display.Driver"]).is_err());
        assert_eq!(fs::read(root.join("user-owned")).unwrap(), b"keep");
        let _ = fs::remove_dir_all(root);
    }
}
