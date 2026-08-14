//! Minimal NVAPI DRS ABI mirrored from NVIDIA's MIT-licensed public SDK.
//!
//! Source snapshot: NVIDIA/nvapi commit
//! `cd6918f60b3c9a0476fdfe7e89bb32330602049d` (`nvapi.h` and
//! `nvapi_interface.h`). Keeping the layouts here makes the Windows build
//! deterministic without running bindgen or linking an import library.

const NVAPI_UNICODE_UNITS: usize = 2_048;
const NVDRS_SETTING_BYTES: usize = 12_328;
const SETTING_ID_OFFSET: usize = 4_100;
const SETTING_TYPE_OFFSET: usize = 4_104;
const SETTING_CURRENT_VALUE_OFFSET: usize = 8_224;

pub(super) const NVDRS_DWORD_TYPE: u32 = 0;

#[repr(C, align(4))]
pub(super) struct NvDrsProfile {
    pub(super) version: u32,
    pub(super) profile_name: [u16; NVAPI_UNICODE_UNITS],
    pub(super) gpu_support: u32,
    pub(super) is_predefined: u32,
    pub(super) application_count: u32,
    pub(super) setting_count: u32,
}

impl NvDrsProfile {
    pub(super) fn named(name: &str) -> Result<Self, String> {
        let mut value = Self {
            version: version::<Self>(1)?,
            profile_name: [0; NVAPI_UNICODE_UNITS],
            gpu_support: 0,
            is_predefined: 0,
            application_count: 0,
            setting_count: 0,
        };
        write_unicode(&mut value.profile_name, name)?;
        Ok(value)
    }

    pub(super) fn query() -> Result<Self, String> {
        Self::named("")
    }

    pub(super) fn name(&self) -> Result<String, String> {
        read_unicode(&self.profile_name)
    }
}

#[repr(C, align(4))]
pub(super) struct NvDrsApplicationV4 {
    pub(super) version: u32,
    pub(super) is_predefined: u32,
    pub(super) application_name: [u16; NVAPI_UNICODE_UNITS],
    pub(super) friendly_name: [u16; NVAPI_UNICODE_UNITS],
    pub(super) launcher: [u16; NVAPI_UNICODE_UNITS],
    pub(super) file_in_folder: [u16; NVAPI_UNICODE_UNITS],
    pub(super) flags: u32,
    pub(super) command_line: [u16; NVAPI_UNICODE_UNITS],
}

impl NvDrsApplicationV4 {
    pub(super) fn named(name: &str) -> Result<Self, String> {
        let mut value = Self {
            version: version::<Self>(4)?,
            is_predefined: 0,
            application_name: [0; NVAPI_UNICODE_UNITS],
            friendly_name: [0; NVAPI_UNICODE_UNITS],
            launcher: [0; NVAPI_UNICODE_UNITS],
            file_in_folder: [0; NVAPI_UNICODE_UNITS],
            flags: 0,
            command_line: [0; NVAPI_UNICODE_UNITS],
        };
        write_unicode(&mut value.application_name, name)?;
        Ok(value)
    }
}

#[repr(C, align(4))]
pub(super) struct NvDrsSetting {
    bytes: [u8; NVDRS_SETTING_BYTES],
}

impl NvDrsSetting {
    pub(super) fn query() -> Result<Self, String> {
        let mut value = Self {
            bytes: [0; NVDRS_SETTING_BYTES],
        };
        value.write_u32(0, version::<Self>(1)?);
        Ok(value)
    }

    pub(super) fn dword(id: u32, value: u32) -> Result<Self, String> {
        let mut setting = Self::query()?;
        setting.write_u32(SETTING_ID_OFFSET, id);
        setting.write_u32(SETTING_TYPE_OFFSET, NVDRS_DWORD_TYPE);
        setting.write_u32(SETTING_CURRENT_VALUE_OFFSET, value);
        Ok(setting)
    }

    pub(super) fn setting_type(&self) -> u32 {
        self.read_u32(SETTING_TYPE_OFFSET)
    }

    pub(super) fn current_dword(&self) -> u32 {
        self.read_u32(SETTING_CURRENT_VALUE_OFFSET)
    }

    fn write_u32(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn read_u32(&self, offset: usize) -> u32 {
        u32::from_le_bytes(
            self.bytes[offset..offset + 4]
                .try_into()
                .expect("four bytes"),
        )
    }
}

fn version<T>(revision: u32) -> Result<u32, String> {
    let size =
        u32::try_from(std::mem::size_of::<T>()).map_err(|_| "NVAPI structure size exceeds u32")?;
    Ok(size | (revision << 16))
}

pub(super) fn unicode_argument(value: &str) -> Result<[u16; NVAPI_UNICODE_UNITS], String> {
    let mut result = [0; NVAPI_UNICODE_UNITS];
    write_unicode(&mut result, value)?;
    Ok(result)
}

fn write_unicode(target: &mut [u16; NVAPI_UNICODE_UNITS], value: &str) -> Result<(), String> {
    let units = value.encode_utf16().collect::<Vec<_>>();
    if units.is_empty() && !value.is_empty() {
        return Err("NVAPI string is not valid UTF-16".into());
    }
    if units.len() >= target.len() || units.contains(&0) {
        return Err("NVAPI string is empty, embedded-NUL, or too long".into());
    }
    target[..units.len()].copy_from_slice(&units);
    Ok(())
}

fn read_unicode(value: &[u16; NVAPI_UNICODE_UNITS]) -> Result<String, String> {
    let end = value
        .iter()
        .position(|unit| *unit == 0)
        .ok_or("NVAPI string is not NUL terminated")?;
    String::from_utf16(&value[..end]).map_err(|_| "NVAPI returned invalid UTF-16".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_sdk_layouts_and_versions_are_exact() {
        assert_eq!(std::mem::size_of::<NvDrsSetting>(), 12_328);
        assert_eq!(std::mem::size_of::<NvDrsProfile>(), 4_116);
        assert_eq!(std::mem::size_of::<NvDrsApplicationV4>(), 20_492);
        assert_eq!(NvDrsSetting::query().unwrap().read_u32(0), 0x0001_3028);
        let profile = NvDrsProfile::named("Counter-strike 2").unwrap();
        assert_eq!(profile.version, 0x0001_1014);
        assert_eq!(profile.name().unwrap(), "Counter-strike 2");
        assert_eq!(NvDrsProfile::query().unwrap().version, 0x0001_1014);
        assert_eq!(
            NvDrsApplicationV4::named("cs2.exe").unwrap().version,
            0x0004_500c
        );
        assert_eq!(unicode_argument("cs2.exe").unwrap()[0], u16::from(b'c'));
    }

    #[test]
    fn dword_setting_offsets_round_trip() {
        let value = NvDrsSetting::dword(0x10ab_cdef, 77).unwrap();
        assert_eq!(value.read_u32(SETTING_ID_OFFSET), 0x10ab_cdef);
        assert_eq!(value.setting_type(), NVDRS_DWORD_TYPE);
        assert_eq!(value.current_dword(), 77);
    }
}
