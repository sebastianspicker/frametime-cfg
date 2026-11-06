//! Windows-only, handle-backed JSON persistence for the fixed suite root.

use std::{
    mem::{offset_of, size_of},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_SHARING_VIOLATION, GENERIC_READ, GENERIC_WRITE, HANDLE},
        Storage::FileSystem::{
            CREATE_NEW, CreateFileW, DELETE, FILE_BEGIN, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_FLAG_WRITE_THROUGH, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_MODE,
            FILE_SHARE_READ, FileDispositionInfo, FileRenameInfoEx, FlushFileBuffers,
            GetFileSizeEx, OPEN_EXISTING, READ_CONTROL, ReadFile, SetFileInformationByHandle,
            SetFilePointerEx, WriteFile,
        },
        System::WindowsProgramming::FILE_RENAME_FLAG_REPLACE_IF_EXISTS,
    },
    core::PCWSTR,
};

use super::io_contract::is_missing_path_hresult;
use super::io_windows::OpenError;
use super::json_common as trusted_json_common;
use crate::system::wide_path;
use crate::{TrustedWorkDir, trusted_work_dir};

static NEXT_TEMP_NONCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn read_json<T: DeserializeOwned>(
    trusted: &TrustedWorkDir,
    name: &str,
) -> Result<T, String> {
    read_json_if_present(trusted, name)?
        .ok_or_else(|| format!("trusted suite file {name} does not exist"))
}

