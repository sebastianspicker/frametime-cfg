//! Durable recovery records for mutations that cannot always be restored byte-for-byte.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Recovery promise required before a live mutation can begin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RecoveryRequirement {
    #[default]
    LosslessBackup,
    RebuildableAudit,
    ManualRecoveryAudit,
    Mixed,
}

/// Fixed resources that may be invalidated and rebuilt by their owning software.
///
/// This deliberately has no arbitrary path or executable field. Platform
/// backends retain authority over their configured, validated locations.
#[derive(Debug, Clone, Copy, PartialOrd, Ord, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RebuildableTarget {
    Cs2ShaderCache,
    NvidiaDxCache,
    NvidiaGlCache,
    DirectxD3dCache,
}

pub const P1_3_REBUILDABLE_TARGETS: [RebuildableTarget; 4] = [
    RebuildableTarget::Cs2ShaderCache,
    RebuildableTarget::NvidiaDxCache,
    RebuildableTarget::NvidiaGlCache,
    RebuildableTarget::DirectxD3dCache,
];

/// Fixed irreversible catalog subjects. These names carry no paths, commands,
/// package identifiers, or artifact locations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManualRecoveryTarget {
    AppxRemovals,
    ExactDriverPackageRemoval,
    DriverInstallation,
}

pub const P1_13_MANUAL_RECOVERY_TARGET: ManualRecoveryTarget = ManualRecoveryTarget::AppxRemovals;
pub const P2_2_MANUAL_RECOVERY_TARGET: ManualRecoveryTarget =
    ManualRecoveryTarget::ExactDriverPackageRemoval;
pub const P3_1_MANUAL_RECOVERY_TARGET: ManualRecoveryTarget =
    ManualRecoveryTarget::DriverInstallation;

/// The durable state of a rebuildable mutation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "state",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RebuildableAuditOutcome {
    Pending,
    Verified { finalized_at: String },
}

/// A typed audit entry. It records a rebuildable mutation, never restoration bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RebuildableAudit {
    #[serde(rename = "type")]
    pub record_type: RebuildableAuditRecordType,
    pub step: String,
    pub captured_at: String,
    pub targets: Vec<RebuildableTarget>,
    #[serde(flatten)]
    pub outcome: RebuildableAuditOutcome,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RebuildableAuditRecordType {
    RebuildableMutation,
}

impl RebuildableAudit {
    pub fn pending(
        step: impl Into<String>,
        captured_at: impl Into<String>,
        targets: impl IntoIterator<Item = RebuildableTarget>,
    ) -> Result<Self, &'static str> {
        let targets = deduplicated_targets(targets);
        if targets.is_empty() {
            return Err("rebuildable audit requires at least one fixed target");
        }
        Ok(Self {
            record_type: RebuildableAuditRecordType::RebuildableMutation,
            step: step.into(),
            captured_at: captured_at.into(),
            targets,
            outcome: RebuildableAuditOutcome::Pending,
            unknown: BTreeMap::new(),
        })
    }

    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self.outcome, RebuildableAuditOutcome::Pending)
    }

    #[must_use]
    pub fn is_valid_pending_for(&self, step: &str) -> bool {
        self.is_pending()
            && self.step == step
            && !self.captured_at.is_empty()
            && self.targets_are_unique_and_nonempty()
            && (step != "P1:3" || self.targets == P1_3_REBUILDABLE_TARGETS)
    }

    #[must_use]
    pub fn finalized(&self, finalized_at: impl Into<String>) -> Self {
        let mut finalized = self.clone();
        finalized.outcome = RebuildableAuditOutcome::Verified {
            finalized_at: finalized_at.into(),
        };
        finalized
    }

    fn targets_are_unique_and_nonempty(&self) -> bool {
        !self.targets.is_empty()
            && self.targets == deduplicated_targets(self.targets.iter().copied())
    }
}

/// State retained for a mutation whose recovery must be performed manually.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "state",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ManualRecoveryAuditOutcome {
    Pending,
    Verified { finalized_at: String },
    Failed { failed_at: String },
}

