#[cfg(windows)]
#[derive(Debug, Default)]
struct WindowsInterruptRegistry;

#[cfg(windows)]
impl InterruptRegistryReader for WindowsInterruptRegistry {
    fn read_interrupt_value(
        &self,
        key: &str,
        name: &'static str,
    ) -> Result<Option<InterruptRegistryValue>, DeviceBindingError> {
        windows_interrupt_registry::read(key, name)
    }
}

#[cfg(windows)]
impl InterruptRegistryStore for WindowsInterruptRegistry {
    fn write_interrupt_value(
        &self,
        key: &str,
        name: &'static str,
        value: &InterruptRegistryValue,
    ) -> Result<(), DeviceBindingError> {
        windows_interrupt_registry::write(key, name, value)
    }

    fn delete_interrupt_value(
        &self,
        key: &str,
        name: &'static str,
    ) -> Result<(), DeviceBindingError> {
        windows_interrupt_registry::delete(key, name)
    }
}

#[cfg(windows)]
mod windows_interrupt_registry {
    use super::*;
    use windows::{
        Win32::{
            Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR},
            System::Registry::{
                HKEY, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_BINARY, REG_DWORD,
                REG_VALUE_TYPE, RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
                RegSetValueExW,
            },
        },
        core::PCWSTR,
    };

    const MAX_INTERRUPT_VALUE_BYTES: usize = 64;

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    fn error(action: &str, code: WIN32_ERROR) -> DeviceBindingError {
        DeviceBindingError::RegistryAccess(format!("{action} failed with {}", code.0))
    }

    fn open(
        key: &str,
        rights: windows::Win32::System::Registry::REG_SAM_FLAGS,
    ) -> Result<Option<HKEY>, DeviceBindingError> {
        let key = wide(key);
        let mut handle = HKEY::default();
        let result = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(key.as_ptr()),
                None,
                rights,
                &mut handle,
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            Ok(None)
        } else if result.0 == 0 {
            Ok(Some(handle))
        } else {
            Err(error("RegOpenKeyExW", result))
        }
    }

    pub(super) fn read(
        key: &str,
        name: &'static str,
    ) -> Result<Option<InterruptRegistryValue>, DeviceBindingError> {
        let Some(handle) = open(key, KEY_QUERY_VALUE)? else {
            return Ok(None);
        };
        let name = wide(name);
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
        if first.0 != 0 || size as usize > MAX_INTERRUPT_VALUE_BYTES {
            unsafe {
                let _ = RegCloseKey(handle);
            }
            return Err(error("RegQueryValueExW(size)", first));
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
        unsafe {
            let _ = RegCloseKey(handle);
        }
        if second.0 != 0 || size as usize != bytes.len() {
            return Err(error("RegQueryValueExW(value)", second));
        }
        match kind {
            REG_DWORD if bytes.len() == 4 => Ok(Some(InterruptRegistryValue::Dword(
                u32::from_le_bytes(bytes.try_into().map_err(|_| {
                    DeviceBindingError::RegistryAccess("invalid DWORD bytes".into())
                })?),
            ))),
            REG_BINARY => Ok(Some(InterruptRegistryValue::Binary(bytes))),
            _ => Err(DeviceBindingError::RegistryAccess(
                "interrupt value has an unsupported registry type".into(),
            )),
        }
    }

    pub(super) fn write(
        key: &str,
        name: &'static str,
        value: &InterruptRegistryValue,
    ) -> Result<(), DeviceBindingError> {
        let Some(handle) = open(key, KEY_SET_VALUE)? else {
            return Err(DeviceBindingError::RegistryAccess(
                "interrupt device registry key is absent".into(),
            ));
        };
        let name = wide(name);
        let (kind, bytes) = match value {
            InterruptRegistryValue::Dword(value) => (REG_DWORD, value.to_le_bytes().to_vec()),
            InterruptRegistryValue::Binary(value) if value.len() <= MAX_INTERRUPT_VALUE_BYTES => {
                (REG_BINARY, value.clone())
            }
            InterruptRegistryValue::Binary(_) => {
                unsafe {
                    let _ = RegCloseKey(handle);
                }
                return Err(DeviceBindingError::RegistryAccess(
                    "interrupt binary value exceeds its bound".into(),
                ));
            }
        };
        let written =
            unsafe { RegSetValueExW(handle, PCWSTR(name.as_ptr()), None, kind, Some(&bytes)) };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        if written.0 == 0 {
            Ok(())
        } else {
            Err(error("RegSetValueExW", written))
        }
    }

    pub(super) fn delete(key: &str, name: &'static str) -> Result<(), DeviceBindingError> {
        let Some(handle) = open(key, KEY_SET_VALUE)? else {
            return Ok(());
        };
        let name = wide(name);
        let result = unsafe { RegDeleteValueW(handle, PCWSTR(name.as_ptr())) };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        if result.0 == 0 || result == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(error("RegDeleteValueW", result))
        }
    }
}
