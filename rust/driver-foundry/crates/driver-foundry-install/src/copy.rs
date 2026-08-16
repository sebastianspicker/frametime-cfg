//! Prepared-tree filtering and embedded-helper materialization.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use crate::catalog::PackageCatalog;
use crate::copy_source::{copy_source_dir_all, copy_source_file, SourceRoot};
use crate::InstallError;

const RUN_MARKER: &str = ".driver-foundry-run-owned";

/// Create an isolated, new workspace. Existing paths are never reused because later stages
/// create generated trees beneath this directory.
pub(crate) fn create_new_run_workspace(path: &Path) -> Result<(), InstallError> {
    create_new_directory(path)?;
    let marker = path.join(RUN_MARKER);
    let marker_result = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .and_then(|mut file| std::io::Write::write_all(&mut file, b"driver-foundry run-owned\n"));
    if let Err(error) = marker_result {
        let _ = fs::remove_dir(path);
        return Err(InstallError::Other(format!(
            "could not mark new run workspace {}: {error}",
            path.display()
        )));
    }
    Ok(())
}

/// Create a new directory without traversing symlink/reparse ancestors or replacing an existing
/// path. Parents must already exist, so a misspelled output cannot create an unexpected tree.
pub(crate) fn create_new_directory(path: &Path) -> Result<(), InstallError> {
    validate_new_path(path)?;
    fs::create_dir(path).map_err(|error| {
        InstallError::Other(format!("create new directory {}: {error}", path.display()))
    })
}

pub(crate) fn validate_new_output_path(path: &Path) -> Result<(), InstallError> {
    validate_new_path(path)
}

/// Open a fresh output file. This is the file analogue of [`create_new_directory`].
pub(crate) fn create_new_output(path: &Path) -> Result<fs::File, InstallError> {
    validate_new_path(path)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            InstallError::Other(format!("create new output {}: {error}", path.display()))
        })
}

pub(crate) fn paths_overlap(left: &Path, right: &Path) -> bool {
    let left = absolute_path(left);
    let right = absolute_path(right);
    left.starts_with(&right) || right.starts_with(&left)
}

pub(crate) fn path_is_within(child: &Path, parent: &Path) -> bool {
    absolute_path(child).starts_with(absolute_path(parent))
}

fn validate_new_path(path: &Path) -> Result<(), InstallError> {
    if path.as_os_str().is_empty() || is_windows_device_or_unc(path) {
        return Err(InstallError::Other(format!(
            "unsafe output path: {}",
            path.display()
        )));
    }
    let absolute = absolute_path(path);
    if absolute.parent().is_none()
        || absolute
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(InstallError::Other(format!(
            "output path must not be a filesystem root or contain '..': {}",
            path.display()
        )));
    }
    if path_exists_or_link(path)? {
        return Err(InstallError::Other(format!(
            "refusing to replace existing output path: {}",
            path.display()
        )));
    }
    let parent = absolute
        .parent()
        .ok_or_else(|| InstallError::Other("output path has no parent".into()))?;
    if !parent.is_dir() {
        return Err(InstallError::Other(format!(
            "output parent must already exist: {}",
            parent.display()
        )));
    }
    reject_reparse_ancestors(parent)
}

fn path_exists_or_link(path: &Path) -> Result<bool, InstallError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(InstallError::Other(format!(
            "inspect {}: {error}",
            path.display()
        ))),
    }
}