/// A typed pending/final audit for an irreversible, manually recoverable action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManualRecoveryAudit {
    #[serde(rename = "type")]
    pub record_type: ManualRecoveryAuditRecordType,
    pub step: String,
    pub captured_at: String,
    pub target: ManualRecoveryTarget,
    #[serde(flatten)]
    pub outcome: ManualRecoveryAuditOutcome,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

/// A typed pending/final audit when byte-restorable and manual recovery coexist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MixedRecoveryAudit {
    #[serde(rename = "type")]
    pub record_type: MixedRecoveryAuditRecordType,
    pub step: String,
    pub captured_at: String,
    pub target: ManualRecoveryTarget,
    /// Exact irreversible package identities retained for operator-led AppX
    /// reinstallation. This is present only for P1:13 mixed recovery.
    #[serde(
        default,
        rename = "manualRecoverySubjects",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub manual_recovery_subjects: Vec<AppxRemovalSubject>,
    #[serde(flatten)]
    pub outcome: ManualRecoveryAuditOutcome,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManualRecoveryAuditRecordType {
    ManualRecoveryMutation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MixedRecoveryAuditRecordType {
    MixedRecoveryMutation,
}

/// One exact AppX identity removed by P1:13. The type deliberately carries no
/// executable, path, command, or arbitrary recovery instruction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppxRemovalSubject {
    Installed { full_name: String },
    Provisioned { package_name: String },
}

/// The typed durable audit required for manual and mixed recovery operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrreversibleAudit {
    Manual(ManualRecoveryAudit),
    Mixed(MixedRecoveryAudit),
}

impl ManualRecoveryAudit {
    pub fn pending(
        step: impl Into<String>,
        captured_at: impl Into<String>,
        target: ManualRecoveryTarget,
    ) -> Self {
        Self {
            record_type: ManualRecoveryAuditRecordType::ManualRecoveryMutation,
            step: step.into(),
            captured_at: captured_at.into(),
            target,
            outcome: ManualRecoveryAuditOutcome::Pending,
            unknown: BTreeMap::new(),
        }
    }
}

impl MixedRecoveryAudit {
    pub fn pending(
        step: impl Into<String>,
        captured_at: impl Into<String>,
        target: ManualRecoveryTarget,
    ) -> Self {
        Self {
            record_type: MixedRecoveryAuditRecordType::MixedRecoveryMutation,
            step: step.into(),
            captured_at: captured_at.into(),
            target,
            manual_recovery_subjects: Vec::new(),
            outcome: ManualRecoveryAuditOutcome::Pending,
            unknown: BTreeMap::new(),
        }
    }

    pub fn pending_with_appx_subjects(
        step: impl Into<String>,
        captured_at: impl Into<String>,
        subjects: impl IntoIterator<Item = AppxRemovalSubject>,
    ) -> Result<Self, &'static str> {
        let mut audit = Self::pending(step, captured_at, P1_13_MANUAL_RECOVERY_TARGET);
        audit.manual_recovery_subjects = subjects.into_iter().collect();
        if audit.step != "P1:13" {
            return Err("AppX removal audit is bound only to P1:13");
        }
        let subject_count = audit.manual_recovery_subjects.len();
        if audit
            .manual_recovery_subjects
            .iter()
            .any(appx_subject_is_invalid)
        {
            return Err("P1:13 AppX audit subjects are invalid or duplicated");
        }
        let mut deduplicated = audit.manual_recovery_subjects.clone();
        deduplicated.sort();
        deduplicated.dedup();
        if deduplicated.len() != subject_count {
            return Err("P1:13 AppX audit subjects are invalid or duplicated");
        }
        audit.manual_recovery_subjects = deduplicated;
        Ok(audit)
    }
}

