#[cfg(windows)]
use crate::*;
#[cfg(windows)]
pub(crate) const SAFE_MODE_HANDOFF_ARGUMENTS: &str = "boot-safe-mode --yes";

#[cfg(windows)]
pub(crate) fn manifest_for(
    generation: String,
    files: std::collections::BTreeMap<String, String>,
) -> Result<frametime_domain::runtime::RuntimeManifest, String> {
    if !frametime_domain::runtime::valid_generation_id(&generation) {
        return Err("published runtime generation is not lower-hex".into());
    }
    let actual = files
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    let expected = frametime_domain::runtime::RUNTIME_PAYLOAD_PATHS
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    if actual != expected {
        return Err("published runtime file set differs from compiled payload contract".into());
    }
    let executable = files
        .get("frametime.exe")
        .cloned()
        .ok_or("compiled payload does not include frametime.exe")?;
    Ok(frametime_domain::runtime::RuntimeManifest {
        schema_version: frametime_domain::runtime::RUNTIME_SCHEMA_VERSION,
        generation,
        files,
        payload_contract_hash: frametime_domain::runtime::portable_payload_contract_hash(),
        executable: frametime_domain::runtime::RuntimeExecutableRecord {
            path: "frametime.exe".into(),
            sha256: executable,
            unknown: BTreeMap::new(),
        },
        unknown: BTreeMap::new(),
    })
}
