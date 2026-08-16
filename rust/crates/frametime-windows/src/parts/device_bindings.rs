use frametime_core::{
    NetworkAdapterBinding as CoreNetworkAdapterBinding, PciDeviceBinding as CorePciDeviceBinding,
};

/// SetupAPI class identities accepted by the interrupt-policy workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PciDeviceClass {
    Display,
    Network,
    Media,
}

impl PciDeviceClass {
    pub const fn class_guid(self) -> &'static str {
        match self {
            Self::Display => "{4d36e968-e325-11ce-bfc1-08002be10318}",
            Self::Network => "{4d36e972-e325-11ce-bfc1-08002be10318}",
            Self::Media => "{4d36e96c-e325-11ce-bfc1-08002be10318}",
        }
    }

    fn from_class_guid(value: &str) -> Option<Self> {
        [Self::Display, Self::Network, Self::Media]
            .into_iter()
            .find(|class| value.eq_ignore_ascii_case(class.class_guid()))
    }
}

/// A single SetupAPI observation. The flags are deliberately retained so the
/// portable resolver can prove that it only accepts present, status-OK devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PciDeviceObservation {
    pub binding: CorePciDeviceBinding,
    pub present: bool,
    pub status_ok: bool,
}

/// A network observation that has not yet been granted registry authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkAdapterObservation {
    pub binding: CoreNetworkAdapterBinding,
    pub is_up: bool,
    pub is_physical: bool,
    pub is_wired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceBindingError {
    InvalidPciBinding(String),
    InvalidNetworkBinding(String),
    AmbiguousPciIdentity(String),
    AmbiguousNetworkIdentity(String),
    NoEligibleNetworkAdapter,
    MultipleEligibleNetworkAdapters,
    NetworkAdapterDoesNotMatchPciIdentity,
    UnsupportedProcessorTopology,
    RegistryReadbackMismatch { key: String, name: &'static str },
    RegistryAccess(String),
    PlatformAdapterUnavailable(&'static str),
}

impl std::fmt::Display for DeviceBindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPciBinding(error) => write!(formatter, "invalid PCI binding: {error}"),
            Self::InvalidNetworkBinding(error) => write!(formatter, "invalid network binding: {error}"),
            Self::AmbiguousPciIdentity(identity) => {
                write!(formatter, "ambiguous PCI identity: {identity}")
            }
            Self::AmbiguousNetworkIdentity(identity) => {
                write!(formatter, "ambiguous network identity: {identity}")
            }
            Self::NoEligibleNetworkAdapter => formatter.write_str("no active physical wired adapter"),
            Self::MultipleEligibleNetworkAdapters => {
                formatter.write_str("multiple active physical wired adapters")
            }
            Self::NetworkAdapterDoesNotMatchPciIdentity => {
                formatter.write_str("network adapter does not match an exact PCI PnP identity")
            }
            Self::UnsupportedProcessorTopology => formatter.write_str(
                "interrupt affinity requires one processor group with 1 through 64 logical processors",
            ),
            Self::RegistryReadbackMismatch { key, name } => {
                write!(formatter, "registry readback did not match {key}\\{name}")
            }
            Self::RegistryAccess(error) => write!(formatter, "interrupt registry access: {error}"),
            Self::PlatformAdapterUnavailable(adapter) => {
                write!(formatter, "{adapter} is supported only on Windows")
            }
        }
    }
}

impl std::error::Error for DeviceBindingError {}

/// Injectable boundary for authoritative SetupAPI discovery.
pub trait PciDeviceEnumerator {
    fn enumerate_pci_devices(&self) -> Result<Vec<PciDeviceObservation>, DeviceBindingError>;
}

/// Injectable boundary for IP Helper/Ndis network discovery.
pub trait NetworkAdapterEnumerator {
    fn enumerate_network_adapters(
        &self,
    ) -> Result<Vec<NetworkAdapterObservation>, DeviceBindingError>;
}