impl IrreversibleAudit {
    #[must_use]
    pub fn is_valid_pending_for(&self, requirement: RecoveryRequirement, step: &str) -> bool {
        let (kind_matches, captured_at, target, outcome) = match self {
            Self::Manual(audit) => (
                requirement == RecoveryRequirement::ManualRecoveryAudit && audit.step == step,
                &audit.captured_at,
                audit.target,
                &audit.outcome,
            ),
            Self::Mixed(audit) => (
                requirement == RecoveryRequirement::Mixed && audit.step == step,
                &audit.captured_at,
                audit.target,
                &audit.outcome,
            ),
        };
        kind_matches
            && !captured_at.is_empty()
            && matches!(outcome, ManualRecoveryAuditOutcome::Pending)
            && expected_manual_target(step).is_some_and(|expected| target == expected)
            && match self {
                Self::Mixed(audit) if step == "P1:13" => {
                    appx_subjects_are_valid(&audit.manual_recovery_subjects)
                }
                _ => true,
            }
    }

    #[must_use]
    pub fn finalized(&self, finalized_at: impl Into<String>) -> Self {
        self.with_outcome(ManualRecoveryAuditOutcome::Verified {
            finalized_at: finalized_at.into(),
        })
    }

    #[must_use]
    pub fn failed(&self, failed_at: impl Into<String>) -> Self {
        self.with_outcome(ManualRecoveryAuditOutcome::Failed {
            failed_at: failed_at.into(),
        })
    }

    fn with_outcome(&self, outcome: ManualRecoveryAuditOutcome) -> Self {
        match self {
            Self::Manual(audit) => {
                let mut updated = audit.clone();
                updated.outcome = outcome;
                Self::Manual(updated)
            }
            Self::Mixed(audit) => {
                let mut updated = audit.clone();
                updated.outcome = outcome;
                Self::Mixed(updated)
            }
        }
    }
}

fn appx_subjects_are_valid(subjects: &[AppxRemovalSubject]) -> bool {
    let mut previous = None;
    for subject in subjects {
        if appx_subject_is_invalid(subject) || previous.is_some_and(|item| item >= subject) {
            return false;
        }
        previous = Some(subject);
    }
    true
}

fn appx_subject_is_invalid(subject: &AppxRemovalSubject) -> bool {
    let identity = match subject {
        AppxRemovalSubject::Installed { full_name } => full_name,
        AppxRemovalSubject::Provisioned { package_name } => package_name,
    };
    identity.is_empty() || identity.len() > 1024
}

fn expected_manual_target(step: &str) -> Option<ManualRecoveryTarget> {
    match step {
        "P1:13" => Some(P1_13_MANUAL_RECOVERY_TARGET),
        "P2:2" => Some(P2_2_MANUAL_RECOVERY_TARGET),
        "P3:1" => Some(P3_1_MANUAL_RECOVERY_TARGET),
        _ => None,
    }
}

fn deduplicated_targets(
    targets: impl IntoIterator<Item = RebuildableTarget>,
) -> Vec<RebuildableTarget> {
    let mut known = BTreeMap::new();
    for target in targets {
        known.insert(target, ());
    }
    known.into_keys().collect()
}

/// A forward-compatible persisted audit document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditFile {
    #[serde(default)]
    pub entries: Vec<AuditEntry>,
    pub created: String,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

/// Known recovery records and future records retained without interpretation.
#[derive(Debug, Clone, PartialEq)]
pub enum AuditEntry {
    Rebuildable(RebuildableAudit),
    Manual(ManualRecoveryAudit),
    Mixed(MixedRecoveryAudit),
    Unknown(Value),
}

impl Serialize for AuditEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Rebuildable(audit) => audit.serialize(serializer),
            Self::Manual(audit) => audit.serialize(serializer),
            Self::Mixed(audit) => audit.serialize(serializer),
            Self::Unknown(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for AuditEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match value.get("type").and_then(Value::as_str) {
            Some("rebuildable_mutation") => serde_json::from_value(value)
                .map(Self::Rebuildable)
                .map_err(serde::de::Error::custom),
            Some("manual_recovery_mutation") => serde_json::from_value(value)
                .map(Self::Manual)
                .map_err(serde::de::Error::custom),
            Some("mixed_recovery_mutation") => serde_json::from_value(value)
                .map(Self::Mixed)
                .map_err(serde::de::Error::custom),
            _ => Ok(Self::Unknown(value)),
        }
    }
}

#[cfg(test)]
#[path = "audit_tests.rs"]
mod tests;
