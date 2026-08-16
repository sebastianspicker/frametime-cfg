//! Drive the real shipped binary for meta + clean/install dry-run paths.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> PathBuf {
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_dfoundry") {
        return PathBuf::from(p);
    }
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../target/debug/dfoundry.exe");
    if path.is_file() {
        return path;
    }
    path.pop();
    path.push("dfoundry");
    path
}

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

#[test]
fn help_mentions_clean_install_gui_and_scopes() {
    let out = Command::new(bin())
        .args(["--help"])
        .env("DFOUNDRY_DATA_DIR", data_dir())
        .env("DFOUNDRY_FORCE_CLI_HELP", "1")
        .output()
        .expect("run --help");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Remove cleanly. Install only what you need."));
    assert!(text.to_ascii_lowercase().contains("clean"));
    assert!(text.to_ascii_lowercase().contains("install"));
    assert!(text.to_ascii_lowercase().contains("gui"));
}

#[test]
fn clean_help_exposes_productive_flags() {
    let out = Command::new(bin())
        .args(["clean", "--help"])
        .env("DFOUNDRY_DATA_DIR", data_dir())
        .output()
        .expect("clean --help");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for flag in [
        "--execute",
        "--remove-gfe",
        "--remove-install-cache",
        "--prepare-safeboot",
        "--cache-only",
        "--block-driver-search",
        "--plan-report",
    ] {
        assert!(text.contains(flag), "clean --help missing {flag}: {text}");
    }
}

#[test]
fn install_help_exposes_productive_flags() {
    let out = Command::new(bin())
        .args(["install", "--help"])
        .env("DFOUNDRY_DATA_DIR", data_dir())
        .output()
        .expect("install --help");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for flag in [
        "--force-install",
        "--package-archive",
        "--package-url",
        "--export",
        "--archive",
        "--select",
        "--uninstall-drivers",
    ] {
        assert!(text.contains(flag), "install --help missing {flag}: {text}");
    }
}

#[test]
fn materialize_embedded_is_refused_without_authenticated_manifest() {
    let out = Command::new(bin())
        .args(["install", "--materialize-embedded"])
        .env("DFOUNDRY_DATA_DIR", data_dir())
        .output()
        .expect("run materialize-embedded");
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("authenticated release manifest"),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn version_non_empty() {
    let out = Command::new(bin())
        .args(["--version"])
        .output()
        .expect("run --version");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.trim().is_empty());
    assert!(
        text.contains("0.1.0") && text.contains("dfoundry"),
        "unexpected version text: {text}"
    );
}

#[test]
fn clean_nvidia_dry_run_plans_work() {
    let out = Command::new(bin())
        .args(["clean", "--vendor", "nvidia", "--dry-run"])
        .env("DFOUNDRY_DATA_DIR", data_dir())
        .output()
        .expect("run clean");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "clean failed status={:?}\nstdout={stdout}\nstderr={stderr}",
        out.status
    );
    assert!(
        stdout.contains("planned=") || stdout.contains("planned_total="),
        "expected plan counts: {stdout}"
    );
    assert!(
        stdout.contains("DRY-RUN") || stdout.contains("dryRun=true") || stdout.contains("dry-run"),
        "expected dry-run marker: {stdout}"
    );
    assert!(
        stdout.contains("0_resolve_vendor") || stdout.contains("success"),
        "expected stages: {stdout}"
    );
    assert!(
        !stdout.contains("planned=0\n") && !stdout.contains("planned_total=0"),
        "planned work must be non-zero: {stdout}"
    );
}

#[test]
fn clean_amd_and_realtek_dry_run() {
    for vendor in ["amd", "realtek"] {
        let out = Command::new(bin())
            .args(["clean", "--vendor", vendor, "--dry-run", "--no-host-probe"])
            .env("DFOUNDRY_DATA_DIR", data_dir())
            .output()
            .expect("run clean");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "{vendor} failed\nstdout={stdout}\nstderr={stderr}"
        );
        assert!(
            stdout.contains("planned=") || stdout.contains("planned_total="),
            "{vendor}: {stdout}"
        );
    }
}

#[test]
fn clean_preflight() {
    let out = Command::new(bin())
        .args(["clean", "--preflight"])
        .env("DFOUNDRY_DATA_DIR", data_dir())
        .output()
        .expect("preflight");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("preflight:"));
}

#[test]
fn clean_execute_not_not_implemented() {
    let out = Command::new(bin())
        .args(["clean", "--vendor", "nvidia", "--execute"])
        .env("DFOUNDRY_DATA_DIR", data_dir())
        // Avoid interactive UAC prompt hanging the test suite
        .env("DFOUNDRY_NO_UAC_RELAUNCH", "1")
        .output()
        .expect("execute");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}{stderr}").to_ascii_lowercase();
    assert!(
        !combined.contains("not implemented"),
        "must not stub-refuse: {combined}"
    );
    // Either trust-boundary block, elevation error, relaunch, or success if authenticated
    // packaged catalog metadata is available in a future release.
    assert!(
        out.status.success()
            || combined.contains("catalog authentication")
            || combined.contains("elevation")
            || combined.contains("administrator")
            || combined.contains("admin"),
        "unexpected execute outcome: status={:?} {combined}",
        out.status
    );
}

