use std::{collections::BTreeMap, fs, process::Command};

use frametime_driver::{
    ArtifactLocator, AuthenticodeEvidence, AuthenticodeStatus, DriverPlanInput, ExactGpuIdentity,
    GpuVendor, OemPublishedName, PublishedDriverPackage, Sha256Digest, SignedArtifactDescriptor,
};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_frametime")
}

#[test]
fn smoke_test_is_non_elevated() {
    let output = Command::new(binary())
        .arg("smoke-test")
        .output()
        .expect("run");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert!(String::from_utf8_lossy(&output.stdout).contains("SMOKE TEST OK: frametime"));
}

#[test]
fn all_gpu_dry_run_reports_feature_aware_support_without_persistence() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let output = Command::new(binary())
        .args(["dry-run", "all"])
        .current_dir(directory.path())
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    if cfg!(feature = "qualified-shader-cache-delete") {
        assert_eq!(output.status.code(), Some(0));
        assert!(stderr.is_empty());
    } else {
        assert_eq!(output.status.code(), Some(1));
        assert!(stderr.contains("dry-run completed with"));
        assert!(stderr.contains("unsupported action preview(s)"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for marker in [
        "PHASE 1 PREVIEW COMPLETE",
        "PHASE 2 PREVIEW COMPLETE",
        "PHASE 3 PREVIEW COMPLETE",
        "ALL 3 PHASES PREVIEW COMPLETE",
        "ALL FOUR GPU BRANCH PREVIEWS COMPLETE",
    ] {
        assert!(stdout.contains(marker), "missing {marker}");
    }
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("read temporary directory")
            .count(),
        0,
        "strict preview created a persistent artifact"
    );
}

#[test]
fn help_exposes_the_complete_command_contract() {
    let output = Command::new(binary()).arg("--help").output().expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in [
        "optimize",
        "dry-run",
        "configure",
        "phase2",
        "phase3",
        "boot-safe-mode",
        "cleanup",
        "fps-cap",
        "baseline-benchmark",
        "final-benchmark",
        "driver",
        "hardware",
        "verify",
        "restore",
        "backup-summary",
        "reset-progress",
        "show-log",
        "smoke-test",
    ] {
        assert!(stdout.contains(command), "missing command {command}");
    }
}

#[test]
fn hardware_help_exposes_bounded_native_diagnostics() {
    let output = Command::new(binary())
        .args(["hardware", "--help"])
        .output()
        .expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in ["doctor", "cpu", "gpu", "system", "whea", "frames"] {
        assert!(
            stdout.contains(command),
            "missing hardware command {command}"
        );
    }

    let invalid = Command::new(binary())
        .args(["hardware", "whea", "--max-records", "129"])
        .output()
        .expect("run");
    assert_eq!(invalid.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&invalid.stdout), "");
}

#[cfg(not(windows))]
#[test]
fn hardware_diagnostics_fail_closed_with_a_versioned_read_only_envelope() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let output = Command::new(binary())
        .args(["hardware", "doctor"])
        .current_dir(directory.path())
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("diagnostic envelope JSON");
    assert_eq!(value["schema_version"], "frametime.hardware/v1");
    assert_eq!(value["status"], "unavailable");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("only performs diagnostics on Windows")
    );
    assert_eq!(
        fs::read_dir(directory.path()).expect("directory").count(),
        0
    );
}

fn driver_plan_input() -> DriverPlanInput {
    let target_gpu = ExactGpuIdentity::new(GpuVendor::Nvidia, 0x2684, 0x1458, 0x40a7, 1);
    DriverPlanInput {
        target_gpu: target_gpu.clone(),
        installed_packages: vec![PublishedDriverPackage {
            target_gpu: target_gpu.clone(),
            published_name: OemPublishedName::parse("oem12.inf").expect("OEM name"),
            original_inf_name: "nv_disp.inf".into(),
            provider_name: "NVIDIA Corporation".into(),
            driver_version: "580.1".into(),
            extensions: BTreeMap::new(),
        }],
        artifact: SignedArtifactDescriptor {
            locator: ArtifactLocator {
                artifact_id: "nvidia-580-1".into(),
                artifact_file_name: "driver-package.exe".into(),
                extensions: BTreeMap::new(),
            },
            target_gpu,
            payload_sha256: Sha256Digest::parse("a".repeat(64)).expect("digest"),
            authenticode: AuthenticodeEvidence {
                status: AuthenticodeStatus::Valid,
                signer_subject: "CN=NVIDIA Corporation".into(),
                signer_thumbprint_sha256: Sha256Digest::parse("b".repeat(64)).expect("thumbprint"),
                observed_at_utc: "2026-08-10T10:00:00Z".into(),
                extensions: BTreeMap::new(),
            },
            extensions: BTreeMap::new(),
        },
        extensions: BTreeMap::new(),
    }
}

