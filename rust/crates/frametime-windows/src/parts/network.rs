#[cfg(any(test, windows))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct PhysicalEthernetCandidate {
    interface_guid: String,
    luid: u64,
    if_index: u32,
    link_speed: u64,
    is_ethernet: bool,
    is_hardware: bool,
    is_up: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NagleBinding {
    interface_guid: String,
    luid: u64,
    if_index: u32,
    link_speed: u64,
    registry_key: String,
}

const TCPIP_INTERFACE_PREFIX: &str =
    "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces\\";
const NAGLE_VALUE_NAMES: [&str; 2] = ["TcpNoDelay", "TcpAckFrequency"];

#[cfg(any(test, windows))]
fn select_unique_physical_ethernet(
    candidates: impl IntoIterator<Item = PhysicalEthernetCandidate>,
) -> Result<PhysicalEthernetCandidate, String> {
    let mut eligible = candidates
        .into_iter()
        .filter(|candidate| {
            candidate.is_ethernet
                && candidate.is_hardware
                && candidate.is_up
                && candidate.if_index != 0
                && candidate.link_speed > 0
                && valid_interface_guid(&candidate.interface_guid)
        })
        .collect::<Vec<_>>();
    eligible.sort_by_key(|candidate| std::cmp::Reverse(candidate.link_speed));
    let selected = eligible
        .first()
        .cloned()
        .ok_or("no Up physical Ethernet interface can be proven")?;
    if eligible
        .get(1)
        .is_some_and(|candidate| candidate.link_speed == selected.link_speed)
    {
        return Err("multiple highest-speed physical Ethernet interfaces make Nagle binding ambiguous".into());
    }
    Ok(selected)
}

