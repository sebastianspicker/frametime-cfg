use std::fs;

use super::*;
use crate::{RebootStage, RebootTransaction, RuntimeRecord};

const ID: &str = "0123456789abcdef0123456789abcdef";
const RECEIPT_ID: &str = "fedcba9876543210fedcba9876543210";
const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn armed_state() -> State {
    State {
        active_reboot_transaction: Some(RebootTransaction {
            schema_version: 1,
            transaction_id: Some(TransactionId::parse(ID).expect("transaction id")),
            initiator_user_sid: Some("S-1-5-21-1".into()),
            stage: RebootStage::PhaseThreeArmed,
            runtime: Some(RuntimeRecord {
                generation: ID.into(),
                manifest_sha256: HASH.into(),
                payload_contract_hash: HASH.into(),
                executable_path: "frametime.exe".into(),
                executable_sha256: HASH.into(),
                unknown: BTreeMap::new(),
            }),
            driver_package: None,
            created_utc: None,
            updated_utc: None,
            unknown: BTreeMap::new(),
        }),
        ..State::default()
    }
}

fn checked_config() -> Config {
    toml::from_str(include_str!("../../../../frametime.toml")).expect("checked-in config")
}

fn progress_before_final_benchmark() -> Progress {
    let mut progress = Progress::default();
    progress.complete(3, 1, "2026-08-10 12:00:00".into());
    for step in 2..13 {
        progress.skip(3, step);
    }
    progress
}

fn record(value: usize) -> BenchmarkRecord {
    BenchmarkRecord {
        timestamp: format!("2026-08-10 12:{value:02}:00"),
        avg_fps: value as f64,
        p1_fps: value as f64 / 2.0,
        label: "test".into(),
        runs: 1,
        receipt_id: None,
        transaction_id: None,
        unknown: BTreeMap::new(),
    }
}

#[test]
fn malformed_history_is_empty_and_append_retains_newest_200() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("benchmark_history.json");
    fs::write(&path, "not json").expect("fixture");
    assert!(load_benchmark_history(&path, false).is_empty());
    for value in 0..=MAX_BENCHMARK_HISTORY {
        append_benchmark_record(&path, record(value), false).expect("append");
    }
    let history = load_benchmark_history(&path, false);
    assert_eq!(history.len(), MAX_BENCHMARK_HISTORY);
    assert_eq!(history[0].avg_fps, 1.0);
}

#[test]
fn dry_run_does_not_read_or_write() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("benchmark_history.json");
    let result = append_benchmark_record(&path, record(1), true).expect("dry run");
    assert!(result.is_empty());
    assert!(!path.exists());
}

#[test]
fn singleton_object_is_normalized_to_history_array() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("benchmark_history.json");
    fs::write(
        &path,
        r#"{"timestamp":"t","avgFps":100,"p1Fps":50,"label":"one"}"#,
    )
    .expect("fixture");
    let history = load_benchmark_history(&path, false);
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].runs, 1);
}

#[test]
fn final_receipt_requires_a_complete_vprof_capture_and_fixed_identity() {
    let receipt_id = TransactionId::parse("fedcba9876543210fedcba9876543210").expect("receipt id");
    let transaction_id = TransactionId::parse(ID).expect("transaction id");
    let receipt = FinalBenchmarkReceipt::new(
        receipt_id,
        transaction_id,
        "2026-08-10 12:34:56".into(),
        BenchmarkCapture {
            average_fps: 300.0,
            p1_fps: 180.0,
            runs: 3,
        },
        273,
    )
    .expect("valid receipt");
    assert_eq!(receipt.label, FINAL_BENCHMARK_LABEL);
    let history = BenchmarkRecord {
        timestamp: receipt.captured_utc.clone(),
        avg_fps: receipt.avg_fps,
        p1_fps: receipt.p1_fps,
        label: receipt.label.clone(),
        runs: receipt.runs,
        receipt_id: Some(receipt.receipt_id.clone()),
        transaction_id: Some(receipt.transaction_id.clone()),
        unknown: BTreeMap::new(),
    };
    assert!(receipt.matches_history_record(&history));

    let mut invalid = receipt.clone();
    invalid.p1_fps = 0.0;
    assert!(invalid.validate().is_err());
    invalid = receipt.clone();
    invalid.runs = 0;
    assert!(invalid.validate().is_err());
    invalid = receipt.clone();
    invalid.label = "Manual benchmark".into();
    assert!(invalid.validate().is_err());
    invalid = receipt.clone();
    invalid.captured_utc = "not-a-timestamp".into();
    assert!(invalid.validate().is_err());

    let transaction = RebootTransaction {
        transaction_id: Some(TransactionId::parse(ID).expect("transaction id")),
        stage: RebootStage::PhaseThreeArmed,
        ..RebootTransaction::default()
    };
    receipt
        .validate_for_transaction(&transaction)
        .expect("matching transaction id");
    let mismatched = RebootTransaction {
        transaction_id: Some(
            TransactionId::parse("fedcba9876543210fedcba9876543210").expect("other id"),
        ),
        ..transaction
    };
    assert!(receipt.validate_for_transaction(&mismatched).is_err());
}

