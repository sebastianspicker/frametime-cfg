#[cfg(windows)]
use frametime_core::runtime::{
    portable_payload_contract_hash, RuntimeCurrent, RUNTIME_PAYLOAD_PATHS, RUNTIME_SCHEMA_VERSION,
};

#[cfg(windows)]
fn selected_generation(current: &RuntimeCurrent) -> Result<&str, String> {
    if current.schema_version != RUNTIME_SCHEMA_VERSION {
        return Err("unsupported runtime selector schema".into());
    }
    let prefix = format!("{RUNTIME_GENERATIONS_DIR}/");
    let Some(generation) = current.relative_path.strip_prefix(&prefix) else {
        return Err("runtime selector is not rooted in runtime-generations".into());
    };
    if current.relative_path != format!("{prefix}{generation}") || !valid_generation(generation) {
        return Err("runtime selector generation is not exact lower-hex".into());
    }
    if !current.manifest_sha256.as_deref().is_some_and(valid_sha256) {
        return Err("runtime selector is missing a valid manifest hash".into());
    }
    Ok(generation)
}

#[cfg(windows)]
fn validate_runtime_contract(
    current: &RuntimeCurrent,
    manifest_hash: &str,
    manifest: &RuntimeManifest,
    payload_hashes: &BTreeMap<String, String>,
) -> Result<String, String> {
    let generation = selected_generation(current)?;
    if current.manifest_sha256.as_deref() != Some(manifest_hash)
        || manifest.schema_version != RUNTIME_SCHEMA_VERSION
        || manifest.generation != generation
    {
        return Err("runtime selector and manifest differ".into());
    }
    let expected = RUNTIME_PAYLOAD_PATHS.iter().copied().collect::<BTreeSet<_>>();
    let declared = manifest.files.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if declared != expected
        || payload_hashes.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected
        || manifest.payload_contract_hash != portable_payload_contract_hash()
        || manifest.executable.path != "frametime.exe"
        || manifest.files.get("frametime.exe") != Some(&manifest.executable.sha256)
        || !valid_sha256(&manifest.executable.sha256)
    {
        return Err("runtime payload contract is invalid".into());
    }
    for (path, declared_hash) in &manifest.files {
        if !valid_sha256(declared_hash) || payload_hashes.get(path) != Some(declared_hash) {
            return Err(format!("runtime payload hash mismatch: {path}"));
        }
    }
    Ok(generation.to_owned())
}

#[cfg(windows)]
fn valid_generation(value: &str) -> bool {
    value.len() == 32
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(windows)]
fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
use frametime_core::{
    runtime::{RuntimeManifest, RUNTIME_GENERATIONS_DIR},
    RuntimeRecord,
};

#[cfg(windows)]
const MAX_RUNTIME_METADATA_BYTES: usize = 1024 * 1024;
#[cfg(windows)]
const MAX_RUNTIME_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct InspectedRuntimeIntegrity {
    generation: String,
    manifest_sha256: String,
    manifest: RuntimeManifest,
    config: VerifiedConfig,
    #[cfg(windows)]
    _nodes: Vec<RetainedRuntimeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeIntegrityDiagnostic {
    pub generation: String,
    pub payload_count: usize,
}

/// A non-cloneable selected-runtime capability. The retained handles prevent a
/// replacement of the selector, generation tree, manifest, or executable
/// between validation and the handoff side effect.
#[derive(Debug)]
pub struct VerifiedSelectedRuntime {
    record: RuntimeRecord,
    executable_path: PathBuf,
    _inspected: InspectedRuntimeIntegrity,
}

impl VerifiedSelectedRuntime {
    #[must_use]
    pub fn record(&self) -> &RuntimeRecord {
        &self.record
    }

    #[must_use]
    pub fn executable_path(&self) -> &Path {
        &self.executable_path
    }

    #[must_use]
    pub fn config(&self) -> &VerifiedConfig {
        &self._inspected.config
    }
}

pub fn inspect_runtime_integrity(work_dir: &Path) -> Result<RuntimeIntegrityDiagnostic, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let inspected = inspect_selected_runtime_integrity(&trusted)?;
    Ok(RuntimeIntegrityDiagnostic {
        generation: inspected.generation.clone(),
        payload_count: inspected.manifest.files.len(),
    })
}

