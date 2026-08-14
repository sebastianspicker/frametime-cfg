// Handle-backed log display and backup export for the fixed suite root.

use std::{ffi::c_void, mem::size_of, os::windows::ffi::OsStrExt};

use sha2::{Digest, Sha256};
use windows::{
    Win32::{
        Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE},
        Storage::FileSystem::{
            CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
            FILE_ATTRIBUTE_TAG_INFO, FILE_BEGIN, FILE_FLAG_BACKUP_SEMANTICS,
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH, FILE_LIST_DIRECTORY,
            FILE_READ_ATTRIBUTES, FILE_SHARE_MODE, FileAttributeTagInfo, FlushFileBuffers,
            GetDriveTypeW, GetFileInformationByHandleEx, GetFileSizeEx, OPEN_EXISTING, ReadFile,
            SetEndOfFile, SetFilePointerEx, WriteFile,
        },
        System::WindowsProgramming::DRIVE_FIXED,
    },
    core::PCWSTR,
};

use crate::{
    BACKUP_FILE, TrustedWorkDir,
    trusted_io_contract::{is_missing_path_hresult, parse_export_destination},
    trusted_work_dir,
};

const LOG_DIRECTORY: &str = "Logs";
const CURRENT_LOG: &str = "Logs\\frametime_current.log";
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const MAX_LOG_BYTES: usize = 16 * 1024 * 1024;
const MAX_BACKUP_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug)]
struct RetainedHandle(HANDLE);

impl Drop for RetainedHandle {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns exactly one CreateFileW handle.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(crate) fn read_current_log(trusted: &TrustedWorkDir) -> Result<String, String> {
    let _logs = open_trusted_logs_directory(trusted)?;
    let log = open_trusted_file(trusted, CURRENT_LOG)?;
    let bytes = read_bounded(log.0, MAX_LOG_BYTES, "current log")?;
    String::from_utf8(bytes).map_err(|_| "current log is not valid UTF-8".into())
}

pub(crate) fn export_backup(
    trusted: &TrustedWorkDir,
    destination: &std::path::Path,
) -> Result<(), String> {
    let destination = parse_export_destination(destination)?;
    require_fixed_local_drive(&destination.drive_root)?;
    let source = open_trusted_file(trusted, BACKUP_FILE)?;
    let source_length = bounded_length(source.0, MAX_BACKUP_BYTES, "backup source")?;
    let source_hash = hash_handle(source.0, source_length, "backup source")?;
    let _parents = open_destination_parents(&destination)?;
    let output = open_destination(&destination)?;
    validate_regular_file(output.0, "backup export destination")?;
    copy_exact(source.0, output.0, source_length, &source_hash)?;
    Ok(())
}

fn open_trusted_logs_directory(trusted: &TrustedWorkDir) -> Result<RetainedHandle, String> {
    trusted_work_dir::validate_exact_security(trusted.root_handle())?;
    let handle = open_existing(
        &trusted.path().join(LOG_DIRECTORY),
        true,
        FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0 | GENERIC_READ.0,
    )?;
    trusted_work_dir::validate_descendant_handle(handle.0, LOG_DIRECTORY, true)?;
    Ok(handle)
}

fn open_trusted_file(trusted: &TrustedWorkDir, relative: &str) -> Result<RetainedHandle, String> {
    trusted_work_dir::validate_exact_security(trusted.root_handle())?;
    let handle = open_existing(
        &trusted.path().join(relative),
        false,
        GENERIC_READ.0 | FILE_READ_ATTRIBUTES.0,
    )?;
    trusted_work_dir::validate_descendant_handle(handle.0, relative, false)?;
    Ok(handle)
}

fn open_destination_parents(
    destination: &crate::trusted_io_contract::ExportDestination,
) -> Result<Vec<RetainedHandle>, String> {
    let mut retained = Vec::with_capacity(destination.directories.len());
    let mut current = destination.drive_root.clone();
    let root = open_directory(&current)?;
    validate_directory(root.0, &current)?;
    retained.push(root);
    for component in &destination.directories {
        current.push_str(component);
        let handle = open_directory(&current)?;
        validate_directory(handle.0, &current)?;
        retained.push(handle);
        current.push('\\');
    }
    Ok(retained)
}

fn open_destination(
    destination: &crate::trusted_io_contract::ExportDestination,
) -> Result<RetainedHandle, String> {
    let expected = destination.absolute_path();
    let path = wide(&expected);
    let access = GENERIC_READ.0 | GENERIC_WRITE.0 | FILE_READ_ATTRIBUTES.0;
    let flags = FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH;
    let opened = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            access,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            flags,
            None,
        )
    };
    let handle = match opened {
        Ok(handle) => RetainedHandle(handle),
        Err(error) if is_missing_path_hresult(error.code().0) => unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                access,
                FILE_SHARE_MODE(0),
                None,
                CREATE_NEW,
                flags,
                None,
            )
            .map(RetainedHandle)
            .map_err(|error| format!("create backup export destination: {error}"))?
        },
        Err(error) => return Err(format!("open backup export destination: {error}")),
    };
    validate_final_path(handle.0, &expected, "backup export destination")?;
    Ok(handle)
}

