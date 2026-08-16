//! Archive extract (zip) and portable package build.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, copy, Read, Write};
use std::path::{Path, PathBuf};

use crate::{copy, InstallError};

// Limits are deliberately below the acquisition limit. Extraction is performed in a caller
// controlled workspace, so a valid archive must not be allowed to consume unbounded storage.
const MAX_ZIP_ENTRIES: usize = 20_000;
const MAX_ZIP_ENTRY_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ZIP_EXPANDED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Extract a zip archive to `dest`.
pub(crate) fn extract_zip(archive: &Path, dest: &Path) -> Result<(), InstallError> {
    let file = File::open(archive)
        .map_err(|e| InstallError::Other(format!("open archive {}: {e}", archive.display())))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| InstallError::Other(format!("zip open: {e}")))?;
    validate_zip_entries(&mut zip)?;
    copy::create_new_directory(dest)?;
    let mut extracted_bytes = 0u64;
    for i in 0..zip.len() {
        let mut file = zip
            .by_index(i)
            .map_err(|e| InstallError::Other(format!("zip entry: {e}")))?;
        let outpath = match file.enclosed_name() {
            Some(p) => dest.join(p),
            None => {
                return Err(InstallError::Other(format!(
                    "zip entry {} has an unsafe path",
                    file.name()
                )));
            }
        };
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = File::create(&outpath)?;
            let copied = copy(
                &mut file.by_ref().take(MAX_ZIP_ENTRY_BYTES + 1),
                &mut outfile,
            )?;
            let next_total = extracted_bytes
                .checked_add(copied)
                .ok_or_else(|| InstallError::Other("zip expanded size overflow".into()))?;
            if copied > MAX_ZIP_ENTRY_BYTES || next_total > MAX_ZIP_EXPANDED_BYTES {
                let _ = fs::remove_file(&outpath);
                return Err(InstallError::Other(format!(
                    "zip extraction exceeded configured size limit at {}",
                    file.name()
                )));
            }
            extracted_bytes = next_total;
        }
    }
    Ok(())
}

fn validate_zip_entries<R: Read + io::Seek>(
    zip: &mut zip::ZipArchive<R>,
) -> Result<(), InstallError> {
    let entry_count = zip.len();
    let mut expanded_bytes = 0u64;
    let mut names = BTreeSet::new();
    for index in 0..entry_count {
        let file = zip
            .by_index(index)
            .map_err(|error| InstallError::Other(format!("zip entry: {error}")))?;
        let safe_name = file.enclosed_name().ok_or_else(|| {
            InstallError::Other(format!("zip entry {} has an unsafe path", file.name()))
        })?;
        let normalized = safe_name
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if !names.insert(normalized) {
            return Err(InstallError::Other(format!(
                "zip contains duplicate or case-colliding entry: {}",
                file.name()
            )));
        }
        expanded_bytes = check_zip_expansion_limits(entry_count, file.size(), expanded_bytes)?;
    }
    Ok(())
}

fn check_zip_expansion_limits(
    entries: usize,
    entry_bytes: u64,
    expanded_bytes: u64,
) -> Result<u64, InstallError> {
    if entries > MAX_ZIP_ENTRIES {
        return Err(InstallError::Other(format!(
            "zip has {entries} entries; maximum is {MAX_ZIP_ENTRIES}"
        )));
    }
    if entry_bytes > MAX_ZIP_ENTRY_BYTES {
        return Err(InstallError::Other(format!(
            "zip entry declares {entry_bytes} bytes; maximum is {MAX_ZIP_ENTRY_BYTES}"
        )));
    }
    let total = expanded_bytes
        .checked_add(entry_bytes)
        .ok_or_else(|| InstallError::Other("zip expanded size overflow".into()))?;
    if total > MAX_ZIP_EXPANDED_BYTES {
        return Err(InstallError::Other(format!(
            "zip declares {total} expanded bytes; maximum is {MAX_ZIP_EXPANDED_BYTES}"
        )));
    }
    Ok(total)
}

/// Extract only format handled in-process. External archive helpers are intentionally disabled:
/// a PATH or mutable embedded binary is not authenticated release tooling.
pub(crate) fn extract_with_helpers(archive: &Path, dest: &Path) -> Result<(), InstallError> {
    let ext = archive
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "zip" {
        return extract_zip(archive, dest);
    }

    Err(InstallError::Other(format!(
        "archive type .{ext} for {} requires an external helper, which is disabled until an authenticated helper manifest is shipped",
        archive.display()
    )))
}

/// 7z creation needs a release-authenticated helper and is unavailable in this build.
pub(crate) fn build_7z(_src_dir: &Path, _out_7z: &Path) -> Result<(), InstallError> {
    Err(InstallError::Other(
        "7z creation is disabled until an authenticated helper manifest is shipped".into(),
    ))
}

/// SFX creation needs release-authenticated helper and stub binaries and is unavailable here.
pub(crate) fn build_sfx(_src_dir: &Path, _out_exe: &Path) -> Result<(), InstallError> {
    Err(InstallError::Other(
        "SFX creation is disabled until authenticated helper and stub manifests are shipped".into(),
    ))
}

/// Build a portable zip of the prepared tree.
pub(crate) fn build_zip(src_dir: &Path, out_zip: &Path) -> Result<(), InstallError> {
    let stage = create_output_stage(out_zip)?;
    let staged_output = stage.join("package.zip");
    let file = copy::create_new_output(&staged_output)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    add_dir_to_zip(&mut zip, src_dir, src_dir, options)?;
    zip.finish()
        .map_err(|e| InstallError::Other(format!("zip finish: {e}")))?;
    finalize_new_output(&staged_output, out_zip, &stage)
}

