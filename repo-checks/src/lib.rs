//! Repository policy check helpers.
//!
//! This crate holds no runtime code. It provides the small helpers for
//! repository-wide policy checks: locating the repository root and reading a
//! file as UTF-8. It also checks that batch scripts define every label they
//! call and that the shipped CS2 cfg assets use only names the pinned game
//! build accepts.

use std::{
    fs,
    path::{Path, PathBuf},
};

mod batch_labels;
mod cfg_cvars;

pub use batch_labels::undefined_batch_labels;
pub use cfg_cvars::{cfg_command_names, snapshot_names, video_setting_keys};

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
