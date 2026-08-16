#[cfg(any(test, windows))]
fn selected_generation(current: &RuntimeCurrent) -> Result<&str, String> {
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

#[cfg(any(test, windows))]
fn validate_runtime_contract(
    current: &RuntimeCurrent,
    manifest_hash: &str,
    manifest: &RuntimeManifest,
    payload_hashes: &BTreeMap<String, String>,
) -> Result<String, String> {
    let generation = selected_generation(current)?;
    if current.manifest_sha256.as_deref() != Some(manifest_hash) {
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
    if declared != expected
        || payload_hashes
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != expected
    {
        return Err("runtime payload set does not equal the compiled contract".into());
    }
    if manifest.payload_contract_hash != portable_payload_contract_hash() {
        return Err("runtime payload contract hash mismatch".into());
    }
    if manifest.executable.path != "frametime.exe"
        || manifest.files.get("frametime.exe") != Some(&manifest.executable.sha256)
        || !valid_sha256(&manifest.executable.sha256)
    {
        return Err("runtime executable record is invalid".into());
    }
    for (path, declared_hash) in &manifest.files {
        if !valid_sha256(declared_hash) || payload_hashes.get(path) != Some(declared_hash) {
            return Err(format!("runtime payload hash mismatch: {path}"));
        }
    }
    Ok(generation.to_owned())
}

#[cfg(any(test, windows))]
fn valid_generation(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(any(test, windows))]
fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
