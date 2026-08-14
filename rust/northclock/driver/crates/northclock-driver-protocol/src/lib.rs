#![no_std]
#![forbid(unsafe_code)]

pub const PROTOCOL_MAGIC: u32 = u32::from_le_bytes(*b"NCLK");
pub const PROTOCOL_VERSION: u16 = 1;
pub const CURVE_OPTIMIZER_MIN: i16 = -50;
pub const CURVE_OPTIMIZER_MAX: i16 = 50;
pub const MAX_LOGICAL_CORE_INDEX: u16 = 511;
pub const RYZEN_7000_DESKTOP_TABLE_VERSION: u8 = 1;
pub const PROTOCOL_HEADER_WIRE_SIZE: u16 = 16;
pub const CURVE_OPTIMIZER_REQUEST_WIRE_SIZE: u16 = 42;
pub const CURVE_OPTIMIZER_RESPONSE_WIRE_SIZE: u16 = 28;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    WrongLength,
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    QueryCapabilities = 1,
    CaptureCurveOptimizerState = 2,
    ApplyCurveOptimizer = 3,
    RestoreCurveOptimizerState = 4,
    QueryWatchdog = 5,
}

impl Command {
    #[must_use]
    pub const fn wire_value(self) -> u16 {
        self as u16
    }
}

impl TryFrom<u16> for Command {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::QueryCapabilities),
            2 => Ok(Self::CaptureCurveOptimizerState),
            3 => Ok(Self::ApplyCurveOptimizer),
            4 => Ok(Self::RestoreCurveOptimizerState),
            5 => Ok(Self::QueryWatchdog),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolHeader {
    pub magic: u32,
    pub version: u16,
    pub structure_size: u16,
    pub sequence: u64,
}

impl ProtocolHeader {
    #[must_use]
    pub const fn new(structure_size: u16, sequence: u64) -> Self {
        Self {
            magic: PROTOCOL_MAGIC,
            version: PROTOCOL_VERSION,
            structure_size,
            sequence,
        }
    }

