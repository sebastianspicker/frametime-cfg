use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    cs2_config::{
        Cs2ConfigController, Cs2ConfigError, Cs2ConfigFs, Cs2ConfigRequest, Cs2ConfigTarget,
    },
    logging::legacy_timestamp_now,
    steam::{CS2_APP_ID, Cs2Install},
};

use super::BackupEntry;

pub const CS2_CONFIG_TRANSACTION_STEP: &str = "P1:34";
pub const CS2_CONFIG_MAX_FILE_BYTES: usize = 1024 * 1024;
pub const CS2_CONFIG_MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;

/// A path-free identity of one already validated CS2 install.
///
/// This fingerprint is comparison evidence only. It cannot be used to choose
/// a restore location; a future restore must accept a new `Cs2Install` and
/// independently validate its trusted Steam binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cs2InstallIdentity {
    #[serde(rename = "steamAppId")]
    pub steam_app_id: String,
    #[serde(rename = "installFingerprint")]
    pub install_fingerprint: String,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

/// One exact original state for a fixed CS2 CFG target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cs2ConfigSnapshot {
    pub target: Cs2ConfigTarget,
    pub existed: bool,
    #[serde(
        default,
        rename = "originalBytes",
        skip_serializing_if = "Option::is_none"
    )]
    pub original_bytes: Option<Vec<u8>>,
    #[serde(
        default,
        rename = "originalSha256",
        skip_serializing_if = "Option::is_none"
    )]
    pub original_sha256: Option<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Error)]
