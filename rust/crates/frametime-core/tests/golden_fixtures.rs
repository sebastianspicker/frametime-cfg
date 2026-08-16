use std::{collections::BTreeSet, fs, path::Path};

use frametime_core::{
    BackupEntry, BackupFile, FinalBenchmarkReceipt, Progress, RebootStage, RebootTransaction,
    State,
    benchmark::load_benchmark_history,
    cs2::{ensure_autoexec_line, render_optimization_cfg_at},
    latency::load_latency_history,
    persistence::safe_relative_path,
    runtime::{RuntimeCurrent, load_selected_generation},
};
use serde_json::Value;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/golden");

fn fixture(name: &str) -> String {
    fs::read_to_string(Path::new(FIXTURES).join(name)).expect("golden fixture")
}

#[test]
fn legacy_state_and_progress_round_trip_unknown_and_null_fields() {
    let state: State = serde_json::from_str(&fixture("state-legacy.json")).expect("state");
    assert_eq!(state.profile, frametime_core::policy::Profile::Competitive);
    assert_eq!(state.mode, "CONTROL");
    assert_eq!(state.pagefile_mb, 4096);
    assert_eq!(state.unknown["futureState"]["schema"], 9);
    assert!(state.unknown["nullableState"].is_null());
    state.validate().expect("legacy state validates");
    let state_value = serde_json::to_value(state).expect("state JSON");
    assert_eq!(state_value["futureState"]["enabled"], true);
    assert!(state_value["nullableState"].is_null());
    assert_eq!(state_value["pagefileMB"], 4096);
    assert!(state_value.get("pagefileMb").is_none());

    let progress: Progress =
        serde_json::from_str(&fixture("progress-legacy.json")).expect("progress");
    assert_eq!(progress.phase, 2);
    assert_eq!(progress.last_completed_step, 3);
    assert_eq!(progress.last_skipped_step, 2);
    assert_eq!(
        progress.completed_steps,
        BTreeSet::from(["P1:1".to_owned(), "P2:3".to_owned()])
    );
    assert_eq!(progress.skipped_steps, BTreeSet::from(["P1:2".to_owned()]));
    assert_eq!(progress.timestamps["1-1"], "2026-08-10 10:00:00");
    assert_eq!(progress.unknown["futureProgress"]["attempt"], 4);
    assert!(progress.unknown["nullableProgress"].is_null());
    let progress_value = serde_json::to_value(progress).expect("progress JSON");
    assert_eq!(
        progress_value["completedSteps"].as_array().unwrap().len(),
        2
    );
    assert!(progress_value["nullableProgress"].is_null());
}

#[test]
fn final_benchmark_receipt_golden_is_valid_and_unknown_tolerant() {
    let raw: Value =
        serde_json::from_str(&fixture("final-benchmark-receipt.json")).expect("raw receipt");
    let receipt: FinalBenchmarkReceipt =
        serde_json::from_value(raw.clone()).expect("final benchmark receipt");
    receipt.validate().expect("valid final benchmark receipt");
    assert_eq!(receipt.avg_fps, 300.0);
    assert_eq!(receipt.p1_fps, 180.0);
    assert_eq!(receipt.runs, 3);
    assert_eq!(receipt.fps_cap, 273);
    assert_eq!(receipt.unknown["futureReceipt"]["retained"], true);
    assert_eq!(serde_json::to_value(receipt).expect("receipt JSON"), raw);
}

#[test]
fn legacy_backup_round_trip_covers_all_eleven_types_and_unknown_values() {
    let raw: Value = serde_json::from_str(&fixture("backup-legacy.json")).expect("raw backup");
    let backup: BackupFile = serde_json::from_value(raw.clone()).expect("backup");
    assert_eq!(backup.entries.len(), 12);
    assert_eq!(backup.unknown["futureBackup"]["schema"], 3);
    assert!(backup.unknown["nullableBackup"].is_null());

    let mut kinds = BTreeSet::new();
    let mut unknown_entry = false;
    for entry in &backup.entries {
        assert_legacy_backup_entry(entry, &mut kinds, &mut unknown_entry);
    }
    assert_eq!(
        kinds,
        BTreeSet::from([
            "registry",
            "service",
            "powerplan",
            "bootconfig",
            "scheduledtask",
            "nic_adapter",
            "qos_uro",
            "defender",
            "pagefile",
            "dns",
            "drs",
        ])
    );
    assert!(unknown_entry);

    let round_trip = serde_json::to_value(backup).expect("backup JSON");
    assert_eq!(round_trip["entries"].as_array().unwrap().len(), 12);
    assert!(round_trip["nullableBackup"].is_null());
    assert!(round_trip["entries"][0]["futureRegistry"].is_null());
}

