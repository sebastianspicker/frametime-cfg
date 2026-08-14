//! Durable receipts for read-only prerequisite observations.
//!
//! Receipts make preparatory workflow rows useful across phases without
//! pretending that persisted JSON is live authority. A later mutation must
//! re-observe the same subject before using a receipt.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{
    BindingReceiptId, NetworkAdapterBinding, PciDeviceBinding, TransactionId,
    binding::NATIVE_BINDING_SCHEMA_VERSION,
};

pub const EVIDENCE_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EvidenceRequirement {
    #[default]
    None,
    DurableReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EvidenceError {
    #[error("evidence schema version is unsupported")]
    UnsupportedSchema,
    #[error("evidence step is not the exact subject binding")]
    StepMismatch,
    #[error("evidence receipt id does not match its canonical subject")]
    ReceiptMismatch,
    #[error("evidence contains unknown fields and cannot authorize work")]
    UnknownFields,
    #[error("evidence contains an empty or invalid field: {0}")]
    InvalidField(&'static str),
    #[error("evidence subject set is empty, duplicated, or noncanonical")]
    InvalidSubjectSet,
    #[error("nested Windows identity is invalid: {0}")]
    InvalidBinding(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ObservationSubject {
    DriverCleanupPreparation {
        target_gpu: PciDeviceBinding,
        installed_packages: Vec<PciDeviceBinding>,
    },
    NvidiaDrsPreparation {
        target_gpu: PciDeviceBinding,
        driver_version: String,
        nvapi_module_sha256: String,
        nvapi_interface_version: String,
        profile_name: String,
        application_name: String,
    },
    MsiDeviceSet {
        devices: Vec<PciDeviceBinding>,
    },
    NicAffinityProposal {
        adapter: Box<NetworkAdapterBinding>,
        processor_group: u16,
        logical_processor_count: u16,
        target_processor: u16,
        assignment_mask: u64,
    },
}

impl ObservationSubject {
    fn expected_step(&self) -> &'static str {
        match self {
            Self::DriverCleanupPreparation { .. } => "P1:18",
            Self::NvidiaDrsPreparation { .. } => "P1:20",
            Self::MsiDeviceSet { .. } => "P1:21",
            Self::NicAffinityProposal { .. } => "P1:22",
        }
    }

    fn validate(&self) -> Result<(), EvidenceError> {
        match self {
            Self::DriverCleanupPreparation {
                target_gpu,
                installed_packages,
            } => {
                validate_device(target_gpu)?;
                validate_device_set(installed_packages)?;
                if installed_packages.iter().any(|package| {
                    package.vendor_id != target_gpu.vendor_id
                        || package.device_id != target_gpu.device_id
                        || package.subsystem_vendor_id != target_gpu.subsystem_vendor_id
                        || package.subsystem_device_id != target_gpu.subsystem_device_id
                }) {
                    return Err(EvidenceError::InvalidSubjectSet);
                }
                Ok(())
            }
            Self::NvidiaDrsPreparation {
                target_gpu,
                driver_version,
                nvapi_module_sha256,
                nvapi_interface_version,
                profile_name,
                application_name,
            } => {
                validate_device(target_gpu)?;
                if target_gpu.vendor_id != 0x10de {
                    return Err(EvidenceError::InvalidField("targetGpu.vendorId"));
                }
                require_text(driver_version, "driverVersion")?;
                require_sha256(nvapi_module_sha256, "nvapiModuleSha256")?;
                require_text(nvapi_interface_version, "nvapiInterfaceVersion")?;
                if !matches!(
                    profile_name.as_str(),
                    "Counter-Strike 2" | "Counter-strike 2"
                ) || application_name != "cs2.exe"
                {
                    return Err(EvidenceError::InvalidField("drsProfileIdentity"));
                }
                Ok(())
            }
            Self::MsiDeviceSet { devices } => validate_device_set(devices),
            Self::NicAffinityProposal {
                adapter,
                processor_group,
                logical_processor_count,
                target_processor,
                assignment_mask,
            } => {
                adapter
                    .validate()
                    .map_err(|error| EvidenceError::InvalidBinding(error.to_string()))?;
                if *logical_processor_count == 0
                    || *logical_processor_count > 64
                    || *processor_group != 0
                    || *target_processor >= *logical_processor_count
                    || *assignment_mask
                        != 1_u64.checked_shl(u32::from(*target_processor)).unwrap_or(0)
                {
                    return Err(EvidenceError::InvalidField("processorTopology"));
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ObservationReceipt {
    pub schema_version: u8,
    pub receipt_id: BindingReceiptId,
    pub step: String,
    pub captured_at_utc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<TransactionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_generation: Option<String>,
    pub subject: ObservationSubject,
    #[serde(flatten, default)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReceiptIdentity<'a> {
    schema_version: u8,
    step: &'a str,
    captured_at_utc: &'a str,
    transaction_id: &'a Option<TransactionId>,
    runtime_generation: &'a Option<String>,
    subject: &'a ObservationSubject,
}

impl ObservationReceipt {
    pub fn new(
        captured_at_utc: impl Into<String>,
        transaction_id: Option<TransactionId>,
        runtime_generation: Option<String>,
        subject: ObservationSubject,
    ) -> Result<Self, EvidenceError> {
        let step = subject.expected_step().to_owned();
        let captured_at_utc = captured_at_utc.into();
        let mut receipt = Self {
            schema_version: EVIDENCE_SCHEMA_VERSION,
            receipt_id: BindingReceiptId::parse("0".repeat(64)).expect("fixed digest placeholder"),
            step,
            captured_at_utc,
            transaction_id,
            runtime_generation,
            subject,
            unknown: BTreeMap::new(),
        };
        receipt.receipt_id = receipt.canonical_receipt_id()?;
        receipt.validate_for(&receipt.step.clone())?;
        Ok(receipt)
    }

    pub fn validate_for(&self, expected_step: &str) -> Result<(), EvidenceError> {
        if self.schema_version != EVIDENCE_SCHEMA_VERSION {
            return Err(EvidenceError::UnsupportedSchema);
        }
        if !self.unknown.is_empty() {
            return Err(EvidenceError::UnknownFields);
        }
        if self.step != expected_step || self.step != self.subject.expected_step() {
            return Err(EvidenceError::StepMismatch);
        }
        require_text(&self.captured_at_utc, "capturedAtUtc")?;
        if let Some(generation) = &self.runtime_generation
            && !valid_generation(generation)
        {
            return Err(EvidenceError::InvalidField("runtimeGeneration"));
        }
        self.subject.validate()?;
        if self.canonical_receipt_id()? != self.receipt_id {
            return Err(EvidenceError::ReceiptMismatch);
        }
        Ok(())
    }

    fn canonical_receipt_id(&self) -> Result<BindingReceiptId, EvidenceError> {
        BindingReceiptId::digest_serializable(&ReceiptIdentity {
            schema_version: self.schema_version,
            step: &self.step,
            captured_at_utc: &self.captured_at_utc,
            transaction_id: &self.transaction_id,
            runtime_generation: &self.runtime_generation,
            subject: &self.subject,
        })
        .map_err(|_| EvidenceError::ReceiptMismatch)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceFile {
    #[serde(default)]
    pub entries: Vec<EvidenceEntry>,
    pub created: String,
    #[serde(flatten, default)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum EvidenceEntry {
    Observation(Box<ObservationReceipt>),
    Unknown(Value),
}

impl EvidenceFile {
    pub fn replace_observation(&mut self, receipt: ObservationReceipt) {
        self.entries.retain(|entry| {
            !matches!(entry, EvidenceEntry::Observation(existing) if existing.step == receipt.step)
        });
        self.entries
            .push(EvidenceEntry::Observation(Box::new(receipt)));
    }

    pub fn observation_for(
        &self,
        step: &str,
    ) -> Result<Option<&ObservationReceipt>, EvidenceError> {
        let mut matches = self.entries.iter().filter_map(|entry| match entry {
            EvidenceEntry::Observation(receipt) if receipt.step == step => Some(receipt.as_ref()),
            EvidenceEntry::Observation(_) | EvidenceEntry::Unknown(_) => None,
        });
        let first = matches.next();
        if matches.next().is_some() {
            return Err(EvidenceError::InvalidSubjectSet);
        }
        if let Some(receipt) = first {
            receipt.validate_for(step)?;
        }
        Ok(first)
    }
}

fn validate_device(device: &PciDeviceBinding) -> Result<(), EvidenceError> {
    device
        .validate()
        .map_err(|error| EvidenceError::InvalidBinding(error.to_string()))?;
    if device.schema_version != NATIVE_BINDING_SCHEMA_VERSION || !device.unknown.is_empty() {
        return Err(EvidenceError::UnknownFields);
    }
    Ok(())
}

fn validate_device_set(devices: &[PciDeviceBinding]) -> Result<(), EvidenceError> {
    if devices.is_empty() {
        return Err(EvidenceError::InvalidSubjectSet);
    }
    let mut previous = None;
    let mut identities = BTreeSet::new();
    for device in devices {
        validate_device(device)?;
        let identity = device.instance_id.to_ascii_uppercase();
        if previous
            .as_ref()
            .is_some_and(|known: &String| known >= &identity)
            || !identities.insert(identity.clone())
        {
            return Err(EvidenceError::InvalidSubjectSet);
        }
        previous = Some(identity);
    }
    Ok(())
}

fn require_text(value: &str, field: &'static str) -> Result<(), EvidenceError> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(EvidenceError::InvalidField(field))
    } else {
        Ok(())
    }
}

fn require_sha256(value: &str, field: &'static str) -> Result<(), EvidenceError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(EvidenceError::InvalidField(field))
    }
}

fn valid_generation(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
#[path = "evidence/tests.rs"]
mod tests;