fn valid_interface_guid(value: &str) -> bool {
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

fn nagle_registry_key(guid: &str) -> Result<String, String> {
    if !valid_interface_guid(guid) {
        return Err("network interface GUID is not an exact registry identity".into());
    }
    Ok(format!("{TCPIP_INTERFACE_PREFIX}{guid}"))
}

fn nagle_changes(binding: &NagleBinding) -> Vec<RegistryChange> {
    NAGLE_VALUE_NAMES
        .iter()
        .map(|name| RegistryChange {
            hive: Hive::LocalMachine,
            key: Box::leak(binding.registry_key.clone().into_boxed_str()),
            name,
            value: RegValue::Dword(1),
        })
        .collect()
}

fn capture_nagle_batch(step: String) -> Result<(NagleBinding, Vec<BackupEntry>), String> {
    let binding = discover_nagle_binding()?;
    let entries = nagle_changes(&binding)
        .iter()
        .map(|change| {
            let mut entry = capture_registry(change, step.clone())?;
            let BackupEntry::Registry { unknown, .. } = &mut entry else {
                return Err("Nagle capture did not create a registry backup".into());
            };
            unknown.insert("interfaceGuid".into(), Value::String(binding.interface_guid.clone()));
            unknown.insert("interfaceLuid".into(), Value::from(binding.luid));
            unknown.insert("interfaceIndex".into(), Value::from(binding.if_index));
            Ok(entry)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((binding, entries))
}

fn inspect_nagle() -> Result<Inspection, String> {
    let binding = discover_nagle_binding()?;
    let changes = nagle_changes(&binding);
    if changes
        .iter()
        .all(|change| registry_read(change).ok().flatten() == Some(RegValue::Dword(1)))
    {
        Ok(Inspection::Satisfied)
    } else {
        Ok(Inspection::NeedsApply)
    }
}

fn apply_nagle(binding: &NagleBinding) -> Result<(), String> {
    reobserve_nagle_binding(binding)?;
    for change in nagle_changes(binding) {
        registry_write(&change)?;
    }
    Ok(())
}

fn verify_nagle(binding: &NagleBinding) -> Result<(), String> {
    reobserve_nagle_binding(binding)?;
    for change in nagle_changes(binding) {
        if registry_read(&change)?.as_ref() != Some(&RegValue::Dword(1)) {
            return Err("Nagle registry readback did not equal DWORD 1".into());
        }
    }
    Ok(())
}

fn validate_nagle_restore_binding(
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
fn discover_nagle_binding() -> Result<NagleBinding, String> {
    native_network::discover()
}
#[cfg(not(windows))]
fn discover_nagle_binding() -> Result<NagleBinding, String> {
    Err("Nagle interface discovery requires Windows IP Helper".into())
}

#[cfg(windows)]
fn reobserve_nagle_binding(binding: &NagleBinding) -> Result<(), String> {
    native_network::reobserve(binding)
}
#[cfg(not(windows))]
fn reobserve_nagle_binding(_: &NagleBinding) -> Result<(), String> {
    Err("Nagle interface reobservation requires Windows IP Helper".into())
}

#[cfg(windows)]
mod native_network {
    use std::{ptr, slice};

    use windows::{
        Win32::{
        NetworkManagement::{
            IpHelper::{FreeMibTable, GetIfEntry2, GetIfTable2, MIB_IF_ROW2, MIB_IF_TABLE2},
            Ndis::{IfOperStatusUp, NET_LUID_LH},
        },
        },
        core::GUID,
    };

    use super::{
        NagleBinding, PhysicalEthernetCandidate, nagle_registry_key, select_unique_physical_ethernet,
    };

    struct MibTable(*mut MIB_IF_TABLE2);
    impl Drop for MibTable {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { FreeMibTable(self.0.cast()) };
            }
        }
    }

    pub(super) fn discover() -> Result<NagleBinding, String> {
        let mut raw = ptr::null_mut();
        let result = unsafe { GetIfTable2(&mut raw) };
        if result.0 != 0 || raw.is_null() {
            return Err(format!("enumerate IP Helper interfaces failed: {}", result.0));
        }
        let table = MibTable(raw);
        let count = unsafe { (*table.0).NumEntries as usize };
        let rows = unsafe { slice::from_raw_parts((*table.0).Table.as_ptr(), count) };
        let candidates = rows
            .iter()
            .map(|row| PhysicalEthernetCandidate {
                interface_guid: guid_string(row.InterfaceGuid),
                luid: unsafe { row.InterfaceLuid.Value },
                if_index: row.InterfaceIndex,
                link_speed: row.TransmitLinkSpeed.max(row.ReceiveLinkSpeed),
                is_ethernet: row.Type == 6,
                // HardwareInterface is the first documented flag bit. This
                // excludes filter, virtual, loopback, and tunnel interfaces.
                is_hardware: row.InterfaceAndOperStatusFlags._bitfield & 1 != 0,
                is_up: row.OperStatus == IfOperStatusUp,
            })
            .collect::<Vec<_>>();
        let selected = select_unique_physical_ethernet(candidates)?;
        Ok(NagleBinding {
            registry_key: nagle_registry_key(&selected.interface_guid)?,
            interface_guid: selected.interface_guid,
            luid: selected.luid,
            if_index: selected.if_index,
            link_speed: selected.link_speed,
        })
    }

    pub(super) fn reobserve(binding: &NagleBinding) -> Result<(), String> {
        let mut row = MIB_IF_ROW2 {
            InterfaceLuid: NET_LUID_LH { Value: binding.luid },
            ..Default::default()
        };
        let result = unsafe { GetIfEntry2(&mut row) };
        if result.0 != 0 {
            return Err(format!("reobserve captured IP Helper interface failed: {}", result.0));
        }
        let candidate = PhysicalEthernetCandidate {
            interface_guid: guid_string(row.InterfaceGuid),
            luid: unsafe { row.InterfaceLuid.Value },
            if_index: row.InterfaceIndex,
            link_speed: row.TransmitLinkSpeed.max(row.ReceiveLinkSpeed),
            is_ethernet: row.Type == 6,
            is_hardware: row.InterfaceAndOperStatusFlags._bitfield & 1 != 0,
            is_up: row.OperStatus == IfOperStatusUp,
        };
        if !candidate.is_ethernet
            || !candidate.is_hardware
            || !candidate.is_up
            || candidate.interface_guid != binding.interface_guid
            || candidate.luid != binding.luid
            || candidate.if_index != binding.if_index
        {
            return Err("captured Nagle interface no longer has the exact physical identity".into());
        }
        Ok(())
    }

    fn guid_string(guid: GUID) -> String {
        format!("{{{guid:?}}}")
    }
}
