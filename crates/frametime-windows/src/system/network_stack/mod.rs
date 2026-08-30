use crate::*;
// Native P1:16 network latency transaction. Driver configuration is a closed
// set of typed values; missing properties are inapplicable and identity drift
// stops the transaction.

use frametime_domain::{
    NETWORK_STACK_TRANSACTION_STEP, NetworkAdapterBinding, NetworkStackNlaBackup,
    NetworkStackPolicy, NetworkStackPolicySnapshot, NetworkStackSetting, NetworkStackSettingBackup,
    NetworkStackTransaction, NetworkStackValue,
};

#[cfg(windows)]
mod native;
#[cfg(windows)]
mod qos;
#[cfg(windows)]
mod windows;

// RSS is the only automatic P1:16 target. It has a standardized portable
// meaning and is the supported receive-processing baseline for wired NICs.
// EEE, flow control, URO, interrupt moderation, buffer sizes, and checksum
// offloads retain the adapter or Windows default unless an explicit measured
// experiment authorizes a device-specific change.
pub(crate) const SETTINGS: [NetworkStackSetting; 1] = [NetworkStackSetting::RssEnabled];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesiredSetting {
    Value(NetworkStackValueKind),
    Inapplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NetworkStackValueKind {
    Dword(u32),
}

impl NetworkStackValueKind {
    pub(crate) fn into_value(self) -> NetworkStackValue {
        match self {
            Self::Dword(value) => NetworkStackValue::Dword(value),
        }
    }
}

/// Small, adversarial-testable boundary.  The production implementation uses
/// SetupAPI-derived driver registry properties, IP Helper identity checks, and
/// documented registry policy stores; it never invokes a command processor.
pub(crate) trait NetworkStackHost {
    fn discover_active_wired(&self) -> Result<Vec<NetworkAdapterBinding>, String>;
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
    fn restore_nla(&self, captured: &NetworkStackNlaBackup) -> Result<(), String>;
    fn read_policy(
        &self,
        policy: NetworkStackPolicy,
    ) -> Result<Option<NetworkStackPolicySnapshot>, String>;
    fn delete_policy(&self, policy: NetworkStackPolicy) -> Result<(), String>;
    fn restore_policy(
        &self,
        policy: NetworkStackPolicy,
        snapshot: &NetworkStackPolicySnapshot,
    ) -> Result<(), String>;
}

pub(crate) fn exact_adapter(
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

pub(crate) fn select_adapter(
    observed: Vec<NetworkAdapterBinding>,
) -> Result<NetworkAdapterBinding, String> {
    match observed.as_slice() {
        [adapter] => {
            adapter.validate().map_err(|error| error.to_string())?;
            Ok(adapter.clone())
        }
        [] => Err("P1:16 has no active physical wired adapter".into()),
        _ => Err("P1:16 has multiple active physical wired adapters".into()),
    }
}

pub(crate) fn desired(setting: NetworkStackSetting) -> DesiredSetting {
    let value = match setting {
        NetworkStackSetting::RssEnabled => NetworkStackValueKind::Dword(1),
        _ => return DesiredSetting::Inapplicable,
    };
    DesiredSetting::Value(value)
}

pub(crate) fn accepted_type(setting: NetworkStackSetting, value: &NetworkStackValue) -> bool {
    setting == NetworkStackSetting::RssEnabled && matches!(value, NetworkStackValue::Dword(_))
}

pub(crate) fn capture_network_stack<H: NetworkStackHost>(
    host: &H,
    step: String,
) -> Result<(NetworkAdapterBinding, BackupEntry), String> {
    if step != NETWORK_STACK_TRANSACTION_STEP {
        return Err("network-stack capture must be P1:16".into());
    }
    let adapter = select_adapter(host.discover_active_wired()?)?;
    let mut settings = Vec::new();
    for setting in SETTINGS {
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
    let policies = Vec::new();
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

pub(crate) fn reobserve<H: NetworkStackHost>(
    host: &H,
    captured: &NetworkAdapterBinding,
) -> Result<NetworkAdapterBinding, String> {
    exact_adapter(captured, &host.discover_active_wired()?)
}

pub(crate) fn apply_network_stack<H: NetworkStackHost>(
    host: &H,
    captured: &NetworkAdapterBinding,
) -> Result<(), String> {
    for setting in SETTINGS {
        let DesiredSetting::Value(value) = desired(setting) else {
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
    Ok(())
}

pub(crate) fn verify_network_stack<H: NetworkStackHost>(
    host: &H,
    captured: &NetworkAdapterBinding,
) -> Result<(), String> {
    for setting in SETTINGS {
        let DesiredSetting::Value(expected) = desired(setting) else {
            continue;
        };
        let actual = host.read_setting(&reobserve(host, captured)?, setting)?;
        if actual.is_some() && actual != Some(expected.into_value()) {
            return Err(format!(
                "P1:16 {setting:?} readback did not equal the fixed target"
            ));
        }
    }
    Ok(())
}

pub(crate) fn restore_network_stack<H: NetworkStackHost>(
    host: &H,
    entry: &BackupEntry,
) -> Result<(), String> {
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

pub(crate) struct NativeNetworkStackHost;

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
    fn read_setting(
        &self,
        adapter: &NetworkAdapterBinding,
        setting: NetworkStackSetting,
    ) -> Result<Option<NetworkStackValue>, String> {
        native::read_setting(adapter, setting)
    }
    fn write_setting(
        &self,
        adapter: &NetworkAdapterBinding,
        setting: NetworkStackSetting,
        value: &NetworkStackValue,
    ) -> Result<(), String> {
        native::write_setting(adapter, setting, value)
    }
    fn read_nla(&self) -> Result<NetworkStackNlaBackup, String> {
        native::read_nla()
    }
    fn restore_nla(&self, captured: &NetworkStackNlaBackup) -> Result<(), String> {
        native::restore_nla(captured)
    }
    fn read_policy(
        &self,
        policy: NetworkStackPolicy,
    ) -> Result<Option<NetworkStackPolicySnapshot>, String> {
        native::read_policy(policy)
    }
    fn delete_policy(&self, policy: NetworkStackPolicy) -> Result<(), String> {
        native::delete_policy(policy)
    }
    fn restore_policy(
        &self,
        policy: NetworkStackPolicy,
        snapshot: &NetworkStackPolicySnapshot,
    ) -> Result<(), String> {
        native::restore_policy(policy, snapshot)
    }
}

#[cfg(not(windows))]
impl NetworkStackHost for NativeNetworkStackHost {
    fn discover_active_wired(&self) -> Result<Vec<NetworkAdapterBinding>, String> {
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
    fn restore_nla(&self, _: &NetworkStackNlaBackup) -> Result<(), String> {
        Err("P1:16 requires native Windows QoS registry APIs".into())
    }
    fn read_policy(
        &self,
        _: NetworkStackPolicy,
    ) -> Result<Option<NetworkStackPolicySnapshot>, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub(crate) fn automatic_transaction_only_targets_supported_rss() {
        assert_eq!(SETTINGS, [NetworkStackSetting::RssEnabled]);
        assert_eq!(
            desired(NetworkStackSetting::RssEnabled),
            DesiredSetting::Value(NetworkStackValueKind::Dword(1))
        );
        assert_eq!(
            desired(NetworkStackSetting::Eee),
            DesiredSetting::Inapplicable
        );
        assert_eq!(
            desired(NetworkStackSetting::FlowControl),
            DesiredSetting::Inapplicable
        );
        assert_eq!(
            desired(NetworkStackSetting::UroEnabled),
            DesiredSetting::Inapplicable
        );
        assert_eq!(
            desired(NetworkStackSetting::QosNlaBypass),
            DesiredSetting::Inapplicable
        );
    }
}