#[test]
fn install_synthetic_dry_run_report() {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let work = std::env::temp_dir().join(format!("dfoundry-cli-test-{n}"));
    let _ = std::fs::remove_dir_all(&work);

    let out = Command::new(bin())
        .args([
            "install",
            "--work",
            work.to_str().unwrap(),
            "--preset",
            "clean",
        ])
        .env("DFOUNDRY_DATA_DIR", data_dir())
        .output()
        .expect("run install");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "install failed status={:?}\nstdout={stdout}\nstderr={stderr}",
        out.status
    );
    assert!(
        stdout.contains("synthetic") || stdout.contains("fixture"),
        "expected synthetic fixture: {stdout}"
    );
    assert!(
        stdout.contains("S2-Filter") || stdout.contains("kept"),
        "expected filter evidence: {stdout}"
    );
    assert!(
        stdout.to_ascii_lowercase().contains("dry-run") || stdout.contains("dryRunInstall=true"),
        "expected install dry-run: {stdout}"
    );

    let reports: Vec<_> = std::fs::read_dir(&work)
        .expect("work dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|x| x.to_str()) == Some("json")
                || p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.contains("report"))
                    .unwrap_or(false)
        })
        .collect();
    assert!(
        !reports.is_empty() || stdout.contains("run-report:"),
        "expected run-report artifact under work or log"
    );
    if let Some(r) = reports.first() {
        let body = std::fs::read_to_string(r).expect("read report");
        assert!(body.contains("Display.Driver"), "report content: {body}");
        assert!(body.contains("dry_run_install") || body.contains("DryRun"));
    }

    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn clean_intel_and_lisuan_dry_run() {
    for vendor in ["intel", "lisuan"] {
        let out = Command::new(bin())
            .args(["clean", "--vendor", vendor, "--dry-run", "--no-host-probe"])
            .env("DFOUNDRY_DATA_DIR", data_dir())
            .output()
            .expect("run clean");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "{vendor} failed\nstdout={stdout}\nstderr={stderr}"
        );
        assert!(
            stdout.contains("planned=") && !stdout.contains("planned=0"),
            "{vendor} planned work: {stdout}"
        );
        assert!(
            stdout.contains("0_resolve_vendor") || stdout.contains("success"),
            "{vendor} stages: {stdout}"
        );
    }
}

#[test]
fn list_languages_and_packages() {
    let out = Command::new(bin())
        .args(["list-languages"])
        .env("DFOUNDRY_DATA_DIR", data_dir())
        .output()
        .expect("list-languages");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("count:") || stdout.contains("English") || stdout.contains(".xml"),
        "languages: {stdout}"
    );

    let out = Command::new(bin())
        .args(["list-packages"])
        .env("DFOUNDRY_DATA_DIR", data_dir())
        .output()
        .expect("list-packages");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(
        stdout.contains("Display.Driver"),
        "packages should list Display.Driver: {stdout}"
    );
}

#[test]
fn install_export_and_zip_archive() {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let work = std::env::temp_dir().join(format!("dfoundry-cli-export-{n}"));
    let export = work.join("export-out");
    let archive = work.join("portable.zip");
    let _ = std::fs::remove_dir_all(&work);

    let out = Command::new(bin())
        .args([
            "install",
            "--work",
            work.to_str().unwrap(),
            "--preset",
            "minimal",
            "--export",
            export.to_str().unwrap(),
            "--archive",
            archive.to_str().unwrap(),
            "--deep-inf",
            "--uninstall-drivers",
        ])
        .env("DFOUNDRY_DATA_DIR", data_dir())
        .output()
        .expect("install export");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "install export failed\nstdout={stdout}\nstderr={stderr}"
    );
    assert!(
        export.is_dir() || stdout.contains("Export"),
        "export: {stdout}"
    );
    assert!(
        archive.is_file()
            || stdout.contains("Portable archive")
            || stdout.contains("S5c-BuildPackage"),
        "archive evidence: {stdout}"
    );
    assert!(
        stdout.contains("S5d-UninstallDrivers") || stdout.contains("uninstall"),
        "uninstall stage: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn clean_scopes_and_plan_report_file() {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let report = std::env::temp_dir().join(format!("dfoundry-plan-{n}.txt"));
    let _ = std::fs::remove_file(&report);

    let out = Command::new(bin())
        .args([
            "clean",
            "--vendor",
            "nvidia",
            "--dry-run",
            "--no-host-probe",
            "--remove-gfe",
            "--remove-physx",
            "--plan-report",
            report.to_str().unwrap(),
        ])
        .env("DFOUNDRY_DATA_DIR", data_dir())
        .output()
        .expect("clean scopes");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("remove-gfe") || stdout.contains("GFE") || stdout.contains("gfe"),
        "gfe scope evidence: {stdout}"
    );
    assert!(report.is_file(), "plan-report file missing");
    let body = std::fs::read_to_string(&report).expect("read plan");
    assert!(!body.trim().is_empty(), "plan report empty");
    let _ = std::fs::remove_file(&report);
}
