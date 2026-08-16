//! Handle-observation and hostile-directory-record validation for P1:3.

use std::mem::{offset_of, size_of, zeroed};

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
    let mut parsed = Vec::new();
    let header = offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
    loop {
        let record = record_bytes(buffer, offset, header)?;
        let file_name_length =
            read_u32_field(record, offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength))?;
        if !file_name_length.is_multiple_of(2) {
            return Err("P1:3 directory enumeration returned an odd UTF-16 name length".into());
        }
        let name_units = usize::try_from(file_name_length / 2)
            .map_err(|_| "P1:3 directory name length overflow")?;
        let name_start = offset
            .checked_add(header)
            .ok_or("P1:3 directory name length overflow")?;
        let name_end = name_start
            .checked_add(
                name_units
                    .checked_mul(size_of::<u16>())
                    .ok_or("P1:3 directory name length overflow")?,
            )
            .ok_or("P1:3 directory name length overflow")?;
        let name_bytes = buffer
            .get(name_start..name_end)
            .ok_or("P1:3 directory enumeration returned an invalid name length")?;
        let record_length = header
            .checked_add(name_bytes.len())
            .ok_or("P1:3 directory name length overflow")?;
        let name: Vec<u16> = name_bytes
            .chunks_exact(size_of::<u16>())
            .map(|unit| u16::from_ne_bytes([unit[0], unit[1]]))
            .collect();
        if name != [b'.' as u16] && name != [b'.' as u16, b'.' as u16] {
            validate_shader_cache_entry_name(&name)?;
            parsed.push(DirectoryEntry {
                id: read_i64_field(record, offset_of!(FILE_ID_BOTH_DIR_INFO, FileId))?,
                name,
                attributes: read_u32_field(
                    record,
                    offset_of!(FILE_ID_BOTH_DIR_INFO, FileAttributes),
                )?,
            });
        }
        let next_entry_offset =
            read_u32_field(record, offset_of!(FILE_ID_BOTH_DIR_INFO, NextEntryOffset))?;
        if next_entry_offset == 0 {
            output.extend(parsed);
            return Ok(());
        }
        let next = usize::try_from(next_entry_offset)
            .map_err(|_| "P1:3 directory enumeration offset overflow")?;
        if next < record_length {
            return Err(
                "P1:3 directory enumeration next offset overlaps the current record".into(),
            );
        }
        let remaining = buffer
            .len()
            .checked_sub(offset)
            .ok_or("P1:3 directory enumeration offset overflow")?;
        if next > remaining {
            return Err(
                "P1:3 directory enumeration next offset exceeds the returned buffer".into(),
            );
        }
        offset = offset
            .checked_add(next)
            .ok_or("P1:3 directory enumeration offset overflow")?;
    }
}

fn record_bytes(buffer: &[u8], offset: usize, length: usize) -> Result<&[u8], String> {
    let end = offset
        .checked_add(length)
        .ok_or("P1:3 directory enumeration returned a truncated record")?;
    buffer
        .get(offset..end)
        .ok_or_else(|| "P1:3 directory enumeration returned a truncated record".into())
}

fn read_u32_field(record: &[u8], field_offset: usize) -> Result<u32, String> {
    Ok(u32::from_ne_bytes(read_field(record, field_offset)?))
}

fn read_i64_field(record: &[u8], field_offset: usize) -> Result<i64, String> {
    Ok(i64::from_ne_bytes(read_field(record, field_offset)?))
}

