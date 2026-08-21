// Native P1:16 network latency transaction. Driver configuration is a closed
// set of typed values; missing properties are inapplicable and identity drift
// stops the transaction.

use frametime_core::{
    NETWORK_STACK_TRANSACTION_STEP, NetworkAdapterBinding, NetworkStackNlaBackup,
    NetworkStackPolicy, NetworkStackPolicyBackup, NetworkStackPolicySnapshot, NetworkStackSetting,
    NetworkStackSettingBackup, NetworkStackTransaction, NetworkStackValue,
};

#[cfg(windows)]
#[path = "network_stack_native_windows.rs"]
mod native_network_stack;
#[cfg(windows)]
#[path = "network_stack_qos_windows.rs"]
mod network_stack_qos_windows;
#[cfg(windows)]
#[path = "network_stack_windows.rs"]
mod network_stack_windows;

// Only standardized keywords with portable numeric semantics are eligible.
// Vendor aliases, localized descriptors, RSS CPU placement, and buffer sizes
// require stronger device/topology or NDI range proof and remain skipped.
const SETTINGS: [NetworkStackSetting; 5] = [
    NetworkStackSetting::Eee,
    NetworkStackSetting::FlowControl,
    NetworkStackSetting::RssEnabled,
    NetworkStackSetting::UroEnabled,
    NetworkStackSetting::QosNlaBypass,
];
const POLICIES: [NetworkStackPolicy; 2] =
    [NetworkStackPolicy::Cs2UdpPorts, NetworkStackPolicy::Cs2App];

