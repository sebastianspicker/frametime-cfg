//! Installation pipeline orchestration and run-report assembly.

use driver_foundry_common::catalog_path;

use crate::catalog::{PackageCatalog, SelectionPresets};
use crate::launch::default_setup_args;
use crate::report::RunReport;
use crate::{
    archive, copy, sign, source, tweaks, uninstall, InstallError, InstallOptions, InstallResult,
};

/// Full pipeline: S0–S6 style stages (native Rust).
pub fn run_install(opts: &InstallOptions) -> Result<InstallResult, InstallError> {
    let mut messages = Vec::new();
    let mut log = Vec::new();
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
    messages.push(format!(
        "catalog: {} ({} packages)",
        catalog_file.display(),
        catalog.packages.len()
    ));

    if !SelectionPresets::is_known(&opts.preset) {
        return Err(InstallError::UnknownPreset(opts.preset.clone()));
    }

    let mut selected = SelectionPresets::create_selection(&catalog, &opts.preset)?;
    if let Some(ref path) = opts.import_selection {
        let imported = source::import_selection_file(path)?;
        for id in imported {
            selected.insert(id);
        }
        messages.push(format!("import-selection: {}", path.display()));
    }
    for id in &opts.select {
        selected.insert(id.clone());
    }
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

    // The catalog and all derived component names are validated before touching a caller path.
    copy::create_new_run_workspace(&work)?;

    // S0 Elevate
    note(&mut log, "S0-Elevate", "Elevation check");
    if opts.dry_run_install {
        note(
            &mut log,
            "S0-Elevate",
            "Process elevation not required for dry-run install. Continuing dry pipeline.",
        );
    } else {
        let admin = driver_foundry_common::elevation::is_administrator();
        note(
            &mut log,
            "S0-Elevate",
            if admin {
                "Administrator confirmed for force-install."
            } else {
                "Not elevated — force-install may fail without admin; continuing launch attempt."
            },
        );
    }
    note(&mut log, "S0-Elevate", "Elevation check complete");

    // S1 Acquire
    note(&mut log, "S1-Acquire", "Acquiring package");
    let acquired = source::acquire_package(opts, &work, &catalog, &mut log, &mut messages)?;
    let package_root = acquired.root;
    let synthetic = acquired.synthetic;
    let source_label = acquired.source_label;

    let force = !opts.dry_run_install;
    // No shipped platform signer verifier can authenticate setup.exe. Refuse before reaching
    // live setup, registry, PnPUtil, or helper paths; a caller-provided URL/hash is not authority.
    if force {
        acquired.trust.authorize_live_install()?;
    }

    note(
        &mut log,
        "S1-Acquire",
        &format!(
            "Using package root: {} (setup.exe: {}) source={}",
            package_root.display(),
            if package_root.join("setup.exe").is_file() {
                "present"
            } else {
                "absent"
            },
            source_label
        ),
    );

    // S5d Uninstall drivers (optional, before install)
    if opts.uninstall_drivers {
        note(&mut log, "S5d-UninstallDrivers", "Uninstall drivers stage");
        uninstall::run_uninstall_stage(&work, opts.dry_run_install, &mut log)?;
    }

    // S2 Filter
    note(
        &mut log,
        "S2-Filter",
        &format!(
            "Resolved {} component(s): {}",
            resolved.len(),
            resolved.join(", ")
        ),
    );
    note(&mut log, "S2-Filter", "Filtering components");
    let prepared = work.join("prepared");
    let (kept, stripped) = copy::prepare_copy_strip(&package_root, &prepared, &catalog, &resolved)?;
    note(
        &mut log,
        "S2-Filter",
        &format!(
            "Prepare/strip complete: prepared={}; kept=[{}]; stripped=[{}]; filesCopied",
            prepared.display(),
            kept.join(", "),
            stripped.join(", ")
        ),
    );

    // S3 Tweaks
    note(&mut log, "S3-Tweaks", "Applying tweaks");
    tweaks::apply_tweaks(&prepared, opts, &mut log)?;

    // S4 Rebuild catalogs — skipped Not WHQL unless try_sign + proven successful sign
    let mut not_whql = true;
    if opts.try_sign {
        note(
            &mut log,
            "S4-RebuildCatalog",
            "try-sign requested — writing plan only; unauthenticated signtool execution is disabled.",
        );
        let sign = sign::try_sign_catalog(&prepared, &mut log);
        if !sign.tools_present {
            note(
                &mut log,
                "S4-RebuildCatalog",
                "Skipped (authenticated signtool manifest not available). Not WHQL.",
            );
        } else if sign.proven_signed {
            not_whql = false;
            note(
                &mut log,
                "S4-RebuildCatalog",
                "Catalog sign proven successful; clearing Not WHQL.",
            );
        } else {
            note(
                &mut log,
                "S4-RebuildCatalog",
                "sign probe attempted; no proven successful sign. Not WHQL.",
            );
        }
    } else {
        note(
            &mut log,
            "S4-RebuildCatalog",
            "Skipped (RebuildCatalogs=false). Not WHQL.",
        );
    }

    // S5a Install
    note(&mut log, "S5a-Install", "Install");
    let setup_path = prepared.join("setup.exe");
    let mut setup_arguments = default_setup_args(opts.clean_install);
    setup_arguments.extend(opts.setup_args.iter().cloned());

    if opts.enable_install {
        if opts.dry_run_install || synthetic {
            note(
                &mut log,
                "S5a-Install",
                &format!(
                    "Dry-run install (setup path: {}; args: {:?}). No process launched.",
                    setup_path.display(),
                    setup_arguments
                ),
            );
        } else {
            return Err(InstallError::UntrustedInstaller(
                "live setup launch is disabled until a platform signer verifier is shipped".into(),
            ));
        }
    } else {
        note(
            &mut log,
            "S5a-Install",
            "Skipped (install not enabled). Dry-run default.",
        );
    }

    // S5b Export
    let mut export_path = None;
    if let Some(ref exp) = opts.export_path {
        note(&mut log, "S5b-ExportWorkspace", "Exporting workspace");
        archive::export_workspace(&prepared, exp)?;
        export_path = Some(exp.clone());
        note(
            &mut log,
            "S5b-ExportWorkspace",
            &format!("Exported prepared tree to {}", exp.display()),
        );
    } else {
        note(
            &mut log,
            "S5b-ExportWorkspace",
            "Skipped (workspace export not enabled).",
        );
    }

    // S5c Archive
    let mut archive_path = None;
    if let Some(ref arch) = opts.archive_out {
        note(&mut log, "S5c-BuildPackage", "Building portable archive");
        let mut fmt = opts.archive_format.to_ascii_lowercase();
        if fmt.is_empty() {
            fmt = arch
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("zip")
                .to_ascii_lowercase();
        }
        let written = arch.clone();
        match fmt.as_str() {
            "7z" => {
                archive::build_7z(&prepared, arch)?;
            }
            "sfx" | "exe" => {
                archive::build_sfx(&prepared, arch)?;
            }
            _ => {
                archive::build_zip(&prepared, arch)?;
                fmt = "zip".into();
            }
        }
        archive_path = Some(written.clone());
        note(
            &mut log,
            "S5c-BuildPackage",
            &format!(
                "Portable archive written: {} format={fmt}",
                written.display()
            ),
        );
    } else {
        note(
            &mut log,
            "S5c-BuildPackage",
            "Skipped (portable archive not enabled).",
        );
    }

    // S6 Run report
    let report_path = if opts.enable_run_report {
        let path = opts
            .run_report_path
            .clone()
            .unwrap_or_else(|| work.join("driver-foundry-run-report.json"));
        note(&mut log, "S6-RunReport", "Writing run report");
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
        if export_path.is_some() {
            stages.push("S5b-ExportWorkspace".into());
        }
        if archive_path.is_some() {
            stages.push("S5c-BuildPackage".into());
        }
        stages.push("S6-RunReport".into());
        let report = RunReport {
            product: driver_foundry_common::PRODUCT_NAME.to_string(),
            version: driver_foundry_common::PRODUCT_VERSION.to_string(),
            dry_run_install: opts.dry_run_install || synthetic,
            force_install: force && !synthetic,
            preset: opts.preset.to_ascii_lowercase(),
            package_source: source_label.clone(),
            package_root: package_root.display().to_string(),
            prepared_root: prepared.display().to_string(),
            kept_components: kept.clone(),
            stripped_components: stripped.clone(),
            setup_arguments: setup_arguments.clone(),
            not_whql,
            stages,
            export_path: export_path.as_ref().map(|p| p.display().to_string()),
            archive_path: archive_path.as_ref().map(|p| p.display().to_string()),
            launch_command: None,
        };
        report.write_to(&path)?;
        note(
            &mut log,
            "S6-RunReport",
            &format!(
                "Run report written: {}; resolved={}, stripped={}, source={}, Not WHQL.",
                path.display(),
                kept.len(),
                stripped.len(),
                source_label
            ),
        );
        Some(path)
    } else {
        None
    };

    note(&mut log, "Completed", "Pipeline completed.");
    messages.extend(log.iter().cloned());
    messages.push(format!("prepared: {}", prepared.display()));
    messages.push(format!("kept ({}): {}", kept.len(), kept.join(", ")));
    messages.push(format!(
        "stripped ({}): {}",
        stripped.len(),
        stripped.join(", ")
    ));
    if let Some(ref rp) = report_path {
        messages.push(format!("run-report: {}", rp.display()));
    }

    let ok = prepared.is_dir() && !kept.is_empty() && kept.iter().any(|k| k == "Display.Driver");

    Ok(InstallResult {
        exit_code: if ok { 0 } else { 1 },
        dry_run_install: opts.dry_run_install || synthetic,
        work_directory: work,
        prepared_root: Some(prepared),
        package_root: Some(package_root),
        run_report_path: report_path,
        kept_components: kept,
        stripped_components: stripped,
        log,
        messages,
        used_synthetic_fixture: synthetic,
        export_path,
        archive_path,
        launch_command: None,
    })
}

