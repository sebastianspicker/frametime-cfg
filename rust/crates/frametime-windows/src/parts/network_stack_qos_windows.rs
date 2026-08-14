//! Typed local QoS provider and NLA registry boundary for P1:16.

use frametime_core::{
    NetworkStackNlaBackup, NetworkStackPolicy, NetworkStackPolicySnapshot,
    NetworkStackRawRegistryValue,
};
use windows::{
    Win32::{
        Foundation::ERROR_FILE_NOT_FOUND,
        System::{
            Registry::{
                HKEY, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_CREATED_NEW_KEY,
                REG_OPTION_NON_VOLATILE, REG_SZ, REG_VALUE_TYPE, RegCloseKey, RegCreateKeyExW,
                RegDeleteKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryInfoKeyW,
                RegQueryValueExW, RegSetValueExW,
            },
            Wmi::{WBEM_FLAG_CREATE_ONLY, WBEM_GENERIC_FLAG_TYPE},
        },
    },
    core::{BSTR, PCWSTR},
};

use crate::wmi::{
    boolean, nullable_string, on_mta, property_qualifier_bstr_array, put_bool, put_sint8,
    put_string, put_uint8, put_uint16, put_uint32, put_uint64, query, require_class, services_at,
    sint8, string, uint8, uint16, uint32, uint64,
};

const QOS_NAMESPACE: &str = "ROOT\\StandardCimv2";
const QOS_CLASS: &str = "MSFT_NetQosPolicySettingData";
const QOS_NAME_PORTS: &str = "CS2_UDP_Ports";
const QOS_NAME_APP: &str = "CS2_App";
const QOS_PRECEDENCE: u32 = 127;
const QOS_DSCP: i8 = 46;
const QOS_NLA_KEY: &str = r"SYSTEM\CurrentControlSet\Services\Tcpip\QoS";
const QOS_NLA_NAME: &str = "Do not use NLA";
const MAX_NLA_BYTES: usize = 64;

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

pub(super) fn read_nla() -> Result<NetworkStackNlaBackup, String> {
    let Some(key) = open_nla(KEY_QUERY_VALUE)? else {
        return Ok(NetworkStackNlaBackup {
            key_existed: false,
            value_existed: false,
            original_value: None,
        });
    };
    let original_value = read_nla_raw(&key)?;
    Ok(NetworkStackNlaBackup {
        key_existed: true,
        value_existed: original_value.is_some(),
        original_value,
    })
}

pub(super) fn write_nla_target() -> Result<(), String> {
    let key = create_nla()?;
    let bytes = target_nla_bytes();
    unsafe {
        RegSetValueExW(
            key.0,
            PCWSTR(wide(QOS_NLA_NAME).as_ptr()),
            None,
            REG_SZ,
            Some(&bytes),
        )
    }
    .ok()
    .map_err(|error| format!("P1:16 write Do not use NLA: {error}"))?;
    if read_nla_raw(&key)? != Some(target_nla_value()) {
        return Err("P1:16 Do not use NLA readback differs from its exact REG_SZ write".into());
    }
    Ok(())
}

pub(super) fn nla_is_fixed_target() -> Result<bool, String> {
    Ok(read_nla()?.original_value == Some(target_nla_value()))
}

