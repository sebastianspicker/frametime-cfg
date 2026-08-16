//! Lossless, device-bound state for the closed P1:16 network latency stack.
//!
//! This deliberately does not reuse the legacy name-only NIC records.  A
//! restore is authorized only by a freshly observed `NetworkAdapterBinding`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::NetworkAdapterBinding;

pub const NETWORK_STACK_TRANSACTION_STEP: &str = "P1:16";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkStackSetting {
    Eee,
    FlowControl,
    InterruptModeration,
    ReceiveBuffers,
    TransmitBuffers,
    GreenEthernet,
    PowerSavingMode,
    RssEnabled,
    RssBaseProcessor,
    RssMaxProcessor,
    RssMaxProcessors,
    UroEnabled,
    QosNlaBypass,
}

impl NetworkStackSetting {
    pub const P1_16_INVENTORY: [Self; 5] = [
        Self::Eee,
        Self::FlowControl,
        Self::RssEnabled,
        Self::UroEnabled,
        Self::QosNlaBypass,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "value")]
pub enum NetworkStackValue {
    Dword(u32),
    String(String),
    MultiString(Vec<String>),
    Binary(Vec<u8>),
}

/// Lossless bounded registry data for the suite-owned QoS NLA value.  Unlike
/// driver keywords, this value may legitimately have any registry type before
/// P1:16 runs, so it must not be coerced through `NetworkStackValue`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStackRawRegistryValue {
    pub value_type: u32,
    pub bytes: Vec<u8>,
}

/// Capture state for `Tcpip\\QoS` / `Do not use NLA`.  Key and value absence
/// are separate facts: applying P1:16 may create the missing key and restore
/// may remove only that empty, suite-created key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStackNlaBackup {
    pub key_existed: bool,
    pub value_existed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_value: Option<NetworkStackRawRegistryValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStackSettingBackup {
    pub setting: NetworkStackSetting,
    pub existed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_value: Option<NetworkStackValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nla: Option<NetworkStackNlaBackup>,
    #[serde(flatten, default)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkStackPolicy {
    Cs2UdpPorts,
    Cs2App,
}

impl NetworkStackPolicy {
    pub const P1_16_INVENTORY: [Self; 2] = [Self::Cs2UdpPorts, Self::Cs2App];
}

/// Provider-independent, semantic snapshot of one repository-owned QoS
/// policy. Fixed policy names select the omitted protocol and port constants;
/// the remaining writable WMI fields are carried with their native types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStackPolicySnapshot {
    pub network_profile: u32,
    pub precedence: u32,
    pub template_match_condition: u32,
    pub user_match_condition: String,
    pub ip_protocol: u32,
    pub ip_port_match_condition: u16,
    pub source_prefix_match_condition: String,
    pub source_port_start: u16,
    pub source_port_end: u16,
    pub destination_prefix_match_condition: String,
    pub destination_port_start: u16,
    pub destination_port_end: u16,
    pub app_path_match_condition: String,
    pub uri_match_condition: String,
    pub uri_recursive_match_condition: bool,
    pub net_direct_port_match_condition: u16,
    pub priority_value_8021_action: i8,
    pub dscp_action: i8,
    pub min_bandwidth_weight_action: u8,
    pub throttle_rate_action: u64,
}

