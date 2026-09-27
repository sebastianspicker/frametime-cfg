use super::*;
use std::{io::Cursor, time::Instant};
use tempfile::tempdir;

fn entry(name: &str, size: u64, compressed: u64) -> EntryMetadata {
    EntryMetadata {
        name: name.into(),
        is_directory: false,
        has_reparse_attribute: false,
        size,
        compressed_size: compressed,
    }
}

fn minimal_selection() -> NvidiaComponentSelection {
    NvidiaComponentSelection {
        preset: frametime_domain::driver::NvidiaComponentPreset::Minimal,
        selected: ["Display.Driver".into()].into_iter().collect(),
        protected: ["Display.Driver".into()].into_iter().collect(),
        source_components: vec![SourceComponentClassification::Known(
            "Display.Driver".into(),
        )],
        required_unclassified: [frametime_domain::driver::RequiredUnclassifiedComponent {
            directory: "Mystery.Component".into(),
        }]
        .into_iter()
        .collect(),
    }
}

fn assert_lexically_sorted(files: &[NvidiaPackageFile]) {
    assert!(files.windows(2).all(|pair| pair[0].path < pair[1].path));
}

#[test]
fn archive_policy_rejects_traversal_case_collision_and_reparse_points() {
    assert!(validate_entries(&[entry("../escape", 1, 1)]).is_err());
    assert!(
        validate_entries(&[
            entry("Display.Driver/a", 1, 1),
            entry("display.driver/A", 1, 1),
        ])
        .is_err()
    );
    let mut reparse = entry("Display.Driver/link", 1, 1);
    reparse.has_reparse_attribute = true;
    assert!(validate_entries(&[reparse]).is_err());
}

#[test]
fn archive_policy_rejects_resource_limits() {
    assert!(validate_entries(&[entry("huge", MAX_FILE_BYTES + 1, 1)]).is_err());
    assert!(validate_entries(&[entry("ratio", MAX_DECOMPRESSION_RATIO + 1, 1)]).is_err());
    assert!(validate_entries(&[]).is_err());
}

#[test]
fn malformed_sfx_is_rejected_without_creating_output() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("driver.exe");
    fs::write(&input, b"MZ-not-a-seven-z").unwrap();
    let output = temp.path().join("out");
    assert!(extract_nvidia_sfx(&input, &output).is_err());
    assert!(!output.exists());
}

#[test]
fn streaming_hash_covers_multiple_buffers() {
    let bytes = vec![0xa5; HASH_BUFFER_BYTES * 2 + 137];
    let expected = format!("{:x}", Sha256::digest(&bytes));
    let actual = hash_reader(
        &mut Cursor::new(&bytes),
        u64::try_from(bytes.len()).unwrap(),
        "multi-buffer.bin",
    )
    .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn streaming_hash_rejects_lengths_that_differ_from_inspected_metadata() {
    assert!(hash_reader(&mut Cursor::new(b"abc"), 2, "grew.bin").is_err());
    assert!(hash_reader(&mut Cursor::new(b"abc"), 4, "shrunk.bin").is_err());
}

#[test]
fn persisted_and_returned_manifests_have_the_required_sorted_forms() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::create_dir(source.join("Display.Driver")).unwrap();
    fs::create_dir(source.join("Mystery.Component")).unwrap();
    fs::write(source.join("setup.exe"), b"vendor-setup").unwrap();
    fs::write(source.join("Display.Driver/z.bin"), b"driver").unwrap();
    fs::write(source.join("Mystery.Component/x.bin"), b"unknown").unwrap();
    let selection = minimal_selection();
    let output = temp.path().join("output");
    let manifest = build_nvidia_package(&source, &output, &selection).unwrap();
    assert_eq!(fs::read(output.join("setup.exe")).unwrap(), b"vendor-setup");
    assert_eq!(
        fs::read(output.join("Mystery.Component/x.bin")).unwrap(),
        b"unknown"
    );
    assert_lexically_sorted(&manifest.files);

    let persisted_bytes = fs::read(output.join("frametime-package.json")).unwrap();
    let persisted: PreparedNvidiaPackageManifest =
        serde_json::from_slice(&persisted_bytes).unwrap();
    assert_lexically_sorted(&persisted.files);
    assert!(
        persisted
            .files
            .iter()
            .all(|file| file.path != "frametime-package.json")
    );
    let returned_manifest_file = manifest
        .files
        .iter()
        .find(|file| file.path == "frametime-package.json")
        .unwrap();
    assert_eq!(returned_manifest_file.bytes, persisted_bytes.len() as u64);
    assert_eq!(
        returned_manifest_file.sha256,
        format!("{:x}", Sha256::digest(&persisted_bytes))
    );
    assert_eq!(verify_prepared_nvidia_package(&output).unwrap(), persisted);
    let digest = prepared_nvidia_package_digest(&persisted).unwrap();
    let mut different_selection = persisted.clone();
    different_selection.selected_components.push("PhysX".into());
    assert_ne!(
        prepared_nvidia_package_digest(&different_selection).unwrap(),
        digest
    );
}

