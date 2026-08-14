use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::persistence::{read_json_preserving_corrupt, safe_relative_path, write_json_atomic};

pub const RUNTIME_SCHEMA_VERSION: u8 = 1;
pub const RUNTIME_GENERATIONS_DIR: &str = "runtime-generations";

/// Exact portable payload required by reboot phases. The GUI and launchers are
/// deliberately excluded because handoffs execute the CLI directly.
pub const RUNTIME_PAYLOAD_PATHS: [&str; 18] = [
    "frametime.exe",
    "frametime.toml",
    "assets/video.txt",
    "assets/cfgs/audio_lowlatency_001.cfg",
    "assets/cfgs/audio_lowlatency_025.cfg",
    "assets/cfgs/audio_stable.cfg",
    "assets/cfgs/autoexec.cfg.example",
    "assets/cfgs/debug_hud.cfg",
    "assets/cfgs/debug_hud_off.cfg",
    "assets/cfgs/net_bad.cfg",
    "assets/cfgs/net_highping.cfg",
    "assets/cfgs/net_stable.cfg",
    "assets/cfgs/net_unstable.cfg",
    "assets/cfgs/optimization.cfg.template",
    "assets/cfgs/valve-latency-targets.json",
    "docs/nvidia-drs-settings.md",
    "licenses/LICENSE",
    "licenses/THIRD_PARTY_NOTICES.md",
];

#[must_use]
pub fn runtime_payload_paths() -> Vec<PathBuf> {
    RUNTIME_PAYLOAD_PATHS.iter().map(PathBuf::from).collect()
}

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
    /// Legacy selectors deserialize without this field, but cannot authorize a
    /// reboot handoff because the selected manifest is not selector-bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_sha256: Option<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

/// Publishes an already-staged generation after exact-set and hash validation.
pub fn publish_generation(
    runtime_root: &Path,
    generation: &str,
    expected: &[PathBuf],
) -> Result<RuntimeManifest, String> {
    if !valid_generation_id(generation) {
        return Err("unsafe runtime generation id".into());
    }
    fs::create_dir_all(runtime_root).map_err(|error| error.to_string())?;
    let _lock = PublicationLock::acquire(runtime_root.join("runtime-publication.lock"))?;
    let generation_root = runtime_root.join(RUNTIME_GENERATIONS_DIR).join(generation);
    let manifest = build_manifest(&generation_root, generation.to_owned(), expected)?;
    let manifest_path = generation_root.join("runtime-manifest.json");
    write_json_atomic(&manifest_path, &manifest).map_err(|error| error.to_string())?;
    verify_manifest(&generation_root, &manifest)?;
    let current = RuntimeCurrent {
        schema_version: RUNTIME_SCHEMA_VERSION,
        relative_path: format!("{RUNTIME_GENERATIONS_DIR}/{generation}"),
        published_utc: None,
        manifest_sha256: Some(hex_sha256(
            &fs::read(&manifest_path).map_err(|error| format!("read runtime manifest: {error}"))?,
        )),
        unknown: BTreeMap::new(),
    };
    write_json_atomic(&runtime_root.join("runtime-current.json"), &current)
        .map_err(|error| error.to_string())?;
    Ok(manifest)
}

pub fn publish_portable_generation(
    runtime_root: &Path,
    generation: &str,
) -> Result<RuntimeManifest, String> {
    publish_generation(runtime_root, generation, &runtime_payload_paths())
}