fn read_field<const LENGTH: usize>(
    record: &[u8],
    field_offset: usize,
) -> Result<[u8; LENGTH], String> {
    let field_end = field_offset
        .checked_add(LENGTH)
        .ok_or("P1:3 directory enumeration returned a truncated record")?;
    record
        .get(field_offset..field_end)
        .and_then(|field| field.try_into().ok())
        .ok_or_else(|| "P1:3 directory enumeration returned a truncated record".into())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of_val;

    fn header_size() -> usize {
        offset_of!(FILE_ID_BOTH_DIR_INFO, FileName)
    }

    fn write_u32(buffer: &mut [u8], offset: usize, value: u32) {
        buffer[offset..offset + size_of::<u32>()].copy_from_slice(&value.to_ne_bytes());
    }

    fn write_i64(buffer: &mut [u8], offset: usize, value: i64) {
        buffer[offset..offset + size_of::<i64>()].copy_from_slice(&value.to_ne_bytes());
    }

    fn directory_record(name: &[u16], next_entry_offset: u32) -> Vec<u8> {
        let mut record = vec![0_u8; header_size() + size_of_val(name)];
        write_u32(
            &mut record,
            offset_of!(FILE_ID_BOTH_DIR_INFO, NextEntryOffset),
            next_entry_offset,
        );
        write_u32(
            &mut record,
            offset_of!(FILE_ID_BOTH_DIR_INFO, FileAttributes),
            0x1234_5678,
        );
        write_u32(
            &mut record,
            offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength),
            u32::try_from(size_of_val(name)).expect("test name length fits in u32"),
        );
        write_i64(
            &mut record,
            offset_of!(FILE_ID_BOTH_DIR_INFO, FileId),
            0x0123_4567_89AB_CDEF,
        );
        for (bytes, unit) in record[header_size()..]
            .chunks_exact_mut(size_of::<u16>())
            .zip(name)
        {
            bytes.copy_from_slice(&unit.to_ne_bytes());
        }
        record
    }

    #[test]
    fn rejects_zero_length_names_without_reading_past_the_fixed_record() {
        let mut entries = Vec::new();

        let error = parse_directory_buffer(&directory_record(&[], 0), &mut entries)
            .expect_err("zero-length names are hostile");

        assert_eq!(error, "P1:3 directory entry has a hostile or alias name");
        assert!(entries.is_empty());
    }

    #[test]
    fn rejects_truncated_fixed_record_fields() {
        let mut entries = Vec::new();
        let buffer = vec![0_u8; header_size() - 1];

        let error = parse_directory_buffer(&buffer, &mut entries)
            .expect_err("fixed record fields must be present");

        assert_eq!(
            error,
            "P1:3 directory enumeration returned a truncated record"
        );
    }

    #[test]
    fn rejects_truncated_file_name_bytes() {
        let mut entries = Vec::new();
        let mut buffer = directory_record(&[], 0);
        write_u32(
            &mut buffer,
            offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength),
            size_of::<u16>() as u32,
        );

        let error = parse_directory_buffer(&buffer, &mut entries)
            .expect_err("declared name bytes must be present");

        assert_eq!(
            error,
            "P1:3 directory enumeration returned an invalid name length"
        );
    }

    #[test]
    fn rejects_odd_length_utf16_names() {
        let mut entries = Vec::new();
        let mut buffer = directory_record(&[], 0);
        write_u32(
            &mut buffer,
            offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength),
            1,
        );

        let error = parse_directory_buffer(&buffer, &mut entries)
            .expect_err("UTF-16 names must have an even byte length");

        assert_eq!(
            error,
            "P1:3 directory enumeration returned an odd UTF-16 name length"
        );
    }

    #[test]
    fn rejects_next_offsets_beyond_the_buffer() {
        let mut entries = Vec::new();
        let buffer = directory_record(
            &[u16::from(b'.')],
            u32::try_from(header_size() + 4).expect("test offset fits in u32"),
        );

        let error = parse_directory_buffer(&buffer, &mut entries)
            .expect_err("the next record must remain inside the returned buffer");

        assert_eq!(
            error,
            "P1:3 directory enumeration next offset exceeds the returned buffer"
        );
    }

    #[test]
    fn rejects_next_offsets_inside_the_declared_name() {
        let mut entries = Vec::new();
        let buffer = directory_record(
            &[u16::from(b'x'), u16::from(b'y')],
            u32::try_from(header_size() + size_of::<u16>()).expect("test offset fits in u32"),
        );

        let error = parse_directory_buffer(&buffer, &mut entries)
            .expect_err("the next record must not overlap the current name");

        assert_eq!(
            error,
            "P1:3 directory enumeration next offset overlaps the current record"
        );
        assert!(entries.is_empty());
    }

    #[test]
    fn exact_end_next_offset_is_not_treated_as_an_in_bounds_record() {
        let mut entries = Vec::new();
        let mut buffer = directory_record(&[u16::from(b'x')], 0);
        let offset = u32::try_from(buffer.len()).expect("test offset fits in u32");
        write_u32(
            &mut buffer,
            offset_of!(FILE_ID_BOTH_DIR_INFO, NextEntryOffset),
            offset,
        );

        let error = parse_directory_buffer(&buffer, &mut entries)
            .expect_err("a next offset at the buffer end has no complete next record");

        assert_eq!(
            error,
            "P1:3 directory enumeration returned a truncated record"
        );
        assert!(entries.is_empty());
    }

    #[test]
    fn rejects_next_offsets_one_past_the_returned_buffer() {
        let mut entries = Vec::new();
        let mut buffer = directory_record(&[u16::from(b'x')], 0);
        let offset = u32::try_from(buffer.len() + 1).expect("test offset fits in u32");
        write_u32(
            &mut buffer,
            offset_of!(FILE_ID_BOTH_DIR_INFO, NextEntryOffset),
            offset,
        );

        let error = parse_directory_buffer(&buffer, &mut entries)
            .expect_err("the next record must stay within the returned buffer");

        assert_eq!(
            error,
            "P1:3 directory enumeration next offset exceeds the returned buffer"
        );
        assert!(entries.is_empty());
    }

    #[test]
    fn rejects_a_later_bad_record_without_appending_earlier_entries() {
        let first = directory_record(&[u16::from(b'x')], 0);
        let mut buffer = first.clone();
        write_u32(
            &mut buffer,
            offset_of!(FILE_ID_BOTH_DIR_INFO, NextEntryOffset),
            u32::try_from(first.len()).expect("test offset fits in u32"),
        );
        buffer.extend_from_slice(&directory_record(&[], 0));
        let mut entries = vec![DirectoryEntry {
            id: 1,
            name: vec![u16::from(b's')],
            attributes: 2,
        }];

        let error = parse_directory_buffer(&buffer, &mut entries)
            .expect_err("an invalid later record must roll back the whole buffer");

        assert_eq!(error, "P1:3 directory entry has a hostile or alias name");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, 1);
        assert_eq!(entries[0].name, [u16::from(b's')]);
        assert_eq!(entries[0].attributes, 2);
    }

    #[test]
    fn parses_a_valid_record_from_a_deliberately_misaligned_slice() {
        let record = directory_record(&[u16::from(b'x')], 0);
        let mut padded = vec![0_u8];
        padded.extend_from_slice(&record);
        let mut entries = Vec::new();

        parse_directory_buffer(&padded[1..], &mut entries)
            .expect("byte-oriented parsing must not require u16 alignment");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, 0x0123_4567_89AB_CDEF);
        assert_eq!(entries[0].attributes, 0x1234_5678);
        assert_eq!(entries[0].name, [u16::from(b'x')]);
    }
}
