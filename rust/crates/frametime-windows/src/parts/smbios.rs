/// Immutable SMBIOS memory-topology evidence for P1:24.  This deliberately
/// records firmware associations only: it does not infer a memory-controller
/// mode from slot labels, locator text, or the number of installed modules.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryChannel {
    handle: u16,
    device_handles: Vec<u16>,
}

/// A complete usable Type 37 mapping for every populated Type 17 device.
/// It is inspection evidence only and is never backed up or persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryTopology {
    channel_count: u16,
    populated_channels: Vec<MemoryChannel>,
}

const RAW_SMBIOS_HEADER_SIZE: usize = 8;
#[cfg(windows)]
const MAX_RAW_SMBIOS_BYTES: usize = 16 * 1024 * 1024;

fn capture_memory_topology() -> Result<Option<MemoryTopology>, String> {
    parse_memory_topology(&read_raw_smbios()?)
}

fn verify_memory_topology(captured: &Option<MemoryTopology>) -> Result<(), String> {
    let observed = capture_memory_topology()?;
    if captured == &observed {
        Ok(())
    } else {
        Err("P1:24 SMBIOS memory-topology evidence changed after inspection".into())
    }
}

/// Parse only the SMBIOS structures relevant to factual physical-memory
/// membership: Type 17 devices and Type 37 channels.  Every structure is
/// still bounds-checked and must include its required double-NUL string-set
/// terminator so an ignored structure cannot desynchronise the walk.
fn parse_memory_topology(raw: &[u8]) -> Result<Option<MemoryTopology>, String> {
    let table = raw_smbios_table(raw)?;
    let mut devices = std::collections::BTreeMap::<u16, bool>::new();
    let mut channels = Vec::<MemoryChannel>::new();
    let mut seen_selected_handles = std::collections::BTreeSet::new();
    let mut offset = 0;

    while offset < table.len() {
        let structure = next_smbios_structure(table, &mut offset)?;
        let kind = structure[0];
        let handle = u16::from_le_bytes([structure[2], structure[3]]);
        match kind {
            17 => {
                if !seen_selected_handles.insert(handle) || devices.contains_key(&handle) {
                    return Err("P1:24 SMBIOS has duplicate selected structure handles".into());
                }
                devices.insert(handle, type17_is_populated(structure)?);
            }
            37 => {
                if !seen_selected_handles.insert(handle) {
                    return Err("P1:24 SMBIOS has duplicate selected structure handles".into());
                }
                channels.push(parse_type37_channel(handle, structure)?);
            }
            _ => {}
        }
    }

    if channels.is_empty() {
        return Ok(None);
    }
    let channel_count = u16::try_from(channels.len())
        .map_err(|_| "P1:24 SMBIOS channel count exceeds u16")?;
    let mut membership = std::collections::BTreeSet::new();
    let mut populated_channels = Vec::new();
    for channel in channels {
        let mut populated = Vec::new();
        for device_handle in channel.device_handles {
            let populated_device = devices.get(&device_handle).ok_or(
                "P1:24 SMBIOS memory channel references a missing Type 17 device",
            )?;
            if !membership.insert(device_handle) {
                return Err("P1:24 SMBIOS has duplicate Type 17 channel membership".into());
            }
            if *populated_device {
                populated.push(device_handle);
            }
        }
        if !populated.is_empty() {
            populated_channels.push(MemoryChannel {
                handle: channel.handle,
                device_handles: populated,
            });
        }
    }

    let every_populated_device_is_mapped = devices
        .iter()
        .filter(|(_, populated)| **populated)
        .all(|(handle, _)| membership.contains(handle));
    if populated_channels.is_empty() || !every_populated_device_is_mapped {
        return Ok(None);
    }
    Ok(Some(MemoryTopology {
        channel_count,
        populated_channels,
    }))
}

fn raw_smbios_table(raw: &[u8]) -> Result<&[u8], String> {
    if raw.len() < RAW_SMBIOS_HEADER_SIZE {
        return Err("P1:24 RawSMBIOSData header is truncated".into());
    }
    let declared = u32::from_le_bytes(
        raw[4..RAW_SMBIOS_HEADER_SIZE]
            .try_into()
            .map_err(|_| "P1:24 RawSMBIOSData length is malformed")?,
    );
    let declared = usize::try_from(declared)
        .map_err(|_| "P1:24 RawSMBIOSData length overflows usize")?;
    if declared != raw.len() - RAW_SMBIOS_HEADER_SIZE {
        return Err("P1:24 RawSMBIOSData length does not exactly match the buffer".into());
    }
    Ok(&raw[RAW_SMBIOS_HEADER_SIZE..])
}

