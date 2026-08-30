//! Windows-only adapter, driver-key, QoS, and NLA dispatch for P1:16.

use super::{qos, windows};
use frametime_domain::{
    NetworkAdapterBinding, NetworkStackNlaBackup, NetworkStackPolicy, NetworkStackPolicySnapshot,
    NetworkStackSetting, NetworkStackValue,
};

pub(crate) fn registry_name(setting: NetworkStackSetting) -> Option<&'static str> {
    match setting {
        NetworkStackSetting::Eee => Some("*EEE"),
        NetworkStackSetting::FlowControl => Some("*FlowControl"),
        NetworkStackSetting::RssEnabled => Some("*RSS"),
        NetworkStackSetting::UroEnabled => Some("*UdpRsc"),
        _ => None,
    }
}

pub(super) fn read_setting(
    adapter: &NetworkAdapterBinding,
    setting: NetworkStackSetting,
) -> Result<Option<NetworkStackValue>, String> {
    let Some(name) = registry_name(setting) else {
        return Ok(None);
    };
    windows::read_driver_setting(adapter, name)
}

pub(super) fn write_setting(
    adapter: &NetworkAdapterBinding,
    setting: NetworkStackSetting,
    value: &NetworkStackValue,
) -> Result<(), String> {
    let name = registry_name(setting).ok_or("P1:16 setting has no native registry identity")?;
    windows::write_driver_setting(adapter, name, value)
}

pub(super) fn read_nla() -> Result<NetworkStackNlaBackup, String> {
    qos::read_nla()
}
pub(super) fn restore_nla(captured: &NetworkStackNlaBackup) -> Result<(), String> {
    qos::restore_nla(captured)
}
pub(super) fn read_policy(
    policy: NetworkStackPolicy,
) -> Result<Option<NetworkStackPolicySnapshot>, String> {
    qos::read_policy(policy)
}
pub(super) fn delete_policy(policy: NetworkStackPolicy) -> Result<(), String> {
    qos::delete_policy(policy)
}
pub(super) fn restore_policy(
    policy: NetworkStackPolicy,
    snapshot: &NetworkStackPolicySnapshot,
) -> Result<(), String> {
    qos::restore_policy(policy, snapshot)
}