/// `Ok(None)` only when the file is missing; every other open or read error propagates.
pub(super) fn read_json_if_present<T: DeserializeOwned>(
    trusted: &TrustedWorkDir,
    name: &str,
) -> Result<Option<T>, String> {
    let file = match open_existing_child(trusted, name) {
        Ok(file) => file,
        Err(OpenError::Missing(_)) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    trusted_work_dir::validate_child_handle(file.raw(), name)?;
    let bytes = read_all(file.raw())?;
    serde_json::from_slice(&bytes).map(Some).map_err(|error| {
        format!(
            "corrupt trusted suite file is retained in place; refusing unsafe corrupt-copy creation: {error}"
        )
    })
}

pub(super) fn write_json_atomic<T: Serialize>(
    trusted: &TrustedWorkDir,
    name: &str,
    value: &T,
) -> Result<(), String> {
    let bytes = trusted_json_common::serialize_json(value)?;
    let expected_hash = hash_bytes(&bytes);
    let mut temporary = create_temporary_child(trusted, name)?;
    let result = (|| {
        write_all(temporary.raw(), &bytes)?;
        flush(temporary.raw(), "flush temporary trusted suite JSON")?;
        trusted_work_dir::validate_temporary_handle(temporary.raw(), temporary.leaf())?;
        if hash_handle(temporary.raw())? != expected_hash {
            return Err("trusted suite JSON changed before atomic replacement".into());
        }

        trusted_work_dir::validate_exact_security(trusted.root_handle())?;
        temporary.keep()?;
        rename_into_parent(temporary.raw(), trusted.root_handle(), name)?;
        temporary.mark_renamed();
        flush(temporary.raw(), "flush replaced trusted suite JSON")?;
        trusted_work_dir::validate_child_handle(temporary.raw(), name)?;
        if hash_handle(temporary.raw())? != expected_hash {
            return Err("trusted suite JSON hash changed after atomic replacement".into());
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) if temporary.renamed => Err(error),
        Err(error) => match temporary.dispose() {
            Ok(()) => Err(error),
            Err(cleanup) => Err(format!(
                "{error}; also failed to dispose temporary handle: {cleanup}"
            )),
        },
    }
}

#[derive(Debug)]
struct FileHandle(HANDLE);

impl FileHandle {
    fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for FileHandle {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the HANDLE returned by CreateFileW exactly once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct TemporaryFile {
    file: FileHandle,
    leaf: String,
    renamed: bool,
    disposed: bool,
}

impl TemporaryFile {
    fn raw(&self) -> HANDLE {
        self.file.raw()
    }

    fn leaf(&self) -> &str {
        &self.leaf
    }

    fn mark_renamed(&mut self) {
        self.renamed = true;
    }

    fn dispose(&mut self) -> Result<(), String> {
        if !self.renamed && !self.disposed {
            dispose_handle(self.raw(), true)?;
            self.disposed = true;
        }
        Ok(())
    }

    /// Clears the delete-on-close disposition; a delete-pending file cannot be renamed.
    fn keep(&mut self) -> Result<(), String> {
        dispose_handle(self.raw(), false)?;
        self.disposed = false;
        Ok(())
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if !self.renamed && !self.disposed {
            let _ = dispose_handle(self.raw(), true);
        }
    }
}

fn open_existing_child(trusted: &TrustedWorkDir, name: &str) -> Result<FileHandle, OpenError> {
    if !trusted_json_common::is_allowed_child(name) {
        return Err("suite child identity is not allowlisted".to_owned().into());
    }
    let path = wide_path(&trusted.path().join(name));
    trusted_work_dir::validate_exact_security(trusted.root_handle())?;
    // A writer briefly holds the just-renamed file exclusively while it verifies it.
    let mut attempts = 0;
    let handle = loop {
        // SAFETY: `path` is a NUL-terminated UTF-16 buffer kept alive for the call.
        let opened = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                GENERIC_READ.0 | FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0,
                // Read-only: an atomic rename-over by a writer may replace the file; we keep the old content.
                FILE_SHARE_READ | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
        };
        match opened {
            Err(error) if error.code() == ERROR_SHARING_VIOLATION.to_hresult() && attempts < 10 => {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            other => break other,
        }
    }
    .map_err(|error| {
        let message = format!("open trusted suite JSON handle: {error}");
        if is_missing_path_hresult(error.code().0) {
            OpenError::Missing(message)
        } else {
            OpenError::Failed(message)
        }
    })?;
    Ok(FileHandle(handle))
}

fn create_temporary_child(trusted: &TrustedWorkDir, parent: &str) -> Result<TemporaryFile, String> {
    if !trusted_json_common::is_allowed_child(parent) {
        return Err("suite child identity is not allowlisted".into());
    }
    let attributes = trusted_work_dir::ProtectedSecurityAttributes::new()?;
    for _ in 0..32 {
        let leaf = trusted_json_common::temporary_leaf(parent, next_temp_nonce())?;
        let path = wide_path(&trusted.path().join(&leaf));
        trusted_work_dir::validate_exact_security(trusted.root_handle())?;
        // SAFETY: path and SECURITY_ATTRIBUTES pointers remain valid for this synchronous call.
        let opened = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                GENERIC_READ.0
                    | GENERIC_WRITE.0
                    | DELETE.0
                    | FILE_READ_ATTRIBUTES.0
                    | READ_CONTROL.0,
                // Exclusive: the temp file is private until it is renamed into place.
                FILE_SHARE_MODE(0),
                Some(attributes.as_ptr()),
                CREATE_NEW,
                FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH,
                None,
            )
        };
        match opened {
            Ok(handle) => {
                let mut temporary = TemporaryFile {
                    file: FileHandle(handle),
                    leaf,
                    renamed: false,
                    disposed: false,
                };
                // The kernel deletes the file if the process dies before the rename.
                temporary.dispose()?;
                trusted_work_dir::validate_temporary_handle(temporary.raw(), temporary.leaf())?;
                return Ok(temporary);
            }
            Err(error) if error.code().0 == 80 => continue,
            Err(error) => {
                return Err(format!(
                    "create trusted suite JSON temporary handle: {error}"
                ));
            }
        }
    }
    Err("create trusted suite JSON temporary handle exhausted unique names".into())
}

fn next_temp_nonce() -> u64 {
    let counter = NEXT_TEMP_NONCE.fetch_add(1, Ordering::Relaxed);
    (u64::from(std::process::id()) << 32) | counter
}

fn read_all(handle: HANDLE) -> Result<Vec<u8>, String> {
    seek_to_start(handle)?;
    let mut signed_length = 0_i64;
    // SAFETY: `handle` is an open file handle and the out-pointer is valid.
    unsafe { GetFileSizeEx(handle, &mut signed_length) }
        .map_err(|error| format!("query trusted suite JSON size: {error}"))?;
    let length = usize::try_from(signed_length)
        .map_err(|_| "trusted suite JSON size is invalid or exceeds usize")?;
    trusted_json_common::ensure_bounded_size(length)?;
    let mut bytes = vec![0_u8; length];
    let mut offset = 0;
    while offset < bytes.len() {
        let mut read = 0_u32;
        // SAFETY: the owned slice and byte-count out-pointer are valid for this synchronous call.
        unsafe { ReadFile(handle, Some(&mut bytes[offset..]), Some(&mut read), None) }
            .map_err(|error| format!("read trusted suite JSON handle: {error}"))?;
        let read = usize::try_from(read).map_err(|_| "trusted suite JSON read size overflows")?;
        if read == 0 {
            return Err("trusted suite JSON ended before its validated size".into());
        }
        offset = offset
            .checked_add(read)
            .ok_or("trusted suite JSON read offset overflows")?;
    }
    Ok(bytes)
}

fn write_all(handle: HANDLE, bytes: &[u8]) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() {
        let mut written = 0_u32;
        // SAFETY: the immutable slice and byte-count out-pointer are valid for this synchronous call.
        unsafe { WriteFile(handle, Some(&bytes[offset..]), Some(&mut written), None) }
            .map_err(|error| format!("write trusted suite JSON handle: {error}"))?;
        let written =
            usize::try_from(written).map_err(|_| "trusted suite JSON write size overflows")?;
        if written == 0 {
            return Err("trusted suite JSON handle accepted a zero-byte write".into());
        }
        offset = offset
            .checked_add(written)
            .ok_or("trusted suite JSON write offset overflows")?;
    }
    Ok(())
}

fn hash_handle(handle: HANDLE) -> Result<[u8; 32], String> {
    Ok(Sha256::digest(read_all(handle)?).into())
}

fn hash_bytes(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn seek_to_start(handle: HANDLE) -> Result<(), String> {
    // SAFETY: `handle` is an open synchronous file handle; no output pointer is requested.
    unsafe { SetFilePointerEx(handle, 0, None, FILE_BEGIN) }
        .map_err(|error| format!("rewind trusted suite JSON handle: {error}"))
}

fn flush(handle: HANDLE, action: &str) -> Result<(), String> {
    // SAFETY: `handle` is an open file handle owned by this module.
    unsafe { FlushFileBuffers(handle) }.map_err(|error| format!("{action}: {error}"))
}

fn dispose_handle(handle: HANDLE, delete_file: bool) -> Result<(), String> {
    let disposition = windows::Win32::Storage::FileSystem::FILE_DISPOSITION_INFO {
        DeleteFile: delete_file,
    };
    // SAFETY: `handle` is the exact temporary handle and `disposition` lives for the call.
    unsafe {
        SetFileInformationByHandle(
            handle,
            FileDispositionInfo,
            (&disposition as *const windows::Win32::Storage::FileSystem::FILE_DISPOSITION_INFO)
                .cast(),
            u32::try_from(size_of::<
                windows::Win32::Storage::FileSystem::FILE_DISPOSITION_INFO,
            >())
            .map_err(|_| "trusted suite temporary disposition size exceeds u32")?,
        )
    }
    .map_err(|error| format!("dispose trusted suite JSON temporary handle: {error}"))
}

#[repr(C)]
struct RenameInfoHeader {
    flags: u32,
    root_directory: HANDLE,
    file_name_length: u32,
    file_name: [u16; 0],
}

fn rename_into_parent(handle: HANDLE, root: HANDLE, leaf: &str) -> Result<(), String> {
    if !trusted_json_common::is_allowed_child(leaf) {
        return Err("suite child identity is not allowlisted".into());
    }
    let name: Vec<u16> = leaf.encode_utf16().collect();
    let name_bytes = name
        .len()
        .checked_mul(size_of::<u16>())
        .ok_or("trusted suite JSON destination name is too long")?;
    let name_offset = offset_of!(RenameInfoHeader, file_name);
    let bytes = name_offset
        .checked_add(name_bytes)
        .ok_or("trusted suite JSON rename buffer is too large")?;
    let words = bytes
        .checked_add(size_of::<usize>() - 1)
        .ok_or("trusted suite JSON rename allocation overflows")?
        / size_of::<usize>();
    let mut storage = vec![0_usize; words];
    let base = storage.as_mut_ptr().cast::<u8>();
    // SAFETY: aligned storage is large enough for the documented rename header and UTF-16 leaf.
    unsafe {
        base.cast::<RenameInfoHeader>().write(RenameInfoHeader {
            flags: FILE_RENAME_FLAG_REPLACE_IF_EXISTS,
            root_directory: root,
            file_name_length: u32::try_from(name_bytes)
                .map_err(|_| "trusted suite JSON destination name exceeds u32")?,
            file_name: [],
        });
        std::ptr::copy_nonoverlapping(
            name.as_ptr().cast::<u8>(),
            base.add(name_offset),
            name_bytes,
        );
        SetFileInformationByHandle(
            handle,
            FileRenameInfoEx,
            base.cast(),
            u32::try_from(bytes).map_err(|_| "trusted suite JSON rename buffer exceeds u32")?,
        )
    }
    .map_err(|error| format!("atomically replace trusted suite JSON handle: {error}"))
}
