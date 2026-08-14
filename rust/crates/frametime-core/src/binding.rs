//! Platform-neutral identities captured by native Windows capability adapters.
//!
//! A display name, interface index, or registry path alone is never mutation
//! authority. These records bind the durable Windows identity to the transient
//! observation that a platform adapter must re-observe immediately before a
//! write or restore.

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const NATIVE_BINDING_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BindingError {
    #[error("{0} is not a canonical braced GUID")]
    InvalidGuid(&'static str),
    #[error("{0} is not a canonical PCI instance identity")]
    InvalidPciIdentity(&'static str),
    #[error("{0} is not a canonical driver INF leaf")]
    InvalidPublishedName(&'static str),
    #[error("{0} contains invalid or missing text")]
    InvalidText(&'static str),
    #[error("device numeric identity disagrees with its PCI instance id")]
    DeviceIdentityMismatch,
    #[error("network adapter permanent name and interface GUID disagree")]
    AdapterGuidMismatch,
    #[error("network adapter has no usable interface or physical identity")]
    IncompleteAdapterIdentity,
    #[error("binding schema version is unsupported")]
    UnsupportedSchema,
    #[error("binding receipt digest is invalid")]
    InvalidReceiptId,
}

/// Lower-case SHA-256 digest used to bind a prerequisite observation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingReceiptId(String);

impl BindingReceiptId {
    pub fn parse(value: impl Into<String>) -> Result<Self, BindingError> {
        let value = value.into();
        if value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(value))
        } else {
            Err(BindingError::InvalidReceiptId)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn digest_serializable(value: &impl Serialize) -> Result<Self, BindingError> {
        let bytes = serde_json::to_vec(value).map_err(|_| BindingError::InvalidReceiptId)?;
        Self::parse(format!("{:x}", Sha256::digest(bytes)))
    }
}

impl fmt::Display for BindingReceiptId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for BindingReceiptId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for BindingReceiptId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Exact SetupAPI/PnP evidence for one PCI device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PciDeviceBinding {
    pub schema_version: u8,
    pub instance_id: String,
    pub container_id: String,
    pub class_guid: String,
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_vendor_id: u16,
    pub subsystem_device_id: u16,
    pub revision_id: u8,
    pub driver_provider: String,
    pub driver_version: String,
    pub published_inf: String,
    pub observed_at_utc: String,
    #[serde(flatten, default)]
    pub unknown: BTreeMap<String, Value>,
}

impl PciDeviceBinding {
    pub fn validate(&self) -> Result<(), BindingError> {
        require_schema(self.schema_version)?;
        require_guid(&self.container_id, "containerId")?;
        require_guid(&self.class_guid, "classGuid")?;
        require_text(&self.driver_provider, "driverProvider")?;
        require_text(&self.driver_version, "driverVersion")?;
        require_text(&self.observed_at_utc, "observedAtUtc")?;
        require_inf_leaf(&self.published_inf, "publishedInf")?;
        let parsed = parse_pci_instance(&self.instance_id)?;
        if parsed
            != (
                self.vendor_id,
                self.device_id,
                self.subsystem_vendor_id,
                self.subsystem_device_id,
                self.revision_id,
            )
        {
            return Err(BindingError::DeviceIdentityMismatch);
        }
        Ok(())
    }

    pub fn receipt_id(&self) -> Result<BindingReceiptId, BindingError> {
        self.validate()?;
        BindingReceiptId::digest_serializable(self)
    }

    /// Compare only the durable PnP/PCI subject identity. Driver metadata and
    /// observation time are deliberately excluded because a later driver
    /// transaction may legitimately replace them for the same physical device.
    #[must_use]
    pub fn same_pnp_device(&self, other: &Self) -> bool {
        self.instance_id.eq_ignore_ascii_case(&other.instance_id)
            && self.container_id.eq_ignore_ascii_case(&other.container_id)
            && self.class_guid.eq_ignore_ascii_case(&other.class_guid)
            && self.vendor_id == other.vendor_id
            && self.device_id == other.device_id
            && self.subsystem_vendor_id == other.subsystem_vendor_id
            && self.subsystem_device_id == other.subsystem_device_id
            && self.revision_id == other.revision_id
    }
}

/// Durable and transient evidence for one active physical network adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkAdapterBinding {
    pub schema_version: u8,
    /// Permanent `IP_ADAPTER_ADDRESSES::AdapterName`, represented as a GUID.
    pub adapter_name: String,
    /// COM interface GUID used by the per-interface DNS APIs.
    pub interface_guid: String,
    pub interface_luid: u64,
    pub interface_index: u32,
    pub friendly_name: String,
    pub interface_description: String,
    #[serde(default)]
    pub physical_address: Vec<u8>,
    pub device: PciDeviceBinding,
    pub observed_at_utc: String,
    #[serde(flatten, default)]
    pub unknown: BTreeMap<String, Value>,
}

impl NetworkAdapterBinding {
    pub fn validate(&self) -> Result<(), BindingError> {
        require_schema(self.schema_version)?;
        require_guid(&self.adapter_name, "adapterName")?;
        require_guid(&self.interface_guid, "interfaceGuid")?;
        if !self.adapter_name.eq_ignore_ascii_case(&self.interface_guid) {
            return Err(BindingError::AdapterGuidMismatch);
        }
        if self.interface_luid == 0
            || self.interface_index == 0
            || !(6..=32).contains(&self.physical_address.len())
            || self.physical_address.iter().all(|byte| *byte == 0)
        {
            return Err(BindingError::IncompleteAdapterIdentity);
        }
        require_text(&self.friendly_name, "friendlyName")?;
        require_text(&self.interface_description, "interfaceDescription")?;
        require_text(&self.observed_at_utc, "observedAtUtc")?;
        self.device.validate()
    }

    pub fn receipt_id(&self) -> Result<BindingReceiptId, BindingError> {
        self.validate()?;
        BindingReceiptId::digest_serializable(self)
    }
}

fn require_schema(version: u8) -> Result<(), BindingError> {
    if version == NATIVE_BINDING_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(BindingError::UnsupportedSchema)
    }
}

fn require_text(value: &str, field: &'static str) -> Result<(), BindingError> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(BindingError::InvalidText(field))
    } else {
        Ok(())
    }
}

fn require_guid(value: &str, field: &'static str) -> Result<(), BindingError> {
    let bytes = value.as_bytes();
    if bytes.len() == 38
        && bytes.first() == Some(&b'{')
        && bytes.last() == Some(&b'}')
        && [9, 14, 19, 24].iter().all(|index| bytes[*index] == b'-')
        && bytes[1..37]
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 8 | 13 | 18 | 23) || byte.is_ascii_hexdigit())
    {
        Ok(())
    } else {
        Err(BindingError::InvalidGuid(field))
    }
}

