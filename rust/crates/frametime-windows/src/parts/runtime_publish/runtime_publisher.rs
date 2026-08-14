use std::{
    collections::BTreeMap,
    os::windows::ffi::OsStrExt,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use frametime_core::{
    RuntimeRecord,
    runtime::{
        RUNTIME_GENERATIONS_DIR, RUNTIME_PAYLOAD_PATHS, RUNTIME_SCHEMA_VERSION, RuntimeCurrent,
    },
};
use sha2::{Digest, Sha256};
use windows::{
    Win32::{
        Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE},
        Storage::FileSystem::{
            CREATE_NEW, CreateDirectoryW, CreateFileW, DELETE, FILE_BEGIN,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_DELETE_ON_CLOSE, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_FLAG_WRITE_THROUGH, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
            FlushFileBuffers, GetFileSizeEx, OPEN_EXISTING, READ_CONTROL, ReadFile,
            SetFilePointerEx, WriteFile,
        },
    },
    core::PCWSTR,
};

use super::{
    AuthenticatedPackage, VerifiedPublishedRuntime, manifest_for, payload_directories,
    valid_generated_id,
};
use crate::{TrustedWorkDir, trusted_work_dir, write_json_atomic_trusted};

const MAX_ATTEMPTS: usize = 32;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const MAX_RUNTIME_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;
static NEXT_GENERATION_NONCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub(super) struct RetainedHandle(HANDLE);

impl Drop for RetainedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[derive(Debug)]
pub(super) struct PublicationRetention {
    _handles: Vec<RetainedHandle>,
}

pub(super) fn publish(
    package: &AuthenticatedPackage,
) -> Result<VerifiedPublishedRuntime, String> {
    let trusted = TrustedWorkDir::acquire_fixed()?;
    let mut retained = Vec::new();
    let _lock = PublicationLock::acquire(&trusted)?;
    let generations = ensure_directory(&trusted, RUNTIME_GENERATIONS_DIR, &mut retained)?;
    let (generation, generation_handle) = create_generation(&trusted)?;
    let generation_relative = format!("{RUNTIME_GENERATIONS_DIR}\\{generation}");
    let _ = generations;
    for directory in payload_directories() {
        let relative = format!("{generation_relative}\\{}", directory.replace('/', "\\"));
        ensure_directory(&trusted, &relative, &mut retained)?;
    }

    let mut hashes = BTreeMap::new();
    let mut executable_path = None;
    for payload in RUNTIME_PAYLOAD_PATHS {
        let source = package.retained_payload_handle(payload)?;
        let expected = package
            .manifest()
            .files()
            .iter()
            .find(|file| file.path().eq_ignore_ascii_case(payload))
            .ok_or_else(|| format!("authenticated package omits runtime payload: {payload}"))?;
        let relative = format!("{generation_relative}\\{}", payload.replace('/', "\\"));
        let destination = create_destination_file(&trusted, &relative)?;
        let hash = copy_bounded(source, destination.0, expected.size(), expected.sha256())?;
        validate_destination(&trusted, destination.0, &relative)?;
        drop(destination);
        retained.push(reopen_destination(&trusted, &relative, &hash)?);
        hashes.insert(payload.to_owned(), hash.clone());
        if payload == "frametime.exe" {
            executable_path = Some(
                trusted
                    .path()
                    .join(RUNTIME_GENERATIONS_DIR)
                    .join(&generation)
                    .join(payload),
            );
        }
    }
    let manifest = manifest_for(generation.clone(), hashes)?;
    let manifest_relative = format!("{generation_relative}\\runtime-manifest.json");
    let manifest_bytes = serialize_manifest(&manifest)?;
    let manifest_handle = create_destination_file(&trusted, &manifest_relative)?;
    write_all(manifest_handle.0, &manifest_bytes)?;
    flush(manifest_handle.0, "flush published runtime manifest")?;
    validate_destination(&trusted, manifest_handle.0, &manifest_relative)?;
    let manifest_hash = hex_sha256(&manifest_bytes);
    if hash_handle(manifest_handle.0)? != manifest_hash {
        return Err("published runtime manifest hash changed after write".into());
    }
    drop(manifest_handle);
    retained.push(reopen_destination(
        &trusted,
        &manifest_relative,
        &manifest_hash,
    )?);
    validate_generation_handle(&trusted, generation_handle.0, &generation_relative)?;
    retained.push(generation_handle);

    let current = RuntimeCurrent {
        schema_version: RUNTIME_SCHEMA_VERSION,
        relative_path: format!("{RUNTIME_GENERATIONS_DIR}/{generation}"),
        published_utc: None,
        manifest_sha256: Some(manifest_hash.clone()),
        unknown: BTreeMap::new(),
    };
    write_json_atomic_trusted(&trusted, "runtime-current.json", &current)?;
    let executable_path = executable_path.ok_or("compiled payload lacks frametime.exe")?;
    let record = RuntimeRecord {
        generation,
        manifest_sha256: manifest_hash,
        payload_contract_hash: manifest.payload_contract_hash,
        executable_path: manifest.executable.path,
        executable_sha256: manifest.executable.sha256,
        unknown: BTreeMap::new(),
    };
    Ok(VerifiedPublishedRuntime {
        record,
        executable_path,
        _retained: PublicationRetention { _handles: retained },
    })
}

