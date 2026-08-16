#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsIpHelperNetworkAdapterEnumerator;

#[cfg(windows)]
impl NetworkAdapterEnumerator for WindowsIpHelperNetworkAdapterEnumerator {
    fn enumerate_network_adapters(
        &self,
    ) -> Result<Vec<NetworkAdapterObservation>, DeviceBindingError> {
        windows_ip_helper::enumerate()
    }
}

/// IP Helper supplies transient interface state. SetupAPI supplies the PCI
/// identity, joined only through the driver's exact `NetCfgInstanceId` value.
/// No friendly name, description, index, or MAC address is used as a join key.
#[cfg(windows)]
mod windows_ip_helper {
    use super::*;
    use std::{collections::BTreeMap, mem::size_of};
    use windows::{
        Win32::{
            Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS},
            NetworkManagement::{
                IpHelper::{
                    GET_ADAPTERS_ADDRESSES_FLAGS, GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH,
                },
                Ndis::IfOperStatusUp,
            },
        },
        core::GUID,
    };

    const MAX_ADAPTER_BUFFER_BYTES: usize = 1024 * 1024;
    const MAX_ADAPTERS: usize = 4096;
    const MAX_WIDE_UNITS: usize = 1024;
    const AF_UNSPEC: u32 = 0;
    const IF_TYPE_ETHERNET_CSMACD: u32 = 6;

    struct IpAdapterRecord {
        guid: String,
        luid: u64,
        interface_index: u32,
        friendly_name: String,
        description: String,
        physical_address: Vec<u8>,
        is_up: bool,
        is_wired: bool,
    }

    pub(super) fn enumerate() -> Result<Vec<NetworkAdapterObservation>, DeviceBindingError> {
        let links = pci_links()?;
        let mut observations = BTreeMap::new();
        for adapter in adapters()? {
            let Some(device) = links.get(&adapter.guid).cloned() else {
                if adapter.is_wired && adapter.is_up {
                    return Err(DeviceBindingError::NetworkAdapterDoesNotMatchPciIdentity);
                }
                continue;
            };
            let binding = CoreNetworkAdapterBinding {
                schema_version: frametime_core::NATIVE_BINDING_SCHEMA_VERSION,
                adapter_name: adapter.guid.clone(),
                interface_guid: adapter.guid.clone(),
                interface_luid: adapter.luid,
                interface_index: adapter.interface_index,
                friendly_name: adapter.friendly_name,
                interface_description: adapter.description,
                physical_address: adapter.physical_address,
                device,
                observed_at_utc: timestamp(),
                unknown: BTreeMap::new(),
            };
            binding
                .validate()
                .map_err(|reason| DeviceBindingError::InvalidNetworkBinding(reason.to_string()))?;
            let observation = NetworkAdapterObservation {
                binding,
                is_up: adapter.is_up,
                is_physical: true,
                is_wired: adapter.is_wired,
            };
            match observations.get(&adapter.guid) {
                Some(existing) if *existing == observation => {}
                Some(_) => return Err(DeviceBindingError::AmbiguousNetworkIdentity(adapter.guid)),
                None => {
                    observations.insert(adapter.guid, observation);
                }
            }
        }
        Ok(observations.into_values().collect())
    }

    fn pci_links() -> Result<BTreeMap<String, CorePciDeviceBinding>, DeviceBindingError> {
        let mut links = BTreeMap::new();
        for (device, net_cfg_instance_id) in windows_setupapi::enumerate_network_pci_links()? {
            let key = net_cfg_instance_id.to_ascii_uppercase();
            match links.get(&key) {
                Some(existing) if *existing == device => {}
                Some(_) => return Err(DeviceBindingError::AmbiguousNetworkIdentity(key)),
                None => {
                    links.insert(key, device);
                }
            }
        }
        Ok(links)
    }

    fn adapters() -> Result<Vec<IpAdapterRecord>, DeviceBindingError> {
        let mut required = 0_u32;
        let first = unsafe {
            GetAdaptersAddresses(
                AF_UNSPEC,
                GET_ADAPTERS_ADDRESSES_FLAGS(0),
                None,
                None,
                &mut required,
            )
        };
        if first != ERROR_BUFFER_OVERFLOW.0 || required == 0 || required as usize > MAX_ADAPTER_BUFFER_BYTES {
            return Err(error("GetAdaptersAddresses did not return a bounded required buffer"));
        }
        for _ in 0..2 {
            let words = (required as usize).div_ceil(size_of::<u64>());
            let mut storage = vec![0_u64; words];
            let mut size = required;
            let result = unsafe {
                GetAdaptersAddresses(
                    AF_UNSPEC,
                    GET_ADAPTERS_ADDRESSES_FLAGS(0),
                    None,
                    Some(storage.as_mut_ptr().cast()),
                    &mut size,
                )
            };
            if result == ERROR_BUFFER_OVERFLOW.0 && size as usize <= MAX_ADAPTER_BUFFER_BYTES {
                required = size;
                continue;
            }
            if result != ERROR_SUCCESS.0 {
                return Err(error(format!("GetAdaptersAddresses returned {result}")));
            }
            return copy_adapter_list(&storage);
        }
        Err(error("GetAdaptersAddresses changed its required size repeatedly"))
    }

    fn copy_adapter_list(storage: &[u64]) -> Result<Vec<IpAdapterRecord>, DeviceBindingError> {
        let start = storage.as_ptr() as usize;
        let end = start + std::mem::size_of_val(storage);
        let mut current = start;
        let mut result = Vec::new();
        for _ in 0..MAX_ADAPTERS {
            if current < start || current.checked_add(size_of::<IP_ADAPTER_ADDRESSES_LH>()) > Some(end) {
                return Err(error("IP Helper returned an adapter pointer outside its buffer"));
            }
            let adapter = unsafe { &*(current as *const IP_ADAPTER_ADDRESSES_LH) };
            let next = adapter.Next as usize;
            result.push(adapter_record(adapter, start, end)?);
            if next == 0 {
                return Ok(result);
            }
            current = next;
        }
        Err(error("IP Helper adapter list exceeds the safe record limit"))
    }

    fn adapter_record(
        adapter: &IP_ADAPTER_ADDRESSES_LH,
        start: usize,
        end: usize,
    ) -> Result<IpAdapterRecord, DeviceBindingError> {
        let physical_address_length = usize::try_from(adapter.PhysicalAddressLength)
            .map_err(|_| error("network physical address length overflows usize"))?;
        if physical_address_length > adapter.PhysicalAddress.len() {
            return Err(error("network physical address length exceeds IP Helper storage"));
        }
        Ok(IpAdapterRecord {
            guid: adapter_guid(adapter, start, end)?,
            luid: unsafe { adapter.Luid.Value },
            interface_index: unsafe { adapter.Anonymous1.Anonymous.IfIndex },
            friendly_name: wide_text(adapter.FriendlyName.0, "adapter friendly name", start, end)?,
            description: wide_text(adapter.Description.0, "adapter description", start, end)?,
            physical_address: adapter.PhysicalAddress[..physical_address_length].to_vec(),
            is_up: adapter.OperStatus == IfOperStatusUp,
            is_wired: adapter.IfType == IF_TYPE_ETHERNET_CSMACD,
        })
    }

    fn adapter_guid(
        adapter: &IP_ADAPTER_ADDRESSES_LH,
        start: usize,
        end: usize,
    ) -> Result<String, DeviceBindingError> {
        if adapter.AdapterName.0.is_null() {
            return Err(error("IP Helper adapter has no AdapterName"));
        }
        let mut bytes = Vec::new();
        for index in 0..MAX_WIDE_UNITS {
            let address = (adapter.AdapterName.0 as usize)
                .checked_add(index)
                .ok_or_else(|| error("IP Helper AdapterName overflows"))?;
            if address < start || address >= end {
                return Err(error("IP Helper AdapterName points outside its buffer"));
            }
            let byte = unsafe { (address as *const u8).read() };
            if byte == 0 {
                break;
            }
            bytes.push(byte);
        }
        if bytes.len() == MAX_WIDE_UNITS {
            return Err(error("IP Helper AdapterName is not bounded"));
        }
        let raw = std::str::from_utf8(&bytes)
            .map_err(|reason| error(format!("IP Helper AdapterName is not UTF-8: {reason}")))?;
        let value = GUID::try_from(raw.trim_matches(['{', '}']))
            .map_err(|_| error("IP Helper AdapterName is not a GUID"))?;
        Ok(format!("{{{value:?}}}"))
    }

    fn wide_text(
        pointer: *mut u16,
        label: &str,
        start: usize,
        end: usize,
    ) -> Result<String, DeviceBindingError> {
        if pointer.is_null() {
            return Err(error(format!("{label} is null")));
        }
        let mut units = Vec::new();
        for index in 0..MAX_WIDE_UNITS {
            let address = (pointer as usize)
                .checked_add(index * size_of::<u16>())
                .ok_or_else(|| error(format!("{label} overflows")))?;
            if address < start || address.checked_add(size_of::<u16>()) > Some(end) {
                return Err(error(format!("{label} points outside its buffer")));
            }
            let unit = unsafe { (address as *const u16).read() };
            if unit == 0 {
                return String::from_utf16(&units)
                    .map_err(|reason| error(format!("{label} is not valid UTF-16: {reason}")));
            }
            units.push(unit);
        }
        Err(error(format!("{label} is not bounded")))
    }

    fn error(message: impl Into<String>) -> DeviceBindingError {
        DeviceBindingError::InvalidNetworkBinding(message.into())
    }
}