#[test]
fn baseline_commit_is_complete_and_does_not_touch_fps_cap_or_final_receipt() {
    let capture = BenchmarkCapture {
        average_fps: 300.0,
        p1_fps: 180.0,
        runs: 3,
    };
    let commit = prepare_baseline_benchmark_commit(
        &State::default(),
        &Progress::default(),
        &[],
        "2026-08-10 12:34:56".into(),
        capture,
    )
    .expect("baseline commit");
    assert_eq!(commit.state.baseline_avg, 300.0);
    assert_eq!(commit.state.baseline_p1, Some(180.0));
    assert_eq!(commit.state.fps_cap, 0);
    assert_eq!(commit.state.avg_fps, 0.0);
    assert!(commit.state.final_benchmark.is_none());
    assert_eq!(commit.history.len(), 1);
    assert_eq!(commit.history[0].label, BASELINE_BENCHMARK_LABEL);
    assert_eq!(commit.progress.timestamps["1-17"], "2026-08-10 12:34:56");
    assert_eq!(
        validate_persisted_baseline_benchmark(&commit.state, &commit.progress, &commit.history)
            .expect("coherent persisted baseline")
            .runs,
        3
    );
}

#[test]
fn baseline_retry_reconciles_only_an_exact_history_prefix() {
    let capture = BenchmarkCapture {
        average_fps: 240.0,
        p1_fps: 120.0,
        runs: 2,
    };
    let first = prepare_baseline_benchmark_commit(
        &State::default(),
        &Progress::default(),
        &[],
        "2026-08-10 12:34:56".into(),
        capture,
    )
    .expect("initial baseline");
    let repaired = prepare_baseline_benchmark_commit(
        &State::default(),
        &Progress::default(),
        &first.history,
        "2026-08-10 12:35:56".into(),
        capture,
    )
    .expect("exact retry repairs history prefix");
    assert_eq!(repaired.captured_utc, "2026-08-10 12:34:56");
    assert!(repaired.progress.completed_steps.contains("P1:17"));
    assert!(
        prepare_baseline_benchmark_commit(
            &State::default(),
            &Progress::default(),
            &first.history,
            "2026-08-10 12:35:56".into(),
            BenchmarkCapture {
                p1_fps: 119.0,
                ..capture
            },
        )
        .is_err()
    );
}

#[test]
fn baseline_rejects_skipped_or_incomplete_capture() {
    let mut skipped = Progress::default();
    skipped.skip(1, 17);
    let invalid = BenchmarkCapture {
        average_fps: 100.0,
        p1_fps: 0.0,
        runs: 1,
    };
    assert!(
        prepare_baseline_benchmark_commit(
            &State::default(),
            &Progress::default(),
            &[],
            "2026-08-10 12:34:56".into(),
            invalid,
        )
        .is_err()
    );
    assert!(
        prepare_baseline_benchmark_commit(
            &State::default(),
            &skipped,
            &[],
            "2026-08-10 12:34:56".into(),
            BenchmarkCapture {
                p1_fps: 50.0,
                ..invalid
            },
        )
        .is_err()
    );
}