impl NetworkStackPolicySnapshot {
    pub fn validate(&self) -> Result<(), NetworkStackBackupError> {
        if self.precedence > 255
            || self.template_match_condition != 0
            || !matches!(self.ip_protocol, 0..=3)
            || !(-1..=7).contains(&self.priority_value_8021_action)
            || !(-1..=63).contains(&self.dscp_action)
            || self.min_bandwidth_weight_action > 100
            || [
                &self.user_match_condition,
                &self.source_prefix_match_condition,
                &self.destination_prefix_match_condition,
                &self.app_path_match_condition,
                &self.uri_match_condition,
            ]
            .into_iter()
            .any(|value| value.len() > 1024 || value.chars().any(char::is_control))
        {
            return Err(NetworkStackBackupError::InvalidPolicySnapshot);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStackPolicyBackup {
    pub policy: NetworkStackPolicy,
    pub existed: bool,
    /// This remains semantic and typed so an opaque provider implementation
    /// cannot turn a foreign policy into a restorable backup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_policy: Option<NetworkStackPolicySnapshot>,
    #[serde(flatten, default)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStackTransaction {
    pub step: String,
    pub timestamp: String,
    pub adapter: NetworkAdapterBinding,
    pub settings: Vec<NetworkStackSettingBackup>,
    pub policies: Vec<NetworkStackPolicyBackup>,
    #[serde(flatten, default)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NetworkStackBackupError {
    #[error("network-stack backup contains unrecognized fields")]
    UnknownFields,
    #[error("network-stack backup is not P1:16")]
    WrongStep,
    #[error("network-stack backup has invalid adapter evidence: {0}")]
    InvalidAdapter(String),
    #[error("network-stack backup timestamp is invalid")]
    InvalidTimestamp,
    #[error("network-stack backup repeats a setting or policy")]
    DuplicateIdentity,
    #[error("network-stack backup has inconsistent existence state")]
    ExistenceMismatch,
    #[error("network-stack backup has a value incompatible with its fixed setting")]
    ValueTypeMismatch,
    #[error("network-stack backup has invalid bounded NLA registry data")]
    InvalidNlaValue,
    #[error("network-stack backup has an invalid typed QoS policy snapshot")]
    InvalidPolicySnapshot,
    #[error("network-stack backup does not contain the complete P1:16 inventory")]
    IncompleteInventory,
}

impl NetworkStackTransaction {
    pub fn validate(&self) -> Result<(), NetworkStackBackupError> {
        if self.step != NETWORK_STACK_TRANSACTION_STEP {
            return Err(NetworkStackBackupError::WrongStep);
        }
        if self.timestamp.trim().is_empty()
            || self.timestamp.len() > 64
            || self.timestamp.chars().any(char::is_control)
        {
            return Err(NetworkStackBackupError::InvalidTimestamp);
        }
        if !self.unknown.is_empty() {
            return Err(NetworkStackBackupError::UnknownFields);
        }
        self.adapter
            .validate()
            .map_err(|error| NetworkStackBackupError::InvalidAdapter(error.to_string()))?;
        let mut settings = self
            .settings
            .iter()
            .map(|item| item.setting)
            .collect::<Vec<_>>();
        settings.sort_unstable();
        if settings.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(NetworkStackBackupError::DuplicateIdentity);
        }
        let mut policies = self
            .policies
            .iter()
            .map(|item| item.policy)
            .collect::<Vec<_>>();
        policies.sort_unstable();
        if policies.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(NetworkStackBackupError::DuplicateIdentity);
        }
        if settings != NetworkStackSetting::P1_16_INVENTORY
            || policies != NetworkStackPolicy::P1_16_INVENTORY
        {
            return Err(NetworkStackBackupError::IncompleteInventory);
        }
        for item in &self.settings {
            if !item.unknown.is_empty() {
                return Err(NetworkStackBackupError::UnknownFields);
            }
            if item.existed != item.original_value.is_some()
                && item.setting != NetworkStackSetting::QosNlaBypass
            {
                return Err(NetworkStackBackupError::ExistenceMismatch);
            }
            if item.setting == NetworkStackSetting::QosNlaBypass {
                let Some(nla) = &item.nla else {
                    return Err(NetworkStackBackupError::ValueTypeMismatch);
                };
                if item.original_value.is_some() || item.existed != nla.value_existed {
                    return Err(NetworkStackBackupError::ExistenceMismatch);
                }
                if nla.value_existed != nla.original_value.is_some() {
                    return Err(NetworkStackBackupError::ExistenceMismatch);
                }
                if !nla.key_existed && nla.value_existed {
                    return Err(NetworkStackBackupError::ExistenceMismatch);
                }
                if nla
                    .original_value
                    .as_ref()
                    .is_some_and(|value| value.bytes.len() > 64)
                {
                    return Err(NetworkStackBackupError::InvalidNlaValue);
                }
                continue;
            }
            if item.nla.is_some() {
                return Err(NetworkStackBackupError::ValueTypeMismatch);
            }
            if let Some(value) = &item.original_value {
                let expected_string =
                    matches!(item.setting, NetworkStackSetting::InterruptModeration);
                if expected_string != matches!(value, NetworkStackValue::String(_)) {
                    return Err(NetworkStackBackupError::ValueTypeMismatch);
                }
            }
        }
        for item in &self.policies {
            if !item.unknown.is_empty() {
                return Err(NetworkStackBackupError::UnknownFields);
            }
            if item.existed != item.original_policy.is_some() {
                return Err(NetworkStackBackupError::ExistenceMismatch);
            }
            if let Some(policy) = &item.original_policy {
                policy.validate()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NATIVE_BINDING_SCHEMA_VERSION, PciDeviceBinding};

    fn transaction() -> NetworkStackTransaction {
        NetworkStackTransaction {
            step: NETWORK_STACK_TRANSACTION_STEP.into(),
            timestamp: "2026-08-13T00:00:00Z".into(),
            adapter: NetworkAdapterBinding {
                schema_version: NATIVE_BINDING_SCHEMA_VERSION,
                adapter_name: "{11111111-1111-1111-1111-111111111111}".into(),
                interface_guid: "{11111111-1111-1111-1111-111111111111}".into(),
                interface_luid: 1,
                interface_index: 1,
                friendly_name: "Ethernet".into(),
                interface_description: "Test NIC".into(),
                physical_address: vec![1, 2, 3, 4, 5, 6],
                device: PciDeviceBinding {
                    schema_version: NATIVE_BINDING_SCHEMA_VERSION,
                    instance_id: "PCI\\VEN_1234&DEV_5678&SUBSYS_56781234&REV_01\\1".into(),
                    container_id: "{22222222-2222-2222-2222-222222222222}".into(),
                    class_guid: "{4d36e972-e325-11ce-bfc1-08002be10318}".into(),
                    vendor_id: 0x1234,
                    device_id: 0x5678,
                    subsystem_vendor_id: 0x1234,
                    subsystem_device_id: 0x5678,
                    revision_id: 1,
                    driver_provider: "Test".into(),
                    driver_version: "1.0".into(),
                    published_inf: "oem1.inf".into(),
                    observed_at_utc: "2026-08-13T00:00:00Z".into(),
                    unknown: BTreeMap::new(),
                },
                observed_at_utc: "2026-08-13T00:00:00Z".into(),
                unknown: BTreeMap::new(),
            },
            settings: NetworkStackSetting::P1_16_INVENTORY
                .map(|setting| NetworkStackSettingBackup {
                    setting,
                    existed: true,
                    original_value: if setting == NetworkStackSetting::QosNlaBypass {
                        None
                    } else {
                        Some(NetworkStackValue::Dword(1))
                    },
                    nla: (setting == NetworkStackSetting::QosNlaBypass).then(|| {
                        NetworkStackNlaBackup {
                            key_existed: true,
                            value_existed: true,
                            original_value: Some(NetworkStackRawRegistryValue {
                                value_type: 1,
                                bytes: vec![b'0', 0, 0, 0],
                            }),
                        }
                    }),
                    unknown: BTreeMap::new(),
                })
                .to_vec(),
            policies: NetworkStackPolicy::P1_16_INVENTORY
                .map(|policy| NetworkStackPolicyBackup {
                    policy,
                    existed: false,
                    original_policy: None,
                    unknown: BTreeMap::new(),
                })
                .to_vec(),
            unknown: BTreeMap::new(),
        }
    }

    #[test]
    fn transaction_requires_exact_identity_types_and_unknown_free_state() {
        let mut value = transaction();
        value.validate().expect("valid transaction");
        value.settings[0].original_value = Some(NetworkStackValue::String("1".into()));
        assert_eq!(
            value.validate(),
            Err(NetworkStackBackupError::ValueTypeMismatch)
        );
        value = transaction();
        value.settings.push(value.settings[0].clone());
        assert_eq!(
            value.validate(),
            Err(NetworkStackBackupError::DuplicateIdentity)
        );
    }

    #[test]
    fn typed_qos_snapshot_rejects_out_of_contract_values() {
        let mut value = transaction();
        value.policies[0].existed = true;
        value.policies[0].original_policy = Some(NetworkStackPolicySnapshot {
            network_profile: 7,
            precedence: 256,
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
            dscp_action: 46,
            min_bandwidth_weight_action: 0,
            throttle_rate_action: 0,
        });
        assert_eq!(
            value.validate(),
            Err(NetworkStackBackupError::InvalidPolicySnapshot)
        );
    }

    #[test]
    fn transaction_requires_complete_p1_16_inventory() {
        let mut value = transaction();
        value.settings.pop();
        assert_eq!(
            value.validate(),
            Err(NetworkStackBackupError::IncompleteInventory)
        );
        let mut value = transaction();
        value.policies.pop();
        assert_eq!(
            value.validate(),
            Err(NetworkStackBackupError::IncompleteInventory)
        );
    }

    #[test]
    fn nla_backup_requires_exact_bounded_key_and_value_facts() {
        let mut value = transaction();
        let nla = value
            .settings
            .iter_mut()
            .find(|item| item.setting == NetworkStackSetting::QosNlaBypass)
            .and_then(|item| item.nla.as_mut())
            .unwrap();
        nla.key_existed = false;
        assert_eq!(
            value.validate(),
            Err(NetworkStackBackupError::ExistenceMismatch)
        );

        let mut value = transaction();
        let nla = value
            .settings
            .iter_mut()
            .find(|item| item.setting == NetworkStackSetting::QosNlaBypass)
            .and_then(|item| item.nla.as_mut())
            .unwrap();
        nla.original_value.as_mut().unwrap().bytes = vec![0; 65];
        assert_eq!(
            value.validate(),
            Err(NetworkStackBackupError::InvalidNlaValue)
        );
    }
}