#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsSetupApiEnumerator;

#[cfg(windows)]
impl PciDeviceEnumerator for WindowsSetupApiEnumerator {
    fn enumerate_pci_devices(&self) -> Result<Vec<PciDeviceObservation>, DeviceBindingError> {
        windows_setupapi::enumerate()
    }
}

/// The only unsafe portion of device discovery. Every raw result is copied into
/// owned Rust data and validated by the portable resolver before it is exposed.
#[cfg(windows)]
mod windows_setupapi {
    use super::*;
    use windows::{
        Win32::{
            Devices::{
                DeviceAndDriverInstallation::{
                    CM_DEVNODE_STATUS_FLAGS, CM_Get_DevNode_Status, CM_PROB, CR_SUCCESS,
                    DIGCF_PRESENT, DN_STARTED, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList,
                    SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW,
                    SetupDiGetDevicePropertyW,
                },
                Properties::{
                    DEVPKEY_Device_ContainerId, DEVPKEY_Device_DriverInfPath,
                    DEVPKEY_Device_DriverProvider, DEVPKEY_Device_DriverVersion, DEVPROP_TYPE_GUID,
                    DEVPROP_TYPE_STRING, DEVPROPTYPE,
                },
            },
            Foundation::{
                DEVPROPKEY, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, GetLastError,
            },
            System::Registry::{
                HKEY, KEY_QUERY_VALUE, REG_SZ, REG_VALUE_TYPE, RegCloseKey, RegQueryValueExW,
            },
        },
        core::{GUID, PCWSTR},
    };

    const MAX_PROPERTY_BYTES: usize = 64 * 1024;
    const MAX_INSTANCE_UNITS: usize = 512;

    pub(super) fn enumerate() -> Result<Vec<PciDeviceObservation>, DeviceBindingError> {
        let mut all = Vec::new();
        for class in [
            PciDeviceClass::Display,
            PciDeviceClass::Network,
            PciDeviceClass::Media,
        ] {
            all.extend(enumerate_class(class)?);
        }
        Ok(all)
    }

    pub(super) fn enumerate_network_pci_links()
    -> Result<Vec<(CorePciDeviceBinding, String)>, DeviceBindingError> {
        let class = PciDeviceClass::Network;
        let guid = GUID::try_from(&class.class_guid()[1..37])
            .map_err(|reason| error(format!("invalid compiled class GUID: {reason}")))?;
        let set = unsafe {
            SetupDiGetClassDevsW(Some(&guid), PCWSTR::null(), None, DIGCF_PRESENT)
                .map_err(|value| error(format!("SetupDiGetClassDevsW: {value}")))?
        };
        let result = network_links_set(set);
        let destroyed = unsafe { SetupDiDestroyDeviceInfoList(set) };
        match (result, destroyed) {
            (Ok(records), Ok(())) => Ok(records),
            (Err(reason), _) => Err(reason),
            (_, Err(reason)) => Err(error(format!("SetupDiDestroyDeviceInfoList: {reason}"))),
        }
    }

    fn network_links_set(
        set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    ) -> Result<Vec<(CorePciDeviceBinding, String)>, DeviceBindingError> {
        let mut links = Vec::new();
        for index in 0..u32::MAX {
            let mut data = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if unsafe { SetupDiEnumDeviceInfo(set, index, &mut data) }.is_err() {
                if unsafe { GetLastError() } == ERROR_NO_MORE_ITEMS {
                    return Ok(links);
                }
                return Err(error(format!(
                    "SetupDiEnumDeviceInfo at index {index} failed"
                )));
            }
            let (status, problem) = status(&data)?;
            if problem != 0 || status & DN_STARTED.0 == 0 {
                continue;
            }
            links.push((
                binding(set, &data, PciDeviceClass::Network)?,
                driver_net_cfg_instance_id(set, &data)?,
            ));
        }
        Err(error(
            "SetupAPI network enumeration exceeded u32::MAX entries",
        ))
    }

