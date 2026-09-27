use frametime_domain::driver::DriverTransactionV2Stage;

use super::DriverTransaction;

pub(super) fn validate_v2_stage_fields(transaction: &DriverTransaction) -> Result<(), String> {
    let stage = transaction
        .lifecycle
        .as_ref()
        .ok_or("v2 driver transaction lacks lifecycle state")?
        .stage;
    let capture = transaction.capture.is_some();
    let removal = transaction.removal.is_some();
    let installation = transaction.installation.is_some();
    let reconciliation = !transaction.profile_reconciliation.is_empty();
    let accepted = !transaction.accepted_profile_omissions.is_empty();
    let cs2_backup = transaction.cs2_backup.is_some();
    let package_bound = transaction.prepared_package_sha256.is_some();
    let legacy_verified = stage == DriverTransactionV2Stage::Verified && !package_bound;

    if !package_bound
        && !matches!(
            stage,
            DriverTransactionV2Stage::InstallerAuthenticated
                | DriverTransactionV2Stage::ProfileBackupPersisted
                | DriverTransactionV2Stage::Verified
        )
    {
        return Err("v2 driver transaction lacks its prepared-package binding".into());
    }

    let coherent = match stage {
        DriverTransactionV2Stage::InstallerAuthenticated
        | DriverTransactionV2Stage::ProfileBackupPersisted => {
            !capture && !removal && !installation && !reconciliation && !accepted && !cs2_backup
        }
        DriverTransactionV2Stage::StateCapturePersisted
        | DriverTransactionV2Stage::SafeModeHandoffArmed => {
            capture && !removal && !installation && !reconciliation && !accepted && !cs2_backup
        }
        DriverTransactionV2Stage::CleanupComplete => {
            capture && removal && !installation && !reconciliation && !accepted && !cs2_backup
        }
        DriverTransactionV2Stage::InstallationComplete => {
            capture && removal && installation && !reconciliation && !accepted && !cs2_backup
        }
        DriverTransactionV2Stage::NeedsProfileReconciliation => {
            capture && removal && installation && reconciliation && !accepted && !cs2_backup
        }
        DriverTransactionV2Stage::ProfilesRestored => {
            capture && removal && installation && !reconciliation
        }
        DriverTransactionV2Stage::BaselineApplied => {
            capture && removal && installation && !reconciliation && cs2_backup
        }
        DriverTransactionV2Stage::Verified => {
            capture && removal && installation && !reconciliation && (cs2_backup || legacy_verified)
        }
        // RecoveryRequired deliberately retains whichever coherent evidence
        // was durable when recovery became necessary. The individual evidence
        // validators still enforce its dependency chain.
        DriverTransactionV2Stage::RecoveryRequired => true,
    };
    if coherent {
        Ok(())
    } else {
        Err("v2 driver transaction fields do not match its lifecycle stage".into())
    }
}