fn add_dir_to_zip<W: Write + io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    base: &Path,
    dir: &Path,
    options: zip::write::SimpleFileOptions,
) -> Result<(), InstallError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            let dir_name = if name.ends_with('/') {
                name
            } else {
                format!("{name}/")
            };
            zip.add_directory(dir_name, options)
                .map_err(|e| InstallError::Other(format!("zip dir: {e}")))?;
            add_dir_to_zip(zip, base, &path, options)?;
        } else {
            zip.start_file(name, options)
                .map_err(|e| InstallError::Other(format!("zip file: {e}")))?;
            let mut f = File::open(&path)?;
            copy(&mut f, zip)?;
        }
    }
    Ok(())
}

/// Copy prepared tree to export destination.
pub(crate) fn export_workspace(prepared: &Path, export_dir: &Path) -> Result<(), InstallError> {
    copy::create_new_directory(export_dir)?;
    crate::copy::copy_dir_all(prepared, export_dir)?;
    Ok(())
}

fn create_output_stage(output: &Path) -> Result<PathBuf, InstallError> {
    copy::validate_new_output_path(output)?;
    let parent = output.parent().ok_or_else(|| {
        InstallError::Other(format!("output has no parent: {}", output.display()))
    })?;
    // Reserve output policy first; staging stays in the same parent so hard_link finalization is
    // atomic with respect to an existing destination and never overwrites it.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let stage = parent.join(format!(".dfoundry-output-{}-{nonce}", std::process::id()));
    copy::create_new_directory(&stage)?;
    Ok(stage)
}

fn finalize_new_output(staged: &Path, output: &Path, stage: &Path) -> Result<(), InstallError> {
    // `hard_link` fails if output exists, unlike rename on Unix. The staging directory was newly
    // created by this run, so removal below cannot target caller-owned data.
    let result = fs::hard_link(staged, output).map_err(|error| {
        InstallError::Other(format!("publish new output {}: {error}", output.display()))
    });
    let _ = fs::remove_dir_all(stage);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn uniq() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("dfoundry-arch-{n}"))
    }

    #[test]
    fn zip_roundtrip() {
        let root = uniq();
        let src = root.join("src");
        fs::create_dir_all(src.join("Display.Driver")).unwrap();
        fs::write(src.join("setup.exe"), b"MZ-fake").unwrap();
        fs::write(src.join("Display.Driver").join("a.txt"), b"hello").unwrap();
        let zip_path = root.join("out.zip");
        build_zip(&src, &zip_path).unwrap();
        assert!(zip_path.is_file());
        let dest = root.join("extracted");
        extract_zip(&zip_path, &dest).unwrap();
        assert!(dest.join("setup.exe").is_file() || dest.join("Display.Driver").is_dir());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn zip_traversal_is_rejected_before_destination_creation() {
        let root = uniq();
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("traversal.zip");
        let file = File::create(&archive).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("../outside.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"attacker bytes").unwrap();
        zip.finish().unwrap();

        let dest = root.join("destination");
        let error = extract_zip(&archive, &dest).unwrap_err();
        assert!(error.to_string().contains("unsafe path"));
        assert!(!dest.exists());
        assert!(!root.join("outside.txt").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn zip_case_collisions_are_rejected_before_destination_creation() {
        let root = uniq();
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("case-collision.zip");
        let file = File::create(&archive).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for name in ["Display.Driver/setup.exe", "display.driver/SETUP.EXE"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"MZ").unwrap();
        }
        zip.finish().unwrap();

        let dest = root.join("destination");
        let error = extract_zip(&archive, &dest).unwrap_err();
        assert!(error.to_string().contains("case-colliding"));
        assert!(!dest.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn zip_expansion_limits_reject_entry_count_entry_size_and_total_size() {
        assert!(check_zip_expansion_limits(MAX_ZIP_ENTRIES + 1, 0, 0).is_err());
        assert!(check_zip_expansion_limits(1, MAX_ZIP_ENTRY_BYTES + 1, 0).is_err());
        assert!(check_zip_expansion_limits(1, 1, MAX_ZIP_EXPANDED_BYTES).is_err());
        assert_eq!(check_zip_expansion_limits(1, 1, 0).unwrap(), 1);
    }

    #[test]
    fn export_refuses_to_replace_user_directory() {
        let root = uniq();
        let prepared = root.join("prepared");
        fs::create_dir_all(&prepared).unwrap();
        fs::write(prepared.join("setup.exe"), b"MZ").unwrap();
        let export = root.join("export");
        fs::create_dir_all(&export).unwrap();
        fs::write(export.join("user-file"), b"keep").unwrap();
        assert!(export_workspace(&prepared, &export).is_err());
        assert_eq!(fs::read(export.join("user-file")).unwrap(), b"keep");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn archive_refuses_to_replace_existing_file() {
        let root = uniq();
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("setup.exe"), b"MZ").unwrap();
        let output = root.join("archive.zip");
        fs::write(&output, b"user archive").unwrap();
        assert!(build_zip(&source, &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"user archive");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn non_zip_archive_never_executes_path_or_embedded_helper() {
        let root = uniq();
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("driver.exe");
        fs::write(&archive, b"MZ-untrusted").unwrap();
        let error = extract_with_helpers(&archive, &root.join("extract")).unwrap_err();
        assert!(
            error.to_string().contains("external helper") && error.to_string().contains("disabled")
        );
        assert!(!root.join("extract").exists());
        let _ = fs::remove_dir_all(root);
    }
}