fn reject_reparse_ancestors(path: &Path) -> Result<(), InstallError> {
    let absolute = absolute_path(path);
    let system_temp = absolute_path(&std::env::temp_dir());
    let mut current = PathBuf::new();
    for component in absolute.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            // macOS commonly exposes its secure temp root through /var -> /private/var.
            // Treat only symlinks that are ancestors of the OS-selected temp directory as a
            // platform alias; a caller-created link (even under /tmp) remains rejected.
            Ok(metadata) if is_reparse_point(&metadata) && !system_temp.starts_with(&current) => {
                return Err(InstallError::Other(format!(
                    "output path traverses symlink/reparse point: {}",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(InstallError::Other(format!(
                    "inspect output ancestor {}: {error}",
                    current.display()
                )));
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    // FILE_ATTRIBUTE_REPARSE_POINT: reject junctions as well as symbolic links.
    metadata.file_attributes() & 0x0400 != 0
}

#[cfg(not(windows))]
pub(crate) fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn absolute_path(path: &Path) -> PathBuf {
    let raw = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in raw.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                let _ = normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn is_windows_device_or_unc(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with(r"\\?\")
        || text.starts_with(r"\\.\")
        || text.starts_with(r"\\")
        || text.starts_with("//")
}

/// Materialize shipped embedded helpers into a work directory.
#[cfg(test)]
pub(crate) fn materialize_embedded(dest: &Path) -> Result<Vec<PathBuf>, InstallError> {
    let src = driver_foundry_common::resolve_data_root().join("embedded");
    if !src.is_dir() {
        return Err(InstallError::Other(format!(
            "embedded helpers not found: {}",
            src.display()
        )));
    }
    create_new_directory(dest)?;
    let mut copied = Vec::new();
    copy_embedded_tree(&src, dest, &mut copied)?;
    Ok(copied)
}

#[cfg(test)]
fn copy_embedded_tree(src: &Path, dst: &Path, copied: &mut Vec<PathBuf>) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_embedded_tree(&entry.path(), &to, copied)?;
        } else {
            fs::copy(entry.path(), &to)?;
            copied.push(to);
        }
    }
    Ok(())
}

/// Copy kept component dirs + setup.exe/NVI2; strip others.
pub(crate) fn prepare_copy_strip(
    package_root: &Path,
    prepared: &Path,
    catalog: &PackageCatalog,
    resolved: &[String],
) -> Result<(Vec<String>, Vec<String>), InstallError> {
    let source_root = SourceRoot::open(package_root)?;
    create_new_directory(prepared)?;

    let resolved_set: BTreeSet<String> = resolved.iter().cloned().collect();
    let mut kept = Vec::new();
    let mut stripped = Vec::new();

    for name in ["setup.exe", "NVI2"] {
        let src = package_root.join(name);
        let dst = prepared.join(name);
        match source_root.metadata_for(&src)? {
            Some(metadata) if metadata.is_file() => {
                copy_source_file(&source_root, &src, &dst, &metadata)?;
            }
            Some(metadata) if metadata.is_dir() => {
                copy_source_dir_all(&source_root, &src, &dst, &metadata, true)?;
            }
            Some(_) => {
                return Err(InstallError::Other(format!(
                    "package entry must be a regular file or directory: {}",
                    src.display()
                )));
            }
            None => {}
        }
    }

    for (id, definition) in &catalog.packages {
        let src = package_root.join(id);
        let metadata = source_root.metadata_for(&src)?;
        let Some(metadata) = metadata else {
            if resolved_set.contains(id) && definition.required {
                return Err(InstallError::Other(format!(
                    "required component missing from package: {id}"
                )));
            }
            if !resolved_set.contains(id) {
                stripped.push(id.clone());
            }
            continue;
        };
        if resolved_set.contains(id) {
            let dst = prepared.join(id);
            if metadata.is_dir() {
                copy_source_dir_all(&source_root, &src, &dst, &metadata, true)?;
            } else if metadata.is_file() {
                copy_source_file(&source_root, &src, &dst, &metadata)?;
            } else {
                return Err(InstallError::Other(format!(
                    "package component must be a regular file or directory: {}",
                    src.display()
                )));
            }
            kept.push(id.clone());
        } else {
            stripped.push(id.clone());
        }
    }

    kept.sort();
    stripped.sort();
    Ok((kept, stripped))
}

/// Identity-stable source tree. This is deliberately path-checked on every entry because the
/// local package root can be caller-controlled even in an unelevated dry-run.
/// Copy a run-owned prepared tree for export. The same strict source checks are retained so an
/// unexpected reparse point cannot turn an unelevated export into an external-file copy.
pub(crate) fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), InstallError> {
    let root = SourceRoot::open(src)?;
    let expected = root.metadata.clone();
    copy_source_dir_all(&root, src, dst, &expected, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::PackageCatalog;
    use crate::copy_source::SourceRoot;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn missing_required_component_fails_instead_of_fabricating_it() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("dfoundry-copy-required-{nonce}"));
        let package = root.join("package");
        let prepared = root.join("prepared");
        fs::create_dir_all(&package).expect("package root");
        fs::write(package.join("setup.exe"), b"MZ").expect("setup fixture");
        let catalog = PackageCatalog::load_from_file(&driver_foundry_common::catalog_path(
            &driver_foundry_common::resolve_data_root(),
        ))
        .expect("catalog");
        let error = prepare_copy_strip(
            &package,
            &prepared,
            &catalog,
            &["Display.Driver".to_owned()],
        )
        .expect_err("missing required component must fail");
        assert!(error.to_string().contains("required component missing"));
        assert!(!prepared.join("Display.Driver").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn new_outputs_reject_existing_roots_and_reparse_ancestors() {
        let root = std::env::temp_dir().join(format!("dfoundry-path-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let existing = root.join("existing");
        fs::create_dir(&existing).unwrap();
        assert!(create_new_directory(&existing).is_err());
        assert!(create_new_directory(Path::new("/")).is_err());
        assert!(create_new_directory(Path::new(r"\\?\C:\\Windows")).is_err());
        #[cfg(unix)]
        {
            let link = root.join("link");
            std::os::unix::fs::symlink("/tmp", &link).unwrap();
            assert!(create_new_directory(&link.join("escaped")).is_err());
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn overlap_detection_catches_ancestor_and_descendant_paths() {
        let root = std::env::temp_dir().join("dfoundry-overlap");
        assert!(paths_overlap(&root, &root.join("prepared")));
        assert!(paths_overlap(
            &root,
            &root.join("one").join("..").join("prepared")
        ));
        assert!(!paths_overlap(&root.join("one"), &root.join("two")));
    }

    #[cfg(unix)]
    #[test]
    fn package_root_symlink_is_rejected_before_prepared_output_creation() {
        let root = std::env::temp_dir().join(format!(
            "dfoundry-copy-root-link-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let actual = root.join("actual-package");
        fs::create_dir_all(&actual).unwrap();
        fs::write(actual.join("setup.exe"), b"MZ").unwrap();
        let package_link = root.join("package-link");
        std::os::unix::fs::symlink(&actual, &package_link).unwrap();
        let prepared = root.join("prepared");
        let catalog = PackageCatalog::load_from_file(&driver_foundry_common::catalog_path(
            &driver_foundry_common::resolve_data_root(),
        ))
        .unwrap();

        let error = prepare_copy_strip(
            &package_link,
            &prepared,
            &catalog,
            &["Display.Driver".to_owned()],
        )
        .unwrap_err();
        assert!(error.to_string().contains("symlink/reparse"));
        assert!(!prepared.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn component_and_nested_symlinks_are_not_followed() {
        let root = std::env::temp_dir().join(format!(
            "dfoundry-copy-entry-link-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let package = root.join("package");
        let outside = root.join("outside");
        fs::create_dir_all(&package).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(package.join("setup.exe"), b"MZ").unwrap();
        fs::write(outside.join("secret.sys"), b"outside bytes").unwrap();
        std::os::unix::fs::symlink(&outside, package.join("Display.Driver")).unwrap();
        let prepared = root.join("prepared");
        let catalog = PackageCatalog::load_from_file(&driver_foundry_common::catalog_path(
            &driver_foundry_common::resolve_data_root(),
        ))
        .unwrap();

        let error = prepare_copy_strip(
            &package,
            &prepared,
            &catalog,
            &["Display.Driver".to_owned()],
        )
        .unwrap_err();
        assert!(error.to_string().contains("symlink/reparse"));
        assert!(!prepared.join("Display.Driver").join("secret.sys").exists());

        fs::remove_file(package.join("Display.Driver")).unwrap();
        fs::create_dir(package.join("Display.Driver")).unwrap();
        std::os::unix::fs::symlink(
            outside.join("secret.sys"),
            package.join("Display.Driver").join("escaped.sys"),
        )
        .unwrap();
        let second_prepared = root.join("prepared-second");
        let error = prepare_copy_strip(
            &package,
            &second_prepared,
            &catalog,
            &["Display.Driver".to_owned()],
        )
        .unwrap_err();
        assert!(error.to_string().contains("symlink/reparse"));
        assert!(!second_prepared
            .join("Display.Driver")
            .join("escaped.sys")
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn source_canonical_containment_rejects_outside_path() {
        let root = std::env::temp_dir().join(format!(
            "dfoundry-copy-containment-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let package = root.join("package");
        let outside = root.join("outside.bin");
        fs::create_dir_all(&package).unwrap();
        fs::write(&outside, b"not a package entry").unwrap();
        let source = SourceRoot::open(&package).unwrap();
        let error = source.metadata_for(&outside).unwrap_err();
        assert!(error.to_string().contains("escapes canonical source root"));
        let _ = fs::remove_dir_all(root);
    }
}
