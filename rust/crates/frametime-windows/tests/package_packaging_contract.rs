use std::collections::BTreeSet;

const PACKAGE_LAYOUT: &str = include_str!("../../../package-layout.txt");
const PACKAGER: &str = include_str!("../../../scripts/package.cmd");
const PACKAGE_VERIFIER: &str = include_str!("../../../scripts/verify.cmd");
const RUST_CI: &str = include_str!("../../../../.github/workflows/rust.yml");

fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing section start {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing section end {end}"))
        .0
}

fn copy_destinations() -> Vec<String> {
    PACKAGER
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            line.starts_with("call :copy_one ").then(|| {
                line.split('"')
                    .nth(3)
                    .expect("copy_one destination must be quoted")
                    .replace('\\', "/")
            })
        })
        .collect()
}

fn package_payloads() -> Vec<&'static str> {
    PACKAGE_LAYOUT
        .lines()
        .filter(|line| !line.is_empty())
        .collect()
}

#[test]
fn authenticated_inventory_is_exact_and_script_free() {
    let payloads = package_payloads();
    assert_eq!(
        payloads.len(),
        27,
        "the package payload is a fixed contract"
    );
    assert_eq!(
        payloads.iter().copied().collect::<BTreeSet<_>>().len(),
        payloads.len(),
        "the package layout cannot contain duplicate paths"
    );
    assert_eq!(
        copy_destinations(),
        payloads
            .iter()
            .map(|payload| (*payload).to_owned())
            .collect::<Vec<_>>(),
        "the explicit copy list must exactly equal the authenticated layout"
    );
    assert!(!payloads.contains(&"package.manifest.json"));
    assert!(!payloads.contains(&"package.cat"));
    assert!(!payloads.iter().any(|path| path.ends_with(".bat")));
    assert!(!PACKAGER.contains("call :copy_one \"START.bat\""));
    assert!(!PACKAGER.contains("call :copy_one \"START-GUI.bat\""));
}

#[test]
fn package_lanes_keep_authentication_distinct_from_transport() {
    for required in [
        "/unsigned",
        "/release",
        "/verify /unsigned",
        "/verify /release",
        "package.manifest.json",
        "package.cat",
        "FRAMETIME_PUBLISHER_SPKI_SHA256",
        "FRAMETIME_SIGNTOOL_PATH",
        "FRAMETIME_MAKECAT_PATH",
        "FRAMETIME_SIGNING_CERT_SHA1",
        "FRAMETIME_SIGNING_TIMESTAMP_URL",
        "HashAlgorithms=SHA256",
        "CatalogVersion=2",
        "^<HASH^>",
        "verify /pa /all /v /c",
        "package-auth-smoke",
        "%dist%\\package.manifest.verify.tmp",
        ".transport.json",
    ] {
        assert!(
            PACKAGER.contains(required),
            "missing packager contract: {required}"
        );
    }
    assert!(!PACKAGER.contains("where signtool"));
    assert!(!PACKAGER.contains("where makecat"));
    assert!(PACKAGE_VERIFIER.contains("/verify /unsigned"));
    assert!(PACKAGE_VERIFIER.contains("/verify /release"));
    assert!(!PACKAGE_VERIFIER.contains("package.cmd\" /verify\n"));
    assert!(RUST_CI.contains("scripts\\package.cmd /unsigned"));
    assert!(RUST_CI.contains("scripts\\package.cmd /verify /unsigned"));
    assert!(!RUST_CI.contains("scripts\\package.cmd /release"));
    let package_step = section(
        RUST_CI,
        "      - name: Build and structurally verify the unsigned development package\n",
        "\n  northclock:\n",
    );
    assert!(package_step.contains("cargo build --release -p frametime-cli -p frametime-gui"));
    assert!(!package_step.contains("--all-features"));
}

#[test]
fn release_assembly_orders_signing_manifest_catalog_and_authentication() {
    let assembly = section(PACKAGER, "\n:package_start\n", "\n:verify_existing\n");
    let sign_cli = assembly
        .find("call :sign_file \"%staging%\\frametime.exe\"")
        .expect("CLI must be signed");
    let write_manifest = assembly
        .find("call :write_manifest \"%target%\"")
        .expect("manifest must be written");
    let make_catalog = assembly
        .find("call :make_and_sign_catalog \"%target%\"")
        .expect("catalog must be created and signed");
    let authenticate = assembly
        .find("call :verify_release_authentication \"%target%\"")
        .expect("assembled package must be authenticated");
    assert!(sign_cli < write_manifest);
    assert!(write_manifest < make_catalog);
    assert!(make_catalog < authenticate);
}

#[test]
fn sha256_catalog_and_member_authentication_cover_the_exact_layout() {
    let catalog_definition = section(
        PACKAGER,
        "\n:write_catalog_definition\n",
        "\n:catalog_entry\n",
    );
    let version = catalog_definition
        .find("echo CatalogVersion=2")
        .expect("SHA-256 catalogs require version 2");
    let algorithm = catalog_definition
        .find("echo HashAlgorithms=SHA256")
        .expect("catalog must use SHA-256");
    assert!(version < algorithm);
    assert!(catalog_definition.contains(
        "for /f \"usebackq delims=\" %%F in (\"%source%\\package-layout.txt\") do call :catalog_entry \"%%F\""
    ));

    let authentication = section(
        PACKAGER,
        "\n:verify_release_authentication\n",
        "\n:verify_direct_signature\n",
    );
    let direct = authentication
        .find("call :verify_direct_signature \"%~1\\frametime.exe\"")
        .expect("CLI direct signature verification");
    let member = authentication
        .find("call :verify_catalog_member \"%~1\\package.cat\" \"%~1\\package.manifest.json\"")
        .expect("manifest catalog membership verification");
    let member_loop = authentication
        .find("for /f \"usebackq delims=\" %%F in (\"%source%\\package-layout.txt\") do call :verify_catalog_member")
        .expect("every layout member must be verified");
    let smoke = authentication
        .find("\"%~1\\frametime.exe\" package-auth-smoke")
        .expect("real package authentication smoke");
    assert!(direct < member);
    assert!(member < member_loop);
    assert!(member_loop < smoke);
}

#[test]
fn independent_release_verification_needs_no_private_signing_inputs() {
    let verification_inputs = section(
        PACKAGER,
        "\n:require_release_verification_inputs\n",
        "\n:missing_verification_input\n",
    );
    assert!(verification_inputs.contains("FRAMETIME_SIGNTOOL_PATH"));
    for private_packaging_input in [
        "FRAMETIME_MAKECAT_PATH",
        "FRAMETIME_SIGNING_CERT_SHA1",
        "FRAMETIME_SIGNING_TIMESTAMP_URL",
    ] {
        assert!(!verification_inputs.contains(private_packaging_input));
    }
}