pub enum Cs2ConfigBackupError {
    #[error(transparent)]
    Config(#[from] Cs2ConfigError),
    #[error("unable to capture {target:?}: {source}")]
    Read {
        target: Cs2ConfigTarget,
        #[source]
        source: io::Error,
    },
    #[error("CS2 CFG snapshot for {target:?} exceeds {limit} bytes ({actual} bytes)")]
    FileTooLarge {
        target: Cs2ConfigTarget,
        actual: usize,
        limit: usize,
    },
    #[error("CS2 CFG snapshot exceeds {limit} total bytes ({actual} bytes)")]
    TotalTooLarge { actual: usize, limit: usize },
    #[error("CS2 CFG backup entry must be for {expected}, found {actual}")]
    WrongStep {
        expected: &'static str,
        actual: String,
    },
    #[error("backup entry is not a CS2 CFG transaction")]
    WrongEntryType,
    #[error("CS2 CFG transaction has duplicate target {0:?}")]
    DuplicateTarget(Cs2ConfigTarget),
    #[error("CS2 CFG transaction is missing target {0:?}")]
    MissingTarget(Cs2ConfigTarget),
    #[error("CS2 CFG transaction includes unexpected target {0:?}")]
    UnexpectedTarget(Cs2ConfigTarget),
    #[error("CS2 CFG transaction target order does not match the controller write order")]
    TargetOrderMismatch,
    #[error("absent CS2 CFG target {0:?} must not carry original bytes or a digest")]
    AbsentTargetHasBytes(Cs2ConfigTarget),
    #[error("existing CS2 CFG target {0:?} is missing original bytes")]
    ExistingTargetMissingBytes(Cs2ConfigTarget),
    #[error("existing CS2 CFG target {0:?} is missing its SHA-256 digest")]
    MissingDigest(Cs2ConfigTarget),
    #[error("CS2 CFG snapshot digest does not match original bytes for {0:?}")]
    DigestMismatch(Cs2ConfigTarget),
    #[error("CS2 CFG install identity does not match the validated current install")]
    InstallMismatch,
    #[error("CS2 CFG restore transaction has unsupported unknown fields")]
    UnknownFields,
}

impl BackupEntry {
    /// Captures every fixed target that a P1:34 controller request may overwrite.
    ///
    /// The function performs no mutation through `files`. It reads all targets
    /// through the supplied seam and returns no entry unless the full snapshot
    /// has passed the size and completeness checks.
    pub fn capture_cs2_config_transaction(
        install: &Cs2Install,
        request: &Cs2ConfigRequest,
        files: &mut dyn Cs2ConfigFs,
    ) -> Result<Self, Cs2ConfigBackupError> {
        let controller = Cs2ConfigController::new(install.clone())?;
        let expected_targets = Cs2ConfigTarget::for_request(request);
        let target_paths = controller.backup_targets(request)?;
        debug_assert_eq!(
            target_paths
                .iter()
                .map(|(target, _)| *target)
                .collect::<Vec<_>>(),
            expected_targets
        );

        let mut total_bytes = 0usize;
        let mut targets = Vec::with_capacity(target_paths.len());
        for (target, path) in target_paths {
            let original_bytes = match files.read_file(&path) {
                Ok(bytes) => {
                    if bytes.len() > CS2_CONFIG_MAX_FILE_BYTES {
                        return Err(Cs2ConfigBackupError::FileTooLarge {
                            target,
                            actual: bytes.len(),
                            limit: CS2_CONFIG_MAX_FILE_BYTES,
                        });
                    }
                    total_bytes = total_bytes.saturating_add(bytes.len());
                    if total_bytes > CS2_CONFIG_MAX_TOTAL_BYTES {
                        return Err(Cs2ConfigBackupError::TotalTooLarge {
                            actual: total_bytes,
                            limit: CS2_CONFIG_MAX_TOTAL_BYTES,
                        });
                    }
                    Some(bytes)
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(source) => return Err(Cs2ConfigBackupError::Read { target, source }),
            };
            let original_sha256 = original_bytes.as_deref().map(sha256_hex);
            targets.push(Cs2ConfigSnapshot {
                target,
                existed: original_bytes.is_some(),
                original_bytes,
                original_sha256,
                unknown: BTreeMap::new(),
            });
        }

        let entry = Self::Cs2ConfigTransaction {
            step: CS2_CONFIG_TRANSACTION_STEP.to_owned(),
            timestamp: legacy_timestamp_now(),
            install_identity: install_identity(controller.install()),
            targets,
            unknown: BTreeMap::new(),
        };
        entry.validate_cs2_config_transaction(install, request)?;
        Ok(entry)
    }

    /// Verifies that this transaction is complete and belongs to exactly this
    /// validated install and controller request. This grants no restore action.
    pub fn validate_cs2_config_transaction(
        &self,
        install: &Cs2Install,
        request: &Cs2ConfigRequest,
    ) -> Result<(), Cs2ConfigBackupError> {
        let Self::Cs2ConfigTransaction {
            step,
            install_identity: captured_install,
            targets,
            ..
        } = self
        else {
            return Err(Cs2ConfigBackupError::WrongEntryType);
        };
        if step != CS2_CONFIG_TRANSACTION_STEP {
            return Err(Cs2ConfigBackupError::WrongStep {
                expected: CS2_CONFIG_TRANSACTION_STEP,
                actual: step.clone(),
            });
        }

        let controller = Cs2ConfigController::new(install.clone())?;
        let expected_install = install_identity(controller.install());
        if captured_install.steam_app_id != expected_install.steam_app_id
            || captured_install.install_fingerprint != expected_install.install_fingerprint
        {
            return Err(Cs2ConfigBackupError::InstallMismatch);
        }
        validate_snapshot_targets(targets, &Cs2ConfigTarget::for_request(request))
    }

    /// Restores only a fully known transaction after a fresh install binding.
    /// Serialized paths are never consulted; the controller derives targets
    /// anew from the rediscovered install and the fixed request.
    pub fn restore_cs2_config_transaction(
        &self,
        install: &Cs2Install,
        request: &Cs2ConfigRequest,
        files: &mut dyn Cs2ConfigFs,
    ) -> Result<(), Cs2ConfigBackupError> {
        self.validate_cs2_config_transaction(install, request)?;
        let Self::Cs2ConfigTransaction {
            install_identity,
            targets,
            unknown,
            ..
        } = self
        else {
            return Err(Cs2ConfigBackupError::WrongEntryType);
        };
        if !unknown.is_empty()
            || !install_identity.unknown.is_empty()
            || targets.iter().any(|snapshot| !snapshot.unknown.is_empty())
        {
            return Err(Cs2ConfigBackupError::UnknownFields);
        }
        let controller = Cs2ConfigController::new(install.clone())?;
        let snapshots = targets
            .iter()
            .map(|snapshot| (snapshot.target, snapshot.original_bytes.as_deref()))
            .collect::<Vec<_>>();
        controller.restore(request, &snapshots, files)?;
        Ok(())
    }
}

fn install_identity(install: &Cs2Install) -> Cs2InstallIdentity {
    let mut hasher = Sha256::new();
    hasher.update(b"frametime-cfg/cs2-install/v1\0");
    for path in [
        &install.steam_root,
        &install.library_root,
        &install.install_root,
    ] {
        let bytes = path.as_os_str().as_encoded_bytes();
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    Cs2InstallIdentity {
        steam_app_id: CS2_APP_ID.to_owned(),
        install_fingerprint: hex_digest(&hasher.finalize()),
        unknown: BTreeMap::new(),
    }
}

fn validate_snapshot_targets(
    snapshots: &[Cs2ConfigSnapshot],
    expected: &[Cs2ConfigTarget],
) -> Result<(), Cs2ConfigBackupError> {
    let mut seen = BTreeSet::new();
    let expected_set = expected.iter().copied().collect::<BTreeSet<_>>();
    let mut total_bytes = 0usize;
    for snapshot in snapshots {
        if !seen.insert(snapshot.target) {
            return Err(Cs2ConfigBackupError::DuplicateTarget(snapshot.target));
        }
        if !expected_set.contains(&snapshot.target) {
            return Err(Cs2ConfigBackupError::UnexpectedTarget(snapshot.target));
        }
        validate_snapshot_bytes(snapshot, &mut total_bytes)?;
    }
    for target in expected {
        if !seen.contains(target) {
            return Err(Cs2ConfigBackupError::MissingTarget(*target));
        }
    }
    if snapshots
        .iter()
        .map(|snapshot| snapshot.target)
        .ne(expected.iter().copied())
    {
        return Err(Cs2ConfigBackupError::TargetOrderMismatch);
    }
    Ok(())
}

fn validate_snapshot_bytes(
    snapshot: &Cs2ConfigSnapshot,
    total_bytes: &mut usize,
) -> Result<(), Cs2ConfigBackupError> {
    match (&snapshot.original_bytes, &snapshot.original_sha256) {
        (None, None) if !snapshot.existed => Ok(()),
        (None, Some(_)) | (Some(_), _) if !snapshot.existed => {
            Err(Cs2ConfigBackupError::AbsentTargetHasBytes(snapshot.target))
        }
        (None, _) => Err(Cs2ConfigBackupError::ExistingTargetMissingBytes(
            snapshot.target,
        )),
        (Some(bytes), digest) => {
            if bytes.len() > CS2_CONFIG_MAX_FILE_BYTES {
                return Err(Cs2ConfigBackupError::FileTooLarge {
                    target: snapshot.target,
                    actual: bytes.len(),
                    limit: CS2_CONFIG_MAX_FILE_BYTES,
                });
            }
            *total_bytes = total_bytes.saturating_add(bytes.len());
            if *total_bytes > CS2_CONFIG_MAX_TOTAL_BYTES {
                return Err(Cs2ConfigBackupError::TotalTooLarge {
                    actual: *total_bytes,
                    limit: CS2_CONFIG_MAX_TOTAL_BYTES,
                });
            }
            let Some(digest) = digest.as_deref() else {
                return Err(Cs2ConfigBackupError::MissingDigest(snapshot.target));
            };
            if digest != sha256_hex(bytes) {
                return Err(Cs2ConfigBackupError::DigestMismatch(snapshot.target));
            }
            Ok(())
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_digest(&Sha256::digest(bytes))
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
}
