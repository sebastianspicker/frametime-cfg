use frametime_windows::{PackageManifest, PublisherPinConfiguration};

fn manifest(files: &str) -> Vec<u8> {
    format!(r#"{{"schema_version":1,"version":"3.0.0","files":[{files}]}}"#).into_bytes()
}

fn file(path: &str) -> String {
    format!(
        r#"{{"path":"{path}","size":1,"sha256":"{}"}}"#,
        "a".repeat(64)
    )
}

fn full_manifest() -> Vec<u8> {
    manifest(
        &include_str!("../../../package-layout.txt")
            .lines()
            .map(file)
            .collect::<Vec<_>>()
            .join(","),
    )
}

#[test]
fn manifest_requires_the_exact_payload_set() {
    assert_eq!(
        PackageManifest::parse(&full_manifest())
            .expect("valid payload")
            .files()
            .len(),
        27
    );
    assert!(PackageManifest::parse(&manifest(&file("extra.txt"))).is_err());
}

#[test]
fn manifest_rejects_duplicate_fields_and_unsafe_paths() {
    assert!(
        PackageManifest::parse(
            br#"{"schema_version":1,"schema_version":1,"version":"x","files":[]}"#
        )
        .is_err()
    );
    assert!(PackageManifest::parse(&manifest(&file("../frametime.exe"))).is_err());
}

#[test]
fn pins_are_strict_and_never_permissive() {
    assert!(PublisherPinConfiguration::parse("a").is_err());
    assert!(matches!(
        PublisherPinConfiguration::parse(&format!("{};{}", "a".repeat(64), "b".repeat(64))),
        Ok(PublisherPinConfiguration::Configured(_))
    ));
}
