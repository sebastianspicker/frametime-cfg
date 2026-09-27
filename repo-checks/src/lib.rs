//! Repository policy checks.
//!
//! This crate holds no runtime code. Its tests are the single home for
//! repository-wide policy checks. `cargo test --workspace` runs them on
//! every platform, including Windows, where the bash equivalents these
//! replace used to be skipped by `scripts\verify.cmd`. The checks are:
//!
//! - `tests/source_size.rs`: a 600-physical-line cap for every `.rs` file
//!   under `crates/` and `repo-checks/`, plus exact-clone detection of
//!   production function bodies across both trees.
//! - `tests/domain_purity.rs`: `frametime-domain` production code stays free
//!   of host side effects (filesystem, environment, process, Windows APIs,
//!   and wall-clock reads), and its `Cargo.toml` carries no `time` or
//!   `windows` dependency.
//! - `tests/dependency_direction.rs`: the required
//!   `domain <- windows <- app <- cli/gui` path dependencies are present,
//!   and no crate depends against that direction.
//! - `tests/presentation_boundaries.rs`: `frametime-app` and
//!   `frametime-windows` contain no ad hoc console output, `frametime-cli`
//!   and `frametime-gui` do not reach `frametime_windows` outside their
//!   `main.rs`, and `frametime-gui` does not hardcode benchmark phase
//!   totals.
//! - `tests/package_surface.rs`: the repository root and the package layout
//!   contain only the native Rust product surface (no PowerShell, batch, or
//!   XAML remnants), and `scripts/package.cmd` still enforces that boundary
//!   at package time.
//!
//! This module also holds the small helpers shared by those test binaries:
//! locating the repository root, reading a file as UTF-8, parsing a
//! `Cargo.toml` as a TOML table, and recursively collecting `.rs` sources.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// The repository root, derived from this crate's own manifest directory.
///
/// `repo-checks` lives directly below the repository root, so tests can
/// locate `crates/`, `scripts/`, and other repository-relative paths
/// regardless of the working directory `cargo test` uses.
pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo-checks must live directly below the repository root")
        .to_path_buf()
}

/// Read `path` as UTF-8, panicking with the path and the underlying error if
/// it cannot be read or is not valid UTF-8.
pub fn read_utf8(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {} as UTF-8: {error}", path.display()))
}

/// Read and parse `path` as a TOML table, panicking with the path and the
/// underlying error on failure.
pub fn read_toml_table(path: &Path) -> toml::Table {
    read_utf8(path)
        .parse()
        .unwrap_or_else(|error| panic!("parse {} as TOML: {error}", path.display()))
}

/// Recursively collect every `.rs` file beneath `directory`, sorted for
/// deterministic output.
pub fn rust_sources(directory: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    collect_rust_sources(directory, &mut sources);
    sources.sort_unstable();
    sources
}

fn collect_rust_sources(directory: &Path, sources: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
    {
        let path = entry.expect("read source entry").path();
        if path.is_dir() {
            collect_rust_sources(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
}
