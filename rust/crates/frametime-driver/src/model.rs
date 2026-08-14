use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub(crate) type Extensions = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} has an invalid value")]
    Invalid { field: &'static str },
    #[error("GPU vendor and PCI vendor ID do not agree")]
    VendorMismatch,
    #[error("published package does not belong to the exact target GPU")]
    PackageGpuMismatch,
    #[error("published package list contains a duplicate OEM name")]
    DuplicatePublishedName,
    #[error("the artifact evidence is not a valid signature observation")]
    InvalidSignatureEvidence,
    #[error("the requested action is not read-only")]
    NotReadOnly,
    #[error("the plan must include an artifact for the installation stage")]
    MissingArtifact,
    #[error("artifact target does not match the exact target GPU")]
    ArtifactGpuMismatch,
    #[error("capture receipt does not bind to this plan")]
    ReceiptPlanMismatch,
    #[error("capture receipt is incomplete")]
    IncompleteReceipt,
    #[error("the package inventory is not a complete canonical set")]
    NonCanonicalPackageSet,
    #[error("the package-set fingerprint does not bind to the inventory")]
    PackageSetFingerprintMismatch,
    #[error("Safe Mode was not positively observed")]
    SafeModeNotConfirmed,
    #[error("the execution capture does not bind to this plan")]
    CapturePlanMismatch,
    #[error("the execution capture is stale or from the future")]
    StaleCapture,
    #[error("the artifact identity does not match the signed descriptor")]
    ArtifactIdentityMismatch,
    #[error("the artifact authorization does not bind to this capture")]
    AuthorizationMismatch,
    #[error("the artifact authorization is expired or not yet valid")]
    AuthorizationExpired,
    #[error("removal evidence is partial, ambiguous, or inconsistent")]
    InvalidRemovalEvidence,
    #[error("installation evidence is partial, ambiguous, or inconsistent")]
    InvalidInstallationEvidence,
}

fn require_text(value: &str, field: &'static str) -> Result<(), ValidationError> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(ValidationError::Invalid { field })
    } else {
        Ok(())
    }
}

fn validate_leaf(value: &str, field: &'static str) -> Result<(), ValidationError> {
    require_nonempty_leaf(value, field)?;
    require_leaf_within_length_limit(value, field)?;
    reject_current_directory_leaf(value, field)?;
    reject_parent_directory_leaf(value, field)?;
    reject_path_syntax_in_leaf(value, field)?;
    reject_control_characters_in_leaf(value, field)
}

fn require_nonempty_leaf(value: &str, field: &'static str) -> Result<(), ValidationError> {
    if value.is_empty() {
        Err(ValidationError::Invalid { field })
    } else {
        Ok(())
    }
}

fn require_leaf_within_length_limit(
    value: &str,
    field: &'static str,
) -> Result<(), ValidationError> {
    if value.len() > 128 {
        Err(ValidationError::Invalid { field })
    } else {
        Ok(())
    }
}

fn reject_current_directory_leaf(value: &str, field: &'static str) -> Result<(), ValidationError> {
    if value == "." {
        Err(ValidationError::Invalid { field })
    } else {
        Ok(())
    }
}

fn reject_parent_directory_leaf(value: &str, field: &'static str) -> Result<(), ValidationError> {
    if value == ".." {
        Err(ValidationError::Invalid { field })
    } else {
        Ok(())
    }
}

fn reject_path_syntax_in_leaf(value: &str, field: &'static str) -> Result<(), ValidationError> {
    if value.contains(['/', '\\', ':']) {
        Err(ValidationError::Invalid { field })
    } else {
        Ok(())
    }
}

fn reject_control_characters_in_leaf(
    value: &str,
    field: &'static str,
) -> Result<(), ValidationError> {
    if value.chars().any(char::is_control) {
        Err(ValidationError::Invalid { field })
    } else {
        Ok(())
    }
}

fn validate_token(value: &str, field: &'static str) -> Result<(), ValidationError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Err(ValidationError::Invalid { field })
    } else {
        Ok(())
    }
}

/// A recognized PCI GPU vendor. Matching is by the fixed PCI vendor number,
/// never provider display text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
}

impl GpuVendor {
    pub const fn pci_vendor_id(self) -> u16 {
        match self {
            Self::Nvidia => 0x10de,
            Self::Amd => 0x1002,
            Self::Intel => 0x8086,
        }
    }
}

/// Exact PCI identity required to bind a package or artifact to one GPU.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactGpuIdentity {
    pub vendor: GpuVendor,
    pub pci_vendor_id: u16,
    pub pci_device_id: u16,
    pub subsystem_vendor_id: u16,
    pub subsystem_device_id: u16,
    pub revision_id: u8,
    #[serde(flatten, default)]
    pub extensions: Extensions,
}