/// Decodes the provider's paired NetworkProfile qualifiers.  The provider
/// supplies the numeric masks, rather than P1:16 assuming a copied mask.
#[cfg(windows)]
fn decode_network_profile_qualifiers(
    bit_map: &[String],
    bit_values: &[String],
) -> Result<u32, String> {
    if bit_map.len() != bit_values.len() || bit_map.len() != 3 {
        return Err("P1:16 NetQos NetworkProfile qualifiers have an invalid paired schema".into());
    }
    let mut result = 0_u32;
    let mut seen = BTreeSet::new();
    for (numeric, label) in bit_map.iter().zip(bit_values) {
        let normalized = label.trim().to_ascii_lowercase();
        if !matches!(normalized.as_str(), "domain" | "public" | "private")
            || !seen.insert(normalized)
        {
            return Err("P1:16 NetQos NetworkProfile qualifier labels drifted".into());
        }
        let value = numeric
            .trim()
            .parse::<u32>()
            .map_err(|_| "P1:16 NetQos NetworkProfile qualifier is not an unsigned mask")?;
        if value == 0 || result & value != 0 {
            return Err(
                "P1:16 NetQos NetworkProfile qualifier masks are invalid or overlap".into(),
            );
        }
        result |= value;
    }
    if seen.len() != 3 {
        return Err("P1:16 NetQos NetworkProfile qualifiers are incomplete".into());
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DesiredSetting {
    Value(NetworkStackValueKind),
    Inapplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NetworkStackValueKind {
    Dword(u32),
    String(&'static str),
}

impl NetworkStackValueKind {
    fn into_value(self) -> NetworkStackValue {
        match self {
            Self::Dword(value) => NetworkStackValue::Dword(value),
            Self::String(value) => NetworkStackValue::String(value.into()),
        }
    }
}

/// Small, adversarial-testable boundary.  The production implementation uses
/// SetupAPI-derived driver registry properties, IP Helper identity checks, and
/// documented registry policy stores; it never invokes a command processor.
trait NetworkStackHost {
    fn discover_active_wired(&self) -> Result<Vec<NetworkAdapterBinding>, String>;
    fn link_speed_bps(&self, adapter: &NetworkAdapterBinding) -> Result<u64, String>;
    fn read_setting(
        &self,
        adapter: &NetworkAdapterBinding,
        setting: NetworkStackSetting,
    ) -> Result<Option<NetworkStackValue>, String>;
    fn write_setting(
        &self,
        adapter: &NetworkAdapterBinding,
        setting: NetworkStackSetting,
        value: &NetworkStackValue,
    ) -> Result<(), String>;
    fn read_nla(&self) -> Result<NetworkStackNlaBackup, String>;
    fn write_nla_target(&self) -> Result<(), String>;
    fn nla_is_fixed_target(&self) -> Result<bool, String>;
    fn restore_nla(&self, captured: &NetworkStackNlaBackup) -> Result<(), String>;
    fn read_policy(
        &self,
        policy: NetworkStackPolicy,
    ) -> Result<Option<NetworkStackPolicySnapshot>, String>;
    fn policy_is_repository_owned(
        &self,
        policy: NetworkStackPolicy,
        snapshot: &NetworkStackPolicySnapshot,
    ) -> Result<bool, String>;
    fn write_policy(&self, policy: NetworkStackPolicy) -> Result<(), String>;
    fn delete_policy(&self, policy: NetworkStackPolicy) -> Result<(), String>;
    fn restore_policy(
        &self,
        policy: NetworkStackPolicy,
        snapshot: &NetworkStackPolicySnapshot,
    ) -> Result<(), String>;
}

fn exact_adapter(
    captured: &NetworkAdapterBinding,
    observed: &[NetworkAdapterBinding],
) -> Result<NetworkAdapterBinding, String> {
    captured.validate().map_err(|error| error.to_string())?;
    let matches = observed
        .iter()
        .filter(|candidate| {
            candidate.validate().is_ok()
                && candidate
                    .adapter_name
                    .eq_ignore_ascii_case(&captured.adapter_name)
                && candidate
                    .interface_guid
                    .eq_ignore_ascii_case(&captured.interface_guid)
                && candidate.interface_luid == captured.interface_luid
                && candidate.interface_index == captured.interface_index
                && candidate.physical_address == captured.physical_address
                && candidate.device.same_pnp_device(&captured.device)
        })
        .cloned()
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [adapter] => Ok(adapter.clone()),
        [] => Err("P1:16 adapter no longer matches its GUID/LUID/PnP identity".into()),
        _ => Err("P1:16 adapter identity reobservation is ambiguous".into()),
    }
}

fn select_adapter(observed: Vec<NetworkAdapterBinding>) -> Result<NetworkAdapterBinding, String> {
    match observed.as_slice() {
        [adapter] => {
            adapter.validate().map_err(|error| error.to_string())?;
            Ok(adapter.clone())
        }
        [] => Err("P1:16 has no active physical wired adapter".into()),
        _ => Err("P1:16 has multiple active physical wired adapters".into()),
    }
}

fn desired(setting: NetworkStackSetting, _link_speed_bps: u64) -> DesiredSetting {
    let value = match setting {
        NetworkStackSetting::Eee | NetworkStackSetting::FlowControl => {
            NetworkStackValueKind::Dword(0)
        }
        NetworkStackSetting::RssEnabled => NetworkStackValueKind::Dword(1),
        NetworkStackSetting::UroEnabled => NetworkStackValueKind::Dword(0),
        NetworkStackSetting::QosNlaBypass => NetworkStackValueKind::String("1"),
        _ => return DesiredSetting::Inapplicable,
    };
    DesiredSetting::Value(value)
}

fn accepted_type(setting: NetworkStackSetting, value: &NetworkStackValue) -> bool {
    matches!(
        setting,
        NetworkStackSetting::Eee
            | NetworkStackSetting::FlowControl
            | NetworkStackSetting::RssEnabled
            | NetworkStackSetting::UroEnabled
    ) && matches!(value, NetworkStackValue::Dword(_))
        || setting == NetworkStackSetting::QosNlaBypass
            && matches!(value, NetworkStackValue::String(_))
}

fn capture_network_stack<H: NetworkStackHost>(
    host: &H,
    step: String,
) -> Result<(NetworkAdapterBinding, BackupEntry), String> {
    if step != NETWORK_STACK_TRANSACTION_STEP {
        return Err("network-stack capture must be P1:16".into());
    }
    let adapter = select_adapter(host.discover_active_wired()?)?;
    let mut settings = Vec::new();
    for setting in SETTINGS {
        if setting == NetworkStackSetting::QosNlaBypass {
            let nla = host.read_nla()?;
            settings.push(NetworkStackSettingBackup {
                setting,
                existed: nla.value_existed,
                original_value: None,
                nla: Some(nla),
                unknown: Default::default(),
            });
            continue;
        }
        let original_value = host.read_setting(&adapter, setting)?;
        if let Some(value) = &original_value
            && !accepted_type(setting, value)
        {
            return Err(format!(
                "P1:16 {setting:?} has an unsupported native value type"
            ));
        }
        settings.push(NetworkStackSettingBackup {
            setting,
            existed: original_value.is_some(),
            original_value,
            nla: None,
            unknown: Default::default(),
        });
    }
    let policies = POLICIES
        .into_iter()
        .map(|policy| {
            let original_policy = host.read_policy(policy)?;
            if let Some(snapshot) = &original_policy
                && !host.policy_is_repository_owned(policy, snapshot)?
            {
                return Err("P1:16 same-name QoS policy is foreign and cannot be captured".into());
            }
            Ok::<_, String>(NetworkStackPolicyBackup {
                policy,
                existed: original_policy.is_some(),
                original_policy,
                unknown: Default::default(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let transaction = NetworkStackTransaction {
        step,
        timestamp: timestamp(),
        adapter: adapter.clone(),
        settings,
        policies,
        unknown: Default::default(),
    };
    transaction.validate().map_err(|error| error.to_string())?;
    Ok((
        adapter,
        BackupEntry::NetworkStackTransaction {
            transaction: Box::new(transaction),
        },
    ))
}

fn reobserve<H: NetworkStackHost>(
    host: &H,
    captured: &NetworkAdapterBinding,
) -> Result<NetworkAdapterBinding, String> {
    exact_adapter(captured, &host.discover_active_wired()?)
}

fn apply_network_stack<H: NetworkStackHost>(
    host: &H,
    captured: &NetworkAdapterBinding,
) -> Result<(), String> {
    let link_speed_bps = host.link_speed_bps(&reobserve(host, captured)?)?;
    for setting in SETTINGS {
        if setting == NetworkStackSetting::QosNlaBypass {
            host.write_nla_target()?;
            if !host.nla_is_fixed_target()? {
                return Err("P1:16 Do not use NLA readback did not equal REG_SZ 1".into());
            }
            continue;
        }
        let DesiredSetting::Value(value) = desired(setting, link_speed_bps) else {
            continue;
        };
        let adapter = reobserve(host, captured)?;
        if host.read_setting(&adapter, setting)?.is_none() {
            continue;
        }
        host.write_setting(&adapter, setting, &value.into_value())?;
        if host.read_setting(&reobserve(host, captured)?, setting)? != Some(value.into_value()) {
            return Err(format!(
                "P1:16 {setting:?} readback did not equal the fixed target"
            ));
        }
    }
    for policy in POLICIES {
        let _ = reobserve(host, captured)?;
        host.write_policy(policy)?;
    }
    Ok(())
}

fn verify_network_stack<H: NetworkStackHost>(
    host: &H,
    captured: &NetworkAdapterBinding,
) -> Result<(), String> {
    let speed = host.link_speed_bps(&reobserve(host, captured)?)?;
    for setting in SETTINGS {
        if setting == NetworkStackSetting::QosNlaBypass {
            if !host.nla_is_fixed_target()? {
                return Err("P1:16 Do not use NLA readback did not equal REG_SZ 1".into());
            }
            continue;
        }
        let DesiredSetting::Value(expected) = desired(setting, speed) else {
            continue;
        };
        let actual = host.read_setting(&reobserve(host, captured)?, setting)?;
        if actual.is_some() && actual != Some(expected.into_value()) {
            return Err(format!(
                "P1:16 {setting:?} readback did not equal the fixed target"
            ));
        }
    }
    for policy in POLICIES {
        let Some(snapshot) = host.read_policy(policy)? else {
            return Err(format!("P1:16 {policy:?} is absent after its write"));
        };
        if !host.policy_is_repository_owned(policy, &snapshot)? {
            return Err(format!(
                "P1:16 {policy:?} semantic readback differs from fixed policy"
            ));
        }
    }
    Ok(())
}

fn restore_network_stack<H: NetworkStackHost>(host: &H, entry: &BackupEntry) -> Result<(), String> {
    let BackupEntry::NetworkStackTransaction { transaction } = entry else {
        return Err("network-stack restore received a non-network-stack backup".into());
    };
    transaction.validate().map_err(|error| error.to_string())?;
    let mut seen = BTreeSet::new();
    for item in &transaction.settings {
        if !seen.insert(item.setting) {
            return Err("network-stack backup repeats a setting".into());
        }
        let adapter = reobserve(host, &transaction.adapter)?;
        if item.setting == NetworkStackSetting::QosNlaBypass {
            let nla = item
                .nla
                .as_ref()
                .ok_or("P1:16 NLA backup lacks exact registry state")?;
            host.restore_nla(nla)?;
            if host.read_nla()? != *nla {
                return Err("P1:16 Do not use NLA restore readback did not match backup".into());
            }
            continue;
        }
        match &item.original_value {
            Some(value) => host.write_setting(&adapter, item.setting, value)?,
            None => continue, // Driver property absence is inapplicable, never synthesized.
        }
        if host.read_setting(&reobserve(host, &transaction.adapter)?, item.setting)?
            != item.original_value
        {
            return Err(format!(
                "P1:16 {:?} restore readback did not match backup",
                item.setting
            ));
        }
    }
    for policy in &transaction.policies {
        match &policy.original_policy {
            Some(snapshot) => host.restore_policy(policy.policy, snapshot)?,
            None => host.delete_policy(policy.policy)?,
        }
        let restored = host.read_policy(policy.policy)?;
        if restored != policy.original_policy {
            return Err(format!(
                "P1:16 {:?} restore readback did not match backup",
                policy.policy
            ));
        }
    }
    Ok(())
}

struct NativeNetworkStackHost;

#[cfg(windows)]
impl NetworkStackHost for NativeNetworkStackHost {
    fn discover_active_wired(&self) -> Result<Vec<NetworkAdapterBinding>, String> {
        use crate::{NetworkAdapterEnumerator, WindowsIpHelperNetworkAdapterEnumerator};
        WindowsIpHelperNetworkAdapterEnumerator
            .enumerate_network_adapters()
            .map(|rows| {
                rows.into_iter()
                    .filter(|row| row.is_up && row.is_physical && row.is_wired)
                    .map(|row| row.binding)
                    .collect()
            })
            .map_err(|error| error.to_string())
    }
    fn link_speed_bps(&self, adapter: &NetworkAdapterBinding) -> Result<u64, String> {
        native_network_stack::link_speed(adapter)
    }
    fn read_setting(
        &self,
        adapter: &NetworkAdapterBinding,
        setting: NetworkStackSetting,
    ) -> Result<Option<NetworkStackValue>, String> {
        native_network_stack::read_setting(adapter, setting)
    }
    fn write_setting(
        &self,
        adapter: &NetworkAdapterBinding,
        setting: NetworkStackSetting,
        value: &NetworkStackValue,
    ) -> Result<(), String> {
        native_network_stack::write_setting(adapter, setting, value)
    }
    fn read_nla(&self) -> Result<NetworkStackNlaBackup, String> {
        native_network_stack::read_nla()
    }
    fn write_nla_target(&self) -> Result<(), String> {
        native_network_stack::write_nla_target()
    }
    fn nla_is_fixed_target(&self) -> Result<bool, String> {
        native_network_stack::nla_is_fixed_target()
    }
    fn restore_nla(&self, captured: &NetworkStackNlaBackup) -> Result<(), String> {
        native_network_stack::restore_nla(captured)
    }
    fn read_policy(
        &self,
        policy: NetworkStackPolicy,
    ) -> Result<Option<NetworkStackPolicySnapshot>, String> {
        native_network_stack::read_policy(policy)
    }
    fn policy_is_repository_owned(
        &self,
        policy: NetworkStackPolicy,
        snapshot: &NetworkStackPolicySnapshot,
    ) -> Result<bool, String> {
        native_network_stack::policy_is_repository_owned(policy, snapshot)
    }
    fn write_policy(&self, policy: NetworkStackPolicy) -> Result<(), String> {
        native_network_stack::write_policy(policy)
    }
    fn delete_policy(&self, policy: NetworkStackPolicy) -> Result<(), String> {
        native_network_stack::delete_policy(policy)
    }
    fn restore_policy(
        &self,
        policy: NetworkStackPolicy,
        snapshot: &NetworkStackPolicySnapshot,
    ) -> Result<(), String> {
        native_network_stack::restore_policy(policy, snapshot)
    }
}

#[cfg(not(windows))]
impl NetworkStackHost for NativeNetworkStackHost {
    fn discover_active_wired(&self) -> Result<Vec<NetworkAdapterBinding>, String> {
        Err("P1:16 requires native Windows network adapters".into())
    }
    fn link_speed_bps(&self, _: &NetworkAdapterBinding) -> Result<u64, String> {
        Err("P1:16 requires native Windows network adapters".into())
    }
    fn read_setting(
        &self,
        _: &NetworkAdapterBinding,
        _: NetworkStackSetting,
    ) -> Result<Option<NetworkStackValue>, String> {
        Err("P1:16 requires native Windows network adapters".into())
    }
    fn write_setting(
        &self,
        _: &NetworkAdapterBinding,
        _: NetworkStackSetting,
        _: &NetworkStackValue,
    ) -> Result<(), String> {
        Err("P1:16 requires native Windows network adapters".into())
    }
    fn read_nla(&self) -> Result<NetworkStackNlaBackup, String> {
        Err("P1:16 requires native Windows QoS registry APIs".into())
    }
    fn write_nla_target(&self) -> Result<(), String> {
        Err("P1:16 requires native Windows QoS registry APIs".into())
    }
    fn nla_is_fixed_target(&self) -> Result<bool, String> {
        Err("P1:16 requires native Windows QoS registry APIs".into())
    }
    fn restore_nla(&self, _: &NetworkStackNlaBackup) -> Result<(), String> {
        Err("P1:16 requires native Windows QoS registry APIs".into())
    }
    fn read_policy(
        &self,
        _: NetworkStackPolicy,
    ) -> Result<Option<NetworkStackPolicySnapshot>, String> {
        Err("P1:16 requires native Windows QoS policy APIs".into())
    }
    fn policy_is_repository_owned(
        &self,
        _: NetworkStackPolicy,
        _: &NetworkStackPolicySnapshot,
    ) -> Result<bool, String> {
        Err("P1:16 requires native Windows QoS policy APIs".into())
    }
    fn write_policy(&self, _: NetworkStackPolicy) -> Result<(), String> {
        Err("P1:16 requires native Windows QoS policy APIs".into())
    }
    fn delete_policy(&self, _: NetworkStackPolicy) -> Result<(), String> {
        Err("P1:16 requires native Windows QoS policy APIs".into())
    }
    fn restore_policy(
        &self,
        _: NetworkStackPolicy,
        _: &NetworkStackPolicySnapshot,
    ) -> Result<(), String> {
        Err("P1:16 requires native Windows QoS policy APIs".into())
    }
}