#[test]
fn driver_plan_is_in_process_read_only_and_ordered() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let input = directory.path().join("driver-input.json");
    fs::write(
        &input,
        serde_json::to_vec(&driver_plan_input()).expect("driver input JSON"),
    )
    .expect("driver input fixture");
    let output = Command::new(binary())
        .args(["driver", "plan", "--input"])
        .arg(&input)
        .output()
        .expect("run");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("driver plan JSON");
    assert_eq!(value["readOnly"], true);
    assert_eq!(value["entries"][0]["step"], "P1:18");
    assert_eq!(value["entries"][3]["step"], "P3:1");
    assert_eq!(
        fs::read_dir(directory.path()).expect("directory").count(),
        1
    );
}

#[test]
fn driver_plan_rejects_untrusted_evidence_without_side_effects() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let input = directory.path().join("driver-input.json");
    let mut request = driver_plan_input();
    request.artifact.authenticode.status = AuthenticodeStatus::Indeterminate;
    fs::write(
        &input,
        serde_json::to_vec(&request).expect("driver input JSON"),
    )
    .expect("driver input fixture");
    let output = Command::new(binary())
        .args(["driver", "plan", "--input"])
        .arg(&input)
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert!(String::from_utf8_lossy(&output.stderr).contains("valid signature observation"));
    assert_eq!(
        fs::read_dir(directory.path()).expect("directory").count(),
        1
    );
}

#[test]
fn invalid_dry_run_branch_is_usage_error_without_stdout() {
    let output = Command::new(binary())
        .args(["dry-run", "5"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid value '5'"));
}

#[test]
fn cleanup_help_exposes_the_separate_full_irreversibility_acknowledgement() {
    let output = Command::new(binary())
        .args(["cleanup", "--help"])
        .output()
        .expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--yes"));
    assert!(stdout.contains("--acknowledge-irreversible"));
}

#[test]
fn cleanup_rejects_missing_consent_before_any_platform_adapter() {
    let missing_yes = Command::new(binary())
        .args(["cleanup", "quick"])
        .output()
        .expect("run");
    assert_eq!(missing_yes.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&missing_yes.stdout), "");
    assert!(String::from_utf8_lossy(&missing_yes.stderr).contains("cleanup requires --yes"));

    let missing_full_ack = Command::new(binary())
        .args(["cleanup", "full", "--yes"])
        .output()
        .expect("run");
    assert_eq!(missing_full_ack.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&missing_full_ack.stdout), "");
    assert!(
        String::from_utf8_lossy(&missing_full_ack.stderr)
            .contains("requires --acknowledge-irreversible after --yes")
    );

    let driver_without_yes = Command::new(binary())
        .args(["cleanup", "driver"])
        .output()
        .expect("run");
    assert_eq!(driver_without_yes.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&driver_without_yes.stdout), "");
    assert!(String::from_utf8_lossy(&driver_without_yes.stderr).contains("cleanup requires --yes"));
}

#[test]
fn safe_mode_handoff_rejects_missing_consent_before_any_platform_adapter() {
    let output = Command::new(binary())
        .arg("boot-safe-mode")
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert!(String::from_utf8_lossy(&output.stderr).contains("boot-safe-mode requires --yes"));
}

#[cfg(not(windows))]
#[test]
fn verify_is_read_only_and_informational_on_an_unsupported_host() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let output = Command::new(binary())
        .arg("verify")
        .current_dir(directory.path())
        .output()
        .expect("run");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"readOnly\": true"));
    assert!(stdout.contains("\"status\": \"INFO\""));
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("read temporary directory")
            .count(),
        0
    );
}