impl ExactGpuIdentity {
    pub fn new(
        vendor: GpuVendor,
        pci_device_id: u16,
        subsystem_vendor_id: u16,
        subsystem_device_id: u16,
        revision_id: u8,
    ) -> Self {
        Self {
            vendor,
            pci_vendor_id: vendor.pci_vendor_id(),
            pci_device_id,
            subsystem_vendor_id,
            subsystem_device_id,
            revision_id,
            extensions: Extensions::new(),
        }
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.pci_vendor_id == self.vendor.pci_vendor_id() {
            Ok(())
        } else {
            Err(ValidationError::VendorMismatch)
        }
    }
}

/// The exact, locally published driver-package name. It is a leaf identity,
/// not a path, and accepts only canonical lower-case `oem<N>.inf` values.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct OemPublishedName(String);

impl OemPublishedName {
    pub fn parse(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        let digits = value
            .strip_prefix("oem")
            .and_then(|suffix| suffix.strip_suffix(".inf"));
        if validate_leaf(&value, "publishedName").is_ok()
            && digits.is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
            })
        {
            Ok(Self(value))
        } else {
            Err(ValidationError::Invalid {
                field: "publishedName",
            })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OemPublishedName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for OemPublishedName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)
            .and_then(|value| Self::parse(value).map_err(serde::de::Error::custom))
    }
}

/// An exact observed package binding for the selected display device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedDriverPackage {
    pub target_gpu: ExactGpuIdentity,
    pub published_name: OemPublishedName,
    pub original_inf_name: String,
    pub provider_name: String,
    pub driver_version: String,
    #[serde(flatten, default)]
    pub extensions: Extensions,
}

impl PublishedDriverPackage {
    pub fn validate_for(&self, target_gpu: &ExactGpuIdentity) -> Result<(), ValidationError> {
        self.target_gpu.validate()?;
        if &self.target_gpu != target_gpu {
            return Err(ValidationError::PackageGpuMismatch);
        }
        validate_leaf(&self.original_inf_name, "originalInfName")?;
        if !self.original_inf_name.ends_with(".inf") {
            return Err(ValidationError::Invalid {
                field: "originalInfName",
            });
        }
        require_text(&self.provider_name, "providerName")?;
        require_text(&self.driver_version, "driverVersion")
    }
}

/// Lower-case SHA-256 hexadecimal digest with exactly 64 characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn parse(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        if value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            Ok(Self(value))
        } else {
            Err(ValidationError::Invalid { field: "sha256" })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)
            .and_then(|value| Self::parse(value).map_err(serde::de::Error::custom))
    }
}

/// Signature status recorded by a future inspection adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthenticodeStatus {
    Valid,
    Invalid,
    NotPresent,
    Indeterminate,
}

/// Evidence only; this type does not perform signature verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticodeEvidence {
    pub status: AuthenticodeStatus,
    pub signer_subject: String,
    pub signer_thumbprint_sha256: Sha256Digest,
    pub observed_at_utc: String,
    #[serde(flatten, default)]
    pub extensions: Extensions,
}

impl AuthenticodeEvidence {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.status != AuthenticodeStatus::Valid {
            return Err(ValidationError::InvalidSignatureEvidence);
        }
        require_text(&self.signer_subject, "signerSubject")?;
        require_text(&self.observed_at_utc, "observedAtUtc")
    }
}

/// A path-free request that can be resolved by a future acquisition adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactLocator {
    pub artifact_id: String,
    pub artifact_file_name: String,
    #[serde(flatten, default)]
    pub extensions: Extensions,
}

impl ArtifactLocator {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_token(&self.artifact_id, "artifactId")?;
        validate_leaf(&self.artifact_file_name, "artifactFileName")
    }
}

/// A signed artifact descriptor bound to one exact target GPU.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedArtifactDescriptor {
    pub locator: ArtifactLocator,
    pub target_gpu: ExactGpuIdentity,
    pub payload_sha256: Sha256Digest,
    pub authenticode: AuthenticodeEvidence,
    #[serde(flatten, default)]
    pub extensions: Extensions,
}

impl SignedArtifactDescriptor {
    pub fn validate_for(&self, target_gpu: &ExactGpuIdentity) -> Result<(), ValidationError> {
        self.locator.validate()?;
        self.target_gpu.validate()?;
        if &self.target_gpu != target_gpu {
            return Err(ValidationError::ArtifactGpuMismatch);
        }
        self.authenticode.validate()
    }
}
