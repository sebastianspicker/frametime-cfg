use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    ExactGpuIdentity, PublishedDriverPackage, SCHEMA_VERSION, Sha256Digest,
    SignedArtifactDescriptor, ValidationError, model::Extensions,
};

/// The fixed workflow stages this domain may describe. It does not mark any
/// platform workflow step as completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriverPlanStep {
    #[serde(rename = "P1:18")]
    P1_18,
    #[serde(rename = "P1:19")]
    P1_19,
    #[serde(rename = "P2:2")]
    P2_2,
    #[serde(rename = "P3:1")]
    P3_1,
}

/// A descriptive dry-run action. This is never an executable command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PlannedDriverAction {
    InspectExactGpu {
        target_gpu: ExactGpuIdentity,
    },
    RecordExactPackages {
        packages: Vec<PublishedDriverPackage>,
    },
    ProposePackageCleanup {
        published_names: Vec<crate::OemPublishedName>,
    },
    ProposeSignedArtifactInstall {
        artifact: Box<SignedArtifactDescriptor>,
    },
}

/// One read-only workflow entry with a stable stage identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunDriverPlanEntry {
    pub step: DriverPlanStep,
    pub action: PlannedDriverAction,
    #[serde(flatten, default)]
    pub extensions: Extensions,
}

/// Validated input supplied by a CLI or GUI read-only status surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverPlanInput {
    pub target_gpu: ExactGpuIdentity,
    pub installed_packages: Vec<PublishedDriverPackage>,
    pub artifact: SignedArtifactDescriptor,
    #[serde(flatten, default)]
    pub extensions: Extensions,
}

impl DriverPlanInput {
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.target_gpu.validate()?;
        if self.installed_packages.is_empty() {
            return Err(ValidationError::Required {
                field: "installedPackages",
            });
        }
        let mut names = BTreeSet::new();
        for package in &self.installed_packages {
            package.validate_for(&self.target_gpu)?;
            if !names.insert(package.published_name.clone()) {
                return Err(ValidationError::DuplicatePublishedName);
            }
        }
        self.artifact.validate_for(&self.target_gpu)
    }
}

/// A deterministic, fully read-only plan suitable for status output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunDriverPlan {
    pub schema_version: u32,
    pub read_only: bool,
    pub target_gpu: ExactGpuIdentity,
    pub input_sha256: Sha256Digest,
    pub entries: Vec<DryRunDriverPlanEntry>,
    #[serde(flatten, default)]
    pub extensions: Extensions,
}

impl DryRunDriverPlan {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ValidationError::Invalid {
                field: "schemaVersion",
            });
        }
        if !self.read_only {
            return Err(ValidationError::NotReadOnly);
        }
        self.target_gpu.validate()?;
        if self.entries.len() != 4
            || self
                .entries
                .iter()
                .map(|entry| entry.step)
                .collect::<Vec<_>>()
                != vec![
                    DriverPlanStep::P1_18,
                    DriverPlanStep::P1_19,
                    DriverPlanStep::P2_2,
                    DriverPlanStep::P3_1,
                ]
        {
            return Err(ValidationError::Invalid { field: "entries" });
        }
        Ok(())
    }
}

/// Validate input and build a canonical dry-run plan. Package input order does
/// not affect the returned entries or input fingerprint.
pub fn generate_dry_run_plan(input: &DriverPlanInput) -> Result<DryRunDriverPlan, ValidationError> {
    input.validate()?;
    let mut packages = input.installed_packages.clone();
    packages.sort_by(|left, right| left.published_name.cmp(&right.published_name));
    let canonical_input = DriverPlanInput {
        target_gpu: input.target_gpu.clone(),
        installed_packages: packages.clone(),
        artifact: input.artifact.clone(),
        extensions: input.extensions.clone(),
    };
    let encoded = serde_json::to_vec(&canonical_input)
        .map_err(|_| ValidationError::Invalid { field: "planInput" })?;
    let input_sha256 = Sha256Digest::parse(format!("{:x}", Sha256::digest(encoded)))?;
    let names = packages
        .iter()
        .map(|package| package.published_name.clone())
        .collect();
    Ok(DryRunDriverPlan {
        schema_version: SCHEMA_VERSION,
        read_only: true,
        target_gpu: input.target_gpu.clone(),
        input_sha256,
        entries: vec![
            DryRunDriverPlanEntry {
                step: DriverPlanStep::P1_18,
                action: PlannedDriverAction::InspectExactGpu {
                    target_gpu: input.target_gpu.clone(),
                },
                extensions: Extensions::new(),
            },
            DryRunDriverPlanEntry {
                step: DriverPlanStep::P1_19,
                action: PlannedDriverAction::RecordExactPackages { packages },
                extensions: Extensions::new(),
            },
            DryRunDriverPlanEntry {
                step: DriverPlanStep::P2_2,
                action: PlannedDriverAction::ProposePackageCleanup {
                    published_names: names,
                },
                extensions: Extensions::new(),
            },
            DryRunDriverPlanEntry {
                step: DriverPlanStep::P3_1,
                action: PlannedDriverAction::ProposeSignedArtifactInstall {
                    artifact: Box::new(input.artifact.clone()),
                },
                extensions: Extensions::new(),
            },
        ],
        extensions: Extensions::new(),
    })
}