/// Resolves and verifies the atomically selected immutable runtime generation.
pub fn load_selected_generation(runtime_root: &Path) -> Result<(PathBuf, RuntimeManifest), String> {
    let current: RuntimeCurrent =
        read_json_preserving_corrupt(&runtime_root.join("runtime-current.json"))
            .map_err(|error| format!("read runtime selector: {error}"))?;
    if current.schema_version != RUNTIME_SCHEMA_VERSION {
        return Err("unsupported runtime selector schema".into());
    }
    let Some(generation) = current
        .relative_path
        .strip_prefix(&format!("{RUNTIME_GENERATIONS_DIR}/"))
    else {
        return Err("unsafe selected runtime generation".into());
    };
    if !valid_generation_id(generation)
        || !safe_relative_path(Path::new(&current.relative_path))
        || current.relative_path.split('/').count() != 2
    {
        return Err("unsafe selected runtime generation".into());
    }
    let generation_root = runtime_root.join(&current.relative_path);
    let metadata = fs::symlink_metadata(&generation_root)
        .map_err(|error| format!("read selected runtime generation: {error}"))?;
    if metadata_is_reparse(&metadata) || !metadata.is_dir() {
        return Err("selected runtime generation must be a real directory".into());
    }
    let manifest_path = generation_root.join("runtime-manifest.json");
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|error| format!("read selected runtime manifest: {error}"))?;
    let selector_manifest_hash = current
        .manifest_sha256
        .as_deref()
        .ok_or("runtime selector missing manifest hash")?;
    if !valid_sha256(selector_manifest_hash)
        || hex_sha256(&manifest_bytes) != selector_manifest_hash
    {
        return Err("runtime selector manifest hash mismatch".into());
    }
    let manifest: RuntimeManifest = read_json_preserving_corrupt(&manifest_path)
        .map_err(|error| format!("read selected runtime manifest: {error}"))?;
    if manifest.generation != generation {
        return Err("runtime selector and manifest generation differ".into());
    }
    verify_manifest(&generation_root, &manifest)?;
    Ok((generation_root, manifest))
}

/// Resolves a selected generation only when it is the exact compiled portable
/// reboot payload. This is the handoff authorization boundary.
pub fn load_selected_portable_generation(
    runtime_root: &Path,
) -> Result<(PathBuf, RuntimeManifest), String> {
    let (root, manifest) = load_selected_generation(runtime_root)?;
    let expected = RUNTIME_PAYLOAD_PATHS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let actual = manifest
        .files
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if actual != expected || manifest.payload_contract_hash != portable_payload_contract_hash() {
        return Err(
            "selected runtime does not match the compiled portable payload contract".into(),
        );
    }
    if manifest.executable.path != "frametime.exe"
        || manifest.files.get(&manifest.executable.path) != Some(&manifest.executable.sha256)
    {
        return Err("selected runtime executable record is invalid".into());
    }
    Ok((root, manifest))
}

fn valid_generation_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

struct PublicationLock {
    _file: fs::File,
    #[cfg(not(windows))]
    path: PathBuf,
}

impl PublicationLock {
    fn acquire(path: PathBuf) -> Result<Self, String> {
        let mut options = fs::OpenOptions::new();
        options.create_new(true).read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows::Win32::Storage::FileSystem::{
                FILE_FLAG_DELETE_ON_CLOSE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH,
            };
            options.share_mode(0).custom_flags(
                FILE_FLAG_DELETE_ON_CLOSE.0
                    | FILE_FLAG_OPEN_REPARSE_POINT.0
                    | FILE_FLAG_WRITE_THROUGH.0,
            );
        }
        let file = options
            .open(&path)
            .map_err(|error| format!("runtime publication lock unavailable: {error}"))?;
        Ok(Self {
            _file: file,
            #[cfg(not(windows))]
            path,
        })
    }
}

impl Drop for PublicationLock {
    fn drop(&mut self) {
        #[cfg(not(windows))]
        let _ = fs::remove_file(&self.path);
    }
}

pub fn build_manifest(
    root: &Path,
    generation: String,
    expected: &[PathBuf],
) -> Result<RuntimeManifest, String> {
    let root_metadata = fs::symlink_metadata(root).map_err(|error| error.to_string())?;
    if metadata_is_reparse(&root_metadata) || !root_metadata.is_dir() {
        return Err("runtime generation must be a real directory".into());
    }
    let canonical_root = fs::canonicalize(root).map_err(|error| error.to_string())?;
    let mut files = BTreeMap::new();
    for relative in expected {
        if !safe_relative_path(relative) {
            return Err(format!("unsafe runtime path: {}", relative.display()));
        }
        let candidate = root.join(relative);
        let metadata = fs::symlink_metadata(&candidate).map_err(|error| error.to_string())?;
        if metadata_is_reparse(&metadata) || !metadata.is_file() {
            return Err(format!(
                "runtime payload must be a regular file: {}",
                relative.display()
            ));
        }
        let canonical_candidate =
            fs::canonicalize(&candidate).map_err(|error| error.to_string())?;
        if !canonical_candidate.starts_with(&canonical_root) {
            return Err(format!(
                "runtime payload escaped generation root: {}",
                relative.display()
            ));
        }
        let bytes = fs::read(&canonical_candidate)
            .map_err(|error| format!("{}: {error}", relative.display()))?;
        files.insert(
            relative.to_string_lossy().replace('\\', "/"),
            hex_sha256(&bytes),
        );
    }
    let payload_contract_hash = contract_hash(files.keys().map(String::as_str));
    let executable = files
        .get("frametime.exe")
        .cloned()
        .ok_or("runtime payload is missing frametime.exe")?;
    Ok(RuntimeManifest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        generation,
        files,
        payload_contract_hash,
        executable: RuntimeExecutableRecord {
            path: "frametime.exe".into(),
            sha256: executable,
            unknown: BTreeMap::new(),
        },
        unknown: BTreeMap::new(),
    })
}