#[test]
fn final_commit_is_transaction_bound_and_completes_one_coherent_bundle() {
    let progress = progress_before_final_benchmark();
    let commit = prepare_final_benchmark_commit(
        &armed_state(),
        &progress,
        &[],
        &checked_config(),
        TransactionId::parse(RECEIPT_ID).expect("receipt id"),
        "2026-08-10 12:34:56".into(),
        BenchmarkCapture {
            average_fps: 300.0,
            p1_fps: 180.0,
            runs: 3,
        },
    )
    .expect("final commit");
    assert_eq!(commit.receipt.fps_cap, 273);
    assert!(commit.progress.completed_steps.contains("P3:13"));
    assert!(!commit.progress.skipped_steps.contains("P3:13"));
    assert!(matches!(
        commit
            .state
            .active_reboot_transaction
            .as_ref()
            .map(|transaction| &transaction.stage),
        Some(RebootStage::PhaseThreeComplete)
    ));
    assert_eq!(commit.state.final_benchmark, Some(commit.receipt.clone()));
    assert!(
        commit
            .receipt
            .matches_history_record(commit.history.last().expect("final history record"))
    );
    assert_eq!(
        validate_persisted_final_benchmark(&commit.state, &commit.progress, &commit.history),
        Ok(commit.receipt)
    );
}

#[test]
fn final_commit_rejects_advisory_or_replayed_inputs() {
    let mut progress = Progress::default();
    let request = |state: &State, progress: &Progress, capture| {
        prepare_final_benchmark_commit(
            state,
            progress,
            &[],
            &checked_config(),
            TransactionId::parse(RECEIPT_ID).expect("receipt id"),
            "2026-08-10 12:34:56".into(),
            capture,
        )
    };
    let capture = BenchmarkCapture {
        average_fps: 300.0,
        p1_fps: 180.0,
        runs: 3,
    };
    assert!(request(&armed_state(), &progress, capture).is_err());
    progress = progress_before_final_benchmark();
    assert!(
        request(
            &armed_state(),
            &progress,
            BenchmarkCapture {
                p1_fps: 0.0,
                ..capture
            }
        )
        .is_err()
    );
    progress.complete(3, 13, "2026-08-10 12:34:56".into());
    assert!(request(&armed_state(), &progress, capture).is_err());
}

#[test]
fn persisted_final_bundle_rejects_partial_or_duplicated_evidence() {
    let commit = prepare_final_benchmark_commit(
        &armed_state(),
        &progress_before_final_benchmark(),
        &[],
        &checked_config(),
        TransactionId::parse(RECEIPT_ID).expect("receipt id"),
        "2026-08-10 12:34:56".into(),
        BenchmarkCapture {
            average_fps: 300.0,
            p1_fps: 180.0,
            runs: 3,
        },
    )
    .expect("final commit");
    let mut partial = commit.progress.clone();
    partial.completed_steps.remove("P3:13");
    assert!(validate_persisted_final_benchmark(&commit.state, &partial, &commit.history).is_err());
    let mut duplicate = commit.history.clone();
    duplicate.push(duplicate[0].clone());
    assert!(
        validate_persisted_final_benchmark(&commit.state, &commit.progress, &duplicate).is_err()
    );
    let mut mismatched = commit.state.clone();
    mismatched.fps_cap += 1;
    assert!(
        validate_persisted_final_benchmark(&mismatched, &commit.progress, &commit.history).is_err()
    );
    let mut unresolved = commit.progress.clone();
    unresolved.skipped_steps.remove("P3:12");
    assert!(
        validate_persisted_final_benchmark(&commit.state, &unresolved, &commit.history).is_err()
    );
    let mut wrong_timestamp = commit.progress.clone();
    wrong_timestamp
        .timestamps
        .insert("3-13".into(), "2026-08-10 12:34:57".into());
    assert!(
        validate_persisted_final_benchmark(&commit.state, &wrong_timestamp, &commit.history)
            .is_err()
    );
}
