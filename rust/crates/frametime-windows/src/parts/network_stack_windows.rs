//! Exact SetupAPI-backed driver-key access for the P1:16 network stack.
//!
//! The driver software key is discovered from the captured PCI instance for
//! every operation.  It is never reconstructed from a class-key index, an
//! adapter display name, or a registry path.

use frametime_core::{NetworkAdapterBinding, NetworkStackValue};
use windows::{
    Win32::{
        Devices::DeviceAndDriverInstallation::{
            DICS_FLAG_GLOBAL, DIGCF_PRESENT, DIREG_DRV, HDEVINFO, SP_DEVINFO_DATA,
            SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
            SetupDiGetDeviceInstanceIdW, SetupDiOpenDevRegKey,
        },
        Foundation::{
            ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, GetLastError,
        },
        System::Registry::{
            HKEY, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD, REG_SZ, REG_VALUE_TYPE, RegCloseKey,
            RegQueryValueExW, RegSetValueExW,
        },
    },
    core::{GUID, PCWSTR},
};

const MAX_INSTANCE_UNITS: usize = 512;
const MAX_VALUE_BYTES: usize = 4096;

struct DeviceInfoSet(HDEVINFO);

impl Drop for DeviceInfoSet {
    fn drop(&mut self) {
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}

struct DriverKey(HKEY);

impl Drop for DriverKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

pub(super) fn read_driver_setting(
    adapter: &NetworkAdapterBinding,
    name: &str,
) -> Result<Option<NetworkStackValue>, String> {
    let key = open_driver_key(adapter, KEY_QUERY_VALUE.0)?;
    let Some((kind, bytes)) = read_raw_value(&key, name)? else {
        return Ok(None);
    };
    if kind != REG_DWORD || bytes.len() != std::mem::size_of::<u32>() {
        return Err(format!(
            "P1:16 {name} has an unsupported driver registry type or size"
        ));
    }
    let value = u32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| format!("P1:16 {name} DWORD length changed during read"))?,
    );
    Ok(Some(NetworkStackValue::Dword(value)))
}

pub(super) fn write_driver_setting(
    adapter: &NetworkAdapterBinding,
    name: &str,
    value: &NetworkStackValue,
) -> Result<(), String> {
    let NetworkStackValue::Dword(value) = value else {
        return Err(format!("P1:16 {name} requires a DWORD driver value"));
    };
    let key = open_driver_key(adapter, KEY_QUERY_VALUE.0 | KEY_SET_VALUE.0)?;
    let Some((kind, original)) = read_raw_value(&key, name)? else {
        return Err(format!("P1:16 {name} became absent before its write"));
    };
    if kind != REG_DWORD || original.len() != std::mem::size_of::<u32>() {
        return Err(format!(
            "P1:16 {name} has an unsupported driver registry type or size"
        ));
    }
    let bytes = value.to_le_bytes();
    let result = unsafe {
        RegSetValueExW(
            key.0,
            PCWSTR(wide(name).as_ptr()),
            None,
            REG_DWORD,
            Some(&bytes),
        )
    };
    if result.0 != 0 {
        return Err(format!(
            "P1:16 RegSetValueExW({name}) failed with {}",
            result.0
        ));
    }
    let Some((written_kind, written)) = read_raw_value(&key, name)? else {
        return Err(format!("P1:16 {name} disappeared after its write"));
    };
    if written_kind != REG_DWORD || written.as_slice() != bytes {
        return Err(format!("P1:16 {name} did not retain its exact DWORD write"));
    }
    Ok(())
}

fn open_driver_key(adapter: &NetworkAdapterBinding, rights: u32) -> Result<DriverKey, String> {
    adapter
        .validate()
        .map_err(|error| format!("P1:16 invalid adapter binding: {error}"))?;
    let class_guid = parse_guid(&adapter.device.class_guid, "device class GUID")?;
    let set =
        unsafe { SetupDiGetClassDevsW(Some(&class_guid), PCWSTR::null(), None, DIGCF_PRESENT) }
            .map(DeviceInfoSet)
            .map_err(|error| format!("P1:16 SetupDiGetClassDevsW failed: {error}"))?;
    let data = exact_device(&set, &adapter.device.instance_id)?;
    let key =
        unsafe { SetupDiOpenDevRegKey(set.0, &data, DICS_FLAG_GLOBAL.0, 0, DIREG_DRV, rights) }
            .map(DriverKey)
            .map_err(|error| format!("P1:16 SetupDiOpenDevRegKey failed: {error}"))?;
    verify_netcfg_identity(&key, adapter)?;
    Ok(key)
}