pub fn verify_manifest(root: &Path, manifest: &RuntimeManifest) -> Result<(), String> {
    if manifest.schema_version != RUNTIME_SCHEMA_VERSION {
        return Err("unsupported runtime manifest schema".into());
    }
    let actual = collect_files(root)?;
    let expected = manifest.files.keys().cloned().collect::<BTreeSet<_>>();
    if actual != expected {
        return Err("runtime file set differs from manifest".into());
    }
    if manifest.payload_contract_hash != contract_hash(manifest.files.keys().map(String::as_str)) {
        return Err("payload contract hash mismatch".into());
    }
    if !safe_relative_path(Path::new(&manifest.executable.path))
        || manifest.files.get(&manifest.executable.path) != Some(&manifest.executable.sha256)
        || !valid_sha256(&manifest.executable.sha256)
    {
        return Err("runtime executable record mismatch".into());
    }
    for (relative, expected_hash) in &manifest.files {
        let bytes = fs::read(root.join(relative)).map_err(|error| error.to_string())?;
        if hex_sha256(&bytes) != *expected_hash {
            return Err(format!("runtime hash mismatch: {relative}"));
        }
    }
    Ok(())
}

fn collect_files(root: &Path) -> Result<BTreeSet<String>, String> {
    fn visit(base: &Path, directory: &Path, out: &mut BTreeSet<String>) -> Result<(), String> {
        for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if metadata_is_reparse(&metadata) {
                return Err("runtime generation contains a symbolic link".into());
            }
            if metadata.is_dir() {
                visit(base, &path, out)?;
            } else {
                let relative = path
                    .strip_prefix(base)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative != "runtime-manifest.json" {
                    out.insert(relative);
                }
            }
        }
        Ok(())
    }
    let mut files = BTreeSet::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT_VALUE: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT_VALUE != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn contract_hash<'a>(paths: impl Iterator<Item = &'a str>) -> String {
    let joined = paths.collect::<Vec<_>>().join("\n");
    hex_sha256(joined.as_bytes())
}