pub(super) fn restore_nla(captured: &NetworkStackNlaBackup) -> Result<(), String> {
    if captured.value_existed {
        let value = captured
            .original_value
            .as_ref()
            .ok_or("P1:16 NLA backup lacks its original raw value")?;
        let Some(key) = open_nla(KEY_QUERY_VALUE | KEY_SET_VALUE)? else {
            return Err("P1:16 QoS key disappeared before raw NLA restore".into());
        };
        write_nla_raw(&key, value)?;
        if read_nla_raw(&key)?.as_ref() != Some(value) {
            return Err("P1:16 Do not use NLA did not retain its raw restore".into());
        }
        return Ok(());
    }
    if captured.original_value.is_some() || (!captured.key_existed && captured.value_existed) {
        return Err("P1:16 invalid NLA absence backup".into());
    }
    let Some(key) = open_nla(KEY_QUERY_VALUE | KEY_SET_VALUE)? else {
        return if captured.key_existed {
            Err("P1:16 QoS key disappeared before NLA absence restore".into())
        } else {
            Ok(())
        };
    };
    match read_nla_raw(&key)? {
        None => return Ok(()),
        Some(value) if value == target_nla_value() => {}
        Some(_) => return Err("P1:16 Do not use NLA changed outside the suite lifecycle".into()),
    }
    unsafe { RegDeleteValueW(key.0, PCWSTR(wide(QOS_NLA_NAME).as_ptr())) }
        .ok()
        .map_err(|error| format!("P1:16 delete Do not use NLA: {error}"))?;
    if read_nla_raw(&key)?.is_some() {
        return Err("P1:16 Do not use NLA remained after delete".into());
    }
    if !captured.key_existed && nla_key_is_empty(&key)? {
        drop(key);
        unsafe {
            RegDeleteKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(wide(QOS_NLA_KEY).as_ptr()),
                KEY_SET_VALUE.0,
                None,
            )
        }
        .ok()
        .map_err(|error| format!("P1:16 delete suite-created QoS key: {error}"))?;
        if open_nla(KEY_QUERY_VALUE)?.is_some() {
            return Err("P1:16 suite-created empty QoS key remained after delete".into());
        }
    }
    Ok(())
}

pub(super) fn read_policy(
    policy: NetworkStackPolicy,
) -> Result<Option<NetworkStackPolicySnapshot>, String> {
    on_mta(move || read_policy_on_mta(policy))
}

pub(super) fn policy_is_repository_owned(
    policy: NetworkStackPolicy,
    snapshot: &NetworkStackPolicySnapshot,
) -> Result<bool, String> {
    snapshot.validate().map_err(|error| error.to_string())?;
    let snapshot = snapshot.clone();
    on_mta(move || Ok(snapshot == fixed_policy(policy, network_profile_all()?)))
}

pub(super) fn write_policy(policy: NetworkStackPolicy) -> Result<(), String> {
    on_mta(move || write_policy_on_mta(policy, &fixed_policy(policy, network_profile_all()?)))
}

pub(super) fn delete_policy(policy: NetworkStackPolicy) -> Result<(), String> {
    on_mta(move || delete_policy_on_mta(policy))
}

pub(super) fn restore_policy(
    policy: NetworkStackPolicy,
    snapshot: &NetworkStackPolicySnapshot,
) -> Result<(), String> {
    snapshot.validate().map_err(|error| error.to_string())?;
    let snapshot = snapshot.clone();
    on_mta(move || write_policy_on_mta(policy, &snapshot))
}

fn open_nla(
    rights: windows::Win32::System::Registry::REG_SAM_FLAGS,
) -> Result<Option<RegistryKey>, String> {
    let mut key = HKEY::default();
    let result = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide(QOS_NLA_KEY).as_ptr()),
            None,
            rights,
            &mut key,
        )
    };
    if result == ERROR_FILE_NOT_FOUND {
        Ok(None)
    } else if result.0 == 0 {
        Ok(Some(RegistryKey(key)))
    } else {
        Err(format!(
            "P1:16 open QoS registry key failed with {}",
            result.0
        ))
    }
}

fn create_nla() -> Result<RegistryKey, String> {
    let mut key = HKEY::default();
    let mut disposition = Default::default();
    unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide(QOS_NLA_KEY).as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            None,
            &mut key,
            Some(&mut disposition),
        )
    }
    .ok()
    .map_err(|error| format!("P1:16 create QoS registry key: {error}"))?;
    let _created = disposition == REG_CREATED_NEW_KEY;
    Ok(RegistryKey(key))
}

fn read_nla_raw(key: &RegistryKey) -> Result<Option<NetworkStackRawRegistryValue>, String> {
    let name = wide(QOS_NLA_NAME);
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
    if first.0 != 0 || size as usize > MAX_NLA_BYTES {
        return Err("P1:16 Do not use NLA has an unsupported registry type or size".into());
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
        return Err("P1:16 Do not use NLA changed during its typed read".into());
    }
    Ok(Some(NetworkStackRawRegistryValue {
        value_type: kind.0,
        bytes,
    }))
}

