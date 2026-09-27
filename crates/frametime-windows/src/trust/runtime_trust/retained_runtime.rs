use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use super::{RetainedRuntimeNode, inspect_selected_runtime_integrity_windows};
use crate::{TrustedWorkDir, VerifiedConfig, WINDOWS_WORK_DIR};

use frametime_domain::{
    handoff::RuntimeRecord,
    runtime::{RUNTIME_GENERATIONS_DIR, RuntimeManifest},
};

#[derive(Debug)]
pub(crate) struct InspectedRuntimeIntegrity {
    pub(crate) generation: String,
    pub(crate) manifest_sha256: String,
    pub(crate) manifest: RuntimeManifest,
    pub(crate) config: VerifiedConfig,
    #[cfg(windows)]
    pub(crate) _nodes: Vec<RetainedRuntimeNode>,
}

/// A non-cloneable selected-runtime capability. The retained handles prevent a
/// replacement of the selector, generation tree, manifest, or executable
/// between validation and the handoff side effect.
#[derive(Debug)]
pub struct VerifiedSelectedRuntime {
    pub(crate) record: RuntimeRecord,
    pub(crate) executable_path: PathBuf,
    pub(crate) _inspected: InspectedRuntimeIntegrity,
}

impl VerifiedSelectedRuntime {
    #[must_use]
    pub fn record(&self) -> &RuntimeRecord {
        &self.record
    }

    #[must_use]
    pub fn executable_path(&self) -> &Path {
        &self.executable_path
    }

    #[must_use]
    pub fn config(&self) -> &VerifiedConfig {
        &self._inspected.config
    }
}

/// Reports whether an existing selector cannot be retained and verified. The
/// filesystem observation remains inside the trusted Windows boundary.
#[must_use]
pub fn runtime_inventory_incomplete() -> bool {
    runtime_inventory_incomplete_at(Path::new(WINDOWS_WORK_DIR))
}

pub(crate) fn runtime_inventory_incomplete_at(work_dir: &Path) -> bool {
    let Ok(trusted) = TrustedWorkDir::acquire(work_dir) else {
        return true;
    };
    #[cfg(windows)]
    {
        match trusted.path().join("runtime-current.json").try_exists() {
            Ok(false) => false,
            Ok(true) => inspect_selected_runtime_integrity(&trusted).is_err(),
            Err(_) => true,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = trusted;
        true
    }
}

/// Retain the exact selected executable while a native coordinator writes or
/// reads a reboot handoff. The capability is deliberately non-cloneable.
pub fn retain_selected_runtime() -> Result<VerifiedSelectedRuntime, String> {
    retain_selected_runtime_at(TrustedWorkDir::acquire_fixed()?.path())
}

pub(crate) fn retain_selected_runtime_at(
    work_dir: &Path,
) -> Result<VerifiedSelectedRuntime, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let inspected = inspect_selected_runtime_integrity(&trusted)?;
    let record = RuntimeRecord {
        generation: inspected.generation.clone(),
        manifest_sha256: inspected.manifest_sha256.clone(),
        payload_contract_hash: inspected.manifest.payload_contract_hash.clone(),
        executable_path: inspected.manifest.executable.path.clone(),
        executable_sha256: inspected.manifest.executable.sha256.clone(),
        unknown: BTreeMap::new(),
    };
    let executable_path = trusted
        .path()
        .join(RUNTIME_GENERATIONS_DIR)
        .join(&record.generation)
        .join(&record.executable_path);
    Ok(VerifiedSelectedRuntime {
        record,
        executable_path,
        _inspected: inspected,
    })
}

pub(crate) fn inspect_selected_runtime_integrity(
    trusted: &TrustedWorkDir,
) -> Result<InspectedRuntimeIntegrity, String> {
    #[cfg(windows)]
    {
        inspect_selected_runtime_integrity_windows(trusted)
    }
    #[cfg(not(windows))]
    {
        let _ = trusted;
        Err("runtime integrity inspection requires supported Windows x64".into())
    }
}
