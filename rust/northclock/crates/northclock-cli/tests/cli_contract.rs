#![forbid(unsafe_code)]

use serde_json::Value;
use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_northclock")
}

#[test]
fn doctor_uses_versioned_json_contract() {
    let output = Command::new(binary())
        .args(["--json", "doctor"])
        .output()
        .unwrap_or_else(|error| panic!("northclock did not run: {error}"));
    assert!(output.status.success());
    let envelope: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("invalid JSON output: {error}"));
    assert_eq!(envelope["schema_version"], "1.0");
    assert_eq!(envelope["command"], "doctor");
    assert_eq!(envelope["status"], "success");
    assert!(envelope["data"].is_array());
    assert!(envelope["error"].is_null());
    let capabilities = envelope["data"]
        .as_array()
        .unwrap_or_else(|| panic!("doctor data was not an array"));
    for required in [
        "cpu.ryzen_telemetry",
        "gpu.adlx_telemetry",
        "driver.kmdf_runtime",
        "windows.task_scheduler",
        "windows.vbs_status",
        "windows.conflict_detection",
        "windows.system_status",
    ] {
        let capability = capabilities
            .iter()
            .find(|capability| capability["name"] == required)
            .unwrap_or_else(|| panic!("doctor omitted {required}"));
        assert!(capability["backend"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
        assert!(capability["detail"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
        assert_eq!(capability["hardware_verified"], false);
    }
}

#[test]
fn unavailable_platform_capability_exits_three() {
    if cfg!(windows) {
        return;
    }
    let output = Command::new(binary())
        .args(["--json", "cpu", "identity"])
        .output()
        .unwrap_or_else(|error| panic!("northclock did not run: {error}"));
    assert_eq!(output.status.code(), Some(3));
    let envelope: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("invalid JSON output: {error}"));
    assert_eq!(envelope["status"], "unavailable");
    assert!(envelope["data"].is_null());
}

#[test]
fn system_status_is_explicitly_unavailable_off_windows() {
    if cfg!(windows) {
        return;
    }
    let output = Command::new(binary())
        .args(["--json", "system", "status"])
        .output()
        .unwrap_or_else(|error| panic!("northclock did not run: {error}"));
    assert_eq!(output.status.code(), Some(3));
    let envelope: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("invalid JSON output: {error}"));
    assert_eq!(envelope["command"], "system.status");
    assert_eq!(envelope["status"], "unavailable");
    assert_eq!(envelope["capability"]["name"], "windows.system_status");
}

#[test]
fn bounded_memory_test_reports_measured_source() {
    let output = Command::new(binary())
        .args([
            "--json",
            "memory",
            "system-test",
            "--bytes",
            "1048576",
            "--passes",
            "1",
        ])
        .output()
        .unwrap_or_else(|error| panic!("northclock did not run: {error}"));
    assert!(output.status.success());
    let envelope: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("invalid JSON output: {error}"));
    assert_eq!(
        envelope["data"]["throughput"]["source"],
        "northclock-bounded-memory-workload"
    );
    assert!(envelope["data"]["whea_correlation"]["status"].is_string());
    assert!(!envelope.to_string().contains("synthetic"));
}

#[test]
fn cpu_workload_reports_validated_throughput() {
    let output = Command::new(binary())
        .args([
            "--json",
            "cpu",
            "workload",
            "--duration-ms",
            "5",
            "--threads",
            "2",
        ])
        .output()
        .unwrap_or_else(|error| panic!("northclock did not run: {error}"));
    assert!(output.status.success());
    let envelope: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("invalid JSON output: {error}"));
    assert_eq!(envelope["data"]["requested_duration_ms"], 5);
    assert_eq!(envelope["data"]["threads"], 2);
    assert_eq!(envelope["data"]["validation_errors"], 0);
    assert!(envelope["data"]["validation_checks"]
        .as_u64()
        .is_some_and(|value| value > 0));
    assert!(envelope["data"]["iterations_per_second"]
        .as_f64()
        .is_some_and(|value| value.is_finite() && value > 0.0));
    assert_eq!(envelope["data"]["hardware_verified"], false);
}

#[test]
fn removed_unsafe_flash_surface_is_invalid_usage() {
    let output = Command::new(binary())
        .arg("--amdvbflash")
        .output()
        .unwrap_or_else(|error| panic!("northclock did not run: {error}"));
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn exact_legacy_alias_warns_and_maps_to_doctor() {
    let output = Command::new(binary())
        .args(["--json", "--vendor"])
        .output()
        .unwrap_or_else(|error| panic!("northclock did not run: {error}"));
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("deprecated"));
    let envelope: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("invalid JSON output: {error}"));
    assert_eq!(envelope["command"], "doctor");
}