fn write_nla_raw(key: &RegistryKey, value: &NetworkStackRawRegistryValue) -> Result<(), String> {
    if value.bytes.len() > MAX_NLA_BYTES {
        return Err("P1:16 NLA raw restore exceeds its registry bound".into());
    }
    unsafe {
        RegSetValueExW(
            key.0,
            PCWSTR(wide(QOS_NLA_NAME).as_ptr()),
            None,
            REG_VALUE_TYPE(value.value_type),
            Some(&value.bytes),
        )
    }
    .ok()
    .map_err(|error| format!("P1:16 raw NLA restore: {error}"))
}

fn nla_key_is_empty(key: &RegistryKey) -> Result<bool, String> {
    let mut subkeys = 0;
    let mut values = 0;
    unsafe {
        RegQueryInfoKeyW(
            key.0,
            None,
            None,
            None,
            Some(&mut subkeys),
            None,
            None,
            Some(&mut values),
            None,
            None,
            None,
            None,
        )
    }
    .ok()
    .map_err(|error| format!("P1:16 inspect suite-created QoS key: {error}"))?;
    Ok(subkeys == 0 && values == 0)
}

fn target_nla_bytes() -> Vec<u8> {
    vec![b'1', 0, 0, 0]
}

fn target_nla_value() -> NetworkStackRawRegistryValue {
    NetworkStackRawRegistryValue {
        value_type: REG_SZ.0,
        bytes: target_nla_bytes(),
    }
}

fn fixed_policy(policy: NetworkStackPolicy, network_profile: u32) -> NetworkStackPolicySnapshot {
    match policy {
        NetworkStackPolicy::Cs2UdpPorts => NetworkStackPolicySnapshot {
            network_profile,
            precedence: QOS_PRECEDENCE,
            template_match_condition: 0,
            user_match_condition: String::new(),
            ip_protocol: 2,
            ip_port_match_condition: 0,
            source_prefix_match_condition: String::new(),
            source_port_start: 0,
            source_port_end: 0,
            destination_prefix_match_condition: String::new(),
            destination_port_start: 27015,
            destination_port_end: 27036,
            app_path_match_condition: String::new(),
            uri_match_condition: String::new(),
            uri_recursive_match_condition: false,
            net_direct_port_match_condition: 0,
            priority_value_8021_action: -1,
            dscp_action: QOS_DSCP,
            min_bandwidth_weight_action: 0,
            throttle_rate_action: 0,
        },
        NetworkStackPolicy::Cs2App => NetworkStackPolicySnapshot {
            network_profile,
            precedence: QOS_PRECEDENCE,
            template_match_condition: 0,
            user_match_condition: String::new(),
            ip_protocol: 3,
            ip_port_match_condition: 0,
            source_prefix_match_condition: String::new(),
            source_port_start: 0,
            source_port_end: 0,
            destination_prefix_match_condition: String::new(),
            destination_port_start: 0,
            destination_port_end: 0,
            app_path_match_condition: r"*\cs2.exe".into(),
            uri_match_condition: String::new(),
            uri_recursive_match_condition: false,
            net_direct_port_match_condition: 0,
            priority_value_8021_action: -1,
            dscp_action: QOS_DSCP,
            min_bandwidth_weight_action: 0,
            throttle_rate_action: 0,
        },
    }
}

/// The NetQos provider owns this bitmask.  Do not substitute a copied numeric
/// mask: provider qualifiers can differ across supported Windows images.
///
/// The qualifier decoding itself is intentionally a hard gate until it is
/// exercised on a Windows VM; a provider whose schema cannot prove all three
/// profile bits must not receive a policy mutation.
fn network_profile_all() -> Result<u32, String> {
    on_mta(|| {
        let services = services_at(QOS_NAMESPACE)?;
        let class = crate::wmi::object(&services, QOS_CLASS)?;
        let bit_map = property_qualifier_bstr_array(&class, "NetworkProfile", "BitMap")?;
        let bit_values = property_qualifier_bstr_array(&class, "NetworkProfile", "BitValues")?;
        super::decode_network_profile_qualifiers(&bit_map, &bit_values)
    })
}