/// Return the exact immutable runtime identity after selector, manifest,
/// payload, DACL, reparse, and current-process identity validation.
pub fn selected_runtime_record(work_dir: &Path) -> Result<RuntimeRecord, String> {
    Ok(retain_selected_runtime(work_dir)?.record)
}

/// Retain the exact selected executable while a native coordinator writes or
/// reads a reboot handoff. The capability is deliberately non-cloneable.
pub fn retain_selected_runtime(work_dir: &Path) -> Result<VerifiedSelectedRuntime, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let inspected = inspect_selected_runtime_integrity(&trusted)?;
    let record = RuntimeRecord {
        generation: inspected.generation.clone(),
        manifest_sha256: inspected.manifest_sha256.clone(),
        payload_contract_hash: inspected.manifest.payload_contract_hash.clone(),
        executable_path: inspected.manifest.executable.path.clone(),
        executable_sha256: inspected.manifest.executable.sha256.clone(),
        unknown: BTreeMap::new(),
    };
    let executable_path = trusted
        .path()
        .join(RUNTIME_GENERATIONS_DIR)
        .join(&record.generation)
        .join(&record.executable_path);
    Ok(VerifiedSelectedRuntime {
        record,
        executable_path,
        _inspected: inspected,
    })
}

fn inspect_selected_runtime_integrity(
    trusted: &TrustedWorkDir,
) -> Result<InspectedRuntimeIntegrity, String> {
    #[cfg(windows)]
    {
        inspect_selected_runtime_integrity_windows(trusted)
    }
    #[cfg(not(windows))]
    {
        let _ = trusted;
        Err("runtime integrity inspection requires supported Windows x64".into())
    }
}

#[cfg(windows)]
mod windows_inspector {
    use std::{
        collections::{BTreeSet, HashSet},
        mem::size_of,
        mem::zeroed,
        os::windows::ffi::OsStrExt,
        path::Path,
    };