#[cfg(not(windows))]
#[test]
fn reboot_commands_fail_closed_without_creating_artifacts_on_unsupported_hosts() {
    let directory = tempfile::tempdir().expect("temporary directory");
    for command in [
        vec!["optimize"],
        vec!["boot-safe-mode", "--yes"],
        vec!["phase2"],
        vec!["phase3"],
        vec!["phase3-handoff"],
    ] {
        let output = Command::new(binary())
            .args(&command)
            .current_dir(directory.path())
            .output()
            .expect("run");
        assert!(
            !output.status.success(),
            "{} unexpectedly succeeded",
            command.join(" ")
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("require x64 Windows 10 or 11"));
    }
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("read temporary directory")
            .count(),
        0,
        "reboot command created a persistent artifact"
    );
}

#[cfg(not(windows))]
#[test]
fn remaining_live_commands_fail_closed_without_creating_artifacts() {
    let directory = tempfile::tempdir().expect("temporary directory");
    for command in [
        vec!["configure", "safe", "--dry-run", "true"],
        vec!["cleanup", "quick", "--yes"],
        vec!["restore", "--yes"],
        vec!["backup-summary"],
        vec!["reset-progress", "--yes"],
        vec!["show-log"],
    ] {
        let output = Command::new(binary())
            .args(&command)
            .current_dir(directory.path())
            .output()
            .expect("run");
        assert_eq!(output.status.code(), Some(1), "{}", command.join(" "));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("live commands require x64 Windows 10 or 11"),
            "{}",
            command.join(" ")
        );
    }
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("read temporary directory")
            .count(),
        0,
        "unsupported-host live command created a persistent artifact"
    );
}

#[test]
fn invalid_fps_arguments_have_stable_usage_exit_and_empty_stdout() {
    let output = Command::new(binary())
        .args(["fps-cap", "240", "--minimum", "12"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "--minimum must be between 30 and 500"
    );
}

#[test]
fn manual_fps_cap_uses_the_nested_floor_formula_without_persistence() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let output = Command::new(binary())
        .args(["fps-cap", "240", "--no-persist"])
        .current_dir(directory.path())
        .output()
        .expect("run");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Recommended fps_max: 219"));
    assert!(stdout.contains("floor(avg - floor(avg * reduction))"));
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("read temporary directory")
            .count(),
        0,
        "manual no-persist calculation created an artifact"
    );
}

#[test]
fn vprof_text_reports_average_p1_ratio_and_run_count_without_stderr() {
    let output = Command::new(binary())
        .args([
            "fps-cap",
            "--vprof-text",
            "[VProf] FPS: Avg=300.0, P1=150.0\n[VProf] FPS: Avg=200.0, P1=100.0",
            "--no-persist",
        ])
        .output()
        .expect("run");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Recommended fps_max: 228"));
    assert!(stdout.contains("Average FPS: 250.0; P1 FPS: 125.0; P1 ratio: 0.500; Runs: 2"));
}

#[test]
fn baseline_benchmark_accepts_only_vprof_sources_and_fails_closed_off_windows() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let positional = Command::new(binary())
        .args(["baseline-benchmark", "240"])
        .current_dir(directory.path())
        .output()
        .expect("run");
    assert_eq!(positional.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&positional.stdout), "");

    let output = Command::new(binary())
        .args([
            "baseline-benchmark",
            "--vprof-text",
            "[VProf] FPS: Avg=300.0, P1=150.0",
        ])
        .current_dir(directory.path())
        .output()
        .expect("run");
    #[cfg(not(windows))]
    {
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("only supported on Windows"));
        assert_eq!(
            std::fs::read_dir(directory.path())
                .expect("read temporary directory")
                .count(),
            0,
            "baseline command created an artifact on an unsupported host"
        );
    }
    #[cfg(windows)]
    assert!(
        !output.status.success(),
        "Windows integration needs a protected test root"
    );
}

#[test]
fn final_benchmark_accepts_only_vprof_sources_and_fails_closed_off_windows() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let positional = Command::new(binary())
        .args(["final-benchmark", "240"])
        .current_dir(directory.path())
        .output()
        .expect("run");
    assert_eq!(positional.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&positional.stdout), "");

    let output = Command::new(binary())
        .args([
            "final-benchmark",
            "--vprof-text",
            "[VProf] FPS: Avg=300.0, P1=150.0",
        ])
        .current_dir(directory.path())
        .output()
        .expect("run");
    #[cfg(not(windows))]
    {
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("only supported on Windows"));
        assert_eq!(
            std::fs::read_dir(directory.path())
                .expect("read temporary directory")
                .count(),
            0,
            "final benchmark command created an artifact on an unsupported host"
        );
    }
    #[cfg(windows)]
    assert!(
        !output.status.success(),
        "Windows integration needs a protected test root"
    );
}