fn read_policy_on_mta(
    policy: NetworkStackPolicy,
) -> Result<Option<NetworkStackPolicySnapshot>, String> {
    let services = services_at(QOS_NAMESPACE)?;
    let records = query(&services, QOS_CLASS)?;
    let name = policy_name(policy);
    let matching = records
        .into_iter()
        .filter(|record| string(record, "Name").ok().as_deref() == Some(name))
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [] => Ok(None),
        [record] => snapshot(record).map(Some),
        _ => Err("P1:16 QoS provider returned duplicate same-name policies".into()),
    }
}

fn snapshot(
    record: &windows::Win32::System::Wmi::IWbemClassObject,
) -> Result<NetworkStackPolicySnapshot, String> {
    require_class(record, QOS_CLASS, "P1:16 QoS policy")?;
    let snapshot = NetworkStackPolicySnapshot {
        network_profile: uint32(record, "NetworkProfile")?,
        precedence: uint32(record, "Precedence")?,
        template_match_condition: uint32(record, "TemplateMatchCondition")?,
        user_match_condition: nullable_string(record, "UserMatchCondition")?,
        ip_protocol: uint32(record, "IPProtocolMatchCondition")?,
        ip_port_match_condition: uint16(record, "IPPortMatchCondition")?,
        source_prefix_match_condition: nullable_string(record, "IPSrcPrefixMatchCondition")?,
        source_port_start: uint16(record, "IPSrcPortStartMatchCondition")?,
        source_port_end: uint16(record, "IPSrcPortEndMatchCondition")?,
        destination_prefix_match_condition: nullable_string(record, "IPDstPrefixMatchCondition")?,
        destination_port_start: uint16(record, "IPDstPortStartMatchCondition")?,
        destination_port_end: uint16(record, "IPDstPortEndMatchCondition")?,
        app_path_match_condition: nullable_string(record, "AppPathNameMatchCondition")?,
        uri_match_condition: nullable_string(record, "URIMatchCondition")?,
        uri_recursive_match_condition: boolean(record, "URIRecursiveMatchCondition")?,
        net_direct_port_match_condition: uint16(record, "NetDirectPortMatchCondition")?,
        priority_value_8021_action: sint8(record, "PriorityValue8021Action")?,
        dscp_action: sint8(record, "DSCPAction")?,
        min_bandwidth_weight_action: uint8(record, "MinBandwidthWeightAction")?,
        throttle_rate_action: uint64(record, "ThrottleRateAction")?,
    };
    snapshot.validate().map_err(|error| error.to_string())?;
    Ok(snapshot)
}

