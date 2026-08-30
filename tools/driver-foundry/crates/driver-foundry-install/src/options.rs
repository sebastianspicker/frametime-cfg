//! GUI-to-engine option mapping.

use std::path::PathBuf;

use crate::InstallOptions;

/// Map GUI wizard state → InstallOptions (shared engine).
pub fn options_from_wizard(
    work: PathBuf,
    preset: &str,
    package_root: Option<PathBuf>,
    dry_run: bool,
    export: Option<PathBuf>,
    archive: Option<PathBuf>,
) -> InstallOptions {
    InstallOptions {
        work_directory: work,
        preset: preset.into(),
        package_root,
        dry_run_install: dry_run,
        enable_install: true,
        export_path: export,
        archive_out: archive,
        ..InstallOptions::default()
    }
}