fn open_existing(path: &Path, directory: bool, access: u32) -> Result<RetainedHandle, String> {
    let wide = wide_path(path);
    let flags = if directory {
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
    } else {
        FILE_FLAG_OPEN_REPARSE_POINT
    };
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            access,
            FILE_SHARE_READ,
            None,
            OPEN_EXISTING,
            flags,
            None,
        )
    }
    .map_err(|error| format!("open packaged runtime payload: {error}"))?;
    Ok(RetainedHandle(handle))
}

fn ensure_directory(
    trusted: &TrustedWorkDir,
    relative: &str,
    retained: &mut Vec<RetainedHandle>,
) -> Result<usize, String> {
    let path = trusted.path().join(relative);
    let attributes = trusted_work_dir::ProtectedSecurityAttributes::new()?;
    let created = match unsafe {
        CreateDirectoryW(PCWSTR(wide_path(&path).as_ptr()), Some(attributes.as_ptr()))
    } {
        Ok(()) => true,
        Err(error) if error.code().0 == 183 => false,
        Err(error) => return Err(format!("create protected runtime directory: {error}")),
    };
    let handle = open_existing(
        &path,
        true,
        FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0,
    )?;
    if created {
        trusted_work_dir::harden_created_child(handle.0)?;
    }
    trusted_work_dir::validate_descendant_handle(handle.0, relative, true)?;
    retained.push(handle);
    Ok(retained.len() - 1)
}

fn create_generation(trusted: &TrustedWorkDir) -> Result<(String, RetainedHandle), String> {
    let attributes = trusted_work_dir::ProtectedSecurityAttributes::new()?;
    for _ in 0..MAX_ATTEMPTS {
        let generation = generated_id();
        let relative = format!("{RUNTIME_GENERATIONS_DIR}\\{generation}");
        let path = trusted.path().join(&relative);
        match unsafe {
            CreateDirectoryW(PCWSTR(wide_path(&path).as_ptr()), Some(attributes.as_ptr()))
        } {
            Ok(()) => {
                let handle = open_existing(
                    &path,
                    true,
                    FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0,
                )?;
                trusted_work_dir::harden_created_child(handle.0)?;
                trusted_work_dir::validate_descendant_handle(handle.0, &relative, true)?;
                return Ok((generation, handle));
            }
            Err(error) if error.code().0 == 183 => continue,
            Err(error) => return Err(format!("create runtime generation: {error}")),
        }
    }
    Err("create runtime generation exhausted unique identities".into())
}

fn create_destination_file(
    trusted: &TrustedWorkDir,
    relative: &str,
) -> Result<RetainedHandle, String> {
    let path = trusted.path().join(relative);
    let attributes = trusted_work_dir::ProtectedSecurityAttributes::new()?;
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path(&path).as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0 | FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0,
            windows::Win32::Storage::FileSystem::FILE_SHARE_MODE(0),
            Some(attributes.as_ptr()),
            CREATE_NEW,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH,
            None,
        )
    }
    .map_err(|error| format!("create runtime payload destination: {error}"))?;
    let handle = RetainedHandle(handle);
    trusted_work_dir::harden_created_child(handle.0)?;
    Ok(handle)
}

fn validate_destination(
    trusted: &TrustedWorkDir,
    handle: HANDLE,
    relative: &str,
) -> Result<(), String> {
    trusted_work_dir::validate_descendant_handle(handle, relative, false)?;
    trusted_work_dir::validate_exact_security(trusted.root_handle())
}

fn reopen_destination(
    trusted: &TrustedWorkDir,
    relative: &str,
    expected_hash: &str,
) -> Result<RetainedHandle, String> {
    let path = trusted.path().join(relative);
    let handle = open_existing(
        &path,
        false,
        GENERIC_READ.0 | FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0,
    )?;
    validate_destination(trusted, handle.0, relative)?;
    if hash_handle(handle.0)? != expected_hash {
        return Err("published runtime payload changed before retention".into());
    }
    Ok(handle)
}

