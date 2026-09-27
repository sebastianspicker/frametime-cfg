use super::*;

const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
const RECEIPT_ID: &str = "fedcba9876543210fedcba9876543210";
const OTHER_RECEIPT_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const CAPTURED_UTC: &str = "2026-08-10 12:34:56";

fn checked_config() -> Config {
    Config::parse_bytes(include_bytes!("../../../../../../frametime.toml"))
        .expect("checked-in configuration")
}

fn capture() -> BenchmarkCapture {
    BenchmarkCapture {
        average_fps: 300.0,
        p1_fps: 180.0,
        runs: 3,
    }
}

fn receipt_id() -> TransactionId {
    TransactionId::parse(RECEIPT_ID).expect("valid receipt id")
}

fn armed_state() -> State {
    State {
        active_reboot_transaction: Some(RebootTransaction {
            schema_version: 1,
            transaction_id: Some(
                TransactionId::parse(TRANSACTION_ID).expect("valid transaction id"),
            ),
            initiator_user_sid: Some("S-1-5-21-1".into()),
            stage: RebootStage::PhaseThreeArmed,
            runtime: Some(RuntimeRecord {
                generation: TRANSACTION_ID.into(),
                manifest_sha256: SHA256.into(),
                payload_contract_hash: SHA256.into(),
                executable_path: "frametime.exe".into(),
                executable_sha256: SHA256.into(),
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

fn progress_before_final_benchmark() -> Progress {
    let mut progress = Progress::default();
    progress.complete(3, 1, "2026-08-10 12:00:00".into());
    for step in 2..13 {
        progress.skip(3, step);
    }
    progress
}

fn reconcile(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
    requested_capture: BenchmarkCapture,
) -> Result<FinalBenchmarkReconciliation, String> {
    reconcile_final_benchmark(
        FinalBenchmarkReconciliationInput {
            state,
            progress,
            history,
            config: &checked_config(),
            captured_utc: CAPTURED_UTC.into(),
            capture: requested_capture,
            run_evidence: None,
        },
        receipt_id(),
    )
}

fn pending_commit(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
) -> FinalBenchmarkCommit {
    let FinalBenchmarkReconciliation::Pending(commit) =
        reconcile(state, progress, history, capture()).expect("pending reconciliation")
    else {
        panic!("expected pending reconciliation");
    };
    *commit
}

fn evidenced_capture() -> ValidatedBenchmarkCapture {
    ValidatedBenchmarkCapture::new(
        BenchmarkRunEvidence::new(
            [180.01, 180.02, 180.03, 180.04, 180.05]
                .into_iter()
                .map(|p1_fps| frametime_domain::fps::BenchmarkObservation {
                    average_fps: 300.0,
                    p1_fps,
                })
                .collect(),
        )
        .expect("run evidence"),
    )
    .expect("validated capture")
}

fn reconcile_evidenced(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
    capture: &ValidatedBenchmarkCapture,
) -> FinalBenchmarkCommit {
    let FinalBenchmarkReconciliation::Pending(commit) = reconcile_final_benchmark(
        FinalBenchmarkReconciliationInput {
            state,
            progress,
            history,
            config: &checked_config(),
            captured_utc: CAPTURED_UTC.into(),
            capture: capture.aggregate(),
            run_evidence: Some(capture.run_evidence()),
        },
        receipt_id(),
    )
    .expect("evidenced reconciliation") else {
        panic!("expected pending reconciliation");
    };
    *commit
}

#[test]
fn complete_bundle_is_idempotent_without_generating_another_receipt() {
    let state = armed_state();
    let progress = progress_before_final_benchmark();
    let commit = pending_commit(&state, &progress, &[]);
    let input = FinalBenchmarkReconciliationInput {
        state: &commit.state,
        progress: &commit.progress,
        history: &commit.history,
        config: &checked_config(),
        captured_utc: CAPTURED_UTC.into(),
        capture: capture(),
        run_evidence: None,
    };
    let reconciliation = reconcile_final_benchmark(input, || -> Result<TransactionId, String> {
        panic!("complete reconciliation must not request another receipt id")
    })
    .expect("idempotent reconciliation");
    let FinalBenchmarkReconciliation::Complete(receipt) = reconciliation else {
        panic!("expected complete reconciliation");
    };
    assert_eq!(receipt, commit.receipt);
}

#[test]
fn history_only_prefix_is_repaired_with_its_original_receipt() {
    let state = armed_state();
    let progress = progress_before_final_benchmark();
    let commit = pending_commit(&state, &progress, &[]);
    let repaired = pending_commit(&state, &progress, &commit.history);
    assert_eq!(repaired, commit);
}

#[test]
fn evidenced_history_and_state_crash_prefixes_retry_exactly() {
    let state = armed_state();
    let progress = progress_before_final_benchmark();
    let capture = evidenced_capture();
    let commit = reconcile_evidenced(&state, &progress, &[], &capture);
    assert_eq!(
        commit.receipt.run_evidence.as_ref(),
        Some(capture.run_evidence())
    );
    assert_eq!(
        reconcile_evidenced(&state, &progress, &commit.history, &capture),
        commit
    );
    assert_eq!(
        reconcile_evidenced(&commit.state, &progress, &[], &capture),
        commit
    );
}

#[test]
fn every_evidenced_prefix_rejects_different_order_with_same_aggregate() {
    let state = armed_state();
    let progress = progress_before_final_benchmark();
    let capture = evidenced_capture();
    let commit = reconcile_evidenced(&state, &progress, &[], &capture);
    let mut reversed = capture.observations().to_vec();
    reversed.reverse();
    let reversed = ValidatedBenchmarkCapture::new(
        BenchmarkRunEvidence::new(reversed).expect("reversed evidence"),
    )
    .expect("reversed capture");
    assert_eq!(reversed.aggregate(), capture.aggregate());

    let cases = [
        (state.clone(), progress.clone(), commit.history.clone()),
        (commit.state.clone(), progress, Vec::new()),
        (
            commit.state.clone(),
            commit.progress.clone(),
            commit.history.clone(),
        ),
    ];
    for (case_state, case_progress, case_history) in cases {
        let result = reconcile_final_benchmark(
            FinalBenchmarkReconciliationInput {
                state: &case_state,
                progress: &case_progress,
                history: &case_history,
                config: &checked_config(),
                captured_utc: CAPTURED_UTC.into(),
                capture: reversed.aggregate(),
                run_evidence: Some(reversed.run_evidence()),
            },
            receipt_id(),
        );
        assert!(
            result.is_err(),
            "changed observation order must fail closed"
        );
    }
}

#[test]
fn state_only_prefix_is_repaired_with_its_original_receipt() {
    let state = armed_state();
    let progress = progress_before_final_benchmark();
    let commit = pending_commit(&state, &progress, &[]);
    let repaired = pending_commit(&commit.state, &progress, &[]);
    assert_eq!(repaired, commit);
}

#[test]
fn conflicting_prefixes_fail_closed() {
    let state = armed_state();
    let progress = progress_before_final_benchmark();
    let commit = pending_commit(&state, &progress, &[]);
    let mut conflict = commit.history.clone();
    conflict[0].receipt_id =
        Some(TransactionId::parse(OTHER_RECEIPT_ID).expect("valid receipt id"));
    let error = match reconcile(&commit.state, &progress, &conflict, capture()) {
        Err(error) => error,
        Ok(_) => panic!("conflicting prefixes must fail"),
    };
    assert_eq!(error, "final benchmark history and state prefixes disagree");
}

#[test]
fn skipped_completion_cannot_be_retried() {
    let state = armed_state();
    let mut progress = progress_before_final_benchmark();
    progress.skip(3, 13);
    let error = match reconcile(&state, &progress, &[], capture()) {
        Err(error) => error,
        Ok(_) => panic!("skipped progress must fail"),
    };
    assert_eq!(
        error,
        "final benchmark progress was skipped and cannot be retried"
    );
}

#[test]
fn completed_bundle_rejects_a_different_capture() {
    let state = armed_state();
    let progress = progress_before_final_benchmark();
    let commit = pending_commit(&state, &progress, &[]);
    let mismatched = BenchmarkCapture {
        p1_fps: 179.0,
        ..capture()
    };
    let error = match reconcile(&commit.state, &commit.progress, &commit.history, mismatched) {
        Err(error) => error,
        Ok(_) => panic!("capture mismatch must fail"),
    };
    assert_eq!(
        error,
        "completed final benchmark conflicts with the requested capture"
    );
}