fn exact_device(set: &DeviceInfoSet, expected_instance: &str) -> Result<SP_DEVINFO_DATA, String> {
    let mut matched = None;
    for index in 0..u32::MAX {
        let mut data = SP_DEVINFO_DATA {
            cbSize: u32::try_from(std::mem::size_of::<SP_DEVINFO_DATA>())
                .map_err(|_| "P1:16 SP_DEVINFO_DATA size overflows u32")?,
            ..Default::default()
        };
        if unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut data) }.is_err() {
            if unsafe { GetLastError() } == ERROR_NO_MORE_ITEMS {
                return matched.ok_or_else(|| {
                    "P1:16 exact PCI instance is no longer present in SetupAPI".into()
                });
            }
            return Err(format!("P1:16 SetupDiEnumDeviceInfo({index}) failed"));
        }
        if device_instance_id(set, &data)?.eq_ignore_ascii_case(expected_instance)
            && matched.replace(data).is_some()
        {
            return Err("P1:16 SetupAPI resolved the PCI instance more than once".into());
        }
    }
    Err("P1:16 SetupAPI enumeration exceeded u32::MAX entries".into())
}

fn device_instance_id(set: &DeviceInfoSet, data: &SP_DEVINFO_DATA) -> Result<String, String> {
    let mut required = 0_u32;
    if unsafe { SetupDiGetDeviceInstanceIdW(set.0, data, None, Some(&mut required)) }.is_ok()
        || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER
        || required == 0
        || required as usize > MAX_INSTANCE_UNITS
    {
        return Err("P1:16 SetupDiGetDeviceInstanceIdW returned an invalid size".into());
    }
    let mut units = vec![0_u16; required as usize];
    unsafe { SetupDiGetDeviceInstanceIdW(set.0, data, Some(&mut units), Some(&mut required)) }
        .map_err(|error| format!("P1:16 SetupDiGetDeviceInstanceIdW failed: {error}"))?;
    utf16_text(&units, "SetupAPI PCI instance")
}

fn verify_netcfg_identity(key: &DriverKey, adapter: &NetworkAdapterBinding) -> Result<(), String> {
    let Some((kind, bytes)) = read_raw_value(key, "NetCfgInstanceId")? else {
        return Err("P1:16 exact driver key has no NetCfgInstanceId".into());
    };
    if kind != REG_SZ {
        return Err("P1:16 exact driver key NetCfgInstanceId is not REG_SZ".into());
    }
    let observed = utf16_bytes(&bytes, "NetCfgInstanceId")?;
    if !observed.eq_ignore_ascii_case(&adapter.interface_guid) {
        return Err("P1:16 driver key NetCfgInstanceId no longer matches adapter GUID".into());
    }
    Ok(())
}

fn read_raw_value(
    key: &DriverKey,
    name: &str,
) -> Result<Option<(REG_VALUE_TYPE, Vec<u8>)>, String> {
    let name = wide(name);
    let mut kind = REG_VALUE_TYPE(0);
    let mut size = 0_u32;
    let first = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            None,
            Some(&mut size),
        )
    };
    if first == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if first.0 != 0 || size as usize > MAX_VALUE_BYTES {
        return Err(format!(
            "P1:16 RegQueryValueExW({}) failed with {}",
            name_label(&name),
            first.0
        ));
    }
    let mut bytes = vec![0_u8; size as usize];
    let second = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            Some(bytes.as_mut_ptr()),
            Some(&mut size),
        )
    };
    if second.0 != 0 || size as usize != bytes.len() {
        return Err(format!(
            "P1:16 RegQueryValueExW(value) failed with {}",
            second.0
        ));
    }
    Ok(Some((kind, bytes)))
}

fn parse_guid(value: &str, label: &str) -> Result<GUID, String> {
    GUID::try_from(value.trim_matches(['{', '}']))
        .map_err(|error| format!("P1:16 {label} is not a GUID: {error}"))
}

fn utf16_bytes(bytes: &[u8], label: &str) -> Result<String, String> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(2) {
        return Err(format!(
            "P1:16 {label} has an invalid UTF-16 registry value"
        ));
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    utf16_text(&units, label)
}

fn utf16_text(units: &[u16], label: &str) -> Result<String, String> {
    let Some(end) = units.iter().position(|unit| *unit == 0) else {
        return Err(format!("P1:16 {label} is not NUL terminated"));
    };
    if end + 1 != units.len() {
        return Err(format!("P1:16 {label} has embedded NUL data"));
    }
    String::from_utf16(&units[..end])
        .map_err(|error| format!("P1:16 {label} is not UTF-16: {error}"))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn name_label(name: &[u16]) -> String {
    String::from_utf16_lossy(name)
        .trim_end_matches('\0')
        .to_owned()
}
