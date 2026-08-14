#[test]
fn runtime_publisher_accepts_only_compiled_lower_hex_generations() {
    assert!(valid_generated_id("0123456789abcdef0123456789abcdef"));
    assert!(!valid_generated_id("0123456789abcdef0123456789abcdeF"));
    assert!(!valid_generated_id("0123456789abcdef0123456789abcdef0"));
}

#[test]
fn runtime_publisher_manifest_is_exact_and_binds_frametime_executable() {
    let hashes = RUNTIME_PAYLOAD_PATHS
        .iter()
        .map(|path| ((*path).to_owned(), "a".repeat(64)))
        .collect();
    let manifest = manifest_for("0123456789abcdef0123456789abcdef".into(), hashes)
        .expect("exact compiled payload");
    assert_eq!(manifest.files.len(), 18);
    assert_eq!(manifest.executable.path, "frametime.exe");
    assert_eq!(manifest.executable.sha256, "a".repeat(64));
    assert_eq!(
        manifest.payload_contract_hash,
        portable_payload_contract_hash()
    );
}

#[test]
fn runtime_publisher_rejects_missing_or_extra_payload_entries() {
    let mut hashes = RUNTIME_PAYLOAD_PATHS
        .iter()
        .map(|path| ((*path).to_owned(), "a".repeat(64)))
        .collect::<BTreeMap<_, _>>();
    hashes.remove("frametime.exe");
    assert!(manifest_for("0123456789abcdef0123456789abcdef".into(), hashes).is_err());
    let mut hashes = RUNTIME_PAYLOAD_PATHS
        .iter()
        .map(|path| ((*path).to_owned(), "a".repeat(64)))
        .collect::<BTreeMap<_, _>>();
    hashes.insert("attacker.exe".into(), "a".repeat(64));
    assert!(manifest_for("0123456789abcdef0123456789abcdef".into(), hashes).is_err());
}

#[test]
fn runtime_publisher_has_bounded_streaming_and_fixed_directory_contracts() {
    assert_eq!(
        payload_directories(),
        ["assets", "assets/cfgs", "docs", "licenses"]
    );
    assert_eq!(SAFE_MODE_HANDOFF_ARGUMENTS, "boot-safe-mode --yes");
    let _publisher: fn(
        &AuthenticatedPackage,
    ) -> Result<VerifiedPublishedRuntime, String> = publish_current_packaged_runtime;
}