    fn driver_net_cfg_instance_id(
        set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
        data: &SP_DEVINFO_DATA,
    ) -> Result<String, DeviceBindingError> {
        use windows::Win32::Devices::DeviceAndDriverInstallation::{
            DICS_FLAG_GLOBAL, DIREG_DRV, SetupDiOpenDevRegKey,
        };
        let key = unsafe {
            SetupDiOpenDevRegKey(
                set,
                data,
                DICS_FLAG_GLOBAL.0,
                0,
                DIREG_DRV,
                KEY_QUERY_VALUE.0,
            )
            .map_err(|reason| error(format!("SetupDiOpenDevRegKey: {reason}")))?
        };
        let value = registry_string(key, "NetCfgInstanceId");
        let _ = unsafe { RegCloseKey(key) };
        value
    }

    fn registry_string(key: HKEY, name: &str) -> Result<String, DeviceBindingError> {
        let wide = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let mut kind = REG_VALUE_TYPE(0);
        let mut size = 0;
        let first = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(wide.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut size),
            )
        };
        if first.0 != 0 || kind != REG_SZ || size == 0 || size as usize > MAX_PROPERTY_BYTES {
            return Err(error(
                "NetCfgInstanceId is missing or has an invalid registry type or size",
            ));
        }
        let mut bytes = vec![0_u8; size as usize];
        let second = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(wide.as_ptr()),
                None,
                Some(&mut kind),
                Some(bytes.as_mut_ptr()),
                Some(&mut size),
            )
        };
        if second.0 != 0
            || kind != REG_SZ
            || size as usize != bytes.len()
            || !bytes.len().is_multiple_of(2)
        {
            return Err(error(
                "NetCfgInstanceId changed or has an invalid registry type or size",
            ));
        }
        let units = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        let raw = utf16_text(&units, "NetCfgInstanceId")?;
        let guid = GUID::try_from(raw.trim_matches(['{', '}']))
            .map_err(|_| error("NetCfgInstanceId is not a canonical GUID"))?;
        Ok(format_guid(guid))
    }

    fn enumerate_class(
        class: PciDeviceClass,
    ) -> Result<Vec<PciDeviceObservation>, DeviceBindingError> {
        let guid = GUID::try_from(&class.class_guid()[1..37])
            .map_err(|reason| error(format!("invalid compiled class GUID: {reason}")))?;
        let set = unsafe {
            SetupDiGetClassDevsW(Some(&guid), PCWSTR::null(), None, DIGCF_PRESENT)
                .map_err(|value| error(format!("SetupDiGetClassDevsW: {value}")))?
        };
        let result = enumerate_set(set, class);
        let destroyed = unsafe { SetupDiDestroyDeviceInfoList(set) };
        match (result, destroyed) {
            (Ok(records), Ok(())) => Ok(records),
            (Err(reason), _) => Err(reason),
            (_, Err(reason)) => Err(error(format!("SetupDiDestroyDeviceInfoList: {reason}"))),
        }
    }

    fn enumerate_set(
        set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
        expected_class: PciDeviceClass,
    ) -> Result<Vec<PciDeviceObservation>, DeviceBindingError> {
        let mut records = Vec::new();
        for index in 0..u32::MAX {
            let mut data = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if unsafe { SetupDiEnumDeviceInfo(set, index, &mut data) }.is_err() {
                if unsafe { GetLastError() } == ERROR_NO_MORE_ITEMS {
                    return Ok(records);
                }
                return Err(error(format!(
                    "SetupDiEnumDeviceInfo at index {index}: {}",
                    unsafe { GetLastError() }.0
                )));
            }
            let (status, problem) = status(&data)?;
            // DIGCF_PRESENT is necessary but insufficient: only an active,
            // problem-free devnode is treated as Status-OK mutation evidence.
            if problem != 0 || status & DN_STARTED.0 == 0 {
                continue;
            }
            let binding = binding(set, &data, expected_class)?;
            records.push(PciDeviceObservation {
                binding,
                present: true,
                status_ok: true,
            });
        }
        Err(error(
            "SetupAPI device enumeration exceeded u32::MAX entries",
        ))
    }

    fn status(data: &SP_DEVINFO_DATA) -> Result<(u32, u32), DeviceBindingError> {
        let mut status = CM_DEVNODE_STATUS_FLAGS(0);
        let mut problem = CM_PROB(0);
        let result = unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, data.DevInst, 0) };
        if result == CR_SUCCESS {
            Ok((status.0, problem.0))
        } else {
            Err(error(format!(
                "CM_Get_DevNode_Status returned {}",
                result.0
            )))
        }
    }

    fn binding(
        set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
        data: &SP_DEVINFO_DATA,
        expected_class: PciDeviceClass,
    ) -> Result<CorePciDeviceBinding, DeviceBindingError> {
        let class_guid = format_guid(data.ClassGuid);
        if !class_guid.eq_ignore_ascii_case(expected_class.class_guid()) {
            return Err(error(
                "SetupAPI class GUID disagrees with the requested class",
            ));
        }
        let instance_id = instance_id(set, data)?;
        let (vendor_id, device_id, subsystem_vendor_id, subsystem_device_id, revision_id) =
            parse_pci_identity(&instance_id)?;
        let binding = CorePciDeviceBinding {
            schema_version: frametime_core::NATIVE_BINDING_SCHEMA_VERSION,
            instance_id,
            container_id: guid_property(set, data, &DEVPKEY_Device_ContainerId, "container ID")?,
            class_guid,
            vendor_id,
            device_id,
            subsystem_vendor_id,
            subsystem_device_id,
            revision_id,
            driver_provider: string_property(
                set,
                data,
                &DEVPKEY_Device_DriverProvider,
                "driver provider",
            )?,
            driver_version: string_property(
                set,
                data,
                &DEVPKEY_Device_DriverVersion,
                "driver version",
            )?,
            published_inf: string_property(
                set,
                data,
                &DEVPKEY_Device_DriverInfPath,
                "published INF",
            )?
            .to_ascii_lowercase(),
            observed_at_utc: timestamp(),
            unknown: BTreeMap::new(),
        };
        binding
            .validate()
            .map_err(|value| error(format!("SetupAPI binding validation failed: {value}")))?;
        Ok(binding)
    }

    fn instance_id(
        set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
        data: &SP_DEVINFO_DATA,
    ) -> Result<String, DeviceBindingError> {
        let mut required = 0;
        if unsafe { SetupDiGetDeviceInstanceIdW(set, data, None, Some(&mut required)) }.is_ok()
            || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER
            || required == 0
            || required as usize > MAX_INSTANCE_UNITS
        {
            return Err(error(
                "SetupDiGetDeviceInstanceIdW did not return a bounded required size",
            ));
        }
        let mut units = vec![0_u16; required as usize];
        unsafe { SetupDiGetDeviceInstanceIdW(set, data, Some(&mut units), Some(&mut required)) }
            .map_err(|value| error(format!("SetupDiGetDeviceInstanceIdW: {value}")))?;
        utf16_text(&units, "device instance ID")
    }

    fn guid_property(
        set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
        data: &SP_DEVINFO_DATA,
        key: &DEVPROPKEY,
        label: &str,
    ) -> Result<String, DeviceBindingError> {
        let (kind, bytes) = property(set, data, key, label)?;
        if kind != DEVPROP_TYPE_GUID || bytes.len() != std::mem::size_of::<GUID>() {
            return Err(error(format!(
                "{label} has an unexpected SetupAPI property type or size"
            )));
        }
        let value = unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<GUID>()) };
        Ok(format_guid(value))
    }

    fn string_property(
        set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
        data: &SP_DEVINFO_DATA,
        key: &DEVPROPKEY,
        label: &str,
    ) -> Result<String, DeviceBindingError> {
        let (kind, bytes) = property(set, data, key, label)?;
        if kind != DEVPROP_TYPE_STRING || bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
            return Err(error(format!(
                "{label} has an unexpected SetupAPI property type or size"
            )));
        }
        let units = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        utf16_text(&units, label)
    }

    fn property(
        set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
        data: &SP_DEVINFO_DATA,
        key: &DEVPROPKEY,
        label: &str,
    ) -> Result<(DEVPROPTYPE, Vec<u8>), DeviceBindingError> {
        let mut kind = DEVPROPTYPE(0);
        let mut required = 0;
        if unsafe {
            SetupDiGetDevicePropertyW(set, data, key, &mut kind, None, Some(&mut required), 0)
        }
        .is_ok()
            || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER
            || required == 0
            || required as usize > MAX_PROPERTY_BYTES
        {
            return Err(error(format!(
                "{label} does not have a bounded readable SetupAPI property"
            )));
        }
        let mut bytes = vec![0_u8; required as usize];
        unsafe {
            SetupDiGetDevicePropertyW(
                set,
                data,
                key,
                &mut kind,
                Some(&mut bytes),
                Some(&mut required),
                0,
            )
        }
        .map_err(|value| error(format!("SetupDiGetDevicePropertyW for {label}: {value}")))?;
        if required as usize != bytes.len() {
            return Err(error(format!(
                "{label} changed size during SetupAPI retrieval"
            )));
        }
        Ok((kind, bytes))
    }

    fn utf16_text(units: &[u16], label: &str) -> Result<String, DeviceBindingError> {
        let Some((&0, value)) = units.split_last() else {
            return Err(error(format!("{label} is not NUL terminated")));
        };
        if value.contains(&0) {
            return Err(error(format!("{label} contains an embedded NUL")));
        }
        let value = String::from_utf16(value)
            .map_err(|reason| error(format!("{label} is not valid UTF-16: {reason}")))?;
        if value.is_empty() {
            return Err(error(format!("{label} is empty")));
        }
        Ok(value)
    }

    fn parse_pci_identity(value: &str) -> Result<(u16, u16, u16, u16, u8), DeviceBindingError> {
        let upper = value.to_ascii_uppercase();
        let mut vendor = None;
        let mut device = None;
        let mut subsystem = None;
        let mut revision = None;
        for part in upper.split(['\\', '&']) {
            if let Some(value) = part.strip_prefix("VEN_") {
                vendor = parse_hex(value, 4);
            } else if let Some(value) = part.strip_prefix("DEV_") {
                device = parse_hex(value, 4);
            } else if let Some(value) = part.strip_prefix("SUBSYS_") {
                subsystem = parse_hex(value, 8);
            } else if let Some(value) = part.strip_prefix("REV_") {
                revision = parse_hex(value, 2);
            }
        }
        let subsystem =
            subsystem.ok_or_else(|| error("PCI instance has no canonical subsystem ID"))?;
        Ok((
            vendor.ok_or_else(|| error("PCI instance has no canonical vendor ID"))? as u16,
            device.ok_or_else(|| error("PCI instance has no canonical device ID"))? as u16,
            (subsystem & 0xffff) as u16,
            (subsystem >> 16) as u16,
            revision.ok_or_else(|| error("PCI instance has no canonical revision ID"))? as u8,
        ))
    }

    fn parse_hex(value: &str, width: usize) -> Option<u32> {
        (value.len() == width)
            .then(|| u32::from_str_radix(value, 16).ok())
            .flatten()
    }

    fn format_guid(value: GUID) -> String {
        format!("{{{value:?}}}")
    }

    fn error(message: impl Into<String>) -> DeviceBindingError {
        DeviceBindingError::InvalidPciBinding(message.into())
    }
}
