//! Runtime selector and manifest contracts.
//!
//! Filesystem walks, hashing, locking, publication, and reparse-point checks
//! are host-adapter responsibilities. This module validates only supplied
//! facts and immutable bytes.

use std::collections::{BTreeMap, BTreeSet};

use crate::hex::lower_hex;
use crate::validate::is_lower_hex;
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

/// Returns the selected generation when the selector is exact and rooted in
/// `runtime-generations`.
pub fn selected_generation(current: &RuntimeCurrent) -> Result<&str, String> {
    if current.schema_version != RUNTIME_SCHEMA_VERSION {
        return Err("unsupported runtime selector schema".into());
    }
    let prefix = format!("{RUNTIME_GENERATIONS_DIR}/");
    let Some(generation) = current.relative_path.strip_prefix(&prefix) else {
        return Err("runtime selector is not rooted in runtime-generations".into());
    };
    if current.relative_path != format!("{prefix}{generation}") || !valid_generation_id(generation)
    {
        return Err("runtime selector generation is not exact lower-hex".into());
    }
    if !current.manifest_sha256.as_deref().is_some_and(valid_sha256) {
        return Err("runtime selector is missing a valid manifest hash".into());
    }
    Ok(generation)
}

/// Validates host-observed selector, manifest hash, manifest, and payload
/// hashes against the compiled runtime payload contract.
pub fn validate_runtime_contract(
    current: &RuntimeCurrent,
    manifest_hash: &str,
    manifest: &RuntimeManifest,
    payload_hashes: &BTreeMap<String, String>,
) -> Result<String, String> {
    let generation = selected_generation(current)?;
    if current.manifest_sha256.as_deref() != Some(manifest_hash)
        || manifest.schema_version != RUNTIME_SCHEMA_VERSION
        || manifest.generation != generation
    {
        return Err("runtime selector and manifest differ".into());
    }
    let expected = RUNTIME_PAYLOAD_PATHS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if !payload_sets_are_exact(manifest, payload_hashes, &expected)
        || !executable_record_is_valid(manifest)
    {
        return Err("runtime payload contract is invalid".into());
    }
    for (path, declared_hash) in &manifest.files {
        if !valid_sha256(declared_hash)
            || payload_hashes
                .get(path)
                .is_none_or(|actual| actual != declared_hash)
        {
            return Err(format!("runtime payload hash mismatch: {path}"));
        }
    }
    Ok(generation.to_owned())
}

fn payload_sets_are_exact(
    manifest: &RuntimeManifest,
    payload_hashes: &BTreeMap<String, String>,
    expected: &BTreeSet<&str>,
) -> bool {
    let declared = manifest
        .files
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let observed = payload_hashes
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    declared == *expected
        && observed == *expected
        && manifest.payload_contract_hash == portable_payload_contract_hash()
}

fn executable_record_is_valid(manifest: &RuntimeManifest) -> bool {
    manifest.executable.path == "frametime.exe"
        && manifest.files.get("frametime.exe") == Some(&manifest.executable.sha256)
        && valid_sha256(&manifest.executable.sha256)
}

/// Every ancestor directory of a compiled payload path, parents first.
#[must_use]
pub fn runtime_payload_directories() -> BTreeSet<&'static str> {
    let mut directories = BTreeSet::new();
    for path in RUNTIME_PAYLOAD_PATHS {
        let mut candidate = path;
        while let Some((parent, _)) = candidate.rsplit_once('/') {
            directories.insert(parent);
            candidate = parent;
        }
    }
    directories
}

#[must_use]
pub fn portable_payload_contract_hash() -> String {
    let paths = RUNTIME_PAYLOAD_PATHS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    hex_sha256(paths.into_iter().collect::<Vec<_>>().join("\n").as_bytes())
}

/// A runtime generation ID: exactly 32 lowercase hex digits.
#[must_use]
pub fn valid_generation_id(value: &str) -> bool {
    is_lower_hex(value, 32)
}

pub(crate) fn valid_sha256(value: &str) -> bool {
    is_lower_hex(value, 64)
}