fn open_directory(path: &str) -> Result<RetainedHandle, String> {
    let path = wide(path);
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0 | GENERIC_READ.0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    }
    .map_err(|error| format!("open backup export directory component: {error}"))?;
    Ok(RetainedHandle(handle))
}

fn open_existing(
    path: &std::path::Path,
    directory: bool,
    access: u32,
) -> Result<RetainedHandle, String> {
    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let flags = if directory {
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
    } else {
        FILE_FLAG_OPEN_REPARSE_POINT
    };
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            access,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            flags,
            None,
        )
    }
    .map_err(|error| format!("open retained trusted suite file: {error}"))?;
    Ok(RetainedHandle(handle))
}

fn require_fixed_local_drive(root: &str) -> Result<(), String> {
    let root = wide(root);
    if unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) } != DRIVE_FIXED {
        return Err("backup export destination must be on a local fixed drive".into());
    }
    Ok(())
}

fn validate_directory(handle: HANDLE, expected: &str) -> Result<(), String> {
    validate_type(handle, true, "backup export directory component")?;
    validate_final_path(handle, expected, "backup export directory component")
}

fn validate_regular_file(handle: HANDLE, label: &str) -> Result<(), String> {
    validate_type(handle, false, label)
}

fn validate_type(handle: HANDLE, directory: bool, label: &str) -> Result<(), String> {
    let mut attributes = FILE_ATTRIBUTE_TAG_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileAttributeTagInfo,
            (&mut attributes as *mut FILE_ATTRIBUTE_TAG_INFO).cast::<c_void>(),
            u32::try_from(size_of::<FILE_ATTRIBUTE_TAG_INFO>())
                .map_err(|_| "attribute record too large")?,
        )
    }
    .map_err(|error| format!("query {label} attributes: {error}"))?;
    let actual_directory = attributes.FileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    if actual_directory != directory
        || attributes.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
    {
        return Err(format!(
            "{label} must be a non-reparse {}",
            if directory {
                "directory"
            } else {
                "regular file"
            }
        ));
    }
    Ok(())
}

fn validate_final_path(handle: HANDLE, expected: &str, label: &str) -> Result<(), String> {
    let actual = final_path(handle)?;
    if !same_windows_path(&actual, expected) {
        return Err(format!(
            "{label} changed identity during handle acquisition"
        ));
    }
    Ok(())
}

fn final_path(handle: HANDLE) -> Result<String, String> {
    use windows::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;

    let mut units = vec![0_u16; 512];
    loop {
        let required = unsafe { GetFinalPathNameByHandleW(handle, &mut units, Default::default()) };
        if required == 0 {
            return Err("resolve retained file final path failed".into());
        }
        let required = usize::try_from(required).map_err(|_| "retained final path is too large")?;
        if required < units.len() {
            return String::from_utf16(&units[..required])
                .map_err(|_| "retained final path is not valid UTF-16".to_owned());
        }
        units.resize(required.saturating_add(1), 0);
    }
}