fn next_smbios_structure<'a>(table: &'a [u8], offset: &mut usize) -> Result<&'a [u8], String> {
    let start = *offset;
    let header_end = start
        .checked_add(4)
        .filter(|end| *end <= table.len())
        .ok_or("P1:24 SMBIOS structure header is truncated")?;
    let formatted_length = usize::from(table[start + 1]);
    if formatted_length < 4 {
        return Err("P1:24 SMBIOS structure has an invalid formatted length".into());
    }
    let formatted_end = start
        .checked_add(formatted_length)
        .filter(|end| *end <= table.len())
        .ok_or("P1:24 SMBIOS formatted structure is truncated")?;
    let strings = &table[formatted_end..];
    let terminator = strings
        .windows(2)
        .position(|pair| pair == [0, 0])
        .ok_or("P1:24 SMBIOS structure is missing its double-NUL terminator")?;
    let end = formatted_end
        .checked_add(terminator)
        .and_then(|position| position.checked_add(2))
        .ok_or("P1:24 SMBIOS structure length overflows usize")?;
    debug_assert_eq!(header_end, start + 4);
    *offset = end;
    Ok(&table[start..formatted_end])
}

fn type17_is_populated(structure: &[u8]) -> Result<bool, String> {
    if structure.len() < 14 {
        return Err("P1:24 SMBIOS Type 17 is truncated before its Size field".into());
    }
    let size = u16::from_le_bytes([structure[12], structure[13]]);
    match size {
        0 | u16::MAX => Ok(false),
        0x7fff => {
            if structure.len() < 32 {
                return Err("P1:24 SMBIOS Type 17 extended Size field is truncated".into());
            }
            let extended = u32::from_le_bytes(
                structure[28..32]
                    .try_into()
                    .map_err(|_| "P1:24 SMBIOS Type 17 extended Size is malformed")?,
            );
            if extended == 0 {
                return Err("P1:24 SMBIOS Type 17 has a zero extended Size".into());
            }
            Ok(true)
        }
        _ => Ok(true),
    }
}

fn parse_type37_channel(handle: u16, structure: &[u8]) -> Result<MemoryChannel, String> {
    if structure.len() < 7 {
        return Err("P1:24 SMBIOS Type 37 is truncated before its device count".into());
    }
    let count = usize::from(structure[6]);
    let required = 7usize
        .checked_add(
            count
                .checked_mul(3)
                .ok_or("P1:24 SMBIOS Type 37 device count overflows")?,
        )
        .ok_or("P1:24 SMBIOS Type 37 length overflows")?;
    if structure.len() < required {
        return Err("P1:24 SMBIOS Type 37 device associations are truncated".into());
    }
    let mut device_handles = Vec::with_capacity(count);
    for index in 0..count {
        let start = 7 + index * 3;
        device_handles.push(u16::from_le_bytes([
            structure[start + 1],
            structure[start + 2],
        ]));
    }
    Ok(MemoryChannel {
        handle,
        device_handles,
    })
}

#[cfg(windows)]
fn read_raw_smbios() -> Result<Vec<u8>, String> {
    use windows::Win32::System::SystemInformation::{GetSystemFirmwareTable, RSMB};

    let required = unsafe { GetSystemFirmwareTable(RSMB, 0, None) };
    let required = usize::try_from(required)
        .map_err(|_| "P1:24 SMBIOS size query overflows usize")?;
    if required < RAW_SMBIOS_HEADER_SIZE {
        return Err("P1:24 SMBIOS firmware-table size query failed or was truncated".into());
    }
    if required > MAX_RAW_SMBIOS_BYTES {
        return Err("P1:24 SMBIOS firmware table exceeds the 16 MiB safety ceiling".into());
    }
    let mut raw = vec![0; required];
    let read = unsafe { GetSystemFirmwareTable(RSMB, 0, Some(&mut raw)) };
    if usize::try_from(read).ok() != Some(required) {
        return Err("P1:24 SMBIOS firmware-table read did not exactly match its size query".into());
    }
    Ok(raw)
}