#[must_use]
pub fn portable_payload_contract_hash() -> String {
    contract_hash(
        RUNTIME_PAYLOAD_PATHS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter(),
    )
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
    const TEST_GENERATION: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn publication_rejects_tampering_and_extra_files() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let generation = temporary
            .path()
            .join(RUNTIME_GENERATIONS_DIR)
            .join(TEST_GENERATION);
        fs::create_dir_all(generation.join("cfgs")).expect("directories");
        fs::write(generation.join("frametime.exe"), b"binary").expect("binary");
        fs::write(generation.join("cfgs/data.json"), b"{}").expect("data");
        let expected = [
            PathBuf::from("frametime.exe"),
            PathBuf::from("cfgs/data.json"),
        ];
        let manifest =
            publish_generation(temporary.path(), TEST_GENERATION, &expected).expect("publish");
        verify_manifest(&generation, &manifest).expect("valid");

        fs::write(generation.join("frametime.exe"), b"tampered").expect("tamper");
        assert!(
            verify_manifest(&generation, &manifest)
                .expect_err("tampered")
                .contains("hash mismatch")
        );
        fs::write(generation.join("frametime.exe"), b"binary").expect("restore fixture");
        fs::write(generation.join("unexpected.dll"), b"extra").expect("extra");
        assert!(
            verify_manifest(&generation, &manifest)
                .expect_err("extra file")
                .contains("file set")
        );
    }

    #[test]
    fn publication_lock_blocks_concurrent_writer() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        fs::write(temporary.path().join("runtime-publication.lock"), b"held").expect("lock");
        let error =
            publish_generation(temporary.path(), TEST_GENERATION, &[]).expect_err("blocked");
        assert!(error.contains("lock unavailable"));
    }

    #[test]
    fn portable_runtime_allowlist_is_safe_and_unique() {
        let paths = runtime_payload_paths();
        assert_eq!(paths.len(), RUNTIME_PAYLOAD_PATHS.len());
        assert!(paths.iter().all(|path| safe_relative_path(path)));
        assert_eq!(
            paths.iter().collect::<BTreeSet<_>>().len(),
            RUNTIME_PAYLOAD_PATHS.len()
        );
    }

    #[test]
    fn selected_generation_is_reloaded_and_verified() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let generation = temporary
            .path()
            .join(RUNTIME_GENERATIONS_DIR)
            .join(TEST_GENERATION);
        fs::create_dir_all(&generation).expect("generation");
        fs::write(generation.join("frametime.exe"), b"binary").expect("binary");
        publish_generation(
            temporary.path(),
            TEST_GENERATION,
            &[PathBuf::from("frametime.exe")],
        )
        .expect("publish");
        let (selected, manifest) =
            load_selected_generation(temporary.path()).expect("selected runtime");
        assert_eq!(selected, generation);
        assert_eq!(manifest.generation, TEST_GENERATION);
    }

    #[test]
    fn selector_manifest_hash_is_required_and_detects_tampering() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let generation = temporary
            .path()
            .join(RUNTIME_GENERATIONS_DIR)
            .join(TEST_GENERATION);
        fs::create_dir_all(&generation).expect("generation");
        fs::write(generation.join("frametime.exe"), b"binary").expect("binary");
        publish_generation(
            temporary.path(),
            TEST_GENERATION,
            &[PathBuf::from("frametime.exe")],
        )
        .expect("publish");
        let selector_path = temporary.path().join("runtime-current.json");
        let mut current: RuntimeCurrent = read_json_preserving_corrupt(&selector_path).unwrap();
        current.manifest_sha256 = None;
        write_json_atomic(&selector_path, &current).unwrap();
        assert!(
            load_selected_generation(temporary.path())
                .expect_err("legacy selector")
                .contains("missing manifest hash")
        );

        current.manifest_sha256 = Some("0".repeat(64));
        write_json_atomic(&selector_path, &current).unwrap();
        assert!(
            load_selected_generation(temporary.path())
                .expect_err("tampered selector")
                .contains("manifest hash mismatch")
        );
    }

    #[test]
    fn portable_loader_requires_the_compiled_file_set_and_contract() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let generation = temporary
            .path()
            .join(RUNTIME_GENERATIONS_DIR)
            .join(TEST_GENERATION);
        for relative in runtime_payload_paths() {
            let path = generation.join(&relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, relative.to_string_lossy().as_bytes()).unwrap();
        }
        publish_portable_generation(temporary.path(), TEST_GENERATION).expect("publish portable");
        let (_, manifest) = load_selected_portable_generation(temporary.path()).expect("portable");
        assert_eq!(
            manifest.payload_contract_hash,
            portable_payload_contract_hash()
        );

        fs::write(generation.join("extra.txt"), b"extra").unwrap();
        assert!(
            load_selected_portable_generation(temporary.path())
                .expect_err("extra payload")
                .contains("file set")
        );
    }

    #[test]
    fn selected_generation_rejects_path_traversal_before_manifest_read() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        write_json_atomic(
            &temporary.path().join("runtime-current.json"),
            &RuntimeCurrent {
                schema_version: RUNTIME_SCHEMA_VERSION,
                relative_path: "../outside".into(),
                published_utc: None,
                manifest_sha256: None,
                unknown: BTreeMap::new(),
            },
        )
        .expect("selector");
        assert!(
            load_selected_generation(temporary.path())
                .expect_err("traversal")
                .contains("unsafe selected")
        );
    }

    #[cfg(unix)]
    #[test]
    fn manifest_rejects_symlink_payloads() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().expect("temporary directory");
        let generation = temporary.path().join(TEST_GENERATION);
        fs::create_dir_all(&generation).expect("generation");
        fs::write(temporary.path().join("outside.exe"), b"outside").expect("outside");
        symlink(
            temporary.path().join("outside.exe"),
            generation.join("frametime.exe"),
        )
        .expect("symlink");
        assert!(
            build_manifest(
                &generation,
                TEST_GENERATION.into(),
                &[PathBuf::from("frametime.exe")]
            )
            .expect_err("symlink")
            .contains("regular file")
        );
    }
}
