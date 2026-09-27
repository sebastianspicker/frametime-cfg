use crate::*;
#[cfg(windows)]
use frametime_domain::runtime::{
    RUNTIME_GENERATIONS_DIR, RUNTIME_PAYLOAD_PATHS, RUNTIME_SCHEMA_VERSION, RuntimeCurrent,
    RuntimeManifest, portable_payload_contract_hash,
};

pub(crate) const MAX_RUNTIME_METADATA_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_RUNTIME_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;

#[cfg(windows)]
pub(crate) fn selected_generation(current: &RuntimeCurrent) -> Result<&str, String> {
    if current.schema_version != RUNTIME_SCHEMA_VERSION {
        return Err("unsupported runtime selector schema".into());
    }
    let prefix = format!("{RUNTIME_GENERATIONS_DIR}/");
    let Some(generation) = current.relative_path.strip_prefix(&prefix) else {
        return Err("runtime selector is not rooted in runtime-generations".into());
    };
    if current.relative_path != format!("{prefix}{generation}") || !valid_generation(generation) {
        return Err("runtime selector generation is not exact lower-hex".into());
    }
    if !current.manifest_sha256.as_deref().is_some_and(valid_sha256) {
        return Err("runtime selector is missing a valid manifest hash".into());
    }
    Ok(generation)
}

#[cfg(windows)]
pub(crate) fn validate_runtime_contract(
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

#[cfg(windows)]
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

#[cfg(windows)]
fn executable_record_is_valid(manifest: &RuntimeManifest) -> bool {
    manifest.executable.path == "frametime.exe"
        && manifest.files.get("frametime.exe") == Some(&manifest.executable.sha256)
        && valid_sha256(&manifest.executable.sha256)
}

#[cfg(windows)]
pub(crate) fn valid_generation(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(windows)]
pub(crate) fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
