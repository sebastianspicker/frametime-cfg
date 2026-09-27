//! Package surface: the repository root and the package layout contain only
//! the native Rust product surface, and `scripts/package.cmd` still enforces
//! that boundary at package time. This ports the two inline steps of the
//! `native-runtime-boundary` job in `.github/workflows/security.yml`.

const LEGACY_EXTENSIONS: [&str; 5] = ["ps1", "psd1", "bat", "cmd", "xaml"];
const REQUIRED_PACKAGE_CMD_MARKERS: [&str; 3] = [
    r#"if /i "%~x1"==".ps1""#,
    "PowerShell source file is not permitted in the package",
    "PowerShell runtime or source dependency marker in package file",
];

#[test]
fn root_product_surface_is_native_rust_only() {
    let repository_root = repo_checks::repository_root();

    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&repository_root)
        .unwrap_or_else(|error| panic!("read {}: {error}", repository_root.display()))
    {
        let entry = entry.expect("read repository root entry");
        let file_type = entry.file_type().unwrap_or_else(|error| {
            panic!("read file type of {}: {error}", entry.path().display())
        });
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                LEGACY_EXTENSIONS
                    .iter()
                    .any(|legacy| extension.eq_ignore_ascii_case(legacy))
            })
        {
            offenders.push(path.display().to_string());
        }
    }

    assert!(
        offenders.is_empty(),
        "the root product surface must contain only the native Rust runtime:\n{}",
        offenders.join("\n")
    );

    for name in ["helpers", "ui"] {
        assert!(
            !repository_root.join(name).exists(),
            "the root product surface must not contain an alternate helper or markup runtime: {name}"
        );
    }
}

#[test]
fn package_layout_exists_and_has_no_legacy_entries() {
    let repository_root = repo_checks::repository_root();
    let package_cmd = repository_root.join("scripts/package.cmd");
    let package_layout = repository_root.join("package-layout.txt");

    assert!(package_cmd.is_file(), "missing {}", package_cmd.display());
    assert!(
        package_layout.is_file(),
        "missing {}",
        package_layout.display()
    );

    let pattern = regex::Regex::new(r"(?i)\.(ps1|psd1|bat|cmd|xaml)(/|$)")
        .expect("compile legacy layout entry pattern");
    let layout = repo_checks::read_utf8(&package_layout);
    let offenders = layout
        .lines()
        .filter(|line| pattern.is_match(line))
        .collect::<Vec<_>>();

    assert!(
        offenders.is_empty(),
        "the native package layout must not contain retired runtime files:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn package_cmd_still_enforces_the_boundary() {
    let repository_root = repo_checks::repository_root();
    let package_cmd = repository_root.join("scripts/package.cmd");
    let source = repo_checks::read_utf8(&package_cmd);

    let missing = REQUIRED_PACKAGE_CMD_MARKERS
        .iter()
        .copied()
        .filter(|marker| !source.contains(marker))
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "scripts/package.cmd no longer enforces the native package boundary: {}",
        missing.join(", ")
    );
}
