use crate::*;
// The native registry implementation is intentionally a small, auditable API:
// it admits only the typed values declared in `action_for`, never arbitrary CLI
// paths or raw user input.
#[cfg(windows)]
mod native_registry {
    use super::*;
    use windows::{
        Win32::{
            Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR},
            System::Registry::{
                HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE,
                REG_BINARY, REG_DWORD, REG_SZ, REG_VALUE_TYPE, RegCloseKey, RegCreateKeyW,
                RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
            },
        },
        core::PCWSTR,
    };
    pub(crate) fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    pub(crate) fn ok(value: WIN32_ERROR) -> Result<(), String> {
        if value.0 == 0 {
            Ok(())
        } else {
            Err(format!("Win32 registry error {}", value.0))
        }
    }
    pub(crate) fn hkey(hive: Hive) -> HKEY {
        match hive {
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
            Hive::CurrentUser => HKEY_CURRENT_USER,
        }
    }
    pub(crate) fn open(
        change: &RegistryChange,
        rights: windows::Win32::System::Registry::REG_SAM_FLAGS,
    ) -> Result<HKEY, String> {
        let key = wide(change.key);
        let mut handle = HKEY::default();
        ok(unsafe {
            RegOpenKeyExW(
                hkey(change.hive),
                PCWSTR(key.as_ptr()),
                None,
                rights,
                &mut handle,
            )
        })?;
        Ok(handle)
    }
    pub(crate) fn create(change: &RegistryChange) -> Result<HKEY, String> {
        let key = wide(change.key);
        let mut handle = HKEY::default();
        ok(unsafe { RegCreateKeyW(hkey(change.hive), PCWSTR(key.as_ptr()), &mut handle) })?;
        Ok(handle)
    }
    pub(super) fn read(change: &RegistryChange) -> Result<Option<RegValue>, String> {
        let handle = match open(change, KEY_QUERY_VALUE) {
            Ok(value) => value,
            Err(error) if error.contains("2") => return Ok(None),
            Err(error) => return Err(error),
        };
        let value = read_opened_value(handle, change.name, false)?;
        decode_value(value, false)
    }
    /// Reads a value for a contract which must distinguish absence from an
    /// inaccessible key, malformed value, or unsupported registry type.
    pub(super) fn read_exact(change: &RegistryChange) -> Result<Option<RegValue>, String> {
        let key = wide(change.key);
        let mut handle = HKEY::default();
        let opened = unsafe {
            RegOpenKeyExW(
                hkey(change.hive),
                PCWSTR(key.as_ptr()),
                None,
                KEY_QUERY_VALUE,
                &mut handle,
            )
        };
        if opened == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        ok(opened)?;
        let value = read_opened_value(handle, change.name, true)?;
        decode_value(value, true)
    }

    fn read_opened_value(
        handle: HKEY,
        name: &str,
        exact: bool,
    ) -> Result<Option<(REG_VALUE_TYPE, Vec<u8>)>, String> {
        let value = super::query_registry_value(handle, name, usize::MAX, false);
        unsafe {
            let _ = RegCloseKey(handle);
        }
        match value {
            Ok(value) => Ok(value),
            Err(super::RegistryQueryFailure::Size(code))
                if !exact || code == ERROR_FILE_NOT_FOUND =>
            {
                Ok(None)
            }
            Err(super::RegistryQueryFailure::Size(code))
            | Err(super::RegistryQueryFailure::Value(code)) => {
                Err(format!("Win32 registry error {}", code.0))
            }
        }
    }

    fn decode_value(
        value: Option<(REG_VALUE_TYPE, Vec<u8>)>,
        exact: bool,
    ) -> Result<Option<RegValue>, String> {
        let Some((kind, bytes)) = value else {
            return Ok(None);
        };
        match kind {
            REG_DWORD if (!exact && bytes.len() >= 4) || (exact && bytes.len() == 4) => {
                let value = u32::from_le_bytes(bytes[..4].try_into().map_err(|_| "invalid DWORD")?);
                Ok(Some(RegValue::Dword(value)))
            }
            REG_SZ if !exact || bytes.len().is_multiple_of(2) => {
                let units = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes(*pair))
                    .take_while(|value| *value != 0)
                    .collect::<Vec<_>>();
                let value = String::from_utf16(&units).map_err(|error| error.to_string())?;
                Ok(Some(RegValue::String(Box::leak(value.into_boxed_str()))))
            }
            REG_BINARY => Ok(Some(RegValue::Binary(Box::leak(bytes.into_boxed_slice())))),
            _ if exact => Err("registry value type is unsupported by the exact contract".into()),
            _ => Ok(None),
        }
    }
    pub(super) fn write(change: &RegistryChange) -> Result<(), String> {
        let handle = create(change)?;
        let name = wide(change.name);
        let (kind, bytes) = match &change.value {
            RegValue::Dword(value) => (REG_DWORD, value.to_le_bytes().to_vec()),
            RegValue::String(value) => (
                REG_SZ,
                wide(value).into_iter().flat_map(u16::to_le_bytes).collect(),
            ),
            RegValue::Binary(value) => (REG_BINARY, value.to_vec()),
        };
        let result =
            unsafe { RegSetValueExW(handle, PCWSTR(name.as_ptr()), None, kind, Some(&bytes)) };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        ok(result)
    }
    pub(super) fn delete(hive: Hive, key: &str, name: &str) -> Result<(), String> {
        let change = RegistryChange {
            hive,
            key: Box::leak(key.to_owned().into_boxed_str()),
            name: Box::leak(name.to_owned().into_boxed_str()),
            value: RegValue::Dword(0),
        };
        let handle = open(&change, KEY_SET_VALUE)?;
        let name = wide(name);
        let result = unsafe { RegDeleteValueW(handle, PCWSTR(name.as_ptr())) };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        ok(result)
    }
}
#[cfg(windows)]
pub(crate) enum RegistryQueryFailure {
    Size(windows::Win32::Foundation::WIN32_ERROR),
    Value(windows::Win32::Foundation::WIN32_ERROR),
}

