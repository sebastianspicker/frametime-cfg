//! Installation pipeline orchestration and run-report assembly.

use std::path::{Path, PathBuf};

use driver_foundry_common::catalog_path;

use crate::catalog::{PackageCatalog, SelectionPresets};
use crate::launch::default_setup_args;
use crate::report::RunReport;
use crate::source::AcquiredPackage;
use crate::{
    archive, copy, sign, source, tweaks, uninstall, InstallError, InstallOptions, InstallResult,
};

/// Full pipeline: S0–S6 style stages (native Rust).
pub fn run_install(opts: &InstallOptions) -> Result<InstallResult, InstallError> {
    let mut state = InstallPipeline::prepare(opts)?;
    state.run_stages(opts)?;
    Ok(state.into_result(opts))
}

struct InstallPipeline {
    work: PathBuf,
    catalog: PackageCatalog,
    resolved: Vec<String>,
    messages: Vec<String>,
    log: Vec<String>,
    acquired: Option<AcquiredPackage>,
    prepared: Option<PathBuf>,
    kept: Vec<String>,
    stripped: Vec<String>,
    not_whql: bool,
    export_path: Option<PathBuf>,
    archive_path: Option<PathBuf>,
    report_path: Option<PathBuf>,
    setup_arguments: Vec<String>,
}

impl InstallPipeline {
    fn prepare(opts: &InstallOptions) -> Result<Self, InstallError> {
        let work = opts.work_directory.clone();
        validate_install_path_separation(opts, &work)?;
        validate_pre_workspace_live_options(opts)?;
        let catalog_file = opts
            .catalog_path
            .clone()
            .unwrap_or_else(|| catalog_path(&driver_foundry_common::resolve_data_root()));
        if !catalog_file.is_file() {
            return Err(InstallError::CatalogMissing(catalog_file));
        }
        let catalog = PackageCatalog::load_from_file(&catalog_file)?;
        let mut messages = vec![format!(
            "catalog: {} ({} packages)",
            catalog_file.display(),
            catalog.packages.len()
        )];
        let resolved = resolve_selection(opts, &catalog, &mut messages)?;
        // Validate all catalog-derived names before touching a caller path.
        copy::create_new_run_workspace(&work)?;
        Ok(Self {
            work,
            catalog,
            resolved,
            messages,
            log: Vec::new(),
            acquired: None,
            prepared: None,
            kept: Vec::new(),
            stripped: Vec::new(),
            not_whql: true,
            export_path: None,
            archive_path: None,
            report_path: None,
            setup_arguments: Vec::new(),
        })
    }

    fn run_stages(&mut self, opts: &InstallOptions) -> Result<(), InstallError> {
        log_elevation(opts, &mut self.log);
        self.acquire_and_authorize(opts)?;
        self.run_optional_uninstall(opts)?;
        self.filter_components()?;
        self.apply_tweaks(opts)?;
        self.rebuild_catalog(opts);
        self.plan_install(opts)?;
        self.export_workspace(opts)?;
        self.build_archive(opts)?;
        self.write_report(opts)
    }

    fn acquire_and_authorize(&mut self, opts: &InstallOptions) -> Result<(), InstallError> {
        note(&mut self.log, "S1-Acquire", "Acquiring package");
        let acquired = source::acquire_package(
            opts,
            &self.work,
            &self.catalog,
            &mut self.log,
            &mut self.messages,
        )?;
        if !opts.dry_run_install {
            acquired.trust.authorize_live_install()?;
        }
        note_package_root(&mut self.log, &acquired);
        self.acquired = Some(acquired);
        Ok(())
    }

    fn run_optional_uninstall(&mut self, opts: &InstallOptions) -> Result<(), InstallError> {
        if opts.uninstall_drivers {
            note(
                &mut self.log,
                "S5d-UninstallDrivers",
                "Uninstall drivers stage",
            );
            uninstall::run_uninstall_stage(&self.work, opts.dry_run_install, &mut self.log)?;
        }
        Ok(())
    }

