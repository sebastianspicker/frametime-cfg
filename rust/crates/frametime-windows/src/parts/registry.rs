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
                RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
            },
        },
        core::PCWSTR,
    };
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    fn ok(value: WIN32_ERROR) -> Result<(), String> {
        if value.0 == 0 {
            Ok(())
        } else {
            Err(format!("Win32 registry error {}", value.0))
        }
    }
    fn hkey(hive: Hive) -> HKEY {
        match hive {
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
            Hive::CurrentUser => HKEY_CURRENT_USER,
        }
    }
    fn open(
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
    fn create(change: &RegistryChange) -> Result<HKEY, String> {
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
        let name = wide(change.name);
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
        if first.0 != 0 {
            unsafe {
                let _ = RegCloseKey(handle);
            }
            return Ok(None);
        }
        let mut bytes = vec![0_u8; size as usize];
        let result = unsafe {
            RegQueryValueExW(
                handle,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                Some(bytes.as_mut_ptr()),
                Some(&mut size),
            )
        };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        ok(result)?;
        match kind {
            REG_DWORD if bytes.len() >= 4 => {
                let value = u32::from_le_bytes(bytes[..4].try_into().map_err(|_| "invalid DWORD")?);
                Ok(Some(RegValue::Dword(value)))
            }
            REG_SZ => {
                let units = bytes
                    .chunks_exact(2)
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .take_while(|value| *value != 0)
                    .collect::<Vec<_>>();
                let value = String::from_utf16(&units).map_err(|error| error.to_string())?;
                Ok(Some(RegValue::String(Box::leak(value.into_boxed_str()))))
            }
            REG_BINARY => Ok(Some(RegValue::Binary(Box::leak(bytes.into_boxed_slice())))),
            _ => Ok(None),
        }
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
        let name = wide(change.name);
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
            unsafe {
                let _ = RegCloseKey(handle);
            }
            return Ok(None);
        }
        if first.0 != 0 {
            unsafe {
                let _ = RegCloseKey(handle);
            }
            return Err(format!("Win32 registry error {}", first.0));
        }
        let mut bytes = vec![0_u8; size as usize];
        let result = unsafe {
            RegQueryValueExW(
                handle,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                Some(bytes.as_mut_ptr()),
                Some(&mut size),
            )
        };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        ok(result)?;
        match kind {
            REG_DWORD if bytes.len() == 4 => Ok(Some(RegValue::Dword(u32::from_le_bytes(
                bytes.try_into().map_err(|_| "invalid DWORD")?,
            )))),
            REG_SZ if bytes.len().is_multiple_of(2) => {
                let units = bytes
                    .chunks_exact(2)
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .take_while(|value| *value != 0)
                    .collect::<Vec<_>>();
                let value = String::from_utf16(&units).map_err(|error| error.to_string())?;
                Ok(Some(RegValue::String(Box::leak(value.into_boxed_str()))))
            }
            REG_BINARY => Ok(Some(RegValue::Binary(Box::leak(bytes.into_boxed_slice())))),
            _ => Err("registry value type is unsupported by the exact contract".into()),
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
fn registry_read(change: &RegistryChange) -> Result<Option<RegValue>, String> {
    native_registry::read(change)
}
#[cfg(windows)]
fn registry_read_exact(change: &RegistryChange) -> Result<Option<RegValue>, String> {
    native_registry::read_exact(change)
}
#[cfg(not(windows))]
fn registry_read_exact(_: &RegistryChange) -> Result<Option<RegValue>, String> {
    Err("the live backend is supported only on Windows".into())
}
#[cfg(not(windows))]
fn registry_read(_: &RegistryChange) -> Result<Option<RegValue>, String> {
    Err("the live backend is supported only on Windows".into())
}
#[cfg(windows)]
fn registry_write(change: &RegistryChange) -> Result<(), String> {
    native_registry::write(change)
}
#[cfg(not(windows))]
fn registry_write(_: &RegistryChange) -> Result<(), String> {
    Err("the live backend is supported only on Windows".into())
}
#[cfg(windows)]
fn registry_delete(hive: Hive, key: &str, name: &str) -> Result<(), String> {
    native_registry::delete(hive, key, name)
}
#[cfg(not(windows))]
fn registry_delete(_: Hive, _: &str, _: &str) -> Result<(), String> {
    Err("the live backend is supported only on Windows".into())
}

fn capture_registry(change: &RegistryChange, step: String) -> Result<BackupEntry, String> {
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
