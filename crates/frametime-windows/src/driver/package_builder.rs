//! Bounded NVIDIA SFX extraction and byte-preserving package assembly.
//!
//! Archive metadata is validated before the destination is created.  The
//! extractor then independently enforces the same limits while streaming each
//! entry to a create-new file.  Package assembly copies vendor bytes without
//! modification and generates only Frametime-owned metadata and `setup.cfg`.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

use frametime_domain::driver::{NvidiaComponentSelection, SourceComponentClassification};
use serde::{Deserialize, Serialize};
use sevenz_rust2::{ArchiveEntry, ArchiveReader, Error as SevenZError, Password};
use sha2::{Digest, Sha256};

#[cfg(any(test, windows))]
mod verification;
#[cfg(any(test, windows))]
pub(crate) use verification::{prepared_nvidia_package_digest, verify_prepared_nvidia_package};

const SEVEN_Z_SIGNATURE: [u8; 6] = [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c];
const MAX_SFX_PREFIX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 20_000;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_DECOMPRESSION_RATIO: u64 = 500;
const MAX_RELATIVE_PATH_UNITS: usize = 240;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const HASH_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NvidiaPackageFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedNvidiaPackageManifest {
    pub schema: String,
    pub selected_components: Vec<String>,
    pub required_unclassified: Vec<String>,
    pub files: Vec<NvidiaPackageFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryMetadata {
    name: String,
    is_directory: bool,
    has_reparse_attribute: bool,
    size: u64,
    compressed_size: u64,
}

impl From<&ArchiveEntry> for EntryMetadata {
    fn from(entry: &ArchiveEntry) -> Self {
        Self {
            name: entry.name.clone(),
            is_directory: entry.is_directory,
            has_reparse_attribute: entry.has_windows_attributes
                && entry.windows_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0,
            size: entry.size,
            compressed_size: entry.compressed_size,
        }
    }
}

/// Reader view whose logical offset zero begins at an embedded 7z signature.
struct OffsetReader<R> {
    inner: R,
    offset: u64,
}

impl<R: Read> Read for OffsetReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buffer)
    }
}

impl<R: Seek> Seek for OffsetReader<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let absolute = match position {
            SeekFrom::Start(value) => self.offset.checked_add(value).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "archive seek overflow")
            })?,
            SeekFrom::Current(value) => {
                return self
                    .inner
                    .seek(SeekFrom::Current(value))
                    .and_then(|position| {
                        position.checked_sub(self.offset).ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidData, "archive seek before prefix")
                        })
                    });
            }
            SeekFrom::End(value) => {
                return self.inner.seek(SeekFrom::End(value)).and_then(|position| {
                    position.checked_sub(self.offset).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "archive end before prefix")
                    })
                });
            }
        };
        self.inner.seek(SeekFrom::Start(absolute))?;
        Ok(absolute - self.offset)
    }
}