fn hex_sha256(bytes: &[u8]) -> String {
    lower_hex(Sha256::digest(bytes).as_slice())
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

    const GENERATION: &str = "0123456789abcdef0123456789abcdef";
    const MANIFEST_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    fn valid_case() -> (RuntimeCurrent, RuntimeManifest, BTreeMap<String, String>) {
        let hashes = RUNTIME_PAYLOAD_PATHS
            .iter()
            .map(|path| ((*path).to_owned(), hex_sha256(path.as_bytes())))
            .collect::<BTreeMap<_, _>>();
        let current = RuntimeCurrent {
            schema_version: RUNTIME_SCHEMA_VERSION,
            relative_path: format!("{RUNTIME_GENERATIONS_DIR}/{GENERATION}"),
            published_utc: None,
            manifest_sha256: Some(MANIFEST_HASH.into()),
            unknown: BTreeMap::new(),
        };
        let manifest = RuntimeManifest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            generation: GENERATION.into(),
            files: hashes.clone(),
            payload_contract_hash: portable_payload_contract_hash(),
            executable: RuntimeExecutableRecord {
                path: "frametime.exe".into(),
                sha256: hashes["frametime.exe"].clone(),
                unknown: BTreeMap::new(),
            },
            unknown: BTreeMap::new(),
        };
        (current, manifest, hashes)
    }

    fn check(
        current: &RuntimeCurrent,
        manifest: &RuntimeManifest,
        hashes: &BTreeMap<String, String>,
    ) -> Result<String, String> {
        validate_runtime_contract(current, MANIFEST_HASH, manifest, hashes)
    }

    #[test]
    fn valid_runtime_contract_is_accepted() {
        let (current, manifest, hashes) = valid_case();
        assert_eq!(check(&current, &manifest, &hashes), Ok(GENERATION.into()));
    }

    #[test]
    fn differing_payload_hash_is_rejected() {
        let (current, manifest, mut hashes) = valid_case();
        hashes.insert("frametime.toml".into(), MANIFEST_HASH.into());
        assert_eq!(
            check(&current, &manifest, &hashes),
            Err("runtime payload hash mismatch: frametime.toml".into())
        );
    }

    #[test]
    fn extra_or_missing_files_are_rejected() {
        let (current, manifest, mut hashes) = valid_case();
        hashes.insert("extra.dll".into(), MANIFEST_HASH.into());
        assert!(check(&current, &manifest, &hashes).is_err());

        let (current, manifest, mut hashes) = valid_case();
        hashes.remove("licenses/LICENSE");
        assert!(check(&current, &manifest, &hashes).is_err());

        let (current, mut manifest, hashes) = valid_case();
        manifest.files.remove("licenses/LICENSE");
        assert!(check(&current, &manifest, &hashes).is_err());
    }

    #[test]
    fn uppercase_hash_is_rejected() {
        let (current, mut manifest, mut hashes) = valid_case();
        let upper = hashes["frametime.toml"].to_ascii_uppercase();
        hashes.insert("frametime.toml".into(), upper.clone());
        manifest.files.insert("frametime.toml".into(), upper);
        assert!(check(&current, &manifest, &hashes).is_err());
    }

    #[test]
    fn executable_path_other_than_frametime_exe_is_rejected() {
        let (current, mut manifest, hashes) = valid_case();
        manifest.executable.path = "frametime-gui.exe".into();
        assert!(check(&current, &manifest, &hashes).is_err());
    }

    #[test]
    fn selector_and_manifest_mismatches_are_rejected() {
        let (current, mut manifest, hashes) = valid_case();
        manifest.generation = "fedcba9876543210fedcba9876543210".into();
        assert!(check(&current, &manifest, &hashes).is_err());

        let (current, manifest, hashes) = valid_case();
        let other_hash = "2".repeat(64);
        assert!(validate_runtime_contract(&current, &other_hash, &manifest, &hashes).is_err());

        let (current, mut manifest, hashes) = valid_case();
        manifest.payload_contract_hash = other_hash;
        assert!(check(&current, &manifest, &hashes).is_err());
    }

    #[test]
    fn generation_ids_are_exact_lower_hex() {
        assert!(valid_generation_id(GENERATION));
        assert!(!valid_generation_id(&GENERATION.to_ascii_uppercase()));
        assert!(!valid_generation_id(&GENERATION[1..]));
    }

    #[test]
    fn payload_directories_cover_every_payload_parent() {
        assert_eq!(
            runtime_payload_directories()
                .into_iter()
                .collect::<Vec<_>>(),
            ["assets", "assets/cfgs", "docs", "docs/native", "licenses"]
        );
    }

    #[test]
    fn every_runtime_payload_is_in_the_package_layout() {
        let layout = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../package-layout.txt"
        ))
        .expect("read package-layout.txt");
        let lines = layout.lines().map(str::trim).collect::<BTreeSet<_>>();
        for path in RUNTIME_PAYLOAD_PATHS {
            assert!(lines.contains(path), "package-layout.txt lacks {path}");
        }
    }
}