    fn filter_components(&mut self) -> Result<(), InstallError> {
        note(
            &mut self.log,
            "S2-Filter",
            &format!(
                "Resolved {} component(s): {}",
                self.resolved.len(),
                self.resolved.join(", ")
            ),
        );
        note(&mut self.log, "S2-Filter", "Filtering components");
        let prepared = self.work.join("prepared");
        let acquired = self
            .acquired
            .as_ref()
            .expect("acquisition precedes filtering");
        let (kept, stripped) =
            copy::prepare_copy_strip(&acquired.root, &prepared, &self.catalog, &self.resolved)?;
        note_filter_result(&mut self.log, &prepared, &kept, &stripped);
        self.prepared = Some(prepared);
        self.kept = kept;
        self.stripped = stripped;
        Ok(())
    }

    fn apply_tweaks(&mut self, opts: &InstallOptions) -> Result<(), InstallError> {
        note(&mut self.log, "S3-Tweaks", "Applying tweaks");
        let prepared = self.prepared().to_path_buf();
        tweaks::apply_tweaks(&prepared, opts, &mut self.log)
    }

    fn rebuild_catalog(&mut self, opts: &InstallOptions) {
        if !opts.try_sign {
            note(
                &mut self.log,
                "S4-RebuildCatalog",
                "Skipped (RebuildCatalogs=false). Not WHQL.",
            );
            return;
        }
        note(
            &mut self.log,
            "S4-RebuildCatalog",
            "try-sign requested; writing plan only because unauthenticated signtool execution is disabled.",
        );
        let prepared = self.prepared().to_path_buf();
        let sign = sign::try_sign_catalog(&prepared, &mut self.log);
        self.not_whql = !sign.proven_signed;
        note_sign_outcome(&mut self.log, sign.tools_present, sign.proven_signed);
    }

    fn plan_install(&mut self, opts: &InstallOptions) -> Result<(), InstallError> {
        note(&mut self.log, "S5a-Install", "Install");
        self.setup_arguments = default_setup_args(opts.clean_install);
        self.setup_arguments.extend(opts.setup_args.iter().cloned());
        if !opts.enable_install {
            note(
                &mut self.log,
                "S5a-Install",
                "Skipped (install not enabled). Dry-run default.",
            );
            return Ok(());
        }
        let acquired = self
            .acquired
            .as_ref()
            .expect("acquisition precedes installation");
        if opts.dry_run_install || acquired.synthetic {
            let prepared = self.prepared().to_path_buf();
            note_dry_install(&mut self.log, &prepared, &self.setup_arguments);
            return Ok(());
        }
        Err(InstallError::UntrustedInstaller(
            "live setup launch is disabled until a platform signer verifier is shipped".into(),
        ))
    }

    fn export_workspace(&mut self, opts: &InstallOptions) -> Result<(), InstallError> {
        let Some(export_path) = opts.export_path.as_ref() else {
            note(
                &mut self.log,
                "S5b-ExportWorkspace",
                "Skipped (workspace export not enabled).",
            );
            return Ok(());
        };
        note(&mut self.log, "S5b-ExportWorkspace", "Exporting workspace");
        archive::export_workspace(self.prepared(), export_path)?;
        note(
            &mut self.log,
            "S5b-ExportWorkspace",
            &format!("Exported prepared tree to {}", export_path.display()),
        );
        self.export_path = Some(export_path.clone());
        Ok(())
    }

    fn build_archive(&mut self, opts: &InstallOptions) -> Result<(), InstallError> {
        let Some(archive_path) = opts.archive_out.as_ref() else {
            note(
                &mut self.log,
                "S5c-BuildPackage",
                "Skipped (portable archive not enabled).",
            );
            return Ok(());
        };
        note(
            &mut self.log,
            "S5c-BuildPackage",
            "Building portable archive",
        );
        let format = build_archive_for_format(self.prepared(), archive_path, &opts.archive_format)?;
        note(
            &mut self.log,
            "S5c-BuildPackage",
            &format!(
                "Portable archive written: {} format={format}",
                archive_path.display()
            ),
        );
        self.archive_path = Some(archive_path.clone());
        Ok(())
    }

