use frametime_domain::driver::DriverTransactionV2Stage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResumeAction {
    RestoreProfiles,
    ApplyBaseline,
    VerifyBaseline,
    ReconcileProfiles,
    Complete,
}

fn resume_action(stage: DriverTransactionV2Stage) -> Result<ResumeAction, String> {
    match stage {
        DriverTransactionV2Stage::InstallationComplete => Ok(ResumeAction::RestoreProfiles),
        DriverTransactionV2Stage::ProfilesRestored => Ok(ResumeAction::ApplyBaseline),
        DriverTransactionV2Stage::BaselineApplied => Ok(ResumeAction::VerifyBaseline),
        DriverTransactionV2Stage::NeedsProfileReconciliation => Ok(ResumeAction::ReconcileProfiles),
        DriverTransactionV2Stage::Verified => Ok(ResumeAction::Complete),
        _ => Err("profile restore is not at a resumable installation boundary".into()),
    }
}

#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
use crate::TrustedWorkDir;
#[cfg(windows)]
use crate::driver::transaction::{load_profile_backup_at, persist_driver_transaction};
#[cfg(windows)]
use crate::driver::{
    DriverTransaction, NativeNvapiDrs, apply_cs2_profile, capture_cs2_backup,
    load_driver_transaction_at, verify_cs2_profile,
};

#[cfg(windows)]
pub(super) fn complete_profile_restore(
    transaction: DriverTransaction,
    work_dir: &Path,
) -> Result<DriverTransaction, String> {
    let stage = transaction
        .lifecycle
        .as_ref()
        .ok_or("profile restore refuses a v1 transaction")?
        .stage;
    match resume_action(stage)? {
        ResumeAction::RestoreProfiles => restore_profiles(transaction, work_dir),
        ResumeAction::ApplyBaseline => apply_baseline(transaction, work_dir),
        ResumeAction::VerifyBaseline => verify_baseline(transaction, work_dir),
        ResumeAction::ReconcileProfiles => {
            Err("NVIDIA profile restoration requires explicit reconciliation".into())
        }
        ResumeAction::Complete => Ok(transaction),
    }
}

#[cfg(windows)]
fn restore_profiles(
    mut transaction: DriverTransaction,
    work_dir: &Path,
) -> Result<DriverTransaction, String> {
    let lifecycle = transaction
        .lifecycle
        .clone()
        .ok_or("profile restore refuses a v1 transaction")?;
    let reference = lifecycle
        .profile_backup
        .as_ref()
        .ok_or("profile restore lacks backup digest")?;
    let snapshot = load_profile_backup_at(work_dir, reference)?;
    let mut drs = NativeNvapiDrs::load().map_err(|error| error.to_string())?;
    transaction.profile_reconciliation = drs
        .merge_customized_profiles(&snapshot)
        .map_err(|error| error.to_string())?;
    let next = if transaction.profile_reconciliation.is_empty() {
        DriverTransactionV2Stage::ProfilesRestored
    } else {
        DriverTransactionV2Stage::NeedsProfileReconciliation
    };
    transaction.lifecycle = Some(
        lifecycle
            .transition(next, None)
            .map_err(|error| format!("advance profile restoration stage: {error:?}"))?,
    );
    let transaction = persist_driver_transaction(work_dir, &transaction)?;
    complete_profile_restore(transaction, work_dir)
}