pub fn extract_nvidia_sfx(installer: &Path, destination: &Path) -> Result<(), String> {
    require_unused_output(destination)?;
    let file = File::open(installer)
        .map_err(|error| format!("open NVIDIA installer {}: {error}", installer.display()))?;
    let offset = locate_7z_signature(file.try_clone().map_err(|e| e.to_string())?)?;
    let mut reader = ArchiveReader::new(
        OffsetReader {
            inner: file,
            offset,
        },
        Password::empty(),
    )
    .map_err(|error| format!("parse NVIDIA 7z/SFX payload: {error}"))?;
    let metadata = reader
        .archive()
        .files
        .iter()
        .map(EntryMetadata::from)
        .collect::<Vec<_>>();
    validate_entries(&metadata)?;

    fs::create_dir(destination)
        .map_err(|error| format!("create extraction destination: {error}"))?;
    let result = extract_entries(&mut reader, destination);
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

fn extract_entries<R: Read + Seek>(
    reader: &mut ArchiveReader<R>,
    destination: &Path,
) -> Result<(), String> {
    let mut extracted = 0_u64;
    reader
        .for_each_entries(|entry, source| {
            let relative = checked_relative_path(&entry.name).map_err(archive_error)?;
            let target = destination.join(relative);
            if entry.is_directory {
                create_directory_chain(destination, &target).map_err(archive_error)?;
                return Ok(true);
            }
            let parent = target
                .parent()
                .ok_or_else(|| archive_error("archive entry has no parent".into()))?;
            create_directory_chain(destination, parent).map_err(archive_error)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(SevenZError::from)?;
            let copied = io::copy(&mut source.take(MAX_FILE_BYTES + 1), &mut output)
                .map_err(SevenZError::from)?;
            extracted = extracted
                .checked_add(copied)
                .ok_or_else(|| archive_error("expanded size overflow".into()))?;
            if copied != entry.size || copied > MAX_FILE_BYTES || extracted > MAX_EXPANDED_BYTES {
                return Err(archive_error(
                    "archive entry exceeded extraction limits".into(),
                ));
            }
            output.sync_all().map_err(SevenZError::from)?;
            Ok(true)
        })
        .map_err(|error| format!("extract NVIDIA 7z/SFX payload: {error}"))
}

fn archive_error(reason: String) -> SevenZError {
    io::Error::new(io::ErrorKind::InvalidData, reason).into()
}

fn locate_7z_signature(file: File) -> Result<u64, String> {
    let mut reader = BufReader::new(file);
    let mut matched = 0_usize;
    let mut offset = 0_u64;
    let mut buffer = [0_u8; 8192];
    while offset < MAX_SFX_PREFIX_BYTES {
        let buffer_length = u64::try_from(buffer.len())
            .map_err(|_| "SFX buffer length does not fit the archive limit")?;
        let remaining = usize::try_from((MAX_SFX_PREFIX_BYTES - offset).min(buffer_length))
            .map_err(|_| "SFX prefix limit does not fit memory size")?;
        let read = reader
            .read(&mut buffer[..remaining])
            .map_err(|error| format!("scan NVIDIA SFX: {error}"))?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            if *byte == SEVEN_Z_SIGNATURE[matched] {
                matched += 1;
                if matched == SEVEN_Z_SIGNATURE.len() {
                    let signature_length = u64::try_from(SEVEN_Z_SIGNATURE.len())
                        .map_err(|_| "7z signature length does not fit archive offset")?;
                    return Ok(offset + 1 - signature_length);
                }
            } else {
                matched = usize::from(*byte == SEVEN_Z_SIGNATURE[0]);
            }
            offset += 1;
        }
    }
    Err("NVIDIA installer has no bounded 7z/SFX signature".into())
}

fn validate_entries(entries: &[EntryMetadata]) -> Result<(), String> {
    if entries.is_empty() || entries.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!(
            "archive entry count must be 1..={MAX_ARCHIVE_ENTRIES}"
        ));
    }
    let mut names = BTreeSet::new();
    let mut total = 0_u64;
    for entry in entries {
        let relative = checked_relative_path(&entry.name)?;
        let folded = relative
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if !names.insert(folded) {
            return Err(format!(
                "archive contains a duplicate or case-colliding entry: {}",
                entry.name
            ));
        }
        if entry.has_reparse_attribute {
            return Err(format!(
                "archive contains a reparse-point entry: {}",
                entry.name
            ));
        }
        if !entry.is_directory {
            if entry.size > MAX_FILE_BYTES {
                return Err(format!("archive entry is too large: {}", entry.name));
            }
            total = total
                .checked_add(entry.size)
                .ok_or("archive expanded size overflow")?;
            if total > MAX_EXPANDED_BYTES {
                return Err("archive expanded size exceeds the configured limit".into());
            }
            if (entry.size > 0 && entry.compressed_size == 0)
                || (entry.compressed_size > 0
                    && entry.size / entry.compressed_size > MAX_DECOMPRESSION_RATIO)
            {
                return Err(format!("archive entry ratio is too large: {}", entry.name));
            }
        }
    }
    Ok(())
}

