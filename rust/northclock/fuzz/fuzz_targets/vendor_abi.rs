#![no_main]

use libfuzzer_sys::fuzz_target;
use northclock_platform_windows::{
    validate_nvapi_load_fields, validate_nvapi_temperature_fields, validate_nvapi_thermal_header,
};

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() < 28 {
        return;
    }
    let u32_at = |offset| {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    let _ = validate_nvapi_load_fields(u32_at(0), u32_at(4), u32_at(8), u32_at(12));
    let _ = validate_nvapi_thermal_header(
        u32_at(0),
        u32_at(4),
        u32_at(8) as usize,
        u32_at(12) as usize,
    );
    let _ =
        validate_nvapi_temperature_fields(u32_at(16) as i32, u32_at(20) as i32, u32_at(24) as i32);
});
