use crate::{NorthclockError, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RomInspection {
    pub image_bytes: usize,
    pub declared_image_bytes: usize,
    pub checksum_valid: bool,
    pub pci_data_offset: Option<usize>,
    pub pci_vendor_id: Option<u16>,
    pub pci_device_id: Option<u16>,
    pub has_atom_bios_marker: bool,
    pub write_supported: bool,
}

pub fn inspect_rom(bytes: &[u8]) -> Result<RomInspection> {
    if bytes.len() < 0x1a {
        return Err(NorthclockError::HardwareOperation(
            "ROM image is too short for a PCI expansion header".into(),
        ));
    }
    if bytes[0] != 0x55 || bytes[1] != 0xaa {
        return Err(NorthclockError::HardwareOperation(
            "ROM image does not start with the 0x55AA signature".into(),
        ));
    }
    let declared_image_bytes = usize::from(bytes[2]) * 512;
    if declared_image_bytes == 0 || declared_image_bytes > bytes.len() {
        return Err(NorthclockError::HardwareOperation(
            "ROM header declares an image beyond the supplied buffer".into(),
        ));
    }

    let pci_offset = usize::from(u16::from_le_bytes([bytes[0x18], bytes[0x19]]));
    let (pci_data_offset, pci_vendor_id, pci_device_id) = if pci_offset
        .checked_add(8)
        .is_some_and(|end| end <= bytes.len())
        && bytes.get(pci_offset..pci_offset + 4) == Some(b"PCIR")
    {
        (
            Some(pci_offset),
            Some(u16::from_le_bytes([
                bytes[pci_offset + 4],
                bytes[pci_offset + 5],
            ])),
            Some(u16::from_le_bytes([
                bytes[pci_offset + 6],
                bytes[pci_offset + 7],
            ])),
        )
    } else {
        (None, None, None)
    };

    let checksum_valid = bytes[..declared_image_bytes]
        .iter()
        .fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
        == 0;
    let has_atom_bios_marker = bytes.windows(8).any(|window| window == b"ATOMBIOS");

    Ok(RomInspection {
        image_bytes: bytes.len(),
        declared_image_bytes,
        checksum_valid,
        pci_data_offset,
        pci_vendor_id,
        pci_device_id,
        has_atom_bios_marker,
        write_supported: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_rom() -> Vec<u8> {
        let mut bytes = vec![0_u8; 512];
        bytes[0] = 0x55;
        bytes[1] = 0xaa;
        bytes[2] = 1;
        bytes[0x18] = 0x20;
        bytes[0x20..0x24].copy_from_slice(b"PCIR");
        bytes[0x24..0x26].copy_from_slice(&0x1002_u16.to_le_bytes());
        bytes[0x26..0x28].copy_from_slice(&0x73bf_u16.to_le_bytes());
        let checksum = bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte));
        bytes[511] = 0_u8.wrapping_sub(checksum);
        bytes
    }

    #[test]
    fn parses_bounded_pci_header() {
        let report = inspect_rom(&minimal_rom())
            .unwrap_or_else(|error| panic!("valid ROM rejected: {error}"));
        assert!(report.checksum_valid);
        assert_eq!(report.pci_vendor_id, Some(0x1002));
        assert!(!report.write_supported);
    }

    #[test]
    fn rejects_truncated_declared_image() {
        let mut bytes = minimal_rom();
        bytes[2] = 2;
        let error = inspect_rom(&bytes)
            .err()
            .unwrap_or_else(|| panic!("truncated ROM unexpectedly succeeded"));
        assert_eq!(error.exit_code(), 5);
    }
}
