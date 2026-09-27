use std::path::{Path, PathBuf};

use clone_detection::{
    MIN_CLONE_LINES, MIN_CLONE_TOKENS, find_exact_clones, format_clone_report,
    load_production_functions,
};

#[path = "source_size/clone_detection.rs"]
mod clone_detection;

const MAX_PHYSICAL_LINES: usize = 600;
const SCANNED_DIRECTORIES: &[&str] = &["crates", "repo-checks"];

#[test]
fn active_rust_sources_stay_within_the_physical_line_limit() {
    let repository_root = repo_checks::repository_root();
    let mut offenders = Vec::new();
    for directory in SCANNED_DIRECTORIES {
        for path in repo_checks::rust_sources(&repository_root.join(directory)) {
            inspect_rust_source(&repository_root, &path, &mut offenders);
        }
    }
    offenders.sort_unstable();

    assert!(
        offenders.is_empty(),
        "Rust source files exceed the {MAX_PHYSICAL_LINES}-physical-line limit:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn active_production_rust_has_no_exact_function_body_clones() {
    let repository_root = repo_checks::repository_root();
    let scan_roots = SCANNED_DIRECTORIES
        .iter()
        .map(|directory| repository_root.join(directory))
        .collect::<Vec<_>>();
    let directories = scan_roots.iter().map(PathBuf::as_path).collect::<Vec<_>>();
    let functions = load_production_functions(&repository_root, &directories);
    let clones = find_exact_clones(&functions, MIN_CLONE_TOKENS, MIN_CLONE_LINES);

    assert!(
        clones.is_empty(),
        "production function bodies contain exact clones of at least \
         {MIN_CLONE_LINES} physical lines and {MIN_CLONE_TOKENS} tokens:\n{}",
        format_clone_report(&clones)
    );
}

fn inspect_rust_source(repository_root: &Path, path: &Path, offenders: &mut Vec<String>) {
    let source = repo_checks::read_utf8(path);
    let newline_count = source.bytes().filter(|byte| *byte == b'\n').count();
    let physical_lines = newline_count + usize::from(!source.is_empty() && !source.ends_with('\n'));

    if physical_lines > MAX_PHYSICAL_LINES {
        let relative = path.strip_prefix(repository_root).unwrap_or(path);
        offenders.push(format!("{}: {physical_lines}", relative.display()));
    }
}