fn require_inf_leaf(value: &str, field: &'static str) -> Result<(), BindingError> {
    if value.len() <= 128
        && value.ends_with(".inf")
        && value.len() > 4
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        Ok(())
    } else {
        Err(BindingError::InvalidPublishedName(field))
    }
}

fn parse_pci_instance(value: &str) -> Result<(u16, u16, u16, u16, u8), BindingError> {
    if value.len() > 512 || value.chars().any(char::is_control) {
        return Err(BindingError::InvalidPciIdentity("instanceId"));
    }
    let upper = value.to_ascii_uppercase();
    let mut parts = upper.split(['\\', '&']);
    if parts.next() != Some("PCI") {
        return Err(BindingError::InvalidPciIdentity("instanceId"));
    }
    let mut vendor = None;
    let mut device = None;
    let mut subsystem = None;
    let mut revision = None;
    for part in parts {
        if let Some(value) = part.strip_prefix("VEN_") {
            vendor = parse_hex::<u16>(value, 4);
        } else if let Some(value) = part.strip_prefix("DEV_") {
            device = parse_hex::<u16>(value, 4);
        } else if let Some(value) = part.strip_prefix("SUBSYS_") {
            subsystem = parse_hex::<u32>(value, 8);
        } else if let Some(value) = part.strip_prefix("REV_") {
            revision = parse_hex::<u8>(value, 2);
        }
    }
    let subsystem = subsystem.ok_or(BindingError::InvalidPciIdentity("instanceId"))?;
    Ok((
        vendor.ok_or(BindingError::InvalidPciIdentity("instanceId"))?,
        device.ok_or(BindingError::InvalidPciIdentity("instanceId"))?,
        (subsystem & 0xffff) as u16,
        (subsystem >> 16) as u16,
        revision.ok_or(BindingError::InvalidPciIdentity("instanceId"))?,
    ))
}