    fn write_report(&mut self, opts: &InstallOptions) -> Result<(), InstallError> {
        if !opts.enable_run_report {
            return Ok(());
        }
        let path = opts
            .run_report_path
            .clone()
            .unwrap_or_else(|| self.work.join("driver-foundry-run-report.json"));
        note(&mut self.log, "S6-RunReport", "Writing run report");
        self.report(opts).write_to(&path)?;
        let acquired = self
            .acquired
            .as_ref()
            .expect("acquisition precedes reports");
        note(
            &mut self.log,
            "S6-RunReport",
            &format!(
                "Run report written: {}; resolved={}, stripped={}, source={}, Not WHQL.",
                path.display(),
                self.kept.len(),
                self.stripped.len(),
                acquired.source_label
            ),
        );
        self.report_path = Some(path);
        Ok(())
    }

    fn report(&self, opts: &InstallOptions) -> RunReport {
        let acquired = self
            .acquired
            .as_ref()
            .expect("acquisition precedes reports");
        RunReport {
            product: driver_foundry_common::PRODUCT_NAME.to_string(),
            version: driver_foundry_common::PRODUCT_VERSION.to_string(),
            dry_run_install: opts.dry_run_install || acquired.synthetic,
            force_install: !opts.dry_run_install && !acquired.synthetic,
            preset: opts.preset.to_ascii_lowercase(),
            package_source: acquired.source_label.clone(),
            package_root: acquired.root.display().to_string(),
            prepared_root: self.prepared().display().to_string(),
            kept_components: self.kept.clone(),
            stripped_components: self.stripped.clone(),
            setup_arguments: self.setup_arguments.clone(),
            not_whql: self.not_whql,
            stages: self.report_stages(opts),
            export_path: self
                .export_path
                .as_ref()
                .map(|path| path.display().to_string()),
            archive_path: self
                .archive_path
                .as_ref()
                .map(|path| path.display().to_string()),
            launch_command: None,
        }
    }

    fn report_stages(&self, opts: &InstallOptions) -> Vec<String> {
        let mut stages = vec![
            "S0-Elevate".into(),
            "S1-Acquire".into(),
            "S2-Filter".into(),
            "S3-Tweaks".into(),
            "S4-RebuildCatalog".into(),
            "S5a-Install".into(),
        ];
        if opts.uninstall_drivers {
            stages.insert(2, "S5d-UninstallDrivers".into());
        }
        if self.export_path.is_some() {
            stages.push("S5b-ExportWorkspace".into());
        }
        if self.archive_path.is_some() {
            stages.push("S5c-BuildPackage".into());
        }
        stages.push("S6-RunReport".into());
        stages
    }

    fn into_result(mut self, opts: &InstallOptions) -> InstallResult {
        let acquired = self
            .acquired
            .take()
            .expect("acquisition completes before result");
        let prepared = self
            .prepared
            .take()
            .expect("filtering completes before result");
        note(&mut self.log, "Completed", "Pipeline completed.");
        self.messages.extend(self.log.iter().cloned());
        append_result_messages(
            &mut self.messages,
            &prepared,
            &self.kept,
            &self.stripped,
            &self.report_path,
        );
        let ok = prepared.is_dir()
            && !self.kept.is_empty()
            && self
                .kept
                .iter()
                .any(|component| component == "Display.Driver");
        InstallResult {
            exit_code: i32::from(!ok),
            dry_run_install: opts.dry_run_install || acquired.synthetic,
            work_directory: self.work,
            prepared_root: Some(prepared),
            package_root: Some(acquired.root),
            run_report_path: self.report_path,
            kept_components: self.kept,
            stripped_components: self.stripped,
            log: self.log,
            messages: self.messages,
            used_synthetic_fixture: acquired.synthetic,
            export_path: self.export_path,
            archive_path: self.archive_path,
            launch_command: None,
        }
    }

    fn prepared(&self) -> &Path {
        self.prepared
            .as_deref()
            .expect("filtering precedes this stage")
    }
}

