use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::PciDeviceBinding;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InterruptPolicyKind {
    MsiSupported,
    MessageNumberLimit,
    DevicePolicy,
    AssignmentSetOverride,
}

impl InterruptPolicyKind {
    #[must_use]
    pub const fn value_name(self) -> &'static str {
        match self {
            Self::MsiSupported => "MSISupported",
            Self::MessageNumberLimit => "MessageNumberLimit",
            Self::DevicePolicy => "DevicePolicy",
            Self::AssignmentSetOverride => "AssignmentSetOverride",
        }
    }

    #[must_use]
    pub const fn expected_step(self) -> &'static str {
        match self {
            Self::MsiSupported | Self::MessageNumberLimit => "P3:2",
            Self::DevicePolicy | Self::AssignmentSetOverride => "P3:3",
        }
    }

    #[must_use]
    pub const fn registry_suffix(self) -> &'static str {
        match self {
            Self::MsiSupported | Self::MessageNumberLimit => {
                "Device Parameters\\Interrupt Management\\MessageSignaledInterruptProperties"
            }
            Self::DevicePolicy | Self::AssignmentSetOverride => {
                "Device Parameters\\Interrupt Management\\Affinity Policy"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum InterruptPolicyValue {
    Dword(u32),
    Binary(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterruptPolicyBackup {
    pub step: String,
    pub timestamp: String,
    pub device: PciDeviceBinding,
    pub policy: InterruptPolicyKind,
    pub existed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_value: Option<InterruptPolicyValue>,
    #[serde(flatten, default)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InterruptPolicyBackupError {
    #[error("interrupt-policy backup contains unrecognized fields")]
    UnknownFields,
    #[error("interrupt-policy backup step and policy do not match")]
    StepMismatch,
    #[error("interrupt-policy backup has invalid device evidence: {0}")]
    InvalidDevice(String),
    #[error("interrupt-policy backup existence and value do not agree")]
    ExistenceMismatch,
    #[error("interrupt-policy backup value type does not match its policy")]
    ValueTypeMismatch,
    #[error("interrupt-policy backup timestamp is invalid")]
    InvalidTimestamp,
}

impl InterruptPolicyBackup {
    pub fn validate(&self) -> Result<(), InterruptPolicyBackupError> {
        if !self.unknown.is_empty() {
            return Err(InterruptPolicyBackupError::UnknownFields);
        }
        if self.step != self.policy.expected_step() {
            return Err(InterruptPolicyBackupError::StepMismatch);
        }
        if self.timestamp.trim().is_empty()
            || self.timestamp.len() > 64
            || self.timestamp.chars().any(char::is_control)
        {
            return Err(InterruptPolicyBackupError::InvalidTimestamp);
        }
        self.device
            .validate()
            .map_err(|error| InterruptPolicyBackupError::InvalidDevice(error.to_string()))?;
        if self.existed != self.original_value.is_some() {
            return Err(InterruptPolicyBackupError::ExistenceMismatch);
        }
        match (&self.policy, &self.original_value) {
            (_, None)
            | (
                InterruptPolicyKind::MsiSupported
                | InterruptPolicyKind::MessageNumberLimit
                | InterruptPolicyKind::DevicePolicy,
                Some(InterruptPolicyValue::Dword(_)),
            )
            | (InterruptPolicyKind::AssignmentSetOverride, Some(InterruptPolicyValue::Binary(_))) => {
                Ok(())
            }
            _ => Err(InterruptPolicyBackupError::ValueTypeMismatch),
        }
    }

    #[must_use]
    pub fn registry_key(&self) -> String {
        format!(
            "SYSTEM\\CurrentControlSet\\Enum\\{}\\{}",
            self.device.instance_id,
            self.policy.registry_suffix()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::NATIVE_BINDING_SCHEMA_VERSION;

    fn device() -> PciDeviceBinding {
        PciDeviceBinding {
            schema_version: NATIVE_BINDING_SCHEMA_VERSION,
            instance_id: "PCI\\VEN_10DE&DEV_2684&SUBSYS_41051458&REV_A1\\1".into(),
            container_id: "{11111111-1111-1111-1111-111111111111}".into(),
            class_guid: "{4d36e968-e325-11ce-bfc1-08002be10318}".into(),
            vendor_id: 0x10de,
            device_id: 0x2684,
            subsystem_vendor_id: 0x1458,
            subsystem_device_id: 0x4105,
            revision_id: 0xa1,
            driver_provider: "NVIDIA".into(),
            driver_version: "32.0.15.8000".into(),
            published_inf: "oem42.inf".into(),
            observed_at_utc: "2026-08-13T10:00:00Z".into(),
            unknown: BTreeMap::new(),
        }
    }

    #[test]
    fn exact_interrupt_backup_round_trips_without_a_registry_path() {
        let backup = InterruptPolicyBackup {
            step: "P3:2".into(),
            timestamp: "2026-08-13T10:00:00Z".into(),
            device: device(),
            policy: InterruptPolicyKind::MsiSupported,
            existed: true,
            original_value: Some(InterruptPolicyValue::Dword(0)),
            unknown: BTreeMap::new(),
        };
        backup.validate().expect("valid backup");
        let encoded = serde_json::to_value(&backup).expect("encode");
        let decoded: InterruptPolicyBackup =
            serde_json::from_value(encoded).expect("decode exact backup");
        assert_eq!(decoded, backup);
        assert!(backup.registry_key().contains(&backup.device.instance_id));
    }

    #[test]
    fn policy_step_value_and_existence_are_bound() {
        let mut backup = InterruptPolicyBackup {
            step: "P3:3".into(),
            timestamp: "now".into(),
            device: device(),
            policy: InterruptPolicyKind::AssignmentSetOverride,
            existed: true,
            original_value: Some(InterruptPolicyValue::Dword(4)),
            unknown: BTreeMap::new(),
        };
        assert_eq!(
            backup.validate(),
            Err(InterruptPolicyBackupError::ValueTypeMismatch)
        );
        backup.original_value = None;
        assert_eq!(
            backup.validate(),
            Err(InterruptPolicyBackupError::ExistenceMismatch)
        );
    }
}
