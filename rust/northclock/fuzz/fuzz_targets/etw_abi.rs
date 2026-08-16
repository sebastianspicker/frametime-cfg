#![no_main]

use libfuzzer_sys::fuzz_target;
use northclock_platform_windows::{validate_etw_present_header, EtwPresentHeaderFields};

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() < 28 {
        return;
    }
    let u16_at = |offset| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
    let u32_at = |offset| {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    let i64_at = |offset| {
        i64::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ])
    };
    let _ = validate_etw_present_header(EtwPresentHeaderFields {
        total_size: usize::from(u16_at(0)),
        header_size: usize::from(u16_at(2)),
        user_data_length: usize::from(u16_at(4)),
        user_data_present: bytes[6] & 1 != 0,
        provider_matches: bytes[7] & 1 != 0,
        event_id: u16_at(8),
        expected_event_id: u16_at(10),
        process_id: u32_at(12),
        timestamp_100ns: i64_at(16),
        minimum_timestamp_100ns: i64::from(u32_at(24)),
    });
});
