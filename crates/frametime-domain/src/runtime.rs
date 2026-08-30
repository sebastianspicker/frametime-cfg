//! Runtime selector and manifest contracts.
//!
//! Filesystem walks, hashing, locking, publication, and reparse-point checks
//! are host-adapter responsibilities. This module validates only supplied
//! facts and immutable bytes.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const RUNTIME_SCHEMA_VERSION: u8 = 1;
pub const RUNTIME_GENERATIONS_DIR: &str = "runtime-generations";

pub const RUNTIME_PAYLOAD_PATHS: [&str; 24] = [
    "frametime.exe",
    "frametime.toml",
    "assets/video.txt",
    "assets/cfgs/audio_eq_crisp.cfg",
    "assets/cfgs/audio_eq_smooth.cfg",
    "assets/cfgs/audio_legacy_lr_isolation.cfg",
    "assets/cfgs/audio_lowlatency_001.cfg",
    "assets/cfgs/audio_lowlatency_025.cfg",
    "assets/cfgs/audio_stable.cfg",
    "assets/cfgs/autoexec.cfg.example",
    "assets/cfgs/competitive_2026.cfg",
    "assets/cfgs/debug_hud.cfg",
    "assets/cfgs/debug_hud_off.cfg",
    "assets/cfgs/frame_baseline_400.cfg",
    "assets/cfgs/frame_raw_uncapped.cfg",
    "assets/cfgs/net_bad.cfg",
    "assets/cfgs/net_highping.cfg",
    "assets/cfgs/net_stable.cfg",
    "assets/cfgs/net_unstable.cfg",
    "assets/cfgs/optimization.cfg.template",
    "assets/cfgs/valve-latency-targets.json",
    "docs/native/nvidia-drs-settings.md",
    "licenses/LICENSE",
    "licenses/THIRD_PARTY_NOTICES.md",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifest {
    pub schema_version: u8,
    pub generation: String,
    pub files: BTreeMap<String, String>,
    pub payload_contract_hash: String,
    pub executable: RuntimeExecutableRecord,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeExecutableRecord {
    pub path: String,
    pub sha256: String,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCurrent {
    pub schema_version: u8,
    pub relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_sha256: Option<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

pub fn validate_selected_runtime(
    current: &RuntimeCurrent,
    manifest_bytes: &[u8],
    manifest: &RuntimeManifest,
    payload_hashes: &BTreeMap<String, String>,
) -> Result<String, String> {
    if current.schema_version != RUNTIME_SCHEMA_VERSION {
        return Err("unsupported runtime selector schema".into());
    }
    let prefix = format!("{RUNTIME_GENERATIONS_DIR}/");
    let generation = current
        .relative_path
        .strip_prefix(&prefix)
        .filter(|value| current.relative_path == format!("{prefix}{value}"))
        .filter(|value| valid_generation_id(value))
        .ok_or("unsafe selected runtime generation")?;
    let selector_hash = current
        .manifest_sha256
        .as_deref()
        .filter(|value| valid_sha256(value))
        .ok_or("runtime selector missing manifest hash")?;
    if hex_sha256(manifest_bytes) != selector_hash {
        return Err("runtime selector manifest hash mismatch".into());
    }
    if manifest.schema_version != RUNTIME_SCHEMA_VERSION || manifest.generation != generation {
        return Err("runtime selector and manifest generation differ".into());
    }
    let expected = RUNTIME_PAYLOAD_PATHS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let declared = manifest
        .files
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let observed = payload_hashes
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if declared != expected || observed != expected {
        return Err("runtime file set differs from compiled payload contract".into());
    }
    if manifest.payload_contract_hash != portable_payload_contract_hash()
        || manifest.executable.path != "frametime.exe"
        || manifest.files.get("frametime.exe") != Some(&manifest.executable.sha256)
        || !valid_sha256(&manifest.executable.sha256)
    {
        return Err("runtime executable record is invalid".into());
    }
    for (path, hash) in &manifest.files {
        if !valid_sha256(hash) || payload_hashes.get(path) != Some(hash) {
            return Err(format!("runtime hash mismatch: {path}"));
        }
    }
    Ok(generation.to_owned())
}

#[must_use]
pub fn portable_payload_contract_hash() -> String {
    let paths = RUNTIME_PAYLOAD_PATHS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    hex_sha256(paths.into_iter().collect::<Vec<_>>().join("\n").as_bytes())
}

fn valid_generation_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_contract_is_stable_and_unique() {
        assert_eq!(
            RUNTIME_PAYLOAD_PATHS.iter().collect::<BTreeSet<_>>().len(),
            RUNTIME_PAYLOAD_PATHS.len()
        );
        assert_eq!(portable_payload_contract_hash().len(), 64);
    }
}
