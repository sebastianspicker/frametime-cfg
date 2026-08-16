//! Windows-only adapter, driver-key, QoS, and NLA dispatch for P1:16.

use frametime_core::{
    NetworkAdapterBinding, NetworkStackNlaBackup, NetworkStackPolicy, NetworkStackPolicySnapshot,
    NetworkStackSetting, NetworkStackValue,
};
use windows::Win32::NetworkManagement::IpHelper::{GetIfEntry2, MIB_IF_ROW2};
use windows::Win32::NetworkManagement::Ndis::NET_LUID_LH;

use super::{network_stack_qos_windows, network_stack_windows};

fn registry_name(setting: NetworkStackSetting) -> Option<&'static str> {
    match setting {
        NetworkStackSetting::Eee => Some("*EEE"),
        NetworkStackSetting::FlowControl => Some("*FlowControl"),
        NetworkStackSetting::RssEnabled => Some("*RSS"),
        NetworkStackSetting::UroEnabled => Some("*UdpRsc"),
        _ => None,
    }
}

pub(super) fn link_speed(adapter: &NetworkAdapterBinding) -> Result<u64, String> {
    let mut row = MIB_IF_ROW2 {
        InterfaceLuid: NET_LUID_LH {
            Value: adapter.interface_luid,
        },
        ..Default::default()
    };
    let code = unsafe { GetIfEntry2(&mut row) }.0;
    if code != 0 || row.InterfaceIndex != adapter.interface_index {
        return Err(format!("P1:16 IP Helper reobservation failed: {code}"));
    }
    Ok(row.TransmitLinkSpeed.max(row.ReceiveLinkSpeed))
}

pub(super) fn read_setting(
    adapter: &NetworkAdapterBinding,
    setting: NetworkStackSetting,
) -> Result<Option<NetworkStackValue>, String> {
    let Some(name) = registry_name(setting) else {
        return Ok(None);
    };
    network_stack_windows::read_driver_setting(adapter, name)
}

pub(super) fn write_setting(
    adapter: &NetworkAdapterBinding,
    setting: NetworkStackSetting,
    value: &NetworkStackValue,
) -> Result<(), String> {
    let name = registry_name(setting).ok_or("P1:16 setting has no native registry identity")?;
    network_stack_windows::write_driver_setting(adapter, name, value)
}

pub(super) fn read_nla() -> Result<NetworkStackNlaBackup, String> {
    network_stack_qos_windows::read_nla()
}
pub(super) fn write_nla_target() -> Result<(), String> {
    network_stack_qos_windows::write_nla_target()
}
pub(super) fn nla_is_fixed_target() -> Result<bool, String> {
    network_stack_qos_windows::nla_is_fixed_target()
}
pub(super) fn restore_nla(captured: &NetworkStackNlaBackup) -> Result<(), String> {
    network_stack_qos_windows::restore_nla(captured)
}
pub(super) fn read_policy(
    policy: NetworkStackPolicy,
) -> Result<Option<NetworkStackPolicySnapshot>, String> {
    network_stack_qos_windows::read_policy(policy)
}
pub(super) fn policy_is_repository_owned(
    policy: NetworkStackPolicy,
    snapshot: &NetworkStackPolicySnapshot,
) -> Result<bool, String> {
    network_stack_qos_windows::policy_is_repository_owned(policy, snapshot)
}
pub(super) fn write_policy(policy: NetworkStackPolicy) -> Result<(), String> {
    network_stack_qos_windows::write_policy(policy)
}
pub(super) fn delete_policy(policy: NetworkStackPolicy) -> Result<(), String> {
    network_stack_qos_windows::delete_policy(policy)
}
pub(super) fn restore_policy(
    policy: NetworkStackPolicy,
    snapshot: &NetworkStackPolicySnapshot,
) -> Result<(), String> {
    network_stack_qos_windows::restore_policy(policy, snapshot)
}
