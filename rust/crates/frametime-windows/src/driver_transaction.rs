//! Fixed-root durable NVIDIA driver transaction state.  The record retains no
//! caller paths, URLs, signer values, or executable authority.

use std::path::Path;

use frametime_driver::{
    ArtifactAcquisitionAuthorization, CaptureFreshnessPolicy, DriverExecutionCapture,
    DryRunDriverPlan, InstallationEvidence, RemovalExecutionEvidence, SignedArtifactDescriptor,
};
use serde::{Deserialize, Serialize};

use crate::{TrustedWorkDir, WorkLock, read_json_trusted, timestamp, write_json_atomic_trusted};

const DRIVER_TRANSACTION_FILE: &str = "driver-transaction.json";
const DRIVER_TRANSACTION_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverTransaction {
    pub schema_version: u32,
    pub plan: DryRunDriverPlan,
    pub artifact: SignedArtifactDescriptor,
    pub authorization: ArtifactAcquisitionAuthorization,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<DriverExecutionCapture>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removal: Option<RemovalExecutionEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation: Option<InstallationEvidence>,
}

impl DriverTransaction {
    pub fn prepared(
        plan: DryRunDriverPlan,
        artifact: SignedArtifactDescriptor,
        authorization: ArtifactAcquisitionAuthorization,
    ) -> Result<Self, String> {
        plan.validate().map_err(|error| error.to_string())?;
        artifact
            .validate_for(&plan.target_gpu)
            .map_err(|error| error.to_string())?;
        if authorization.plan_sha256 != plan.input_sha256
            || authorization.target_gpu != plan.target_gpu
        {
            return Err("driver authorization does not bind the prepared plan".into());
        }
        Ok(Self {
            schema_version: DRIVER_TRANSACTION_SCHEMA,
            plan,
            artifact,
            authorization,
            capture: None,
            removal: None,
            installation: None,
        })
    }

    pub fn validate(&self, now_utc: &str, freshness: CaptureFreshnessPolicy) -> Result<(), String> {
        if self.schema_version != DRIVER_TRANSACTION_SCHEMA {
            return Err("driver transaction schema is unsupported".into());
        }
        let expected = Self::prepared(
            self.plan.clone(),
            self.artifact.clone(),
            self.authorization.clone(),
        )?;
        if expected.plan != self.plan || expected.artifact != self.artifact {
            return Err("driver transaction preparation is incoherent".into());
        }
        if let Some(capture) = &self.capture {
            capture
                .validate_for_plan_at(&self.plan, freshness, now_utc)
                .map_err(|error| error.to_string())?;
        }
        if let Some(removal) = &self.removal {
            removal
                .validate_for_plan_at(&self.plan, freshness, now_utc)
                .map_err(|error| error.to_string())?;
            if self.capture.as_ref() != Some(&removal.capture) {
                return Err("driver removal is not bound to retained capture".into());
            }
        }
        if let Some(installation) = &self.installation {
            let capture = self
                .capture
                .as_ref()
                .ok_or("driver installation lacks retained capture")?;
            installation
                .validate_for_plan_at(&self.plan, capture, &self.artifact, freshness, now_utc)
                .map_err(|error| error.to_string())?;
            if installation.authorization != self.authorization {
                return Err("driver installation is not bound to retained authorization".into());
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn removal_complete(&self) -> bool {
        self.removal.is_some()
    }
}

pub fn persist_driver_transaction(
    work_dir: &Path,
    transaction: &DriverTransaction,
) -> Result<DriverTransaction, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let _lock = WorkLock::acquire(trusted.path())?;
    transaction.validate(
        &timestamp(),
        CaptureFreshnessPolicy {
            maximum_age_seconds: 86_400,
        },
    )?;
    write_json_atomic_trusted(&trusted, DRIVER_TRANSACTION_FILE, transaction)
        .map_err(|error| format!("persist driver transaction: {error}"))?;
    let persisted: DriverTransaction = read_json_trusted(&trusted, DRIVER_TRANSACTION_FILE)
        .map_err(|error| format!("read back driver transaction: {error}"))?;
    persisted.validate(
        &timestamp(),
        CaptureFreshnessPolicy {
            maximum_age_seconds: 86_400,
        },
    )?;
    if &persisted != transaction {
        return Err("driver transaction readback verification failed".into());
    }
    Ok(persisted)
}

pub fn load_driver_transaction(work_dir: &Path) -> Result<Option<DriverTransaction>, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    if !trusted.path().join(DRIVER_TRANSACTION_FILE).exists() {
        return Ok(None);
    }
    let transaction: DriverTransaction = read_json_trusted(&trusted, DRIVER_TRANSACTION_FILE)
        .map_err(|error| format!("read driver transaction: {error}"))?;
    transaction.validate(
        &timestamp(),
        CaptureFreshnessPolicy {
            maximum_age_seconds: 86_400,
        },
    )?;
    Ok(Some(transaction))
}