    use sha2::{Digest, Sha256};
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{CloseHandle, GENERIC_READ, HANDLE},
            Storage::FileSystem::{
                CreateFileW, FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo, FileIdType,
                GetFileInformationByHandle, GetFileInformationByHandleEx, GetFileSizeEx,
                OpenFileById, ReadFile, SetFilePointerEx, BY_HANDLE_FILE_INFORMATION, FILE_BEGIN,
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_DESCRIPTOR,
                FILE_ID_DESCRIPTOR_0, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
                OPEN_EXISTING, READ_CONTROL,
            },
            System::LibraryLoader::GetModuleFileNameW,
        },
    };

    use super::{
        validate_runtime_contract, BTreeMap, InspectedRuntimeIntegrity, RuntimeCurrent,
        RuntimeManifest, TrustedWorkDir, MAX_RUNTIME_METADATA_BYTES, MAX_RUNTIME_PAYLOAD_BYTES,
        RUNTIME_GENERATIONS_DIR, RUNTIME_PAYLOAD_PATHS, VerifiedConfig,
    };

    #[derive(Debug)]
    pub(super) struct RetainedRuntimeNode {
        handle: HANDLE,
    }

    impl Drop for RetainedRuntimeNode {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }

    pub(super) fn inspect(trusted: &TrustedWorkDir) -> Result<InspectedRuntimeIntegrity, String> {
        let mut nodes = Vec::new();
        let selector = open_walked_node(trusted, "runtime-current.json", false, &mut nodes)?;
        let selector_bytes = read_bounded(nodes[selector].handle, MAX_RUNTIME_METADATA_BYTES)?;
        let current: RuntimeCurrent = serde_json::from_slice(&selector_bytes)
            .map_err(|error| format!("parse selected runtime selector: {error}"))?;
        let generation = super::selected_generation(&current)?.to_owned();

        let generations = open_walked_node(trusted, RUNTIME_GENERATIONS_DIR, true, &mut nodes)?;
        let selected = format!("{RUNTIME_GENERATIONS_DIR}/{generation}");
        let selected_generation = open_walked_node(trusted, &selected, true, &mut nodes)?;
        let actual_files = enumerate_generation_tree(
            nodes[selected_generation].handle,
            &selected,
            "",
            &mut nodes,
        )?;
        let expected_files = RUNTIME_PAYLOAD_PATHS
            .iter()
            .copied()
            .chain(std::iter::once("runtime-manifest.json"))
            .collect::<BTreeSet<_>>();
        if actual_files
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != expected_files
        {
            return Err(
                "selected runtime generation tree is not the exact compiled file set".into(),
            );
        }
        let manifest_relative = format!("{selected}/runtime-manifest.json");
        let manifest_node = open_walked_node(trusted, &manifest_relative, false, &mut nodes)?;
        let manifest_bytes = read_bounded(nodes[manifest_node].handle, MAX_RUNTIME_METADATA_BYTES)?;
        let manifest_hash = hash_bytes(&manifest_bytes);
        let manifest: RuntimeManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| format!("parse selected runtime manifest: {error}"))?;

        let mut payload_hashes = BTreeMap::new();
        let mut executable_node = None;
        let mut config_node = None;
        for payload in RUNTIME_PAYLOAD_PATHS {
            let relative = format!("{selected}/{payload}");
            let node = open_walked_node(trusted, &relative, false, &mut nodes)?;
            let hash = hash_handle(nodes[node].handle, MAX_RUNTIME_PAYLOAD_BYTES)?;
            payload_hashes.insert(payload.to_owned(), hash);
            if payload == "frametime.exe" {
                executable_node = Some(node);
            }
            if payload == "frametime.toml" {
                config_node = Some(node);
            }
        }
        let _ = (generations, selected_generation, manifest_node);
        let generation =
            validate_runtime_contract(&current, &manifest_hash, &manifest, &payload_hashes)?;
        let executable_node =
            executable_node.ok_or("compiled runtime payload lacks frametime.exe")?;
        ensure_current_process_is_selected(nodes[executable_node].handle)?;
        let config_node = config_node.ok_or("compiled runtime payload lacks frametime.toml")?;
        let config_bytes = read_bounded(nodes[config_node].handle, MAX_RUNTIME_METADATA_BYTES)?;
        let config_size = u64::try_from(config_bytes.len())
            .map_err(|_| "selected runtime configuration size overflows u64")?;
        let config_sha256 = manifest
            .files
            .get("frametime.toml")
            .ok_or("selected runtime manifest omits frametime.toml")?;
        // The byte binder follows exact-tree, manifest, payload-hash, and
        // current-process identity validation while the node remains retained.
        let config = VerifiedConfig::from_verified_bytes(config_bytes, config_size, config_sha256)?;

        Ok(InspectedRuntimeIntegrity {
            generation,
            manifest_sha256: manifest_hash,
            manifest,
            config,
            _nodes: nodes,
        })
    }

    fn open_walked_node(
        trusted: &TrustedWorkDir,
        relative: &str,
        final_directory: bool,
        nodes: &mut Vec<RetainedRuntimeNode>,
    ) -> Result<usize, String> {
        let parts = relative.split('/').collect::<Vec<_>>();
        if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
            return Err("runtime descendant path is empty".into());
        }
        let mut walked = String::new();
        let mut result = None;
        for (index, part) in parts.iter().enumerate() {
            if !walked.is_empty() {
                walked.push('\\');
            }
            walked.push_str(part);
            let directory = index + 1 != parts.len() || final_directory;
            let handle = open_node(trusted.path().join(&walked).as_path(), directory)?;
            crate::trusted_work_dir::validate_descendant_handle(handle, &walked, directory)?;
            nodes.push(RetainedRuntimeNode { handle });
            result = Some(nodes.len() - 1);
        }
        result.ok_or_else(|| "runtime descendant path did not produce a handle".to_owned())
    }

    fn open_node(path: &Path, directory: bool) -> Result<HANDLE, String> {
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let desired_access = if directory {
            FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0
        } else {
            GENERIC_READ.0 | FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0
        };
        let flags = if directory {
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
        } else {
            FILE_FLAG_OPEN_REPARSE_POINT
        };
        unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                desired_access,
                FILE_SHARE_READ,
                None,
                OPEN_EXISTING,
                flags,
                None,
            )
        }
        .map_err(|error| format!("open selected runtime handle: {error}"))
    }

    fn enumerate_generation_tree(
        parent: HANDLE,
        selected: &str,
        relative: &str,
        nodes: &mut Vec<RetainedRuntimeNode>,
    ) -> Result<BTreeSet<String>, String> {
        let allowed_directories = compiled_directories();
        let mut actual_files = BTreeSet::new();
        let mut file_identities = HashSet::new();
        enumerate_generation_directory(
            parent,
            selected,
            relative,
            &allowed_directories,
            &mut actual_files,
            &mut file_identities,
            nodes,
        )?;
        Ok(actual_files)
    }

    fn enumerate_generation_directory(
        parent: HANDLE,
        selected: &str,
        relative: &str,
        allowed_directories: &BTreeSet<String>,
        actual_files: &mut BTreeSet<String>,
        file_identities: &mut HashSet<(u32, u32, u32)>,
        nodes: &mut Vec<RetainedRuntimeNode>,
    ) -> Result<(), String> {
        for entry in directory_entries(parent)? {
            let name = String::from_utf16(&entry.name)
                .map_err(|_| "runtime directory entry is not valid UTF-16")?;
            let child_relative = if relative.is_empty() {
                name
            } else {
                format!("{relative}/{name}")
            };
            let directory = entry.attributes & 0x10 != 0;
            let full_relative = format!("{selected}/{child_relative}").replace('/', "\\");
            let handle = open_entry_by_id(parent, entry.id, directory)?;
            crate::trusted_work_dir::validate_descendant_handle(handle, &full_relative, directory)?;
            nodes.push(RetainedRuntimeNode { handle });
            if directory {
                if !allowed_directories.contains(&child_relative) {
                    return Err("selected runtime generation contains an extra directory".into());
                }
                enumerate_generation_directory(
                    handle,
                    selected,
                    &child_relative,
                    allowed_directories,
                    actual_files,
                    file_identities,
                    nodes,
                )?;
            } else {
                let info = handle_information(handle)?;
                if info.nNumberOfLinks != 1 {
                    return Err("selected runtime generation contains a hardlinked file".into());
                }
                let identity = (
                    info.dwVolumeSerialNumber,
                    info.nFileIndexHigh,
                    info.nFileIndexLow,
                );
                if !actual_files.insert(child_relative) || !file_identities.insert(identity) {
                    return Err(
                        "selected runtime generation contains duplicate file identities".into(),
                    );
                }
            }
        }
        Ok(())
    }

    fn compiled_directories() -> BTreeSet<String> {
        let mut directories = BTreeSet::new();
        for payload in RUNTIME_PAYLOAD_PATHS {
            let mut current = payload;
            while let Some((parent, _)) = current.rsplit_once('/') {
                directories.insert(parent.to_owned());
                current = parent;
            }
        }
        directories
    }

    fn directory_entries(
        handle: HANDLE,
    ) -> Result<Vec<crate::shader_cache_handles::DirectoryEntry>, String> {
        const BUFFER_BYTES: usize = 64 * 1024;
        let mut entries = Vec::new();
        let mut restart = true;
        loop {
            let mut buffer = vec![0_u8; BUFFER_BYTES];
            let information_class = if restart {
                FileIdBothDirectoryRestartInfo
            } else {
                FileIdBothDirectoryInfo
            };
            match unsafe {
                GetFileInformationByHandleEx(
                    handle,
                    information_class,
                    buffer.as_mut_ptr().cast(),
                    u32::try_from(BUFFER_BYTES)
                        .map_err(|_| "runtime directory buffer is too large")?,
                )
            } {
                Ok(()) => {
                    restart = false;
                    crate::shader_cache_handle_validation::parse_directory_buffer(
                        &buffer,
                        &mut entries,
                    )
                    .map_err(|error| format!("parse selected runtime directory: {error}"))?;
                }
                Err(error) if error.code().0 == 18 => break,
                Err(error) => return Err(format!("enumerate selected runtime directory: {error}")),
            }
        }
        Ok(entries)
    }

    fn open_entry_by_id(parent: HANDLE, id: i64, directory: bool) -> Result<HANDLE, String> {
        let descriptor = FILE_ID_DESCRIPTOR {
            dwSize: u32::try_from(size_of::<FILE_ID_DESCRIPTOR>())
                .map_err(|_| "runtime file identity descriptor is too large")?,
            Type: FileIdType,
            Anonymous: FILE_ID_DESCRIPTOR_0 { FileId: id },
        };
        let access = if directory {
            FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0
        } else {
            GENERIC_READ.0 | FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0
        };
        let flags = if directory {
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
        } else {
            FILE_FLAG_OPEN_REPARSE_POINT
        };
        unsafe {
            OpenFileById(
                parent,
                &raw const descriptor,
                access,
                FILE_SHARE_READ,
                None,
                flags,
            )
        }
        .map_err(|error| format!("open selected runtime entry by file ID: {error}"))
    }

    fn read_bounded(handle: HANDLE, maximum: usize) -> Result<Vec<u8>, String> {
        seek_to_start(handle)?;
        let length = bounded_length(handle, maximum)?;
        let mut bytes = vec![0_u8; length];
        let mut offset = 0;
        while offset < bytes.len() {
            let mut read = 0_u32;
            unsafe { ReadFile(handle, Some(&mut bytes[offset..]), Some(&mut read), None) }
                .map_err(|error| format!("read selected runtime handle: {error}"))?;
            let read = usize::try_from(read).map_err(|_| "runtime read size overflows usize")?;
            if read == 0 {
                return Err("selected runtime file ended before its validated size".into());
            }
            offset = offset
                .checked_add(read)
                .ok_or("runtime read offset overflows")?;
        }
        Ok(bytes)
    }

    fn hash_handle(handle: HANDLE, maximum: usize) -> Result<String, String> {
        seek_to_start(handle)?;
        let mut remaining = bounded_length(handle, maximum)?;
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        while remaining != 0 {
            let wanted = remaining.min(buffer.len());
            let mut read = 0_u32;
            unsafe { ReadFile(handle, Some(&mut buffer[..wanted]), Some(&mut read), None) }
                .map_err(|error| format!("hash selected runtime handle: {error}"))?;
            let read = usize::try_from(read).map_err(|_| "runtime hash read size overflows")?;
            if read == 0 || read > remaining {
                return Err("selected runtime file ended before its validated size".into());
            }
            digest.update(&buffer[..read]);
            remaining -= read;
        }
        Ok(format!("{:x}", digest.finalize()))
    }

    fn hash_bytes(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn seek_to_start(handle: HANDLE) -> Result<(), String> {
        unsafe { SetFilePointerEx(handle, 0, None, FILE_BEGIN) }
            .map_err(|error| format!("rewind selected runtime handle: {error}"))
    }

    fn bounded_length(handle: HANDLE, maximum: usize) -> Result<usize, String> {
        let mut signed_length = 0_i64;
        unsafe { GetFileSizeEx(handle, &mut signed_length) }
            .map_err(|error| format!("query selected runtime file size: {error}"))?;
        let length = usize::try_from(signed_length)
            .map_err(|_| "selected runtime file size is invalid or exceeds usize")?;
        if length > maximum {
            return Err("selected runtime file exceeds its bounded read limit".into());
        }
        Ok(length)
    }

    fn ensure_current_process_is_selected(selected: HANDLE) -> Result<(), String> {
        let image = current_process_image_handle()?;
        if file_identity(image.handle)? != file_identity(selected)? {
            return Err("current process image is not the selected frametime.exe object".into());
        }
        Ok(())
    }

    struct CurrentImageHandle {
        handle: HANDLE,
    }

    impl Drop for CurrentImageHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }

    fn current_process_image_handle() -> Result<CurrentImageHandle, String> {
        let mut path = vec![0_u16; 32_768];
        let copied = unsafe { GetModuleFileNameW(None, &mut path) };
        let copied = usize::try_from(copied).map_err(|_| "process image path length overflows")?;
        if copied == 0 || copied >= path.len() {
            return Err("resolve current process image path failed or was truncated".into());
        }
        path.truncate(copied);
        path.push(0);
        let handle = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0,
                FILE_SHARE_READ,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
        }
        .map_err(|error| format!("open current process image handle: {error}"))?;
        Ok(CurrentImageHandle { handle })
    }

    fn file_identity(handle: HANDLE) -> Result<(u32, u32, u32), String> {
        let info = handle_information(handle)?;
        Ok((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        ))
    }

    fn handle_information(handle: HANDLE) -> Result<BY_HANDLE_FILE_INFORMATION, String> {
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
        unsafe { GetFileInformationByHandle(handle, &mut info) }
            .map_err(|error| format!("query selected runtime file identity: {error}"))?;
        Ok(info)
    }
}

#[cfg(windows)]
use windows_inspector::{
    inspect as inspect_selected_runtime_integrity_windows, RetainedRuntimeNode,
};
