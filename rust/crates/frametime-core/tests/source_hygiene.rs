use std::{
    fs,
    path::{Path, PathBuf},
};

const MAX_PRODUCTION_LINES: usize = 600;
const PROHIBITED_RUNTIME_MARKERS: [&str; 5] = [
    "powershell.exe",
    "pwsh.exe",
    "system.management.automation",
    "microsoft.powershell",
    "invoke-expression",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn collect_rust_files(directory: &Path, output: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "tests") {
                continue;
            }
            collect_rust_files(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && path.file_name().is_none_or(|name| name != "tests.rs")
        {
            output.push(path);
        }
    }
}

fn shipping_source_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let crates = workspace_root().join("crates");
    for entry in fs::read_dir(crates).expect("workspace crates") {
        let path = entry.expect("crate entry").path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("frametime-"))
        {
            let source = path.join("src");
            if source.is_dir() {
                collect_rust_files(&source, &mut files);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn production_rust_files_stay_bounded() {
    let oversized = shipping_source_files()
        .into_iter()
        .filter_map(|path| {
            let lines = fs::read_to_string(&path)
                .expect("UTF-8 Rust source")
                .lines()
                .count();
            (lines > MAX_PRODUCTION_LINES).then_some((path, lines))
        })
        .collect::<Vec<_>>();
    assert!(
        oversized.is_empty(),
        "production Rust files exceed {MAX_PRODUCTION_LINES} lines: {oversized:?}"
    );
}

#[test]
fn shipping_rust_has_no_powershell_runtime_dependency() {
    let findings = shipping_source_files()
        .into_iter()
        .filter_map(|path| {
            let source = fs::read_to_string(&path).expect("UTF-8 Rust source");
            let normalized = source.to_ascii_lowercase();
            let markers = PROHIBITED_RUNTIME_MARKERS
                .iter()
                .filter(|marker| normalized.contains(**marker))
                .copied()
                .collect::<Vec<_>>();
            (!markers.is_empty()).then_some((path, markers))
        })
        .collect::<Vec<_>>();
    assert!(
        findings.is_empty(),
        "PowerShell runtime markers in shipping Rust: {findings:?}"
    );
}
