//! Presentation boundaries: `frametime-app` and `frametime-windows` emit no
//! ad hoc console output, `frametime-cli` and `frametime-gui` do not reach
//! `frametime_windows` outside their `main.rs`, and `frametime-gui` does not
//! hardcode benchmark phase totals.

use std::path::Path;

use regex::Regex;

#[test]
fn app_and_windows_crates_have_no_console_output() {
    let repository_root = repo_checks::repository_root();
    let pattern =
        Regex::new(r"\b(e?print|e?println|dbg)!").expect("compile console output pattern");

    let mut offenders = Vec::new();
    for crate_name in ["frametime-app", "frametime-windows"] {
        let source_dir = repository_root.join("crates").join(crate_name).join("src");
        for path in repo_checks::rust_sources(&source_dir) {
            find_matches(&repository_root, &path, &pattern, &mut offenders);
        }
    }

    assert!(
        offenders.is_empty(),
        "architecture boundary: console output:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn frontends_do_not_bypass_the_app_boundary() {
    let repository_root = repo_checks::repository_root();
    let pattern = Regex::new(r"frametime_windows::|use[[:space:]]+frametime_windows")
        .expect("compile app boundary bypass pattern");

    let mut offenders = Vec::new();
    for crate_name in ["frametime-cli", "frametime-gui"] {
        let source_dir = repository_root.join("crates").join(crate_name).join("src");
        for path in repo_checks::rust_sources(&source_dir) {
            if path.file_name().is_some_and(|name| name == "main.rs") {
                continue;
            }
            let mut matches = Vec::new();
            find_matches(&repository_root, &path, &pattern, &mut matches);
            offenders.extend(
                matches
                    .into_iter()
                    .map(|entry| format!("{crate_name} bypasses the app boundary: {entry}")),
            );
        }
    }

    assert!(
        offenders.is_empty(),
        "architecture boundary:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn gui_does_not_hardcode_phase_totals() {
    let repository_root = repo_checks::repository_root();
    let pattern =
        Regex::new(r"[/][[:space:]]*(38|3|13)([^0-9]|$)").expect("compile phase total pattern");
    let source_dir = repository_root.join("crates/frametime-gui/src");

    let mut offenders = Vec::new();
    for path in repo_checks::rust_sources(&source_dir) {
        let source = repo_checks::read_utf8(&path);
        let relative = path.strip_prefix(&repository_root).unwrap_or(&path);

        for (number, line) in repo_checks::production_lines(&source) {
            if pattern.is_match(line) {
                offenders.push(format!("{}:{number}: {line}", relative.display()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "architecture boundary: GUI hardcodes phase totals:\n{}",
        offenders.join("\n")
    );
}

fn find_matches(repository_root: &Path, path: &Path, pattern: &Regex, offenders: &mut Vec<String>) {
    let source = repo_checks::read_utf8(path);
    let relative = path.strip_prefix(repository_root).unwrap_or(path);

    for (index, line) in source.lines().enumerate() {
        if pattern.is_match(line) {
            offenders.push(format!("{}:{}: {line}", relative.display(), index + 1));
        }
    }
}