fn checked_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.encode_utf16().count() > MAX_RELATIVE_PATH_UNITS {
        return Err("archive entry path is empty or too long".into());
    }
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) if !part.is_empty() => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("archive entry escapes destination: {value}"));
            }
            _ => return Err(format!("archive entry has an invalid path: {value}")),
        }
    }
    if relative.as_os_str().is_empty() {
        Err(format!("archive entry has no usable path: {value}"))
    } else {
        Ok(relative)
    }
}

fn create_directory_chain(root: &Path, path: &Path) -> Result<(), String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "extraction directory escaped destination")?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "extraction path contains a non-directory or reparse-like entry: {}",
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|error| {
                    format!("create extraction directory {}: {error}", current.display())
                })?;
            }
            Err(error) => return Err(format!("inspect extraction directory: {error}")),
        }
    }
    Ok(())
}

pub fn build_nvidia_package(
    extracted_source: &Path,
    output: &Path,
    selection: &NvidiaComponentSelection,
) -> Result<PreparedNvidiaPackageManifest, String> {
    require_unused_output(output)?;
    if !extracted_source.is_dir() {
        return Err("extracted NVIDIA source is not a directory".into());
    }
    fs::create_dir(output).map_err(|error| format!("create package output: {error}"))?;
    let result = assemble_package(extracted_source, output, selection);
    if result.is_err() {
        let _ = fs::remove_dir_all(output);
    }
    result
}

fn assemble_package(
    source: &Path,
    output: &Path,
    selection: &NvidiaComponentSelection,
) -> Result<PreparedNvidiaPackageManifest, String> {
    let selected = selection.selected.clone();
    let unclassified = selection
        .required_unclassified
        .iter()
        .map(|component| component.directory.clone())
        .collect::<BTreeSet<_>>();
    let known_source = selection
        .source_components
        .iter()
        .filter_map(|component| match component {
            SourceComponentClassification::Known(name) => Some(name.clone()),
            SourceComponentClassification::RequiredUnclassified(_) => None,
        })
        .collect::<BTreeSet<_>>();
    let mut roots = fs::read_dir(source)
        .map_err(|error| format!("read extracted source: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read extracted source entry: {error}"))?;
    roots.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
    for entry in roots {
        let name = entry.file_name().to_string_lossy().into_owned();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("inspect source entry {name}: {error}"))?;
        if file_type.is_symlink() {
            return Err(format!(
                "source contains a symlink or reparse point: {name}"
            ));
        }
        let include = !file_type.is_dir()
            || selected.contains(&name)
            || unclassified.contains(&name)
            || !known_source.contains(&name);
        if include {
            copy_vendor_tree(&entry.path(), &output.join(&name))?;
        }
    }

    let setup = render_setup_config(selection);
    write_new(output.join("setup.cfg"), setup.as_bytes())?;
    let mut manifest = PreparedNvidiaPackageManifest {
        schema: "frametime.nvidia-package/v1".into(),
        selected_components: selection.selected.iter().cloned().collect(),
        required_unclassified: selection
            .required_unclassified
            .iter()
            .map(|component| component.directory.clone())
            .collect(),
        files: collect_manifest_files(output, &["frametime-package.json"])?,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("serialize package manifest: {error}"))?;
    let manifest_path = output.join("frametime-package.json");
    write_new(manifest_path.clone(), &bytes)?;
    let manifest_file =
        inspect_and_hash_package_file(&manifest_path, "frametime-package.json".into())?;
    let insertion = match manifest
        .files
        .binary_search_by(|file| file.path.cmp(&manifest_file.path))
    {
        Ok(_) => {
            return Err(
                "generated package manifest unexpectedly existed in its payload inventory".into(),
            );
        }
        Err(index) => index,
    };
    manifest.files.insert(insertion, manifest_file);
    Ok(manifest)
}