fn resolve_selection(
    opts: &InstallOptions,
    catalog: &PackageCatalog,
    messages: &mut Vec<String>,
) -> Result<Vec<String>, InstallError> {
    if !SelectionPresets::is_known(&opts.preset) {
        return Err(InstallError::UnknownPreset(opts.preset.clone()));
    }
    let mut selected = SelectionPresets::create_selection(catalog, &opts.preset)?;
    if let Some(path) = opts.import_selection.as_ref() {
        selected.extend(source::import_selection_file(path)?);
        messages.push(format!("import-selection: {}", path.display()));
    }
    selected.extend(opts.select.iter().cloned());
    for id in &opts.deselect {
        selected.remove(id);
    }
    let resolved = catalog.resolve_with_deps(&selected);
    messages.push(format!("preset: {}", opts.preset.to_ascii_lowercase()));
    messages.push(format!(
        "resolved ({}): {}",
        resolved.len(),
        resolved.join(", ")
    ));
    Ok(resolved)
}

fn log_elevation(opts: &InstallOptions, log: &mut Vec<String>) {
    note(log, "S0-Elevate", "Elevation check");
    let message = if opts.dry_run_install {
        "Process elevation not required for dry-run install. Continuing dry pipeline."
    } else if driver_foundry_common::elevation::is_administrator() {
        "Administrator confirmed for force-install."
    } else {
        "Not elevated; force-install may fail without administrator rights. Continuing launch attempt."
    };
    note(log, "S0-Elevate", message);
    note(log, "S0-Elevate", "Elevation check complete");
}

fn note_package_root(log: &mut Vec<String>, acquired: &AcquiredPackage) {
    note(
        log,
        "S1-Acquire",
        &format!(
            "Using package root: {} (setup.exe: {}) source={}",
            acquired.root.display(),
            if acquired.root.join("setup.exe").is_file() {
                "present"
            } else {
                "absent"
            },
            acquired.source_label
        ),
    );
}

fn note_filter_result(
    log: &mut Vec<String>,
    prepared: &Path,
    kept: &[String],
    stripped: &[String],
) {
    note(
        log,
        "S2-Filter",
        &format!(
            "Prepare/strip complete: prepared={}; kept=[{}]; stripped=[{}]; filesCopied",
            prepared.display(),
            kept.join(", "),
            stripped.join(", ")
        ),
    );
}

fn note_sign_outcome(log: &mut Vec<String>, tools_present: bool, proven_signed: bool) {
    let message = if !tools_present {
        "Skipped (authenticated signtool manifest not available). Not WHQL."
    } else if proven_signed {
        "Catalog sign proven successful; clearing Not WHQL."
    } else {
        "sign probe attempted; no proven successful sign. Not WHQL."
    };
    note(log, "S4-RebuildCatalog", message);
}

fn note_dry_install(log: &mut Vec<String>, prepared: &Path, setup_arguments: &[String]) {
    note(
        log,
        "S5a-Install",
        &format!(
            "Dry-run install (setup path: {}; args: {:?}). No process launched.",
            prepared.join("setup.exe").display(),
            setup_arguments
        ),
    );
}

fn build_archive_for_format(
    prepared: &Path,
    archive_path: &Path,
    requested_format: &str,
) -> Result<String, InstallError> {
    let mut format = requested_format.to_ascii_lowercase();
    if format.is_empty() {
        format = archive_path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("zip")
            .to_ascii_lowercase();
    }
    match format.as_str() {
        "7z" => archive::build_7z(prepared, archive_path)?,
        "sfx" | "exe" => archive::build_sfx(prepared, archive_path)?,
        _ => {
            archive::build_zip(prepared, archive_path)?;
            format = "zip".into();
        }
    }
    Ok(format)
}

fn append_result_messages(
    messages: &mut Vec<String>,
    prepared: &Path,
    kept: &[String],
    stripped: &[String],
    report_path: &Option<PathBuf>,
) {
    messages.push(format!("prepared: {}", prepared.display()));
    messages.push(format!("kept ({}): {}", kept.len(), kept.join(", ")));
    messages.push(format!(
        "stripped ({}): {}",
        stripped.len(),
        stripped.join(", ")
    ));
    if let Some(path) = report_path {
        messages.push(format!("run-report: {}", path.display()));
    }
}

