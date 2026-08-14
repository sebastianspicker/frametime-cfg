//! Handle-observation and hostile-directory-record validation for P1:3.

use std::mem::{size_of, zeroed};

use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_TAG_INFO, FILE_ID_BOTH_DIR_INFO, FILE_ID_INFO,
        FILE_NAME_NORMALIZED, FileAttributeTagInfo, FileIdInfo, GetFileInformationByHandle,
        GetFileInformationByHandleEx, GetFinalPathNameByHandleW,
    },
};

use crate::{
    shader_cache_handles::{
        DirectoryEntry, FILE_ATTRIBUTE_DIRECTORY_BITS, FILE_ATTRIBUTE_REPARSE_BITS, FileKey,
        MAX_PATH_UTF16, Node, ObservedNode,
    },
    validate_shader_cache_entry_name,
};

pub(crate) fn parse_directory_buffer(
    buffer: &[u8],
    output: &mut Vec<DirectoryEntry>,
) -> Result<(), String> {
    let mut offset = 0_usize;
    let header = size_of::<FILE_ID_BOTH_DIR_INFO>() - size_of::<u16>();
    loop {
        if offset
            .checked_add(header)
            .is_none_or(|end| end > buffer.len())
        {
            return Err("P1:3 directory enumeration returned a truncated record".into());
        }
        let record = unsafe {
            buffer
                .as_ptr()
                .add(offset)
                .cast::<FILE_ID_BOTH_DIR_INFO>()
                .read_unaligned()
        };
        if !record.FileNameLength.is_multiple_of(2) {
            return Err("P1:3 directory enumeration returned an odd UTF-16 name length".into());
        }
        let name_units = usize::try_from(record.FileNameLength / 2)
            .map_err(|_| "P1:3 directory name length overflow")?;
        let name_start = offset + header;
        let name_end = name_start
            .checked_add(name_units * size_of::<u16>())
            .ok_or("P1:3 directory name length overflow")?;
        if name_end > buffer.len() {
            return Err("P1:3 directory enumeration returned an invalid name length".into());
        }
        let name = unsafe {
            std::slice::from_raw_parts(buffer.as_ptr().add(name_start).cast::<u16>(), name_units)
        };
        if name != [b'.' as u16] && name != [b'.' as u16, b'.' as u16] {
            validate_shader_cache_entry_name(name)?;
            output.push(DirectoryEntry {
                id: record.FileId,
                name: name.to_vec(),
                attributes: record.FileAttributes,
            });
        }
        if record.NextEntryOffset == 0 {
            return Ok(());
        }
        let next = usize::try_from(record.NextEntryOffset)
            .map_err(|_| "P1:3 directory enumeration offset overflow")?;
        if next < header {
            return Err("P1:3 directory enumeration returned a non-forward offset".into());
        }
        offset = offset
            .checked_add(next)
            .ok_or("P1:3 directory enumeration offset overflow")?;
    }
}

pub(crate) fn observe_node(handle: HANDLE) -> Result<ObservedNode, String> {
    let mut tag: FILE_ATTRIBUTE_TAG_INFO = unsafe { zeroed() };
    unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileAttributeTagInfo,
            (&raw mut tag).cast(),
            u32::try_from(size_of::<FILE_ATTRIBUTE_TAG_INFO>())
                .map_err(|_| "P1:3 attribute tag size overflow")?,
        )
    }
    .map_err(|error| format!("inspect P1:3 object attributes: {error}"))?;
    if tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_BITS != 0 {
        return Err("P1:3 cache tree contains a reparse point".into());
    }
    let mut links: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    unsafe { GetFileInformationByHandle(handle, &raw mut links) }
        .map_err(|error| format!("inspect P1:3 object link count: {error}"))?;
    if links.nNumberOfLinks != 1 {
        return Err("P1:3 cache tree contains a hard-linked object".into());
    }
    let mut info: FILE_ID_INFO = unsafe { zeroed() };
    unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&raw mut info).cast(),
            u32::try_from(size_of::<FILE_ID_INFO>()).map_err(|_| "P1:3 file ID size overflow")?,
        )
    }
    .map_err(|error| format!("inspect P1:3 object identity: {error}"))?;
    Ok(ObservedNode {
        id: FileKey {
            volume: info.VolumeSerialNumber,
            id: info.FileId.Identifier,
        },
        legacy_id: ((u64::from(links.nFileIndexHigh) << 32) | u64::from(links.nFileIndexLow))
            as i64,
        attributes: tag.FileAttributes,
        directory: tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY_BITS != 0,
        final_path: final_path(handle)?,
    })
}

pub(crate) fn validate_node(node: &Node, parent: Option<&Node>) -> Result<(), String> {
    let observed = observe_node(node.handle.0)?;
    if observed.id != node.id
        || observed.directory != node.directory
        || observed.final_path != node.final_path
        || node.entry_id.is_some_and(|id| id != observed.legacy_id)
        || node
            .entry_attributes
            .is_some_and(|attributes| attributes != observed.attributes)
    {
        return Err(
            "P1:3 retained handle no longer matches its preflight identity or attributes".into(),
        );
    }
    if let Some(parent) = parent {
        ensure_direct_child(&parent.final_path, &observed.final_path, &node.name)?;
    }
    Ok(())
}

pub(crate) fn validate_directory_entry(entry: &DirectoryEntry) -> Result<(), String> {
    validate_shader_cache_entry_name(&entry.name)?;
    if entry.attributes & FILE_ATTRIBUTE_REPARSE_BITS != 0 {
        return Err("P1:3 cache tree contains a reparse point".into());
    }
    Ok(())
}

pub(crate) fn ensure_direct_child(
    parent: &[u16],
    child: &[u16],
    name: &[u16],
) -> Result<(), String> {
    validate_shader_cache_entry_name(name)?;
    let separator = [b'\\' as u16];
    if !child.starts_with(parent)
        || child.get(parent.len()..parent.len() + 1) != Some(separator.as_slice())
        || child.get(parent.len() + 1..) != Some(name)
    {
        return Err(
            "P1:3 object final path is not an exact direct child of its retained parent".into(),
        );
    }
    Ok(())
}

fn final_path(handle: HANDLE) -> Result<Vec<u16>, String> {
    let required = unsafe { GetFinalPathNameByHandleW(handle, &mut [], FILE_NAME_NORMALIZED) };
    let required = usize::try_from(required).map_err(|_| "P1:3 final path size overflow")?;
    if required == 0 || required > MAX_PATH_UTF16 {
        return Err("P1:3 could not obtain a bounded final path for a retained handle".into());
    }
    let mut path = vec![0_u16; required + 1];
    let actual = usize::try_from(unsafe {
        GetFinalPathNameByHandleW(handle, &mut path, FILE_NAME_NORMALIZED)
    })
    .map_err(|_| "P1:3 final path size overflow")?;
    if actual == 0 || actual >= path.len() || actual > MAX_PATH_UTF16 {
        return Err("P1:3 final path changed while being read".into());
    }
    path.truncate(actual);
    Ok(path)
}
