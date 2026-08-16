//! Identity-stable copying of caller-controlled package trees.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::copy::{create_new_directory, create_new_output, is_reparse_point};
use crate::InstallError;

/// A package root whose identity and containment are checked before each copy.
pub(super) struct SourceRoot {
    canonical: PathBuf,
    pub(super) metadata: fs::Metadata,
}

impl SourceRoot {
    pub(super) fn open(path: &Path) -> Result<Self, InstallError> {
        let metadata =
            source_metadata(path)?.ok_or_else(|| InstallError::PackageMissing(path.into()))?;
        if !metadata.is_dir() {
            return Err(InstallError::Other(format!(
                "package root must be a real directory: {}",
                path.display()
            )));
        }
        let canonical = fs::canonicalize(path).map_err(|error| {
            InstallError::Other(format!(
                "canonicalize package root {}: {error}",
                path.display()
            ))
        })?;
        Ok(Self {
            canonical,
            metadata,
        })
    }

    pub(super) fn metadata_for(&self, path: &Path) -> Result<Option<fs::Metadata>, InstallError> {
        let metadata = source_metadata(path)?;
        let Some(metadata) = metadata else {
            return Ok(None);
        };
        let canonical = fs::canonicalize(path).map_err(|error| {
            InstallError::Other(format!(
                "canonicalize package entry {}: {error}",
                path.display()
            ))
        })?;
        if !canonical.starts_with(&self.canonical) {
            return Err(InstallError::Other(format!(
                "package entry escapes canonical source root: {}",
                path.display()
            )));
        }
        Ok(Some(metadata))
    }

    fn verify_root(&self) -> Result<(), InstallError> {
        let current = source_metadata(&self.canonical)?
            .ok_or_else(|| InstallError::Other("package root disappeared during copy".into()))?;
        if !current.is_dir() || !same_file_identity(&self.metadata, &current) {
            return Err(InstallError::Other(
                "package root changed during copy; refusing source TOCTOU".into(),
            ));
        }
        Ok(())
    }
}

fn source_metadata(path: &Path) -> Result<Option<fs::Metadata>, InstallError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_reparse_point(&metadata) => Err(InstallError::Other(format!(
            "package source contains a symlink/reparse point: {}",
            path.display()
        ))),
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(InstallError::Other(format!(
            "inspect package source {}: {error}",
            path.display()
        ))),
    }
}

fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        left.dev() == right.dev() && left.ino() == right.ino()
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        left.volume_serial_number() == right.volume_serial_number()
            && left.file_index() == right.file_index()
    }
    #[cfg(not(any(unix, windows)))]
    {
        left.len() == right.len() && left.modified().ok() == right.modified().ok()
    }
}

pub(super) fn copy_source_file(
    root: &SourceRoot,
    src: &Path,
    dst: &Path,
    expected: &fs::Metadata,
) -> Result<(), InstallError> {
    root.verify_root()?;
    let source = File::open(src)?;
    let opened_metadata = source.metadata()?;
    if !opened_metadata.is_file() || !same_file_identity(expected, &opened_metadata) {
        return Err(InstallError::Other(format!(
            "package source changed during file open; refusing TOCTOU: {}",
            src.display()
        )));
    }
    let mut destination = create_new_output(dst)?;
    io::copy(&mut source.take(u64::MAX), &mut destination)?;
    root.verify_root()
}

/// Recursively copy a directory without preserving metadata or following links.
pub(super) fn copy_source_dir_all(
    root: &SourceRoot,
    src: &Path,
    dst: &Path,
    expected: &fs::Metadata,
    create_destination: bool,
) -> Result<(), InstallError> {
    root.verify_root()?;
    let current = root.metadata_for(src)?.ok_or_else(|| {
        InstallError::Other(format!(
            "package directory disappeared during copy: {}",
            src.display()
        ))
    })?;
    if !current.is_dir() || !same_file_identity(expected, &current) {
        return Err(InstallError::Other(format!(
            "package directory changed during copy; refusing TOCTOU: {}",
            src.display()
        )));
    }
    if create_destination {
        create_new_directory(dst)?;
    } else {
        let destination = fs::symlink_metadata(dst).map_err(|error| {
            InstallError::Other(format!(
                "inspect export destination {}: {error}",
                dst.display()
            ))
        })?;
        if is_reparse_point(&destination) || !destination.is_dir() {
            return Err(InstallError::Other(format!(
                "export destination must be a real directory: {}",
                dst.display()
            )));
        }
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let metadata = root.metadata_for(&from)?.ok_or_else(|| {
            InstallError::Other(format!(
                "package entry disappeared during copy: {}",
                from.display()
            ))
        })?;
        if metadata.is_dir() {
            copy_source_dir_all(root, &from, &to, &metadata, true)?;
        } else if metadata.is_file() {
            copy_source_file(root, &from, &to, &metadata)?;
        } else {
            return Err(InstallError::Other(format!(
                "package source contains unsupported entry: {}",
                from.display()
            )));
        }
    }
    root.verify_root()
}
