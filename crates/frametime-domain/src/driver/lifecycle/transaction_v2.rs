use serde::{Deserialize, Serialize};

use super::{DriverLifecycleError, NVIDIA_TRANSACTION_V2_SCHEMA_VERSION, Sha256Digest};

/// Digest-only reference to separately persisted DRS recovery data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileBackupDigestReference {
    pub sha256: Sha256Digest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DriverTransactionV2Stage {
    InstallerAuthenticated,
    ProfileBackupPersisted,
    StateCapturePersisted,
    SafeModeHandoffArmed,
    CleanupComplete,
    InstallationComplete,
    ProfilesRestored,
    NeedsProfileReconciliation,
    BaselineApplied,
    Verified,
    RecoveryRequired,
}

impl DriverTransactionV2Stage {
    #[must_use]
    pub const fn allows_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::InstallerAuthenticated, Self::ProfileBackupPersisted)
                | (Self::ProfileBackupPersisted, Self::StateCapturePersisted)
                | (Self::StateCapturePersisted, Self::SafeModeHandoffArmed)
                | (Self::SafeModeHandoffArmed, Self::CleanupComplete)
                | (Self::CleanupComplete, Self::InstallationComplete)
                | (Self::InstallationComplete, Self::ProfilesRestored)
                | (Self::InstallationComplete, Self::NeedsProfileReconciliation)
                | (Self::NeedsProfileReconciliation, Self::ProfilesRestored)
                | (Self::ProfilesRestored, Self::BaselineApplied)
                | (Self::BaselineApplied, Self::Verified)
                | (_, Self::RecoveryRequired)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverTransactionV2 {
    pub schema_version: u32,
    pub transaction_id: String,
    pub stage: DriverTransactionV2Stage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_backup: Option<ProfileBackupDigestReference>,
}

impl DriverTransactionV2 {
    pub fn validate(&self) -> Result<(), DriverLifecycleError> {
        if self.schema_version != NVIDIA_TRANSACTION_V2_SCHEMA_VERSION
            || self.transaction_id.is_empty()
            || self.transaction_id.len() > 128
            || self.transaction_id.chars().any(char::is_control)
        {
            return Err(DriverLifecycleError::InvalidTransaction("record"));
        }
        let needs_backup = matches!(
            self.stage,
            DriverTransactionV2Stage::ProfileBackupPersisted
                | DriverTransactionV2Stage::StateCapturePersisted
                | DriverTransactionV2Stage::SafeModeHandoffArmed
                | DriverTransactionV2Stage::CleanupComplete
                | DriverTransactionV2Stage::InstallationComplete
                | DriverTransactionV2Stage::ProfilesRestored
                | DriverTransactionV2Stage::NeedsProfileReconciliation
                | DriverTransactionV2Stage::BaselineApplied
                | DriverTransactionV2Stage::Verified
        );
        if needs_backup && self.profile_backup.is_none() {
            return Err(DriverLifecycleError::InvalidTransaction("profileBackup"));
        }
        if matches!(self.stage, DriverTransactionV2Stage::InstallerAuthenticated)
            && self.profile_backup.is_some()
        {
            return Err(DriverLifecycleError::InvalidTransaction("profileBackup"));
        }
        Ok(())
    }

    pub fn transition(
        &self,
        next: DriverTransactionV2Stage,
        profile_backup: Option<ProfileBackupDigestReference>,
    ) -> Result<Self, DriverLifecycleError> {
        self.validate()?;
        if !self.stage.allows_transition_to(next) {
            return Err(DriverLifecycleError::InvalidTransaction("stage"));
        }
        let transaction = Self {
            schema_version: self.schema_version,
            transaction_id: self.transaction_id.clone(),
            stage: next,
            profile_backup: profile_backup.or_else(|| self.profile_backup.clone()),
        };
        transaction.validate()?;
        Ok(transaction)
    }
}

/// Presence-only v1 state supplied by a persistence adapter after it has
/// decoded the old record. It deliberately does not pretend to interpret
/// unrecognized v1 extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "V1 persistence retains four independently named boolean fields in the legacy migration schema"
)]
pub struct V1DriverTransactionState {
    pub capture_present: bool,
    pub cleanup_present: bool,
    pub installation_present: bool,
    #[serde(default)]
    pub unknown_resume_state: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum V1TransactionMigrationDecision {
    StartAt(DriverTransactionV2Stage),
    RefuseAmbiguousResume,
}

/// Maps only a v1 record that has not crossed a durable boundary. Any v1 state
/// that could resume after capture is rejected because it has no v2 DRS backup
/// digest and cannot prove the required backup-before-mutation ordering.
#[must_use]
pub const fn migrate_v1_transaction(
    state: V1DriverTransactionState,
) -> V1TransactionMigrationDecision {
    if state.unknown_resume_state
        || state.installation_present
        || state.cleanup_present
        || state.capture_present
    {
        V1TransactionMigrationDecision::RefuseAmbiguousResume
    } else {
        V1TransactionMigrationDecision::StartAt(DriverTransactionV2Stage::InstallerAuthenticated)
    }
}