fn parse_hex<T>(value: &str, width: usize) -> Option<T>
where
    T: TryFrom<u64>,
{
    (value.len() == width)
        .then(|| u64::from_str_radix(value, 16).ok())
        .flatten()
        .and_then(|value| T::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> PciDeviceBinding {
        PciDeviceBinding {
            schema_version: 1,
            instance_id: r"PCI\VEN_10DE&DEV_2684&SUBSYS_47101462&REV_A1\4&abc&0&0008".into(),
            container_id: "{01234567-89ab-cdef-0123-456789abcdef}".into(),
            class_guid: "{4d36e968-e325-11ce-bfc1-08002be10318}".into(),
            vendor_id: 0x10de,
            device_id: 0x2684,
            subsystem_vendor_id: 0x1462,
            subsystem_device_id: 0x4710,
            revision_id: 0xa1,
            driver_provider: "NVIDIA".into(),
            driver_version: "32.0.15.1234".into(),
            published_inf: "oem42.inf".into(),
            observed_at_utc: "2026-08-13T12:00:00Z".into(),
            unknown: BTreeMap::new(),
        }
    }

    #[test]
    fn exact_device_binding_validates_and_has_stable_receipt() {
        let device = device();
        device.validate().expect("device binding");
        let first = device.receipt_id().expect("receipt");
        let round_trip: PciDeviceBinding =
            serde_json::from_value(serde_json::to_value(&device).expect("value")).expect("device");
        assert_eq!(round_trip.receipt_id().expect("receipt"), first);
    }

    #[test]
    fn numeric_device_identity_must_match_the_instance_id() {
        let mut device = device();
        device.device_id ^= 1;
        assert_eq!(device.validate(), Err(BindingError::DeviceIdentityMismatch));
    }

    #[test]
    fn durable_pnp_identity_allows_only_driver_observation_drift() {
        let original = device();
        let mut reobserved = original.clone();
        reobserved.driver_version = "33.0.16.0000".into();
        reobserved.published_inf = "oem99.inf".into();
        reobserved.observed_at_utc = "2026-08-14T12:00:00Z".into();
        assert!(original.same_pnp_device(&reobserved));
        reobserved.container_id = "{aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee}".into();
        assert!(!original.same_pnp_device(&reobserved));
    }

    #[test]
    fn traversal_and_noncanonical_inf_names_are_rejected() {
        let mut device = device();
        device.published_inf = r"..\oem42.inf".into();
        assert!(matches!(
            device.validate(),
            Err(BindingError::InvalidPublishedName("publishedInf"))
        ));
        device.published_inf = "netrtwlane.inf".into();
        assert!(device.validate().is_ok());
        device.published_inf = "NetAdapter.inf".into();
        assert!(matches!(
            device.validate(),
            Err(BindingError::InvalidPublishedName("publishedInf"))
        ));
    }

    #[test]
    fn adapter_binding_requires_matching_durable_and_transient_identity() {
        let mut adapter = NetworkAdapterBinding {
            schema_version: 1,
            adapter_name: "{aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee}".into(),
            interface_guid: "{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}".into(),
            interface_luid: 7,
            interface_index: 9,
            friendly_name: "Ethernet".into(),
            interface_description: "PCIe Ethernet Controller".into(),
            physical_address: vec![1, 2, 3, 4, 5, 6],
            device: device(),
            observed_at_utc: "2026-08-13T12:00:00Z".into(),
            unknown: BTreeMap::new(),
        };
        adapter.validate().expect("adapter binding");
        adapter.interface_guid = "{bbbbbbbb-bbbb-cccc-dddd-eeeeeeeeeeee}".into();
        assert_eq!(adapter.validate(), Err(BindingError::AdapterGuidMismatch));
    }
}
