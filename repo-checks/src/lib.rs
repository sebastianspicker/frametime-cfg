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
//! `Cargo.toml` as a TOML table, recursively collecting `.rs` sources, and
//! separating production lines from `cfg(test)` items.

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

/// The lines of `source` outside items gated by `#[cfg(test)]` or
/// `#[cfg(any())]`, each paired with its 1-based line number.
///
/// A gated item ends at its `;` (for example `mod tests;` or a `use`) or at the
/// brace that balances its first `{`, so production code after a test module
/// is still inspected. Braces are counted without lexing, which is sound for
/// the balanced string literals used in this repository's tests.
pub fn production_lines(source: &str) -> Vec<(usize, &str)> {
    let mut production = Vec::new();
    let mut gated: Option<GatedItem> = None;
    for (index, line) in source.lines().enumerate() {
        if let Some(item) = gated.as_mut() {
            if item.consume(line) {
                gated = None;
            }
            continue;
        }
        let trimmed = line.trim_start();
        let Some(rest) = ["#[cfg(test)]", "#[cfg(any())]"]
            .iter()
            .find_map(|marker| trimmed.strip_prefix(marker))
        else {
            production.push((index + 1, line));
            continue;
        };
        let mut item = GatedItem::default();
        if !item.consume(rest) {
            gated = Some(item);
        }
    }
    production
}

#[derive(Default)]
struct GatedItem {
    depth: usize,
    opened: bool,
}

impl GatedItem {
    /// Consume one line of the gated item; returns true when the item ends.
    fn consume(&mut self, text: &str) -> bool {
        for character in text.chars() {
            match character {
                '{' => {
                    self.depth += 1;
                    self.opened = true;
                }
                '}' => {
                    self.depth = self.depth.saturating_sub(1);
                    if self.opened && self.depth == 0 {
                        return true;
                    }
                }
                ';' if !self.opened => return true,
                _ => {}
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::production_lines;

    fn numbers(source: &str) -> Vec<usize> {
        production_lines(source)
            .into_iter()
            .map(|(number, _)| number)
            .collect()
    }

    #[test]
    fn code_after_a_test_module_is_still_production() {
        let source =
            "fn a() {}\n#[cfg(test)]\nmod tests {\n    fn t() { if x { } }\n}\nfn b() {}\n";
        assert_eq!(numbers(source), vec![1, 6]);
    }

    #[test]
    fn a_gated_single_line_item_hides_only_itself() {
        let source = "#[cfg(test)] use std::fs;\nuse std::env;\n#[cfg(any())]\n#[path = \"x.rs\"]\nmod x;\nfn c() {}\n";
        assert_eq!(numbers(source), vec![2, 6]);
    }

    #[test]
    fn nested_gated_items_are_skipped_with_their_bodies() {
        let source =
            "impl A {\n    #[cfg(test)]\n    fn t() {\n        {}\n    }\n    fn p() {}\n}\n";
        assert_eq!(numbers(source), vec![1, 6, 7]);
    }
}