fn same_windows_path(actual: &str, expected: &str) -> bool {
    actual
        .strip_prefix(r"\\?\")
        .unwrap_or(actual)
        .eq_ignore_ascii_case(expected)
}

fn bounded_length(handle: HANDLE, maximum: usize, label: &str) -> Result<usize, String> {
    let mut length = 0_i64;
    unsafe { GetFileSizeEx(handle, &mut length) }
        .map_err(|error| format!("query {label} size: {error}"))?;
    let length = usize::try_from(length).map_err(|_| format!("{label} size is invalid"))?;
    if length > maximum {
        return Err(format!("{label} exceeds its {maximum}-byte limit"));
    }
    Ok(length)
}

fn read_bounded(handle: HANDLE, maximum: usize, label: &str) -> Result<Vec<u8>, String> {
    let length = bounded_length(handle, maximum, label)?;
    seek_start(handle, label)?;
    let mut bytes = vec![0; length];
    read_exact(handle, &mut bytes, label)?;
    Ok(bytes)
}

fn hash_handle(handle: HANDLE, length: usize, label: &str) -> Result<[u8; 32], String> {
    seek_start(handle, label)?;
    let mut remaining = length;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    while remaining != 0 {
        let wanted = remaining.min(buffer.len());
        read_exact(handle, &mut buffer[..wanted], label)?;
        hasher.update(&buffer[..wanted]);
        remaining -= wanted;
    }
    Ok(hasher.finalize().into())
}

fn copy_exact(
    source: HANDLE,
    destination: HANDLE,
    length: usize,
    expected: &[u8; 32],
) -> Result<(), String> {
    seek_start(source, "backup source")?;
    seek_start(destination, "backup export destination")?;
    unsafe { SetEndOfFile(destination) }
        .map_err(|error| format!("truncate backup export destination: {error}"))?;
    let mut remaining = length;
    let mut copied = 0_usize;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    while remaining != 0 {
        let wanted = remaining.min(buffer.len());
        read_exact(source, &mut buffer[..wanted], "backup source")?;
        write_all(destination, &buffer[..wanted])?;
        hasher.update(&buffer[..wanted]);
        copied = copied
            .checked_add(wanted)
            .ok_or("backup export byte count overflows")?;
        remaining -= wanted;
    }
    unsafe { FlushFileBuffers(destination) }
        .map_err(|error| format!("flush backup export destination: {error}"))?;
    let copied_hash: [u8; 32] = hasher.finalize().into();
    if copied != length || copied_hash != *expected {
        return Err("backup source changed while copying".into());
    }
    if bounded_length(destination, MAX_BACKUP_BYTES, "backup export destination")? != length
        || hash_handle(destination, length, "backup export destination")? != *expected
    {
        return Err(
            "backup export destination failed same-handle byte or SHA-256 verification".into(),
        );
    }
    Ok(())
}

fn read_exact(handle: HANDLE, bytes: &mut [u8], label: &str) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() {
        let mut read = 0_u32;
        unsafe { ReadFile(handle, Some(&mut bytes[offset..]), Some(&mut read), None) }
            .map_err(|error| format!("read {label}: {error}"))?;
        let read = usize::try_from(read).map_err(|_| format!("{label} read count overflows"))?;
        if read == 0 {
            return Err(format!("{label} ended before its retained length"));
        }
        offset = offset
            .checked_add(read)
            .ok_or_else(|| format!("{label} read offset overflows"))?;
    }
    Ok(())
}

fn write_all(handle: HANDLE, bytes: &[u8]) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() {
        let mut written = 0_u32;
        unsafe { WriteFile(handle, Some(&bytes[offset..]), Some(&mut written), None) }
            .map_err(|error| format!("write backup export destination: {error}"))?;
        let written =
            usize::try_from(written).map_err(|_| "backup export write count overflows")?;
        if written == 0 {
            return Err("backup export destination accepted a zero-byte write".into());
        }
        offset = offset
            .checked_add(written)
            .ok_or("backup export write offset overflows")?;
    }
    Ok(())
}

fn seek_start(handle: HANDLE, label: &str) -> Result<(), String> {
    unsafe { SetFilePointerEx(handle, 0, None, FILE_BEGIN) }
        .map_err(|error| format!("rewind {label}: {error}"))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
