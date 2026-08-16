use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::catalog::PackageCatalog;
use crate::fixture::create_synthetic_package;
use crate::launch::looks_like_windows_pe;
use crate::tweaks::rewrite_inf_deep_strip;
use crate::uninstall::{filter_display_gpu_oems, parse_pnputil_enum_drivers_simple};
use crate::*;
use driver_foundry_common::catalog_path;

fn unique_work() -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("dfoundry-test-{n}"))
}

#[test]
fn synthetic_install_dry_run_filters_and_reports() {
    let work = unique_work();
    let opts = InstallOptions {
        work_directory: work.clone(),
        preset: "clean".into(),
        package_root: None,
        catalog_path: None,
        enable_install: true,
        dry_run_install: true,
        enable_run_report: true,
        run_report_path: None,
        ..InstallOptions::default()
    };
    let r = run_install(&opts).expect("install");

    assert_eq!(r.exit_code, 0);
    assert!(r.dry_run_install);
    assert!(r.used_synthetic_fixture);
    assert!(
        r.prepared_root
            .as_ref()
            .map(|p| p.is_dir())
            .unwrap_or(false),
        "prepared root must exist"
    );
    assert!(r
        .kept_components
        .iter()
        .any(|c| c.eq_ignore_ascii_case("Display.Driver")));
    assert!(!r.stripped_components.is_empty());
    assert!(r.log.iter().any(|l| l.contains("S1-Acquire")));
    assert!(r.log.iter().any(|l| l.contains("S2-Filter")));
    assert!(r
        .log
        .iter()
        .any(|l| l.contains("S5a-Install") && l.to_ascii_lowercase().contains("dry-run")));
    let report = r.run_report_path.expect("report path");
    assert!(report.is_file(), "run report file must exist");

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn synthetic_install_writes_report_file() {
    let work = unique_work();
    let report = work.join("report.json");
    let opts = InstallOptions {
        work_directory: work.clone(),
        preset: "clean".into(),
        enable_run_report: true,
        run_report_path: Some(report.clone()),
        ..InstallOptions::default()
    };
    let r = run_install(&opts).expect("install");
    assert_eq!(r.exit_code, 0);
    assert!(report.is_file(), "run report must exist");
    let text = fs::read_to_string(&report).unwrap();
    assert!(text.contains("Display.Driver"));
    assert!(text.contains("dry_run_install"));
    let _ = fs::remove_dir_all(&work);
}

#[test]
fn pe_check_rejects_text_setup() {
    let dir = unique_work();
    fs::create_dir_all(&dir).unwrap();
    let setup = dir.join("setup.exe");
    fs::write(&setup, "fake-setup-placeholder").unwrap();
    assert!(!looks_like_windows_pe(&setup));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn force_install_refuses_synthetic() {
    let work = unique_work();
    let opts = InstallOptions {
        work_directory: work.clone(),
        dry_run_install: false,
        enable_install: true,
        ..InstallOptions::default()
    };
    let err = run_install(&opts).unwrap_err();
    assert!(matches!(err, InstallError::UntrustedInstaller(_)));
    let _ = fs::remove_dir_all(&work);
}

#[test]
fn live_registry_apply_is_refused_before_workspace_creation() {
    let work = unique_work();
    let error = run_install(&InstallOptions {
        work_directory: work.clone(),
        live_registry_apply: true,
        ..InstallOptions::default()
    })
    .unwrap_err();
    assert!(matches!(error, InstallError::UntrustedInstaller(_)));
    assert!(
        !work.exists(),
        "refused live option must not create a workspace"
    );
}

#[test]
fn dry_run_elevation_canary_defers_administrator_probe() {
    let source = include_str!("pipeline.rs");
    let dry_branch = source
        .find("if opts.dry_run_install")
        .expect("dry-run branch");
    let elevation_probe = source
        .find("elevation::is_administrator")
        .expect("elevation probe");
    assert!(
        elevation_probe > dry_branch,
        "administrator probe must remain outside the dry-run branch"
    );
}

#[test]
fn unsafe_catalog_is_refused_before_workspace_creation() {
    let root = unique_work();
    fs::create_dir_all(&root).unwrap();
    let catalog = root.join("catalog.json");
    fs::write(
        &catalog,
        r#"{"schema":"driver-foundry.catalog/v1","packages":[{"id":"..\\escape","required":true}]}"#,
    )
    .unwrap();
    let work = root.join("work");
    let error = run_install(&InstallOptions {
        work_directory: work.clone(),
        catalog_path: Some(catalog),
        ..InstallOptions::default()
    })
    .unwrap_err();
    assert!(error.to_string().contains("unsafe package component id"));
    assert!(
        !work.exists(),
        "invalid catalog must not create a workspace"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn install_never_reuses_existing_work_directory() {
    let work = unique_work();
    fs::create_dir_all(&work).unwrap();
    let user_file = work.join("do-not-delete.txt");
    fs::write(&user_file, "user-owned").unwrap();
    let error = run_install(&InstallOptions {
        work_directory: work.clone(),
        ..InstallOptions::default()
    })
    .expect_err("existing work directory must be rejected");
    assert!(error.to_string().contains("refusing to replace"));
    assert_eq!(fs::read_to_string(user_file).unwrap(), "user-owned");
    let _ = fs::remove_dir_all(work);
}

#[test]
fn install_rejects_work_and_package_or_output_overlaps() {
    let root = unique_work();
    fs::create_dir_all(&root).unwrap();
    let package = root.join("package");
    fs::create_dir_all(&package).unwrap();
    let overlap = run_install(&InstallOptions {
        work_directory: package.join("run"),
        package_root: Some(package.clone()),
        ..InstallOptions::default()
    })
    .expect_err("work inside package must be rejected");
    assert!(overlap
        .to_string()
        .contains("must not overlap package root"));

    let work = root.join("work");
    let export = root.join("export");
    let archive = export.join("archive.zip");
    let overlap = run_install(&InstallOptions {
        work_directory: work,
        export_path: Some(export),
        archive_out: Some(archive),
        ..InstallOptions::default()
    })
    .expect_err("archive inside export must be rejected");
    assert!(overlap
        .to_string()
        .contains("export directory must not overlap archive"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn install_never_replaces_explicit_report_output() {
    let root = unique_work();
    let report = root.join("existing-report.json");
    fs::create_dir_all(&root).unwrap();
    fs::write(&report, "user report").unwrap();
    let error = run_install(&InstallOptions {
        work_directory: root.join("new-work"),
        run_report_path: Some(report.clone()),
        ..InstallOptions::default()
    })
    .expect_err("existing report must be preserved");
    assert!(error.to_string().contains("refusing to replace"));
    assert_eq!(fs::read_to_string(report).unwrap(), "user report");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn archive_source_and_export_zip() {
    let work = unique_work();
    fs::create_dir_all(&work).unwrap();
    // Build a zip package from synthetic tree
    let pkg = work.join("pkg");
    create_synthetic_package(&pkg, ["Display.Driver", "HDAudio", "PhysX", "NGXCore"]).unwrap();
    // Need full catalog keys for strip semantics — use real pipeline with package_root first
    let export = work.join("export");
    let arch_out = work.join("portable.zip");
    let opts = InstallOptions {
        work_directory: work.join("run"),
        package_root: Some(pkg),
        preset: "clean".into(),
        dry_run_install: true,
        export_path: Some(export.clone()),
        archive_out: Some(arch_out.clone()),
        enable_run_report: true,
        ..InstallOptions::default()
    };
    let r = run_install(&opts).expect("install from local");
    assert_eq!(r.exit_code, 0);
    assert!(export.is_dir());
    assert!(arch_out.is_file());
    assert!(r.log.iter().any(|l| l.contains("S5b-ExportWorkspace")));
    assert!(r.log.iter().any(|l| l.contains("S5c-BuildPackage")));
    // Round-trip archive extract as source
    let work2 = work.join("from-zip");
    let opts2 = InstallOptions {
        work_directory: work2,
        package_archive: Some(arch_out),
        preset: "clean".into(),
        dry_run_install: true,
        ..InstallOptions::default()
    };
    let r2 = run_install(&opts2).expect("install from archive");
    assert_eq!(r2.exit_code, 0);
    assert!(!r2.used_synthetic_fixture);
    assert!(r2.messages.iter().any(|m| m.contains("archive")));
    let _ = fs::remove_dir_all(&work);
}

#[test]
fn uninstall_stage_dry_run() {
    let work = unique_work();
    let opts = InstallOptions {
        work_directory: work.clone(),
        uninstall_drivers: true,
        dry_run_install: true,
        ..InstallOptions::default()
    };
    let r = run_install(&opts).unwrap();
    assert!(r.log.iter().any(|l| l.contains("S5d-UninstallDrivers")));
    assert!(
        work.join("uninstall-drivers-plan.txt").is_file()
            || r.work_directory
                .join("uninstall-drivers-plan.txt")
                .is_file()
    );
    let _ = fs::remove_dir_all(&work);
}

#[test]
fn select_deselect_affects_kept() {
    let work = unique_work();
    let opts = InstallOptions {
        work_directory: work.clone(),
        preset: "minimal".into(),
        select: vec!["PhysX".into()],
        dry_run_install: true,
        ..InstallOptions::default()
    };
    let r = run_install(&opts).unwrap();
    assert!(r.kept_components.iter().any(|c| c == "PhysX"));
    let _ = fs::remove_dir_all(&work);
}

#[test]
fn wizard_mapping() {
    let o = options_from_wizard(
        unique_work(),
        "gaming",
        None,
        true,
        Some(PathBuf::from("exp")),
        None,
    );
    assert_eq!(o.preset, "gaming");
    assert!(o.dry_run_install);
    assert!(o.export_path.is_some());
}

#[test]
fn force_install_rejects_local_mz_setup_without_trust_metadata() {
    let work = unique_work();
    fs::create_dir_all(&work).unwrap();
    let pkg = work.join("pe-pkg");
    // Create package with MZ setup and catalog components
    let cat =
        PackageCatalog::load_from_file(&catalog_path(&driver_foundry_common::resolve_data_root()))
            .unwrap();
    create_synthetic_package(&pkg, cat.packages.keys().cloned()).unwrap();
    // Overwrite setup.exe with MZ stub
    launch::write_mz_stub(&pkg.join("setup.exe")).unwrap();
    let opts = InstallOptions {
        work_directory: work.join("run"),
        package_root: Some(pkg),
        dry_run_install: false,
        enable_install: true,
        enable_run_report: true,
        ..InstallOptions::default()
    };
    let error = run_install(&opts).expect_err("local MZ setup must not launch");
    assert!(matches!(error, InstallError::UntrustedInstaller(_)));
    let _ = fs::remove_dir_all(&work);
}

#[test]
fn force_install_rejects_renamed_download_bytes_before_launch() {
    let work = unique_work();
    fs::create_dir_all(&work).unwrap();
    let package = work.join("pe-pkg");
    let catalog =
        PackageCatalog::load_from_file(&catalog_path(&driver_foundry_common::resolve_data_root()))
            .expect("catalog");
    create_synthetic_package(&package, catalog.packages.keys().cloned()).expect("fixture");
    launch::write_mz_stub(&package.join("setup.exe")).expect("MZ setup");
    let options = InstallOptions {
        work_directory: work.join("run"),
        package_root: Some(package),
        dry_run_install: false,
        enable_install: true,
        ..InstallOptions::default()
    };
    let error = run_install(&options).expect_err("renamed MZ bytes must not launch");
    assert!(matches!(error, InstallError::UntrustedInstaller(_)));
    let _ = fs::remove_dir_all(work);
}

#[test]
fn materialize_embedded_helpers() {
    let dest = unique_work().join("emb");
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    let copied = crate::copy::materialize_embedded(&dest).expect("materialize");
    assert!(!copied.is_empty(), "should copy embedded helpers");
    assert!(
        dest.join("7zr.exe").is_file() || dest.join("packages.xml").is_file(),
        "expected known helper under {}",
        dest.display()
    );
    let _ = fs::remove_dir_all(dest.parent().unwrap());
}

#[test]
fn materialize_embedded_refuses_existing_destination() {
    let root = unique_work();
    let dest = root.join("emb");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("user-file"), b"keep").unwrap();
    assert!(crate::copy::materialize_embedded(&dest).is_err());
    assert_eq!(fs::read(dest.join("user-file")).unwrap(), b"keep");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn try_sign_writes_plan_without_executing_helper() {
    let work = unique_work();
    let opts = InstallOptions {
        work_directory: work.clone(),
        try_sign: true,
        dry_run_install: true,
        enable_run_report: true,
        ..InstallOptions::default()
    };
    let r = run_install(&opts).expect("install");
    assert!(
        r.log
            .iter()
            .any(|l| l.contains("helper execution disabled") || l.contains("Not WHQL")),
        "try-sign should remain non-executing: {:?}",
        r.log
            .iter()
            .filter(|l| l.contains("S4"))
            .collect::<Vec<_>>()
    );
    // Plan is retained, but no embedded or PATH signtool is launched.
    let prepared = r.prepared_root.expect("prepared");
    let plan = prepared.join("driver-foundry-sign-plan.txt");
    assert!(plan.is_file(), "sign plan should be written");
    let plan_text = fs::read_to_string(&plan).unwrap();
    assert!(plan_text.contains("execution") && plan_text.contains("disabled"));
    // Honest: without a real re-sign, report remains Not WHQL.
    if let Some(rp) = &r.run_report_path {
        let report = fs::read_to_string(rp).unwrap();
        assert!(
            report.contains("\"not_whql\": true") || report.contains("\"not_whql\":true"),
            "not_whql must stay true unless proven sign: {report}"
        );
    }
    let _ = fs::remove_dir_all(&work);
}

#[test]
fn deep_inf_and_extended_tweaks_write_markers() {
    let work = unique_work();
    let opts = InstallOptions {
        work_directory: work.clone(),
        deep_inf: true,
        disable_hdcp: true,
        disable_mpo: true,
        disable_nvcamera: true,
        dry_run_install: true,
        ..InstallOptions::default()
    };
    let r = run_install(&opts).expect("install");
    let prepared = r.prepared_root.expect("prepared");
    assert!(prepared.join("driver-foundry-deep-inf.flag").is_file());
    let tweaks = fs::read_to_string(prepared.join("driver-foundry-tweaks.json")).unwrap();
    assert!(tweaks.contains("disable_hdcp"));
    assert!(tweaks.contains("\"deep_inf\": true") || tweaks.contains("\"deep_inf\":true"));
    let reg = fs::read_to_string(
        prepared
            .join("driver-foundry-post-install")
            .join("post-install-markers.reg"),
    )
    .unwrap();
    assert!(reg.contains("OverlayTestMode") || reg.contains("RMHdcpKeyglobZero"));
    // Synthetic fixture ships Display.Driver/sample.inf — deep-inf should rewrite it.
    let sample = prepared.join("Display.Driver").join("sample.inf");
    if sample.is_file() {
        let text = fs::read_to_string(&sample).unwrap();
        assert!(
            text.contains("driver-foundry-deep-strip") || text.contains("Telemetry"),
            "expected deep strip or original telemetry content: {text}"
        );
        assert!(
            text.contains("driver-foundry deep-inf applied"),
            "trailing deep-inf marker missing: {text}"
        );
        assert!(
            prepared
                .join("Display.Driver")
                .join("sample.inf.driver-foundry-deep")
                .is_file()
                || prepared
                    .join("Display.Driver")
                    .join("sample.driver-foundry-deep")
                    .is_file()
                || fs::read_dir(prepared.join("Display.Driver"))
                    .unwrap()
                    .flatten()
                    .any(|e| e
                        .file_name()
                        .to_string_lossy()
                        .contains("driver-foundry-deep")),
            "expected .inf.driver-foundry-deep sibling marker"
        );
    }
    let _ = fs::remove_dir_all(&work);
}

#[test]
fn deep_inf_rewrites_telemetry_lines_in_package_inf() {
    let work = unique_work();
    fs::create_dir_all(&work).unwrap();
    let pkg = work.join("pkg");
    let cat =
        PackageCatalog::load_from_file(&catalog_path(&driver_foundry_common::resolve_data_root()))
            .unwrap();
    create_synthetic_package(&pkg, cat.packages.keys().cloned()).unwrap();
    // Explicit foo.inf with Telemetry + GFExperience lines for the assertion.
    let driver = pkg.join("Display.Driver");
    fs::create_dir_all(&driver).unwrap();
    fs::write(
        driver.join("foo.inf"),
        "[Version]\n\
Signature=\"$WINDOWS NT$\"\n\
\n\
[SourceDisksFiles]\n\
driver.sys=1\n\
\n\
AddService=NvTelemetry,,NvTelemetry_Service\n\
GFExperience.CopyFiles=GfeFiles\n\
KeepThisLine=1\n\
; already commented Telemetry stays\n",
    )
    .unwrap();

    let opts = InstallOptions {
        work_directory: work.join("run"),
        package_root: Some(pkg),
        deep_inf: true,
        dry_run_install: true,
        ..InstallOptions::default()
    };
    let r = run_install(&opts).expect("install");
    let prepared = r.prepared_root.expect("prepared");
    assert!(prepared.join("driver-foundry-deep-inf.flag").is_file());

    let foo = prepared.join("Display.Driver").join("foo.inf");
    assert!(foo.is_file(), "prepared foo.inf must exist");
    let text = fs::read_to_string(&foo).unwrap();
    assert!(
        text.contains("; driver-foundry-deep-strip") && text.contains("NvTelemetry"),
        "Telemetry line should be strip-prefixed: {text}"
    );
    assert!(
        text.contains("; driver-foundry-deep-strip") && text.contains("GFExperience"),
        "GFExperience line should be strip-prefixed: {text}"
    );
    assert!(
        text.contains("KeepThisLine=1"),
        "unrelated lines must remain: {text}"
    );
    assert!(
        !text
            .lines()
            .any(|l| l.trim_start().starts_with("AddService=NvTelemetry")),
        "raw Telemetry service line should not remain active: {text}"
    );
    assert!(
        text.contains("; driver-foundry deep-inf applied"),
        "trailing applied comment missing: {text}"
    );
    assert!(
        prepared
            .join("Display.Driver")
            .join("foo.inf.driver-foundry-deep")
            .is_file(),
        "sibling .inf.driver-foundry-deep marker required"
    );
    assert!(
        r.log
            .iter()
            .any(|l| l.contains("Deep-INF") && (l.contains("surgery") || l.contains("INF"))),
        "log should mention deep INF: {:?}",
        r.log
            .iter()
            .filter(|l| l.contains("Deep") || l.contains("INF"))
            .collect::<Vec<_>>()
    );
    let _ = fs::remove_dir_all(&work);
}

#[test]
fn deep_inf_strip_helper_unit() {
    let dir = unique_work();
    fs::create_dir_all(&dir).unwrap();
    let inf = dir.join("t.inf");
    fs::write(
        &inf,
        "Normal=1\nTelemetryService=yes\n; comment Telemetry\nAppxPackage=x\n",
    )
    .unwrap();
    let n = rewrite_inf_deep_strip(&inf).unwrap();
    assert!(
        n >= 2,
        "expected at least Telemetry + Appx stripped, got {n}"
    );
    let text = fs::read_to_string(&inf).unwrap();
    assert!(text.contains("; driver-foundry-deep-strip TelemetryService=yes"));
    assert!(text.contains("; driver-foundry-deep-strip AppxPackage=x"));
    assert!(text.contains("Normal=1"));
    assert!(text.contains("; driver-foundry deep-inf applied"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn uninstall_plan_parses_pnputil_sample() {
    let sample = "\
Published Name:     oem12.inf
Original Name:      nv_disp.inf
Provider Name:      NVIDIA Corporation
Class Name:         Display adapters

Published Name:     oem3.inf
Original Name:      net.inf
Provider Name:      Microsoft
Class Name:         Net
";
    let pkgs = parse_pnputil_enum_drivers_simple(sample);
    assert_eq!(pkgs.len(), 2);
    assert_eq!(pkgs[0].published_name, "oem12.inf");
    let matched = filter_display_gpu_oems(&pkgs);
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].published_name, "oem12.inf");
}

#[test]
fn uninstall_stage_writes_plan_via_run_install() {
    let work = unique_work();
    let opts = InstallOptions {
        work_directory: work.clone(),
        uninstall_drivers: true,
        dry_run_install: true,
        ..InstallOptions::default()
    };
    let r = run_install(&opts).unwrap();
    let plan = work.join("uninstall-drivers-plan.txt");
    let plan_alt = r.work_directory.join("uninstall-drivers-plan.txt");
    let plan_path = if plan.is_file() { &plan } else { &plan_alt };
    assert!(plan_path.is_file(), "uninstall plan missing");
    let text = fs::read_to_string(plan_path).unwrap();
    assert!(
        text.contains("pnputil") && text.contains("enum"),
        "plan should document enum: {text}"
    );
    assert!(
        r.log.iter().any(|l| l.contains("S5d-UninstallDrivers")),
        "stage logged"
    );
    let _ = fs::remove_dir_all(&work);
}