#[cfg(windows)]
fn apply_baseline(
    mut transaction: DriverTransaction,
    work_dir: &Path,
) -> Result<DriverTransaction, String> {
    let mut drs = NativeNvapiDrs::load().map_err(|error| error.to_string())?;
    let backup = match transaction.cs2_backup.clone() {
        Some(backup) => backup,
        None => {
            let backup = capture_cs2_backup(&mut drs).map_err(|error| error.to_string())?;
            transaction.cs2_backup = Some(backup.clone());
            transaction = persist_driver_transaction(work_dir, &transaction)?;
            backup
        }
    };

    // A crash may leave the DRS save complete while the durable lifecycle is
    // still ProfilesRestored. Readback first makes that boundary resumable;
    // otherwise apply_cs2_profile verifies the captured originals fail-closed.
    if verify_cs2_profile(&mut drs, &backup).is_err() {
        apply_cs2_profile(&mut drs, &backup).map_err(|error| error.to_string())?;
    }
    transaction.lifecycle = Some(
        transaction
            .lifecycle
            .as_ref()
            .ok_or("baseline stage lacks lifecycle state")?
            .transition(DriverTransactionV2Stage::BaselineApplied, None)
            .map_err(|error| format!("advance baseline stage: {error:?}"))?,
    );
    let transaction = persist_driver_transaction(work_dir, &transaction)?;
    verify_baseline(transaction, work_dir)
}

#[cfg(windows)]
fn verify_baseline(
    mut transaction: DriverTransaction,
    work_dir: &Path,
) -> Result<DriverTransaction, String> {
    let backup = transaction
        .cs2_backup
        .as_ref()
        .ok_or("verification stage lacks durable CS2 recovery data")?;
    let mut drs = NativeNvapiDrs::load().map_err(|error| error.to_string())?;
    verify_cs2_profile(&mut drs, backup).map_err(|error| error.to_string())?;
    transaction.lifecycle = Some(
        transaction
            .lifecycle
            .as_ref()
            .ok_or("verification stage lacks lifecycle state")?
            .transition(DriverTransactionV2Stage::Verified, None)
            .map_err(|error| format!("advance verification stage: {error:?}"))?,
    );
    persist_driver_transaction(work_dir, &transaction)
}

#[cfg(windows)]
pub fn reconcile_nvidia_profiles(
    accept_driver_incompatible_profile_items: bool,
    yes: bool,
) -> Result<DriverTransaction, String> {
    if !accept_driver_incompatible_profile_items || !yes {
        return Err(
            "profile reconciliation requires --accept-driver-incompatible-profile-items --yes"
                .into(),
        );
    }
    let trusted = TrustedWorkDir::acquire_fixed()?;
    let mut transaction =
        load_driver_transaction_at(trusted.path())?.ok_or("no NVIDIA driver transaction exists")?;
    let lifecycle = transaction
        .lifecycle
        .clone()
        .ok_or("profile reconciliation refuses a v1 transaction")?;
    if lifecycle.stage != DriverTransactionV2Stage::NeedsProfileReconciliation {
        return Err("NVIDIA driver transaction does not need profile reconciliation".into());
    }
    transaction.accepted_profile_omissions = transaction.profile_reconciliation.clone();
    transaction.profile_reconciliation.clear();
    transaction.lifecycle = Some(
        lifecycle
            .transition(DriverTransactionV2Stage::ProfilesRestored, None)
            .map_err(|error| format!("accept profile omissions: {error:?}"))?,
    );
    let transaction = persist_driver_transaction(trusted.path(), &transaction)?;
    complete_profile_restore(transaction, trusted.path())
}

#[cfg(test)]
mod tests {
    use super::{ResumeAction, resume_action};
    use frametime_domain::driver::DriverTransactionV2Stage;

    #[test]
    fn durable_profile_stages_map_to_resumable_actions() {
        assert_eq!(
            resume_action(DriverTransactionV2Stage::InstallationComplete),
            Ok(ResumeAction::RestoreProfiles)
        );
        assert_eq!(
            resume_action(DriverTransactionV2Stage::ProfilesRestored),
            Ok(ResumeAction::ApplyBaseline)
        );
        assert_eq!(
            resume_action(DriverTransactionV2Stage::BaselineApplied),
            Ok(ResumeAction::VerifyBaseline)
        );
        assert_eq!(
            resume_action(DriverTransactionV2Stage::Verified),
            Ok(ResumeAction::Complete)
        );
        assert!(resume_action(DriverTransactionV2Stage::CleanupComplete).is_err());
    }
}
