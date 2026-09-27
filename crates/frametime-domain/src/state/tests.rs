use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::record::MAX_PAGEFILE_MB;
use super::*;
use crate::{
    benchmark::FinalBenchmarkReceipt,
    catalog::{Phase, StepId},
    fps::BenchmarkCapture,
    handoff::{RebootStage, RebootTransaction, RuntimeRecord, TransactionId},
    policy::Profile,
};

const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
const RECEIPT_ID: &str = "fedcba9876543210fedcba9876543210";
const OTHER_TRANSACTION_ID: &str = "11111111111111111111111111111111";
const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn phase_three_transaction(stage: RebootStage) -> RebootTransaction {
    RebootTransaction {
        schema_version: 1,
        transaction_id: Some(TransactionId::parse(TRANSACTION_ID).expect("transaction id")),
        initiator_user_sid: Some("S-1-5-21-1".into()),
        stage,
        runtime: Some(RuntimeRecord {
            generation: TRANSACTION_ID.into(),
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
    }
}

fn final_receipt(transaction_id: &str) -> FinalBenchmarkReceipt {
    FinalBenchmarkReceipt::new(
        TransactionId::parse(RECEIPT_ID).expect("receipt id"),
        TransactionId::parse(transaction_id).expect("transaction id"),
        "2026-08-10 12:34:56".into(),
        BenchmarkCapture {
            average_fps: 300.0,
            p1_fps: 180.0,
            runs: 3,
        },
        273,
    )
    .expect("receipt")
}

#[test]
fn unknown_fields_survive_state_round_trip() {
    let raw = r#"{"mode":"CONTROL","profile":"SAFE","future":{"x":1}}"#;
    let state: State = serde_json::from_str(raw).expect("state");
    assert_eq!(state.unknown["future"]["x"], 1);
    let value: Value =
        serde_json::from_str(&serde_json::to_string(&state).expect("json")).expect("value");
    assert_eq!(value["future"]["x"], 1);
}

#[test]
fn typed_reboot_fields_are_tolerant_and_keep_other_extensions() {
    let state: State = serde_json::from_str(
        r#"{
              "phase1SafeModeReady":true,
              "activeRebootTransaction":{"schemaVersion":1,"stage":"future","futureTxn":null},
              "futureState":{"retained":true}
            }"#,
    )
    .expect("state");
    assert!(state.phase1_safe_mode_ready);
    assert_eq!(
        state.active_reboot_transaction.unwrap().unknown["futureTxn"],
        Value::Null
    );
    assert_eq!(state.unknown["futureState"]["retained"], true);

    let malformed: State = serde_json::from_str(
        r#"{"phase1SafeModeReady":"true","activeRebootTransaction":false,"future":1}"#,
    )
    .expect("state");
    assert!(!malformed.phase1_safe_mode_ready);
    assert!(malformed.active_reboot_transaction.is_none());
    assert_eq!(malformed.unknown["activeRebootTransaction"], false);
    assert_eq!(malformed.unknown["future"], 1);
    let round_trip = serde_json::to_value(malformed).expect("state round trip");
    assert_eq!(round_trip["activeRebootTransaction"], false);
}

#[test]
fn progress_keys_are_phase_qualified() {
    let mut progress = Progress::default();
    progress.complete(1, 5, "now".into());
    assert!(progress.completed_steps.contains("P1:5"));
    progress.skip(1, 5);
    assert!(!progress.completed_steps.contains("P1:5"));
    assert!(progress.skipped_steps.contains("P1:5"));
}

#[test]
fn malformed_known_fields_default_without_losing_unknown_fields() {
    let raw = r#"{"profile":"SAFE","logLevel":["VERBOSE"],"gpuInput":3,"future":true}"#;
    let state: State = serde_json::from_str(raw).expect("tolerant state");
    assert_eq!(state.profile, Profile::Safe);
    assert_eq!(state.mode, "AUTO");
    assert_eq!(state.log_level, "NORMAL");
    assert_eq!(state.gpu_input, None);
    assert_eq!(state.unknown["future"], true);
}

#[test]
fn progress_tolerates_malformed_known_fields_and_drops_unknown_schedule_keys() {
    let raw = r#"{
          "phase":"one",
          "lastCompletedStep":999,
          "completedSteps":["1","P1:2","P1:99",3],
          "skippedSteps":false,
          "timestamps":{"P1:2":"now","3":"bad","P2:1":4},
          "future":{"kept":true}
        }"#;
    let progress: Progress = serde_json::from_str(raw).expect("tolerant progress");
    assert_eq!(progress.phase, 0);
    assert_eq!(progress.last_completed_step, 0);
    assert_eq!(progress.completed_steps, BTreeSet::from(["P1:2".into()]));
    assert!(progress.skipped_steps.is_empty());
    assert_eq!(progress.timestamps["P1:2"], "now");
    assert_eq!(progress.unknown["future"]["kept"], true);
}