fn write_policy_on_mta(
    policy: NetworkStackPolicy,
    desired: &NetworkStackPolicySnapshot,
) -> Result<(), String> {
    let services = services_at(QOS_NAMESPACE)?;
    let existing = read_policy_with_services(&services, policy)?;
    if let Some(existing) = &existing {
        if existing != desired {
            return Err("P1:16 same-name QoS policy is foreign and will not be replaced".into());
        }
        return Ok(());
    }
    let class = crate::wmi::object(&services, QOS_CLASS)?;
    let item = unsafe { class.SpawnInstance(0) }
        .map_err(|error| format!("P1:16 spawn QoS policy: {error}"))?;
    put_string(&item, "Name", policy_name(policy))?;
    put_uint32(&item, "NetworkProfile", desired.network_profile)?;
    put_uint32(&item, "Precedence", desired.precedence)?;
    put_uint32(
        &item,
        "TemplateMatchCondition",
        desired.template_match_condition,
    )?;
    put_string(&item, "UserMatchCondition", &desired.user_match_condition)?;
    put_uint32(&item, "IPProtocolMatchCondition", desired.ip_protocol)?;
    put_uint16(
        &item,
        "IPPortMatchCondition",
        desired.ip_port_match_condition,
    )?;
    put_string(
        &item,
        "IPSrcPrefixMatchCondition",
        &desired.source_prefix_match_condition,
    )?;
    put_uint16(
        &item,
        "IPSrcPortStartMatchCondition",
        desired.source_port_start,
    )?;
    put_uint16(&item, "IPSrcPortEndMatchCondition", desired.source_port_end)?;
    put_string(
        &item,
        "IPDstPrefixMatchCondition",
        &desired.destination_prefix_match_condition,
    )?;
    put_uint16(
        &item,
        "IPDstPortStartMatchCondition",
        desired.destination_port_start,
    )?;
    put_uint16(
        &item,
        "IPDstPortEndMatchCondition",
        desired.destination_port_end,
    )?;
    put_string(
        &item,
        "AppPathNameMatchCondition",
        &desired.app_path_match_condition,
    )?;
    put_string(&item, "URIMatchCondition", &desired.uri_match_condition)?;
    put_bool(
        &item,
        "URIRecursiveMatchCondition",
        desired.uri_recursive_match_condition,
    )?;
    put_uint16(
        &item,
        "NetDirectPortMatchCondition",
        desired.net_direct_port_match_condition,
    )?;
    put_sint8(
        &item,
        "PriorityValue8021Action",
        desired.priority_value_8021_action,
    )?;
    put_sint8(&item, "DSCPAction", desired.dscp_action)?;
    put_uint8(
        &item,
        "MinBandwidthWeightAction",
        desired.min_bandwidth_weight_action,
    )?;
    put_uint64(&item, "ThrottleRateAction", desired.throttle_rate_action)?;
    unsafe {
        services.PutInstance(
            &item,
            WBEM_GENERIC_FLAG_TYPE(WBEM_FLAG_CREATE_ONLY.0),
            None,
            None,
        )
    }
    .map_err(|error| format!("P1:16 CREATE_ONLY QoS policy: {error}"))?;
    if read_policy_with_services(&services, policy)?.as_ref() != Some(desired) {
        return Err("P1:16 QoS provider semantic readback did not match fixed policy".into());
    }
    Ok(())
}

fn delete_policy_on_mta(policy: NetworkStackPolicy) -> Result<(), String> {
    let services = services_at(QOS_NAMESPACE)?;
    let Some(record) = find_policy(&services, policy)? else {
        return Ok(());
    };
    let path = string(&record, "__PATH")?;
    unsafe { services.DeleteInstance(&BSTR::from(path), WBEM_GENERIC_FLAG_TYPE(0), None, None) }
        .map_err(|error| format!("P1:16 delete exact QoS policy: {error}"))?;
    if find_policy(&services, policy)?.is_some() {
        return Err("P1:16 QoS provider retained the deleted policy".into());
    }
    Ok(())
}

fn read_policy_with_services(
    services: &windows::Win32::System::Wmi::IWbemServices,
    policy: NetworkStackPolicy,
) -> Result<Option<NetworkStackPolicySnapshot>, String> {
    find_policy(services, policy)?.map_or(Ok(None), |record| snapshot(&record).map(Some))
}

fn find_policy(
    services: &windows::Win32::System::Wmi::IWbemServices,
    policy: NetworkStackPolicy,
) -> Result<Option<windows::Win32::System::Wmi::IWbemClassObject>, String> {
    let name = policy_name(policy);
    let matching = query(services, QOS_CLASS)?
        .into_iter()
        .filter(|record| string(record, "Name").ok().as_deref() == Some(name))
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [] => Ok(None),
        [record] => Ok(Some(record.clone())),
        _ => Err("P1:16 QoS provider returned duplicate same-name policies".into()),
    }
}

fn policy_name(policy: NetworkStackPolicy) -> &'static str {
    match policy {
        NetworkStackPolicy::Cs2UdpPorts => QOS_NAME_PORTS,
        NetworkStackPolicy::Cs2App => QOS_NAME_APP,
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