#[cfg(not(windows))]
fn read_raw_smbios() -> Result<Vec<u8>, String> {
    Err("P1:24 SMBIOS firmware topology observation is supported only on Windows".into())
}

#[cfg(test)]
mod smbios_tests {
    use super::*;

    fn raw(structures: Vec<Vec<u8>>) -> Vec<u8> {
        let table = structures.into_iter().flatten().collect::<Vec<_>>();
        let mut raw = vec![0, 3, 6, 0];
        raw.extend(u32::try_from(table.len()).expect("small test table").to_le_bytes());
        raw.extend(table);
        raw
    }

    fn structure(kind: u8, handle: u16, formatted: &[u8]) -> Vec<u8> {
        let mut structure = vec![kind, u8::try_from(formatted.len() + 4).expect("small")];
        structure.extend(handle.to_le_bytes());
        structure.extend(formatted);
        structure.extend([0, 0]);
        structure
    }

    fn type17(handle: u16, size: u16) -> Vec<u8> {
        let mut formatted = vec![0; 10];
        formatted[8..10].copy_from_slice(&size.to_le_bytes());
        structure(17, handle, &formatted)
    }

    fn type37(handle: u16, device_handles: &[u16]) -> Vec<u8> {
        let mut formatted = vec![0, 0, u8::try_from(device_handles.len()).expect("small")];
        for device in device_handles {
            formatted.extend([0]);
            formatted.extend(device.to_le_bytes());
        }
        structure(37, handle, &formatted)
    }

    #[test]
    fn parses_complete_populated_channel_membership_without_inferring_mode() {
        let observation = parse_memory_topology(&raw(vec![
            type17(0x1100, 8192),
            type17(0x1101, 8192),
            type37(0x1200, &[0x1100]),
            type37(0x1201, &[0x1101]),
        ]))
        .expect("valid topology")
        .expect("usable mapping");
        assert_eq!(observation.channel_count, 2);
        assert_eq!(
            observation.populated_channels,
            vec![
                MemoryChannel { handle: 0x1200, device_handles: vec![0x1100] },
                MemoryChannel { handle: 0x1201, device_handles: vec![0x1101] },
            ]
        );
        assert!(observation.populated_channels.iter().all(|channel| !channel.device_handles.is_empty()));
    }

    #[test]
    fn absent_or_unusable_channel_mapping_is_inapplicable() {
        assert_eq!(parse_memory_topology(&raw(vec![type17(0x1100, 8192)])), Ok(None));
        assert_eq!(
            parse_memory_topology(&raw(vec![type17(0x1100, 8192), type37(0x1200, &[])])),
            Ok(None)
        );
        assert_eq!(
            parse_memory_topology(&raw(vec![type17(0x1100, 0), type37(0x1200, &[0x1100])])),
            Ok(None)
        );
    }

    #[test]
    fn rejects_truncation_duplicate_handles_dangling_and_duplicate_membership() {
        let mut missing_terminator = type17(0x1100, 8192);
        missing_terminator.truncate(missing_terminator.len() - 2);
        let truncated_raw = raw(vec![missing_terminator]);
        assert!(parse_memory_topology(&truncated_raw).is_err());
        assert!(parse_memory_topology(&raw(vec![type17(0x1100, 8192), type17(0x1100, 8192)])).is_err());
        assert!(parse_memory_topology(&raw(vec![type37(0x1200, &[0x1100])])).is_err());
        assert!(parse_memory_topology(&raw(vec![
            type17(0x1100, 8192),
            type37(0x1200, &[0x1100]),
            type37(0x1201, &[0x1100]),
        ]))
        .is_err());
    }

    #[test]
    fn rejects_raw_header_and_type37_length_mismatches() {
        assert!(parse_memory_topology(&[0; RAW_SMBIOS_HEADER_SIZE - 1]).is_err());
        let mut declared = raw(vec![type17(0x1100, 8192)]);
        declared[4..8].copy_from_slice(&999_u32.to_le_bytes());
        assert!(parse_memory_topology(&declared).is_err());
        let malformed = structure(37, 0x1200, &[0, 0, 1]);
        assert!(parse_memory_topology(&raw(vec![malformed])).is_err());
    }
}