fn assert_legacy_backup_entry(
    entry: &BackupEntry,
    kinds: &mut BTreeSet<&'static str>,
    unknown_entry: &mut bool,
) {
    match entry {
        BackupEntry::Registry { unknown, .. } => {
            kinds.insert("registry");
            assert!(unknown["futureRegistry"].is_null());
        }
        BackupEntry::Service { unknown, .. } => {
            kinds.insert("service");
            assert_eq!(unknown["futureService"]["keep"], true);
        }
        BackupEntry::Powerplan { unknown, .. } => {
            kinds.insert("powerplan");
            assert!(unknown["futurePowerplan"].is_null());
        }
        BackupEntry::Bootconfig { unknown, .. } => {
            kinds.insert("bootconfig");
            assert!(unknown["futureBootconfig"].is_null());
        }
        BackupEntry::Scheduledtask {
            script_path,
            unknown,
            ..
        } => {
            kinds.insert("scheduledtask");
            assert!(script_path.is_none());
            assert_eq!(unknown["futureScheduledtask"]["version"], 2);
        }
        BackupEntry::NicAdapter { unknown, .. } => {
            kinds.insert("nic_adapter");
            assert!(unknown["futureNic"].is_null());
        }
        BackupEntry::QosUro { unknown, .. } => {
            kinds.insert("qos_uro");
            assert!(unknown["futureQos"].is_null());
        }
        BackupEntry::Defender { unknown, .. } => {
            kinds.insert("defender");
            assert!(unknown["futureDefender"].is_null());
        }
        BackupEntry::Pagefile { unknown, .. } => {
            kinds.insert("pagefile");
            assert_eq!(unknown["futurePagefile"]["unit"], "MB");
        }
        BackupEntry::Dns { unknown, .. } => {
            kinds.insert("dns");
            assert!(unknown["futureDns"].is_null());
        }
        BackupEntry::Drs { unknown, .. } => {
            kinds.insert("drs");
            assert_eq!(unknown["futureDrs"]["driver"], "test");
        }
        BackupEntry::Unknown(value) => {
            *unknown_entry = true;
            assert_eq!(value["type"], "future_backup_type");
            assert!(value["nullablePayload"].is_null());
        }
        BackupEntry::PagefileTransaction { .. } => {
            panic!("legacy fixture must not contain a pagefile transaction")
        }
        BackupEntry::Hags { .. } => panic!("legacy fixture must not contain a HAGS transaction"),
        BackupEntry::InterruptPolicy { .. } => {
            panic!("legacy fixture must not contain an interrupt-policy transaction")
        }
        BackupEntry::NetworkStackTransaction { .. } => {
            panic!("legacy fixture must not contain a network-stack transaction")
        }
        BackupEntry::Cs2ConfigTransaction { .. } => {
            panic!("legacy fixture must not contain a CS2 CFG transaction")
        }
    }
}

#[test]
fn pagefile_transaction_fixture_preserves_order_and_unknown_values() {
    let raw: Value = serde_json::from_str(&fixture("backup-pagefile-transaction.json"))
        .expect("raw pagefile transaction backup");
    let backup: BackupFile = serde_json::from_value(raw.clone()).expect("pagefile transaction");
    assert_eq!(backup.unknown["futureBackupRoot"]["retained"], true);
    assert!(matches!(backup.entries[1], BackupEntry::Unknown(_)));

    let BackupEntry::PagefileTransaction {
        step,
        timestamp,
        automatic_managed,
        target_path,
        target_existed,
        settings,
        unknown,
        ..
    } = &backup.entries[0]
    else {
        panic!("expected pagefile_transaction");
    };
    assert_eq!(step, "P1:8");
    assert_eq!(timestamp, "2026-08-10T10:00:08.123+00:00");
    assert!(!automatic_managed);
    assert_eq!(target_path, r"C:\Page Files\pagefile.sys");
    assert!(*target_existed);
    assert_eq!(unknown["futureTransaction"]["generation"], 2);
    assert_eq!(settings.len(), 2);
    assert_eq!(settings[0].path, r"C:\Page Files\pagefile.sys");
    assert_eq!(settings[0].unknown["futureSetting"], "keep");
    assert_eq!(settings[1].path, r"D:\swapfile.sys");

    assert_eq!(serde_json::to_value(backup).expect("backup JSON"), raw);
}