#[test]
fn launch_validation_rejects_payload_tampering() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::create_dir(source.join("Display.Driver")).unwrap();
    fs::write(source.join("setup.exe"), b"vendor-setup").unwrap();
    fs::write(source.join("Display.Driver/z.bin"), b"driver").unwrap();
    let output = temp.path().join("output");
    build_nvidia_package(&source, &output, &minimal_selection()).unwrap();
    fs::write(output.join("Display.Driver/z.bin"), b"altered").unwrap();
    assert!(verify_prepared_nvidia_package(&output).is_err());
}

#[test]
fn assembly_failure_cleans_the_partial_output() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("setup.exe"), b"vendor-setup").unwrap();
    fs::write(source.join("setup.cfg"), b"vendor-collision").unwrap();
    let output = temp.path().join("output");
    assert!(build_nvidia_package(&source, &output, &minimal_selection()).is_err());
    assert!(!output.exists());
}

#[test]
#[ignore = "manual fixed-size package inventory performance fixture"]
fn package_inventory_performance_fixture() {
    const FIXTURE_BYTES: u64 = 256 * 1024 * 1024;
    const WRITE_BUFFER_BYTES: usize = 64 * 1024;

    let temp = tempdir().unwrap();
    let fixture = temp.path().join("fixture.bin");
    let mut file = File::create(&fixture).unwrap();
    let block = [0x5a; WRITE_BUFFER_BYTES];
    for _ in 0..FIXTURE_BYTES / WRITE_BUFFER_BYTES as u64 {
        file.write_all(&block).unwrap();
    }
    file.sync_all().unwrap();
    drop(file);

    let mode =
        std::env::var("FRAMETIME_PACKAGE_HASH_FIXTURE_MODE").unwrap_or_else(|_| "streaming".into());
    let started = Instant::now();
    let inventory = match mode.as_str() {
        "streaming" => collect_manifest_files(temp.path(), &[]).unwrap(),
        "read-all" => {
            let metadata = fs::symlink_metadata(&fixture).unwrap();
            let bytes = fs::read(&fixture).unwrap();
            vec![NvidiaPackageFile {
                path: "fixture.bin".into(),
                bytes: metadata.len(),
                sha256: format!("{:x}", Sha256::digest(&bytes)),
            }]
        }
        value => panic!("unsupported fixture mode: {value}"),
    };
    let elapsed = started.elapsed();
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].bytes, FIXTURE_BYTES);
    writeln!(
        std::io::stderr().lock(),
        "package_inventory_fixture mode={mode} bytes={FIXTURE_BYTES} elapsed_ms={}",
        elapsed.as_millis()
    )
    .unwrap();
}
