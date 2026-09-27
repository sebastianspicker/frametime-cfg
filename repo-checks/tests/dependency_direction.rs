//! Dependency direction: `domain <- windows <- app <- cli/gui`. Every
//! required forward path dependency is present, and no crate depends
//! against that direction.

use std::collections::BTreeMap;

const DEPENDENCY_TABLE_NAMES: [&str; 3] =
    ["dependencies", "dev-dependencies", "build-dependencies"];

const CRATES: [&str; 5] = [
    "frametime-domain",
    "frametime-windows",
    "frametime-app",
    "frametime-cli",
    "frametime-gui",
];

const REQUIRED_FORWARD_EDGES: &[(&str, &str)] = &[
    ("frametime-windows", "frametime-domain"),
    ("frametime-app", "frametime-domain"),
    ("frametime-app", "frametime-windows"),
    ("frametime-cli", "frametime-app"),
    ("frametime-gui", "frametime-app"),
];

const FORBIDDEN_REVERSE_EDGES: &[(&str, &[&str])] = &[
    (
        "frametime-domain",
        &[
            "frametime-windows",
            "frametime-app",
            "frametime-cli",
            "frametime-gui",
        ],
    ),
    (
        "frametime-windows",
        &["frametime-app", "frametime-cli", "frametime-gui"],
    ),
    ("frametime-app", &["frametime-cli", "frametime-gui"]),
];

#[test]
fn required_forward_dependencies_are_present() {
    let manifests = read_manifests();

    let missing = REQUIRED_FORWARD_EDGES
        .iter()
        .filter(|(crate_name, dependency)| !has_path_dependency(&manifests[crate_name], dependency))
        .map(|(crate_name, dependency)| format!("{crate_name} must depend on {dependency}"))
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "architecture boundary: {}",
        missing.join("; ")
    );
}

#[test]
fn no_crate_depends_against_the_dependency_direction() {
    let manifests = read_manifests();

    let mut offenders = Vec::new();
    for (crate_name, forbidden_dependencies) in FORBIDDEN_REVERSE_EDGES {
        for dependency in *forbidden_dependencies {
            if has_path_dependency(&manifests[crate_name], dependency) {
                offenders.push(format!("{crate_name} must not depend on {dependency}"));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "architecture boundary: reverse dependency edge: {}",
        offenders.join("; ")
    );
}

fn read_manifests() -> BTreeMap<&'static str, toml::Table> {
    let repository_root = repo_checks::repository_root();
    CRATES
        .iter()
        .map(|&crate_name| {
            let manifest_path = repository_root
                .join("crates")
                .join(crate_name)
                .join("Cargo.toml");
            (crate_name, repo_checks::read_toml_table(&manifest_path))
        })
        .collect()
}

fn has_path_dependency(manifest: &toml::Table, dependency: &str) -> bool {
    DEPENDENCY_TABLE_NAMES
        .iter()
        .any(|table_name| table_has_path_dependency(manifest, table_name, dependency))
        || target_tables(manifest).any(|target| {
            DEPENDENCY_TABLE_NAMES
                .iter()
                .any(|table_name| table_has_path_dependency(target, table_name, dependency))
        })
}

fn table_has_path_dependency(table: &toml::Table, table_name: &str, dependency: &str) -> bool {
    table
        .get(table_name)
        .and_then(toml::Value::as_table)
        .and_then(|dependencies| dependencies.get(dependency))
        .and_then(toml::Value::as_table)
        .is_some_and(|entry| entry.contains_key("path"))
}

fn target_tables(manifest: &toml::Table) -> impl Iterator<Item = &toml::Table> {
    manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(toml::Table::values)
        .filter_map(toml::Value::as_table)
}