fn render_setup_config(selection: &NvidiaComponentSelection) -> String {
    let components = selection
        .selected
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "<setup version=\"1\">\r\n  <install name=\"Frametime NVIDIA package\" components=\"{components}\" />\r\n</setup>\r\n"
    )
}

pub(super) fn copy_vendor_tree(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("inspect vendor source {}: {error}", source.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("vendor source is a symlink: {}", source.display()));
    }
    if metadata.is_dir() {
        fs::create_dir(destination).map_err(|error| {
            format!("create vendor directory {}: {error}", destination.display())
        })?;
        let mut entries = fs::read_dir(source)
            .map_err(|error| format!("read vendor directory {}: {error}", source.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("read vendor entry: {error}"))?;
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
        for entry in entries {
            copy_vendor_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        let mut input = File::open(source)
            .map_err(|error| format!("open vendor file {}: {error}", source.display()))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|error| format!("create vendor file {}: {error}", destination.display()))?;
        let copied = io::copy(&mut input, &mut output)
            .map_err(|error| format!("copy vendor file {}: {error}", source.display()))?;
        if copied != metadata.len() {
            return Err(format!("vendor copy length changed: {}", source.display()));
        }
    } else {
        return Err(format!(
            "unsupported vendor source type: {}",
            source.display()
        ));
    }
    Ok(())
}

pub(super) fn collect_manifest_files(
    root: &Path,
    excluded: &[&str],
) -> Result<Vec<NvidiaPackageFile>, String> {
    fn visit(
        root: &Path,
        directory: &Path,
        excluded: &[&str],
        files: &mut BTreeMap<String, NvidiaPackageFile>,
    ) -> Result<(), String> {
        let mut entries = fs::read_dir(directory)
            .map_err(|error| format!("read package directory: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("read package entry: {error}"))?;
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "package manifest path escaped output")?
                .to_string_lossy()
                .replace('\\', "/");
            if excluded.contains(&relative.as_str()) {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("inspect package entry: {error}"))?;
            if metadata.is_dir() {
                visit(root, &path, excluded, files)?;
            } else if metadata.is_file() {
                let file = hash_package_file(&path, relative.clone(), metadata.len())?;
                files.insert(relative, file);
            } else {
                return Err(format!("unsupported package entry: {relative}"));
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(root, root, excluded, &mut files)?;
    Ok(files.into_values().collect())
}

fn inspect_and_hash_package_file(
    path: &Path,
    relative: String,
) -> Result<NvidiaPackageFile, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("inspect package entry {relative}: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("unsupported package entry: {relative}"));
    }
    hash_package_file(path, relative, metadata.len())
}

fn hash_package_file(
    path: &Path,
    relative: String,
    inspected_len: u64,
) -> Result<NvidiaPackageFile, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("open package file {relative} for hashing: {error}"))?;
    let sha256 = hash_reader(&mut file, inspected_len, &relative)?;
    Ok(NvidiaPackageFile {
        path: relative,
        bytes: inspected_len,
        sha256,
    })
}

fn hash_reader(
    reader: &mut impl Read,
    inspected_len: u64,
    relative: &str,
) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let mut counted = 0_u64;
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("hash package file {relative}: {error}"))?;
        if read == 0 {
            break;
        }
        let counted_read = u64::try_from(read)
            .map_err(|_| format!("package file length overflow while hashing: {relative}"))?;
        counted = counted
            .checked_add(counted_read)
            .ok_or_else(|| format!("package file length overflow while hashing: {relative}"))?;
        hasher.update(&buffer[..read]);
    }
    if counted != inspected_len {
        return Err(format!(
            "package file length changed while hashing {relative}: inspected {inspected_len}, read {counted}"
        ));
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn require_unused_output(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.exists() {
        Err(format!("output path must be unused: {}", path.display()))
    } else {
        Ok(())
    }
}

fn write_new(path: PathBuf, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| format!("create generated file {}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("write generated file {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("flush generated file {}: {error}", path.display()))
}