#[test]
fn typed_progress_helpers_preserve_the_existing_key_shape() {
    let mut progress = Progress::default();
    let id = StepId::new(Phase::One, 5);
    progress.complete_step(id, "now".into());
    assert!(progress.is_completed(id));
    assert_eq!(Progress::key_for(id), "P1:5");
    assert_eq!(progress.resolved_count_in_phase(Phase::One), 1);
}

#[test]
fn serialization_matches_legacy_acronyms_and_timestamp_keys() {
    let state = State {
        pagefile_mb: 4096,
        ..State::default()
    };
    let state_json = serde_json::to_value(state).expect("state");
    assert_eq!(state_json["pagefileMB"], 4096);
    assert!(state_json.get("pagefileMb").is_none());

    let mut progress = Progress::default();
    progress.complete(1, 5, "now".into());
    progress.skip(1, 6);
    assert_eq!(progress.timestamps["1-5"], "now");
    assert!(!progress.timestamps.contains_key("1-6"));
    assert_eq!(progress.last_skipped_step, 6);
}

#[test]
fn advisory_progress_round_trips_with_future_fields_without_completion() {
    let raw = r#"{
          "advisories": {
            "P1:2": {
              "reason": "XMP/EXPO observation requires authoritative SMBIOS memory-profile data",
              "futureDetail": {"source":"firmware"}
            }
          },
          "futureProgress": true
        }"#;
    let progress: Progress = serde_json::from_str(raw).expect("advisory progress");
    assert!(progress.completed_steps.is_empty());
    assert!(progress.skipped_steps.is_empty());
    assert_eq!(
        progress.advisories["P1:2"].reason,
        "XMP/EXPO observation requires authoritative SMBIOS memory-profile data"
    );
    assert_eq!(
        progress.advisories["P1:2"].unknown["futureDetail"]["source"],
        "firmware"
    );
    let serialized = serde_json::to_value(progress).expect("serialize advisory progress");
    assert_eq!(serialized["futureProgress"], true);
    assert_eq!(
        serialized["advisories"]["P1:2"]["futureDetail"]["source"],
        "firmware"
    );
}

#[test]
fn pagefile_size_bounds_allow_unset_and_limit_values() {
    for pagefile_mb in [0, 1, MAX_PAGEFILE_MB] {
        State {
            pagefile_mb,
            ..State::default()
        }
        .validate()
        .expect("valid pagefile size");
    }
    assert_eq!(
        State {
            pagefile_mb: MAX_PAGEFILE_MB + 1,
            ..State::default()
        }
        .validate(),
        Err("pagefileMB must be 0 or between 1 and 1048576")
    );
}

#[test]
fn invalid_pagefile_input_defaults_without_serializing_an_invalid_value() {
    let state: State = serde_json::from_str(r#"{"pagefileMB":1048577}"#).expect("state");
    assert_eq!(state.pagefile_mb, 0);
    assert_eq!(
        serde_json::to_value(state).expect("state JSON")["pagefileMB"],
        0
    );
}

#[test]
fn final_benchmark_is_transaction_bound_and_unknown_tolerant() {
    let mut receipt = final_receipt(TRANSACTION_ID);
    receipt
        .unknown
        .insert("futureReceipt".into(), serde_json::json!({"keep": true}));
    let state = State {
        active_reboot_transaction: Some(phase_three_transaction(RebootStage::PhaseThreeComplete)),
        final_benchmark: Some(receipt),
        ..State::default()
    };
    state.validate().expect("coherent final benchmark");
    let value = serde_json::to_value(&state).expect("state JSON");
    assert_eq!(value["finalBenchmark"]["futureReceipt"]["keep"], true);
    let round_trip: State = serde_json::from_value(value).expect("state round trip");
    round_trip.validate().expect("round-trip validation");

    let mismatched = State {
        active_reboot_transaction: Some(phase_three_transaction(RebootStage::PhaseThreeArmed)),
        final_benchmark: Some(final_receipt(OTHER_TRANSACTION_ID)),
        ..State::default()
    };
    assert!(mismatched.validate().is_err());
}

#[test]
fn malformed_final_benchmark_is_preserved_and_fails_validation() {
    let state: State = serde_json::from_str(
        r#"{"finalBenchmark":{"schemaVersion":1,"receiptId":false},"future":1}"#,
    )
    .expect("tolerant state");
    assert!(state.final_benchmark.is_none());
    assert_eq!(state.unknown["finalBenchmark"]["receiptId"], false);
    assert_eq!(state.unknown["future"], 1);
    assert_eq!(state.validate(), Err("finalBenchmark is malformed"));
    let value = serde_json::to_value(state).expect("state JSON");
    assert_eq!(value["finalBenchmark"]["receiptId"], false);
}
