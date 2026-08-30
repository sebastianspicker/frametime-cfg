use std::{fs, path::Path};

const FORBIDDEN_SOURCE: &[&str] = &[
    "std::fs",
    "std::env",
    "std::process",
    "fs::",
    "windows::",
    "std::os::windows",
    "SystemTime::now",
    "Instant::now",
    "OffsetDateTime::now",
    "std::process::id",
];

#[test]
fn production_domain_has_no_host_side_effects() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    inspect_directory(&manifest_dir.join("src"));

    let manifest = fs::read_to_string(manifest_dir.join("Cargo.toml"))
        .expect("read frametime-domain Cargo.toml");
    for dependency in ["time", "windows"] {
        assert!(
            !manifest
                .lines()
                .any(|line| line.trim_start().starts_with(&format!("{dependency} ="))),
            "frametime-domain must not depend on {dependency}"
        );
    }
}

fn inspect_directory(directory: &Path) {
    for entry in fs::read_dir(directory).expect("read domain source directory") {
        let path = entry.expect("read domain source entry").path();
        if path.is_dir() {
            inspect_directory(&path);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            inspect_source(&path);
        }
    }
}

fn inspect_source(path: &Path) {
    let source = fs::read_to_string(path).expect("read domain source");
    let production = source.split("\n#[cfg(test)]").next().unwrap_or(&source);

    for forbidden in FORBIDDEN_SOURCE {
        assert!(
            !production.contains(forbidden),
            "{} contains forbidden production-domain host dependency `{forbidden}`",
            path.display()
        );
    }
}
