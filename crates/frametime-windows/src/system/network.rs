use crate::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NagleBinding {
    pub(crate) interface_guid: String,
    pub(crate) luid: u64,
    pub(crate) if_index: u32,
    pub(crate) link_speed: u64,
    pub(crate) registry_key: String,
}

pub(crate) const TCPIP_INTERFACE_PREFIX: &str =
    "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces\\";
pub(crate) const NAGLE_VALUE_NAMES: [&str; 2] = ["TcpNoDelay", "TcpAckFrequency"];

pub(crate) fn valid_interface_guid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 38
        && bytes.first() == Some(&b'{')
        && bytes.last() == Some(&b'}')
        && [9, 14, 19, 24].iter().all(|index| bytes[*index] == b'-')
        && bytes[1..37]
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 8 | 13 | 18 | 23) || byte.is_ascii_hexdigit())
}

pub(crate) fn nagle_registry_key(guid: &str) -> Result<String, String> {
    if !valid_interface_guid(guid) {
        return Err("network interface GUID is not an exact registry identity".into());
    }
    Ok(format!("{TCPIP_INTERFACE_PREFIX}{guid}"))
}

pub(crate) fn validate_nagle_restore_binding(
    key: &str,
    name: &str,
    unknown: &BTreeMap<String, Value>,
) -> Result<(), String> {
    if !NAGLE_VALUE_NAMES.contains(&name) {
        return Err("Nagle restore value name is not allowlisted".into());
    }
    let guid = unknown
        .get("interfaceGuid")
        .and_then(Value::as_str)
        .ok_or("Nagle backup has no interface GUID")?;
    let luid = unknown
        .get("interfaceLuid")
        .and_then(Value::as_u64)
        .ok_or("Nagle backup has no interface LUID")?;
    let if_index = u32::try_from(
        unknown
            .get("interfaceIndex")
            .and_then(Value::as_u64)
            .ok_or("Nagle backup has no interface index")?,
    )
    .map_err(|_| "Nagle backup interface index exceeds u32")?;
    if key != nagle_registry_key(guid)? {
        return Err("Nagle backup registry key does not exactly bind its interface GUID".into());
    }
    reobserve_nagle_binding(&NagleBinding {
        interface_guid: guid.to_owned(),
        luid,
        if_index,
        link_speed: 0,
        registry_key: key.to_owned(),
    })
}

#[cfg(windows)]
pub(crate) fn reobserve_nagle_binding(binding: &NagleBinding) -> Result<(), String> {
    native_network::reobserve(binding)
}
#[cfg(not(windows))]
pub(crate) fn reobserve_nagle_binding(_: &NagleBinding) -> Result<(), String> {
    Err("Nagle interface reobservation requires Windows IP Helper".into())
}

#[cfg(windows)]
mod native_network {
    use windows::{
        Win32::NetworkManagement::{
            IpHelper::{GetIfEntry2, MIB_IF_ROW2},
            Ndis::{IfOperStatusUp, NET_LUID_LH},
        },
        core::GUID,
    };

    use super::NagleBinding;

    pub(super) fn reobserve(binding: &NagleBinding) -> Result<(), String> {
        let mut row = MIB_IF_ROW2 {
            InterfaceLuid: NET_LUID_LH {
                Value: binding.luid,
            },
            ..Default::default()
        };
        let result = unsafe { GetIfEntry2(&mut row) };
        if result.0 != 0 {
            return Err(format!(
                "reobserve captured IP Helper interface failed: {}",
                result.0
            ));
        }
        if row.Type != 6
            || row.InterfaceAndOperStatusFlags._bitfield & 1 == 0
            || row.OperStatus != IfOperStatusUp
            || guid_string(row.InterfaceGuid) != binding.interface_guid
            || unsafe { row.InterfaceLuid.Value } != binding.luid
            || row.InterfaceIndex != binding.if_index
        {
            return Err(
                "captured Nagle interface no longer has the exact physical identity".into(),
            );
        }
        Ok(())
    }

    pub(crate) fn guid_string(guid: GUID) -> String {
        format!("{{{guid:?}}}")
    }
}