#[cfg(windows)]
pub(crate) fn query_registry_value(
    handle: windows::Win32::System::Registry::HKEY,
    name: &str,
    maximum: usize,
    require_stable_length: bool,
) -> Result<Option<(windows::Win32::System::Registry::REG_VALUE_TYPE, Vec<u8>)>, RegistryQueryFailure>
{
    use windows::{
        Win32::{
            Foundation::ERROR_FILE_NOT_FOUND,
            System::Registry::{REG_VALUE_TYPE, RegQueryValueExW},
        },
        core::PCWSTR,
    };

    let name = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut kind = REG_VALUE_TYPE(0);
    let mut size = 0_u32;
    let first = unsafe {
        RegQueryValueExW(
            handle,
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
    if first.0 != 0 || size as usize > maximum {
        return Err(RegistryQueryFailure::Size(first));
    }
    let mut bytes = vec![0_u8; size as usize];
    let second = unsafe {
        RegQueryValueExW(
            handle,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            Some(bytes.as_mut_ptr()),
            Some(&mut size),
        )
    };
    if second.0 != 0 || (require_stable_length && size as usize != bytes.len()) {
        return Err(RegistryQueryFailure::Value(second));
    }
    Ok(Some((kind, bytes)))
}
#[cfg(windows)]
pub(crate) fn registry_read(change: &RegistryChange) -> Result<Option<RegValue>, String> {
    native_registry::read(change)
}
#[cfg(windows)]
pub(crate) fn registry_read_exact(change: &RegistryChange) -> Result<Option<RegValue>, String> {
    native_registry::read_exact(change)
}
#[cfg(not(windows))]
pub(crate) fn registry_read_exact(_: &RegistryChange) -> Result<Option<RegValue>, String> {
    Err("the live backend is supported only on Windows".into())
}
#[cfg(not(windows))]
pub(crate) fn registry_read(_: &RegistryChange) -> Result<Option<RegValue>, String> {
    Err("the live backend is supported only on Windows".into())
}
#[cfg(windows)]
pub(crate) fn registry_write(change: &RegistryChange) -> Result<(), String> {
    native_registry::write(change)
}
#[cfg(not(windows))]
pub(crate) fn registry_write(_: &RegistryChange) -> Result<(), String> {
    Err("the live backend is supported only on Windows".into())
}
#[cfg(windows)]
pub(crate) fn registry_delete(hive: Hive, key: &str, name: &str) -> Result<(), String> {
    native_registry::delete(hive, key, name)
}
#[cfg(not(windows))]
pub(crate) fn registry_delete(_: Hive, _: &str, _: &str) -> Result<(), String> {
    Err("the live backend is supported only on Windows".into())
}

pub(crate) fn capture_registry(
    change: &RegistryChange,
    step: String,
) -> Result<BackupEntry, String> {
    let original = registry_read(change)?;
    let (value, original_type, existed) = match original {
        Some(RegValue::Dword(value)) => (Value::from(value), Some("DWord".into()), true),
        Some(RegValue::String(value)) => (Value::String(value.into()), Some("String".into()), true),
        Some(RegValue::Binary(value)) => (
            Value::Array(value.iter().copied().map(Value::from).collect()),
            Some("Binary".into()),
            true,
        ),
        None => (Value::Null, None, false),
    };
    Ok(BackupEntry::Registry {
        step,
        timestamp: timestamp(),
        path: format!(
            "{}:\\{}",
            match change.hive {
                Hive::LocalMachine => "HKLM",
                Hive::CurrentUser => "HKCU",
            },
            change.key
        ),
        name: change.name.into(),
        original_value: value,
        original_type,
        existed,
        unknown: BTreeMap::new(),
    })
}
