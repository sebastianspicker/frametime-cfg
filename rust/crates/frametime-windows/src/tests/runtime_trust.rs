use super::*;
use frametime_core::runtime::RuntimeExecutableRecord;

const GENERATION: &str = "0123456789abcdef0123456789abcdef";

fn hash(fill: char) -> String {
    std::iter::repeat_n(fill, 64).collect()
}

fn complete_manifest() -> (RuntimeCurrent, RuntimeManifest, BTreeMap<String, String>) {
    let files = RUNTIME_PAYLOAD_PATHS
        .iter()
        .map(|path| ((*path).to_owned(), hash('a')))
        .collect::<BTreeMap<_, _>>();
    let current = RuntimeCurrent {
        schema_version: RUNTIME_SCHEMA_VERSION,
        relative_path: format!("{RUNTIME_GENERATIONS_DIR}/{GENERATION}"),
        published_utc: None,
        manifest_sha256: Some(hash('b')),
        unknown: BTreeMap::new(),
    };
    let manifest = RuntimeManifest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        generation: GENERATION.into(),
        files: files.clone(),
        payload_contract_hash: portable_payload_contract_hash(),
        executable: RuntimeExecutableRecord {
            path: "frametime.exe".into(),
            sha256: hash('a'),
            unknown: BTreeMap::new(),
        },
        unknown: BTreeMap::new(),
    };
    (current, manifest, files)
}

#[test]
fn exact_compiled_payload_contract_is_accepted() {
    let (current, manifest, hashes) = complete_manifest();
    assert_eq!(
        validate_runtime_contract(&current, &hash('b'), &manifest, &hashes),
        Ok(GENERATION.into())
    );
}

#[test]
fn selector_requires_a_lower_hex_generation_and_bound_manifest() {
    let (mut current, manifest, hashes) = complete_manifest();
    current.relative_path = format!("{RUNTIME_GENERATIONS_DIR}/{}", "A".repeat(32));
    assert!(selected_generation(&current).is_err());
    current.relative_path = format!("{RUNTIME_GENERATIONS_DIR}/{GENERATION}");
    assert!(validate_runtime_contract(&current, &hash('c'), &manifest, &hashes).is_err());
}

#[test]
fn extra_or_tampered_payload_is_rejected() {
    let (current, mut manifest, mut hashes) = complete_manifest();
    manifest.files.insert("extra.txt".into(), hash('a'));
    assert!(validate_runtime_contract(&current, &hash('b'), &manifest, &hashes).is_err());
    manifest.files.remove("extra.txt");
    hashes.insert("frametime.exe".into(), hash('d'));
    assert!(validate_runtime_contract(&current, &hash('b'), &manifest, &hashes).is_err());
}