fn copy_bounded(
    source: HANDLE,
    destination: HANDLE,
    expected_size: u64,
    expected_hash: &str,
) -> Result<String, String> {
    seek_to_start(source)?;
    let length = bounded_length(source)?;
    if u64::try_from(length).map_err(|_| "runtime source length overflows")? != expected_size {
        return Err("authenticated runtime source size changed before copy".into());
    }
    let mut remaining = length;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    while remaining != 0 {
        let wanted = remaining.min(buffer.len());
        let mut read = 0_u32;
        unsafe { ReadFile(source, Some(&mut buffer[..wanted]), Some(&mut read), None) }
            .map_err(|error| format!("read packaged runtime payload: {error}"))?;
        let read = usize::try_from(read).map_err(|_| "runtime source read size overflows")?;
        if read == 0 || read > remaining {
            return Err("runtime source changed while copying".into());
        }
        write_all(destination, &buffer[..read])?;
        digest.update(&buffer[..read]);
        remaining -= read;
    }
    flush(destination, "flush published runtime payload")?;
    let expected = format!("{:x}", digest.finalize());
    if expected != expected_hash {
        return Err("authenticated runtime source hash changed before copy".into());
    }
    if hash_handle(destination)? != expected {
        return Err("published runtime payload hash changed after copy".into());
    }
    Ok(expected)
}

fn validate_generation_handle(
    trusted: &TrustedWorkDir,
    generation: HANDLE,
    relative: &str,
) -> Result<(), String> {
    trusted_work_dir::validate_descendant_handle(generation, relative, true)?;
    trusted_work_dir::validate_exact_security(trusted.root_handle())
}

fn serialize_manifest(
    manifest: &frametime_core::runtime::RuntimeManifest,
) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("serialize runtime manifest: {error}"))?;
    bytes.push(b'\n');
    if bytes.len() > 1024 * 1024 {
        return Err("published runtime manifest exceeds 1 MiB".into());
    }
    Ok(bytes)
}

fn generated_id() -> String {
    let nonce = NEXT_GENERATION_NONCE.fetch_add(1, Ordering::Relaxed);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    let digest = Sha256::digest(format!("{}:{}:{}", std::process::id(), nonce, time).as_bytes());
    let value = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    debug_assert!(valid_generated_id(&value));
    value
}

fn bounded_length(handle: HANDLE) -> Result<usize, String> {
    let mut size = 0_i64;
    unsafe { GetFileSizeEx(handle, &mut size) }
        .map_err(|error| format!("query runtime payload size: {error}"))?;
    let size = usize::try_from(size).map_err(|_| "runtime payload size is invalid")?;
    if size > MAX_RUNTIME_PAYLOAD_BYTES {
        return Err("runtime payload exceeds 512 MiB bound".into());
    }
    Ok(size)
}

fn hash_handle(handle: HANDLE) -> Result<String, String> {
    seek_to_start(handle)?;
    let mut remaining = bounded_length(handle)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    while remaining != 0 {
        let wanted = remaining.min(buffer.len());
        let mut read = 0_u32;
        unsafe { ReadFile(handle, Some(&mut buffer[..wanted]), Some(&mut read), None) }
            .map_err(|error| format!("hash published runtime file: {error}"))?;
        let read = usize::try_from(read).map_err(|_| "runtime hash read size overflows")?;
        if read == 0 || read > remaining {
            return Err("published runtime file ended before its validated size".into());
        }
        digest.update(&buffer[..read]);
        remaining -= read;
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_all(handle: HANDLE, bytes: &[u8]) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() {
        let mut written = 0_u32;
        unsafe { WriteFile(handle, Some(&bytes[offset..]), Some(&mut written), None) }
            .map_err(|error| format!("write published runtime file: {error}"))?;
        let written = usize::try_from(written).map_err(|_| "runtime write size overflows")?;
        if written == 0 {
            return Err("published runtime file accepted a zero-byte write".into());
        }
        offset = offset
            .checked_add(written)
            .ok_or("runtime write offset overflows")?;
    }
    Ok(())
}

fn seek_to_start(handle: HANDLE) -> Result<(), String> {
    unsafe { SetFilePointerEx(handle, 0, None, FILE_BEGIN) }
        .map_err(|error| format!("rewind runtime payload: {error}"))
}

fn flush(handle: HANDLE, action: &str) -> Result<(), String> {
    unsafe { FlushFileBuffers(handle) }.map_err(|error| format!("{action}: {error}"))
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

struct PublicationLock {
    _handle: RetainedHandle,
}

impl PublicationLock {
    fn acquire(trusted: &TrustedWorkDir) -> Result<Self, String> {
        let path = trusted.path().join("runtime-publication.lock");
        let attributes = trusted_work_dir::ProtectedSecurityAttributes::new()?;
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide_path(&path).as_ptr()),
                GENERIC_READ.0
                    | GENERIC_WRITE.0
                    | DELETE.0
                    | FILE_READ_ATTRIBUTES.0
                    | READ_CONTROL.0,
                windows::Win32::Storage::FileSystem::FILE_SHARE_MODE(0),
                Some(attributes.as_ptr()),
                CREATE_NEW,
                FILE_FLAG_DELETE_ON_CLOSE | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH,
                None,
            )
        }
        .map_err(|error| format!("runtime publication lock unavailable: {error}"))?;
        trusted_work_dir::harden_created_child(handle)?;
        trusted_work_dir::validate_descendant_handle(handle, "runtime-publication.lock", false)?;
        Ok(Self {
            _handle: RetainedHandle(handle),
        })
    }
}
