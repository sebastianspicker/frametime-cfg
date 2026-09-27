use std::path::Path;

use frametime_domain::driver::{
    CaptureFreshnessPolicy, DriverTransactionV2Stage, DrsSnapshot, Sha256Digest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{DriverTransaction, load_profile_backup_at};
use crate::{TrustedWorkDir, WorkLock, read_json_trusted, write_json_atomic_trusted};

const PREVIOUS_TRANSACTION_FILE: &str = "driver-previous-transaction.json";
const PREVIOUS_TRANSACTION_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviousDriverTransaction {
    schema_version: u32,
    transaction: DriverTransaction,
    profile_backup: DrsSnapshot,
}

impl PreviousDriverTransaction {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != PREVIOUS_TRANSACTION_SCHEMA
            || self
                .transaction
                .lifecycle
                .as_ref()
                .map(|lifecycle| lifecycle.stage)
                != Some(DriverTransactionV2Stage::Verified)
        {
            return Err("previous driver transaction is not a completed v2 record".into());
        }
        self.transaction
            .validate_persisted(CaptureFreshnessPolicy {
                maximum_age_seconds: 86_400,
            })?;
        let expected = self
            .transaction
            .lifecycle
            .as_ref()
            .and_then(|lifecycle| lifecycle.profile_backup.as_ref())
            .ok_or("completed driver transaction lacks its profile backup")?;
        let bytes = self
            .profile_backup
            .to_json()
            .map_err(|error| format!("validate archived NVIDIA profile backup: {error:?}"))?;
        let actual = Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
            .map_err(|error| error.to_string())?;
        if actual != expected.sha256 {
            return Err("archived NVIDIA profile backup digest does not match transaction".into());
        }
        Ok(())
    }
}

pub(crate) fn archive_verified_transaction(
    work_dir: &Path,
    transaction: &DriverTransaction,
) -> Result<(), String> {
    let reference = transaction
        .lifecycle
        .as_ref()
        .and_then(|lifecycle| lifecycle.profile_backup.as_ref())
        .ok_or("completed driver transaction lacks its profile backup")?;
    let archive = PreviousDriverTransaction {
        schema_version: PREVIOUS_TRANSACTION_SCHEMA,
        transaction: transaction.clone(),
        profile_backup: load_profile_backup_at(work_dir, reference)?,
    };
    archive.validate()?;
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let _lock = WorkLock::acquire(trusted.path())?;
    write_json_atomic_trusted(&trusted, PREVIOUS_TRANSACTION_FILE, &archive)
        .map_err(|error| format!("archive completed driver transaction: {error}"))?;
    let persisted: PreviousDriverTransaction =
        read_json_trusted(&trusted, PREVIOUS_TRANSACTION_FILE)
            .map_err(|error| format!("read archived driver transaction: {error}"))?;
    persisted.validate()?;
    if persisted != archive {
        return Err("archived driver transaction readback verification failed".into());
    }
    Ok(())
}