    #[must_use]
    pub const fn valid_for_size(&self, expected_size: u16) -> bool {
        self.magic == PROTOCOL_MAGIC
            && self.version == PROTOCOL_VERSION
            && self.structure_size == expected_size
            && self.sequence != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessorModel {
    pub vendor: [u8; 12],
    pub family: u8,
    pub model: u8,
    pub stepping: u8,
    pub protocol_table_version: u8,
}

impl ProcessorModel {
    /// Returns true only for a protocol table explicitly registered in this
    /// crate. This is a dispatch whitelist, not a physical-support claim.
    #[must_use]
    pub fn has_registered_protocol_table(&self) -> bool {
        self.vendor == *b"AuthenticAMD"
            && self.family == 0x19
            && self.model == 0x61
            && self.protocol_table_version == RYZEN_7000_DESKTOP_TABLE_VERSION
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CurveOptimizerRequest {
    pub header: ProtocolHeader,
    /// Raw wire value. Convert with `Command::try_from` before dispatch.
    pub command: u16,
    pub processor: ProcessorModel,
    pub logical_core_index: u16,
    pub offset_steps: i16,
    pub watchdog_timeout_ms: u32,
}

impl CurveOptimizerRequest {
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() != usize::from(CURVE_OPTIMIZER_REQUEST_WIRE_SIZE) {
            return Err(DecodeError::WrongLength);
        }
        let mut vendor = [0_u8; 12];
        vendor.copy_from_slice(&bytes[18..30]);
        Ok(Self {
            header: ProtocolHeader {
                magic: read_u32(bytes, 0),
                version: read_u16(bytes, 4),
                structure_size: read_u16(bytes, 6),
                sequence: read_u64(bytes, 8),
            },
            command: read_u16(bytes, 16),
            processor: ProcessorModel {
                vendor,
                family: bytes[30],
                model: bytes[31],
                stepping: bytes[32],
                protocol_table_version: bytes[33],
            },
            logical_core_index: read_u16(bytes, 34),
            offset_steps: read_i16(bytes, 36),
            watchdog_timeout_ms: read_u32(bytes, 38),
        })
    }

    #[must_use]
    pub fn encode(&self) -> [u8; CURVE_OPTIMIZER_REQUEST_WIRE_SIZE as usize] {
        let mut bytes = [0_u8; CURVE_OPTIMIZER_REQUEST_WIRE_SIZE as usize];
        write_header(&mut bytes, self.header);
        bytes[16..18].copy_from_slice(&self.command.to_le_bytes());
        bytes[18..30].copy_from_slice(&self.processor.vendor);
        bytes[30] = self.processor.family;
        bytes[31] = self.processor.model;
        bytes[32] = self.processor.stepping;
        bytes[33] = self.processor.protocol_table_version;
        bytes[34..36].copy_from_slice(&self.logical_core_index.to_le_bytes());
        bytes[36..38].copy_from_slice(&self.offset_steps.to_le_bytes());
        bytes[38..42].copy_from_slice(&self.watchdog_timeout_ms.to_le_bytes());
        bytes
    }

    pub fn command(&self) -> Option<Command> {
        Command::try_from(self.command).ok()
    }

    #[must_use]
    pub fn has_supported_command(&self) -> bool {
        matches!(
            self.command(),
            Some(Command::CaptureCurveOptimizerState)
                | Some(Command::ApplyCurveOptimizer)
                | Some(Command::RestoreCurveOptimizerState)
        )
    }

    #[must_use]
    pub fn values_are_bounded(&self) -> bool {
        self.logical_core_index <= MAX_LOGICAL_CORE_INDEX
            && self.offset_steps >= CURVE_OPTIMIZER_MIN
            && self.offset_steps <= CURVE_OPTIMIZER_MAX
            && self.watchdog_timeout_ms >= 1_000
            && self.watchdog_timeout_ms <= 60_000
    }

    #[must_use]
    pub fn validate(&self) -> bool {
        self.header
            .valid_for_size(CURVE_OPTIMIZER_REQUEST_WIRE_SIZE)
            && self.has_supported_command()
            && self.values_are_bounded()
            && self.processor.has_registered_protocol_table()
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolStatus {
    Success = 0,
    InvalidHeader = 1,
    UnsupportedProcessor = 2,
    ValueOutOfBounds = 3,
    PermissionDenied = 4,
    RateLimited = 5,
    WatchdogArmed = 6,
    ReadbackMismatch = 7,
    RestoreFailed = 8,
    UnsupportedCommand = 9,
    InvalidBuffer = 10,
}

impl ProtocolStatus {
    #[must_use]
    pub const fn wire_value(self) -> u32 {
        self as u32
    }
}

impl TryFrom<u32> for ProtocolStatus {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Success),
            1 => Ok(Self::InvalidHeader),
            2 => Ok(Self::UnsupportedProcessor),
            3 => Ok(Self::ValueOutOfBounds),
            4 => Ok(Self::PermissionDenied),
            5 => Ok(Self::RateLimited),
            6 => Ok(Self::WatchdogArmed),
            7 => Ok(Self::ReadbackMismatch),
            8 => Ok(Self::RestoreFailed),
            9 => Ok(Self::UnsupportedCommand),
            10 => Ok(Self::InvalidBuffer),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CurveOptimizerResponse {
    pub header: ProtocolHeader,
    /// Raw `ProtocolStatus` wire value.
    pub status: u32,
    pub captured_offset_steps: i16,
    pub readback_offset_steps: i16,
    pub reserved: u32,
}

impl CurveOptimizerResponse {
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() != usize::from(CURVE_OPTIMIZER_RESPONSE_WIRE_SIZE) {
            return Err(DecodeError::WrongLength);
        }
        Ok(Self {
            header: ProtocolHeader {
                magic: read_u32(bytes, 0),
                version: read_u16(bytes, 4),
                structure_size: read_u16(bytes, 6),
                sequence: read_u64(bytes, 8),
            },
            status: read_u32(bytes, 16),
            captured_offset_steps: read_i16(bytes, 20),
            readback_offset_steps: read_i16(bytes, 22),
            reserved: read_u32(bytes, 24),
        })
    }

    #[must_use]
    pub fn encode(&self) -> [u8; CURVE_OPTIMIZER_RESPONSE_WIRE_SIZE as usize] {
        let mut bytes = [0_u8; CURVE_OPTIMIZER_RESPONSE_WIRE_SIZE as usize];
        write_header(&mut bytes, self.header);
        bytes[16..20].copy_from_slice(&self.status.to_le_bytes());
        bytes[20..22].copy_from_slice(&self.captured_offset_steps.to_le_bytes());
        bytes[22..24].copy_from_slice(&self.readback_offset_steps.to_le_bytes());
        bytes[24..28].copy_from_slice(&self.reserved.to_le_bytes());
        bytes
    }
}

fn write_header(bytes: &mut [u8], header: ProtocolHeader) {
    bytes[0..4].copy_from_slice(&header.magic.to_le_bytes());
    bytes[4..6].copy_from_slice(&header.version.to_le_bytes());
    bytes[6..8].copy_from_slice(&header.structure_size.to_le_bytes());
    bytes[8..16].copy_from_slice(&header.sequence.to_le_bytes());
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_i16(bytes: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(offset_steps: i16) -> CurveOptimizerRequest {
        CurveOptimizerRequest {
            header: ProtocolHeader::new(CURVE_OPTIMIZER_REQUEST_WIRE_SIZE, 1),
            command: Command::ApplyCurveOptimizer.wire_value(),
            processor: ProcessorModel {
                vendor: *b"AuthenticAMD",
                family: 0x19,
                model: 0x61,
                stepping: 2,
                protocol_table_version: 1,
            },
            logical_core_index: 0,
            offset_steps,
            watchdog_timeout_ms: 5_000,
        }
    }

    #[test]
    fn accepts_bounded_versioned_request() {
        let request = request(-20);
        assert!(request.validate());
        assert_eq!(
            CurveOptimizerRequest::decode(&request.encode()),
            Ok(request)
        );
    }

    #[test]
    fn rejects_generic_or_unbounded_request_shapes() {
        assert!(!request(-51).validate());
        let mut invalid = request(-20);
        invalid.header.structure_size = 0;
        assert!(!invalid.validate());
        invalid = request(-20);
        invalid.command = u16::MAX;
        assert!(!invalid.validate());
        invalid = request(-20);
        invalid.processor.model = 0x60;
        assert!(!invalid.validate());
    }

    #[test]
    fn protocol_has_no_arbitrary_access_commands() {
        let commands = [
            Command::QueryCapabilities,
            Command::CaptureCurveOptimizerState,
            Command::ApplyCurveOptimizer,
            Command::RestoreCurveOptimizerState,
            Command::QueryWatchdog,
        ];
        assert_eq!(commands.len(), 5);
    }

    #[test]
    fn decoding_rejects_truncated_and_extended_requests() {
        assert_eq!(
            CurveOptimizerRequest::decode(&[0_u8; 41]),
            Err(DecodeError::WrongLength)
        );
        assert_eq!(
            CurveOptimizerRequest::decode(&[0_u8; 43]),
            Err(DecodeError::WrongLength)
        );
    }

    #[test]
    fn response_uses_the_explicit_wire_layout() {
        let response = CurveOptimizerResponse {
            header: ProtocolHeader::new(CURVE_OPTIMIZER_RESPONSE_WIRE_SIZE, 9),
            status: ProtocolStatus::ReadbackMismatch.wire_value(),
            captured_offset_steps: -10,
            readback_offset_steps: -8,
            reserved: 0,
        };

        assert_eq!(
            CurveOptimizerResponse::decode(&response.encode()),
            Ok(response)
        );
        assert_eq!(
            CurveOptimizerResponse::decode(&[0_u8; 27]),
            Err(DecodeError::WrongLength)
        );
        assert_eq!(
            CurveOptimizerResponse::decode(&[0_u8; 29]),
            Err(DecodeError::WrongLength)
        );
    }

    #[test]
    fn every_protocol_status_has_a_stable_wire_value() {
        for status in [
            ProtocolStatus::Success,
            ProtocolStatus::InvalidHeader,
            ProtocolStatus::UnsupportedProcessor,
            ProtocolStatus::ValueOutOfBounds,
            ProtocolStatus::PermissionDenied,
            ProtocolStatus::RateLimited,
            ProtocolStatus::WatchdogArmed,
            ProtocolStatus::ReadbackMismatch,
            ProtocolStatus::RestoreFailed,
            ProtocolStatus::UnsupportedCommand,
            ProtocolStatus::InvalidBuffer,
        ] {
            assert_eq!(ProtocolStatus::try_from(status.wire_value()), Ok(status));
        }
        assert!(ProtocolStatus::try_from(u32::MAX).is_err());
    }
}