fn validate_install_path_separation(
    opts: &InstallOptions,
    work: &std::path::Path,
) -> Result<(), InstallError> {
    for (label, input) in [
        ("package root", opts.package_root.as_deref()),
        ("package archive", opts.package_archive.as_deref()),
    ] {
        if let Some(input) = input {
            if copy::paths_overlap(work, input) {
                return Err(InstallError::Other(format!(
                    "work directory must not overlap {label}: {}",
                    input.display()
                )));
            }
        }
    }
    for (label, output) in [
        ("export directory", opts.export_path.as_deref()),
        ("archive output", opts.archive_out.as_deref()),
    ] {
        if let Some(output) = output {
            // A fresh export/archive nested in this newly-created workspace is safe: its
            // individual writer also requires a new destination and never replaces it.
            // The inverse (work nested in output) remains unsafe and is rejected.
            if copy::paths_overlap(work, output) && !copy::path_is_within(output, work) {
                return Err(InstallError::Other(format!(
                    "work directory must not overlap {label}: {}",
                    output.display()
                )));
            }
            for input in [
                opts.package_root.as_deref(),
                opts.package_archive.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                if copy::paths_overlap(output, input) {
                    return Err(InstallError::Other(format!(
                        "{label} must not overlap package input: {}",
                        input.display()
                    )));
                }
            }
        }
    }
    if let Some(report) = opts.run_report_path.as_deref() {
        if copy::paths_overlap(work, report) && !copy::path_is_within(report, work) {
            return Err(InstallError::Other(format!(
                "work directory must not overlap run report: {}",
                report.display()
            )));
        }
        for input in [
            opts.package_root.as_deref(),
            opts.package_archive.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if copy::paths_overlap(report, input) {
                return Err(InstallError::Other(format!(
                    "run report must not overlap package input: {}",
                    input.display()
                )));
            }
        }
    }
    let outputs = [
        ("export directory", opts.export_path.as_deref()),
        ("archive output", opts.archive_out.as_deref()),
        ("run report", opts.run_report_path.as_deref()),
    ];
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
            "live_registry_apply is disabled until the platform signer verifier and authenticated installer policy are shipped"
                .into(),
        ));
    }
    if !opts.dry_run_install {
        return Err(InstallError::UntrustedInstaller(
            "force-install is disabled until the platform signer verifier and authenticated vendor signer policy are shipped"
                .into(),
        ));
    }
    // Dry-run still materializes a workspace, archive, report, and optional export. Without
    // retained no-follow handles and a protected root, doing that while elevated turns any
    // caller-controlled output into an arbitrary-write TOCTOU risk.
    if elevated {
        return Err(InstallError::UntrustedInstaller(
            "elevated dry-run is disabled until protected no-follow workspace/output roots are available; rerun unelevated"
                .into(),
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
        let options = InstallOptions::default();
        let error = validate_pre_workspace_options_with_elevation(&options, true).unwrap_err();
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
