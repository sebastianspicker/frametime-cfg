//! Domain purity: `frametime-domain` production code stays free of host
//! side effects, and its `Cargo.toml` carries no clock or platform
//! dependency.

use std::path::Path;

use regex::Regex;

const FORBIDDEN_PATTERN: &str = r"std::(fs|env|process)|\bfs::|\bwindows::|std::os::windows|SystemTime::now|Instant::now|OffsetDateTime::now|std::process::id";
const DEPENDENCY_TABLE_NAMES: [&str; 3] =
    ["dependencies", "dev-dependencies", "build-dependencies"];
const FORBIDDEN_DEPENDENCIES: [&str; 2] = ["time", "windows"];

#[test]
fn production_domain_has_no_host_side_effects() {
    let repository_root = repo_checks::repository_root();
    let domain_src = repository_root.join("crates/frametime-domain/src");
    let pattern = Regex::new(FORBIDDEN_PATTERN).expect("compile domain boundary pattern");

    let mut offenders = Vec::new();
    for path in repo_checks::rust_sources(&domain_src) {
        inspect_source(&repository_root, &path, &pattern, &mut offenders);
    }
    offenders.sort_unstable();

    assert!(
        offenders.is_empty(),
        "domain boundary violation: host side effect in production code:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn domain_manifest_has_no_platform_or_clock_dependency() {
    let repository_root = repo_checks::repository_root();
    let manifest_path = repository_root.join("crates/frametime-domain/Cargo.toml");
    let document = repo_checks::read_toml_table(&manifest_path);

    let offenders = forbidden_dependency_keys(&document);
    assert!(
        offenders.is_empty(),
        "domain boundary violation: platform or clock dependency in Cargo.toml: {}",
        offenders.join(", ")
    );
}

fn inspect_source(
    repository_root: &Path,
    path: &Path,
    pattern: &Regex,
    offenders: &mut Vec<String>,
) {
    let source = repo_checks::read_utf8(path);
    let relative = path.strip_prefix(repository_root).unwrap_or(path);

    for (number, line) in repo_checks::production_lines(&source) {
        if pattern.is_match(line) {
            offenders.push(format!("{}:{number}: {line}", relative.display()));
        }
    }
}

fn forbidden_dependency_keys(document: &toml::Table) -> Vec<String> {
    let mut offenders = Vec::new();
    collect_forbidden(document, "", &mut offenders);
    if let Some(target) = document.get("target").and_then(toml::Value::as_table) {
        for (name, definition) in target {
            if let Some(definition) = definition.as_table() {
                collect_forbidden(definition, &format!("target.{name}."), &mut offenders);
            }
        }
    }
    offenders
}

fn collect_forbidden(table: &toml::Table, location_prefix: &str, offenders: &mut Vec<String>) {
    for table_name in DEPENDENCY_TABLE_NAMES {
        let Some(dependencies) = table.get(table_name).and_then(toml::Value::as_table) else {
            continue;
        };
        for dependency in FORBIDDEN_DEPENDENCIES {
            if dependencies.contains_key(dependency) {
                offenders.push(format!("{location_prefix}{table_name}.{dependency}"));
            }
        }
    }
}