fn validate_install_path_separation(
    opts: &InstallOptions,
    work: &Path,
) -> Result<(), InstallError> {
    for (label, input) in [
        ("package root", opts.package_root.as_deref()),
        ("package archive", opts.package_archive.as_deref()),
    ] {
        if let Some(input) = input {
            reject_overlap_with_path(
                work,
                input,
                &format!("work directory must not overlap {label}"),
            )?;
        }
    }
    validate_output_paths(opts, work)
}

fn validate_output_paths(opts: &InstallOptions, work: &Path) -> Result<(), InstallError> {
    let outputs = [
        ("export directory", opts.export_path.as_deref()),
        ("archive output", opts.archive_out.as_deref()),
        ("run report", opts.run_report_path.as_deref()),
    ];
    for (label, output) in outputs {
        let Some(output) = output else { continue };
        if copy::paths_overlap(work, output) && !copy::path_is_within(output, work) {
            return Err(InstallError::Other(format!(
                "work directory must not overlap {label}: {}",
                output.display()
            )));
        }
        validate_output_inputs(opts, label, output)?;
    }
    validate_distinct_outputs(&outputs)
}

fn validate_output_inputs(
    opts: &InstallOptions,
    label: &str,
    output: &Path,
) -> Result<(), InstallError> {
    for input in [
        opts.package_root.as_deref(),
        opts.package_archive.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        reject_overlap_with_path(
            output,
            input,
            &format!("{label} must not overlap package input"),
        )?;
    }
    Ok(())
}

fn validate_distinct_outputs(outputs: &[(&str, Option<&Path>)]) -> Result<(), InstallError> {
    for (index, (left_label, left)) in outputs.iter().enumerate() {
        let Some(left) = left else { continue };
        for (right_label, right) in outputs.iter().skip(index + 1) {
            if let Some(right) = right {
                if copy::paths_overlap(left, right) {
                    return Err(InstallError::Other(format!(
                        "{left_label} must not overlap {right_label}"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn reject_overlap_with_path(left: &Path, right: &Path, message: &str) -> Result<(), InstallError> {
    if copy::paths_overlap(left, right) {
        return Err(InstallError::Other(format!(
            "{message}: {}",
            right.display()
        )));
    }
    Ok(())
}

fn validate_pre_workspace_live_options(opts: &InstallOptions) -> Result<(), InstallError> {
    validate_pre_workspace_options_with_elevation(
        opts,
        driver_foundry_common::elevation::is_administrator(),
    )
}

fn validate_pre_workspace_options_with_elevation(
    opts: &InstallOptions,
    elevated: bool,
) -> Result<(), InstallError> {
    if opts.live_registry_apply {
        return Err(InstallError::UntrustedInstaller(
            "live_registry_apply is disabled until the platform signer verifier and authenticated installer policy are shipped".into(),
        ));
    }
    if !opts.dry_run_install {
        return Err(InstallError::UntrustedInstaller(
            "force-install is disabled until the platform signer verifier and authenticated vendor signer policy are shipped".into(),
        ));
    }
    if elevated {
        return Err(InstallError::UntrustedInstaller(
            "elevated dry-run is disabled until protected no-follow workspace/output roots are available; rerun unelevated".into(),
        ));
    }
    Ok(())
}

pub(crate) fn note(log: &mut Vec<String>, stage: &str, msg: &str) {
    log.push(format!("[{stage}] {msg}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevated_dry_run_is_refused_before_workspace_creation() {
        let error = validate_pre_workspace_options_with_elevation(&InstallOptions::default(), true)
            .unwrap_err();
        assert!(error.to_string().contains("elevated dry-run"));
    }

    #[test]
    fn unelevated_dry_run_remains_allowed() {
        assert!(
            validate_pre_workspace_options_with_elevation(&InstallOptions::default(), false)
                .is_ok()
        );
    }

    #[test]
    fn live_registry_apply_is_refused_even_when_elevated() {
        let error = validate_pre_workspace_options_with_elevation(
            &InstallOptions {
                live_registry_apply: true,
                ..InstallOptions::default()
            },
            true,
        )
        .unwrap_err();
        assert!(error.to_string().contains("live_registry_apply"));
    }
}
