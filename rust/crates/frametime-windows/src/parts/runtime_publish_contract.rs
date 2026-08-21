#[cfg(windows)]
pub(crate) const SAFE_MODE_HANDOFF_ARGUMENTS: &str = "boot-safe-mode --yes";

#[cfg(windows)]
pub(crate) fn valid_generated_id(value: &str) -> bool {
    value.len() == 32
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(windows)]
pub(crate) fn payload_directories() -> Vec<&'static str> {
    let mut directories = BTreeSet::new();
    for path in frametime_core::runtime::RUNTIME_PAYLOAD_PATHS {
        let mut candidate = path;
        while let Some((parent, _)) = candidate.rsplit_once('/') {
            directories.insert(parent);
            candidate = parent;
        }
    }
    directories.into_iter().collect()
}

#[cfg(windows)]
pub(crate) fn manifest_for(
    generation: String,
    files: std::collections::BTreeMap<String, String>,
) -> Result<frametime_core::runtime::RuntimeManifest, String> {
    if !valid_generated_id(&generation) {
        return Err("published runtime generation is not lower-hex".into());
    }
    let actual = files.keys().map(String::as_str).collect::<std::collections::BTreeSet<_>>();
    let expected = frametime_core::runtime::RUNTIME_PAYLOAD_PATHS.into_iter().collect::<std::collections::BTreeSet<_>>();
    if actual != expected {
        return Err("published runtime file set differs from compiled payload contract".into());
    }
    let executable = files.get("frametime.exe").cloned().ok_or("compiled payload does not include frametime.exe")?;
    Ok(frametime_core::runtime::RuntimeManifest {
        schema_version: frametime_core::runtime::RUNTIME_SCHEMA_VERSION,
        generation,
        files,
        payload_contract_hash: frametime_core::runtime::portable_payload_contract_hash(),
        executable: frametime_core::runtime::RuntimeExecutableRecord { path: "frametime.exe".into(), sha256: executable, unknown: BTreeMap::new() },
        unknown: BTreeMap::new(),
    })
}