#[test]
fn runtime_selector_and_manifest_fixture_are_verified_and_path_safe() {
    const GENERATION: &str = "0123456789abcdef0123456789abcdef";
    assert!(safe_relative_path(Path::new(
        "runtime-generations/0123456789abcdef0123456789abcdef/payload.txt"
    )));
    assert!(!safe_relative_path(Path::new("../payload.txt")));
    assert!(!safe_relative_path(Path::new("/payload.txt")));

    let source = Path::new(FIXTURES).join("runtime");
    let temporary = tempfile::tempdir().expect("runtime directory");
    let generation = temporary
        .path()
        .join("runtime-generations")
        .join(GENERATION);
    fs::create_dir_all(&generation).expect("generation");
    fs::copy(
        source
            .join("runtime-generations")
            .join(GENERATION)
            .join("payload.txt"),
        generation.join("payload.txt"),
    )
    .expect("payload");
    fs::copy(
        source.join("runtime-current.json"),
        temporary.path().join("runtime-current.json"),
    )
    .expect("selector");
    fs::copy(
        source
            .join("runtime-generations")
            .join(GENERATION)
            .join("runtime-manifest.json"),
        generation.join("runtime-manifest.json"),
    )
    .expect("manifest");

    let (selected, manifest) = load_selected_generation(temporary.path()).expect("selected");
    assert_eq!(selected.file_name().unwrap(), GENERATION);
    assert_eq!(manifest.generation, GENERATION);
    assert_eq!(manifest.unknown["futureManifest"]["retained"], true);
    let current: RuntimeCurrent =
        serde_json::from_str(&fixture("runtime/runtime-current.json")).expect("selector");
    assert!(current.unknown["futureSelector"].is_null());
    assert_eq!(
        manifest.files.keys().collect::<Vec<_>>(),
        vec!["payload.txt"]
    );
}

#[test]
fn reboot_transaction_golden_is_typed_forward_compatible_and_authorizable() {
    let raw: Value = serde_json::from_str(&fixture("reboot-transaction.json")).expect("raw");
    let transaction: RebootTransaction = serde_json::from_value(raw.clone()).expect("transaction");
    assert!(transaction.is_authorized_at(&RebootStage::PhaseOneSafeModeArmed));
    assert_eq!(
        transaction.initiator_user_sid.as_deref(),
        Some("S-1-5-21-1")
    );
    assert!(transaction.runtime.as_ref().unwrap().unknown["futureRuntime"].is_null());
    assert_eq!(
        transaction.driver_package.as_ref().unwrap().unknown["futureDriver"]["retain"],
        true
    );
    assert_eq!(transaction.unknown["futureTransaction"], "retained");
    assert_eq!(serde_json::to_value(transaction).expect("round trip"), raw);
}

#[test]
fn benchmark_array_and_singleton_fixtures_are_normalized() {
    let temporary = tempfile::tempdir().expect("benchmark directory");
    let array_path = temporary.path().join("array.json");
    let singleton_path = temporary.path().join("singleton.json");
    fs::write(&array_path, fixture("benchmark-history-array.json")).expect("array");
    fs::write(&singleton_path, fixture("benchmark-history-singleton.json")).expect("singleton");

    let array = load_benchmark_history(&array_path, false);
    assert_eq!(array.len(), 2);
    assert_eq!(array[0].label, "baseline");
    assert_eq!(array[0].runs, 2);
    assert!(array[1].unknown["nullableBenchmark"].is_null());
    let singleton = load_benchmark_history(&singleton_path, false);
    assert_eq!(singleton.len(), 1);
    assert_eq!(singleton[0].label, "singleton");
    assert_eq!(singleton[0].runs, 1);
}

#[test]
fn generated_cs2_optimization_and_autoexec_match_goldens() {
    let generated = render_optimization_cfg_at("2026-08-10 10:08");
    let expected = fixture("optimization-generated.cfg");
    for (index, (actual, golden)) in generated.lines().zip(expected.lines()).enumerate() {
        assert_eq!(actual, golden, "optimization line differs at {index}");
    }
    assert_eq!(generated, expected);
    let generated = ensure_autoexec_line("// frametime.cfg golden autoexec\nbind x y\n");
    assert_eq!(generated, fixture("autoexec-generated.cfg"));
    assert_eq!(ensure_autoexec_line(&generated), generated);
}

#[test]
fn representative_dry_run_markers_are_stable() {
    let output = fixture("dry-run-output.txt");
    for marker in [
        "PHASE 1 PREVIEW COMPLETE",
        "PHASE 2 PREVIEW COMPLETE",
        "PHASE 3 PREVIEW COMPLETE",
        "ALL 3 PHASES PREVIEW COMPLETE",
        "ALL FOUR GPU BRANCH PREVIEWS COMPLETE",
    ] {
        assert!(output.contains(marker), "missing marker {marker}");
    }
}

#[test]
fn latency_history_fixture_round_trips_with_core_api() {
    let temporary = tempfile::tempdir().expect("latency directory");
    let path = temporary.path().join("latency-history.json");
    fs::write(&path, fixture("latency-history.json")).expect("latency fixture");
    let history = load_latency_history(&path).expect("latency history");
    assert_eq!(history.version, 1);
    assert_eq!(history.runs.len(), 1);
    assert_eq!(history.runs[0].kind, "baseline");
    assert_eq!(history.runs[0].results[0].avg_rtt_ms, Some(12.8));
    assert!(
        history.unknown["FutureHistoryField"]["retained"]
            .as_bool()
            .unwrap()
    );
    assert!(history.runs[0].unknown["FutureRunField"].is_null());
}
