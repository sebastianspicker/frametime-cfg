use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    catalog::{
        OrchestrationRole, PHASE_THREE_DRIVER_INSTALL, PHASE_THREE_FINAL_BENCHMARK, Phase,
        step_catalog,
    },
    config::Config,
    fps::{BenchmarkCapture, BenchmarkRunEvidence, ValidatedBenchmarkCapture},
    handoff::{RebootStage, RebootTransaction, TransactionId},
    state::{Progress, State},
};

pub const MAX_BENCHMARK_HISTORY: usize = 200;
pub const FINAL_BENCHMARK_SCHEMA_VERSION: u8 = 1;
pub const FINAL_BENCHMARK_LABEL: &str = "After all optimizations";
pub const BASELINE_BENCHMARK_LABEL: &str = "Baseline (before optimizations)";

pub fn validate_benchmark_run_evidence(history: &[BenchmarkRecord]) -> Result<(), String> {
    for record in history {
        if let Some(evidence) = &record.run_evidence {
            evidence
                .validate_against(BenchmarkCapture {
                    average_fps: record.avg_fps,
                    p1_fps: record.p1_fps,
                    runs: record.runs,
                })
                .map_err(str::to_owned)?;
        }
    }
    Ok(())
}

fn phase_three_engine_steps() -> impl Iterator<Item = &'static crate::catalog::Step> {
    step_catalog().iter().filter(|step| {
        step.id.phase == Phase::Three
            && step.orchestration_role == OrchestrationRole::Engine
            && step.id != PHASE_THREE_DRIVER_INSTALL
    })
}

mod baseline;
mod legacy_retry;
pub use baseline::{
    BaselineBenchmarkCommit, prepare_baseline_benchmark_commit,
    prepare_baseline_benchmark_commit_with_evidence, validate_persisted_baseline_benchmark,
};
pub use legacy_retry::prepare_final_benchmark_legacy_retry;

/// Durable transaction-bound Phase 3 evidence, distinct from advisory `fps-cap` calculation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FinalBenchmarkReceipt {
    pub schema_version: u8,
    pub receipt_id: TransactionId,
    pub transaction_id: TransactionId,
    pub captured_utc: String,
    pub avg_fps: f64,
    pub p1_fps: f64,
    pub runs: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_evidence: Option<BenchmarkRunEvidence>,
    pub fps_cap: u32,
    pub label: String,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

/// Coherent in-memory target for the three files that make Phase 3 benchmark
/// completion durable. Platform adapters still have to persist and read back
/// every member before treating the handoff as clearable.
#[derive(Debug, Clone, PartialEq)]
pub struct FinalBenchmarkCommit {
    pub state: State,
    pub progress: Progress,
    pub history: Vec<BenchmarkRecord>,
    pub receipt: FinalBenchmarkReceipt,
}

struct FinalBenchmarkCommitInput<'a> {
    state: &'a State,
    progress: &'a Progress,
    history: &'a [BenchmarkRecord],
    config: &'a Config,
    receipt_id: TransactionId,
    captured_utc: String,
    capture: BenchmarkCapture,
    run_evidence: Option<&'a BenchmarkRunEvidence>,
    legacy_retry: bool,
}

impl FinalBenchmarkReceipt {
    pub fn new(
        receipt_id: TransactionId,
        transaction_id: TransactionId,
        captured_utc: String,
        capture: BenchmarkCapture,
        fps_cap: u32,
    ) -> Result<Self, &'static str> {
        Self::new_with_evidence(
            receipt_id,
            transaction_id,
            captured_utc,
            capture,
            None,
            fps_cap,
        )
    }

    pub fn new_with_evidence(
        receipt_id: TransactionId,
        transaction_id: TransactionId,
        captured_utc: String,
        capture: BenchmarkCapture,
        run_evidence: Option<BenchmarkRunEvidence>,
        fps_cap: u32,
    ) -> Result<Self, &'static str> {
        let receipt = Self {
            schema_version: FINAL_BENCHMARK_SCHEMA_VERSION,
            receipt_id,
            transaction_id,
            captured_utc,
            avg_fps: capture.average_fps,
            p1_fps: capture.p1_fps,
            runs: capture.runs,
            run_evidence,
            fps_cap,
            label: FINAL_BENCHMARK_LABEL.into(),
            unknown: BTreeMap::new(),
        };
        receipt.validate()?;
        Ok(receipt)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != FINAL_BENCHMARK_SCHEMA_VERSION {
            return Err("unsupported final benchmark schema");
        }
        if self.receipt_id == self.transaction_id {
            return Err("final benchmark receipt id must differ from its reboot transaction id");
        }
        if !valid_capture_timestamp(&self.captured_utc) {
            return Err("final benchmark timestamp is invalid");
        }
        if !receipt_capture_is_complete(self) {
            return Err("final benchmark capture is incomplete or invalid");
        }
        if let Some(evidence) = &self.run_evidence {
            evidence.validate_against(BenchmarkCapture {
                average_fps: self.avg_fps,
                p1_fps: self.p1_fps,
                runs: self.runs,
            })?;
        }
        if self.label != FINAL_BENCHMARK_LABEL {
            return Err("final benchmark label is not the fixed Phase 3 label");
        }
        Ok(())
    }

    pub fn validate_for_transaction(
        &self,
        transaction: &RebootTransaction,
    ) -> Result<(), &'static str> {
        self.validate()?;
        if transaction.transaction_id.as_ref() != Some(&self.transaction_id) {
            return Err("final benchmark transaction id does not match active reboot transaction");
        }
        Ok(())
    }

    #[must_use]
    pub fn matches_history_record(&self, record: &BenchmarkRecord) -> bool {
        let timestamp_matches = record.timestamp == self.captured_utc;
        let average_matches = record.avg_fps == self.avg_fps;
        let percentile_matches = record.p1_fps == self.p1_fps;
        let label_matches = record.label == self.label;
        let run_count_matches = record.runs == self.runs;
        let evidence_matches = record.run_evidence == self.run_evidence;
        let receipt_matches = record.receipt_id.as_ref() == Some(&self.receipt_id);
        let transaction_matches = record.transaction_id.as_ref() == Some(&self.transaction_id);
        timestamp_matches
            && average_matches
            && percentile_matches
            && label_matches
            && run_count_matches
            && evidence_matches
            && receipt_matches
            && transaction_matches
    }
}

fn receipt_capture_is_complete(receipt: &FinalBenchmarkReceipt) -> bool {
    baseline::capture_is_complete(BenchmarkCapture {
        average_fps: receipt.avg_fps,
        p1_fps: receipt.p1_fps,
        runs: receipt.runs,
    })
}

/// Prepare one transaction-bound final benchmark commit without performing
/// filesystem or registry I/O. Ordinary advisory FPS calculations must not
/// call this function.
pub fn prepare_final_benchmark_commit(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
    config: &Config,
    receipt_id: TransactionId,
    captured_utc: String,
    capture: BenchmarkCapture,
) -> Result<FinalBenchmarkCommit, String> {
    prepare_final_benchmark_commit_inner(FinalBenchmarkCommitInput {
        state,
        progress,
        history,
        config,
        receipt_id,
        captured_utc,
        capture,
        run_evidence: None,
        legacy_retry: false,
    })
}

pub fn prepare_final_benchmark_commit_with_evidence(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
    config: &Config,
    receipt_id: TransactionId,
    captured_utc: String,
    capture: &ValidatedBenchmarkCapture,
) -> Result<FinalBenchmarkCommit, String> {
    prepare_final_benchmark_commit_inner(FinalBenchmarkCommitInput {
        state,
        progress,
        history,
        config,
        receipt_id,
        captured_utc,
        capture: capture.aggregate(),
        run_evidence: Some(capture.run_evidence()),
        legacy_retry: false,
    })
}

fn prepare_final_benchmark_commit_inner(
    input: FinalBenchmarkCommitInput<'_>,
) -> Result<FinalBenchmarkCommit, String> {
    let FinalBenchmarkCommitInput {
        state,
        progress,
        history,
        config,
        receipt_id,
        captured_utc,
        capture,
        run_evidence,
        legacy_retry,
    } = input;
    validate_final_commit_prerequisites(config, state, progress)?;
    validate_benchmark_run_evidence(history)?;
    let transaction_id = authorized_phase_three_transaction_id(state)?;
    if history_has_final_benchmark_collision(history, &receipt_id, &transaction_id) {
        return Err("final benchmark receipt or reboot transaction was already recorded".into());
    }
    let fps_cap = match run_evidence {
        Some(evidence) => {
            evidence.validate_against(capture).map_err(str::to_owned)?;
            let validated =
                ValidatedBenchmarkCapture::new(evidence.clone()).map_err(str::to_owned)?;
            crate::fps::measured_fps_cap(config.fps_cap.strategy(), &validated)
        }
        None if legacy_retry => {
            crate::fps::legacy_aggregate_fps_cap(config.fps_cap.strategy(), capture)
        }
        None => match config.fps_cap.strategy() {
            crate::fps::FpsCapStrategy::RawLatency { measured_cap: None } => Some(0),
            _ => None,
        },
    }
    .ok_or_else(|| cap_rejection_message(config, run_evidence))?;
    let receipt = FinalBenchmarkReceipt::new_with_evidence(
        receipt_id,
        transaction_id,
        captured_utc.clone(),
        capture,
        run_evidence.cloned(),
        fps_cap,
    )
    .map_err(str::to_owned)?;

    let mut next_state = state.clone();
    next_state.fps_cap = receipt.fps_cap;
    next_state.avg_fps = receipt.avg_fps;
    next_state.p1_fps = Some(receipt.p1_fps);
    next_state.cap_date = Some(receipt.captured_utc.clone());
    next_state.final_benchmark = Some(receipt.clone());
    let next_transaction = next_state
        .active_reboot_transaction
        .as_mut()
        .ok_or("final benchmark lost its active reboot transaction")?;
    next_transaction
        .transition_to(RebootStage::PhaseThreeComplete)
        .map_err(str::to_owned)?;
    next_transaction.updated_utc = Some(captured_utc.clone());
    next_state.validate().map_err(str::to_owned)?;

    let mut next_progress = progress.clone();
    next_progress.complete_step(PHASE_THREE_FINAL_BENCHMARK, captured_utc.clone());
    let mut next_history = history.to_vec();
    next_history.push(BenchmarkRecord {
        timestamp: captured_utc,
        avg_fps: receipt.avg_fps,
        p1_fps: receipt.p1_fps,
        label: receipt.label.clone(),
        runs: receipt.runs,
        run_evidence: receipt.run_evidence.clone(),
        receipt_id: Some(receipt.receipt_id.clone()),
        transaction_id: Some(receipt.transaction_id.clone()),
        unknown: BTreeMap::new(),
    });
    if next_history.len() > MAX_BENCHMARK_HISTORY {
        next_history.drain(..next_history.len() - MAX_BENCHMARK_HISTORY);
    }
    let persisted_record = next_history
        .last()
        .ok_or("final benchmark history did not retain its receipt")?;
    if !receipt.matches_history_record(persisted_record) {
        return Err("final benchmark history does not match its receipt".into());
    }
    Ok(FinalBenchmarkCommit {
        state: next_state,
        progress: next_progress,
        history: next_history,
        receipt,
    })
}

fn cap_rejection_message(config: &Config, evidence: Option<&BenchmarkRunEvidence>) -> String {
    let requested = match config.fps_cap.strategy() {
        crate::fps::FpsCapStrategy::RawLatency { measured_cap } => measured_cap.unwrap_or_default(),
        crate::fps::FpsCapStrategy::Vrr {
            refresh_hz,
            ceiling_margin_hz,
        } => refresh_hz.saturating_sub(ceiling_margin_hz),
    };
    match evidence {
        Some(evidence) => format!(
            "final benchmark rejected cap {requested}: {} failing runs among {} valid runs, including {} invalid runs with P1 above Avg",
            evidence.failing_runs(requested),
            evidence.observations.len(),
            evidence.invalid_ordered_runs()
        ),
        None => "aggregate-only legacy benchmark data cannot authorize a new nonzero cap".into(),
    }
}

fn validate_final_commit_prerequisites(
    config: &Config,
    state: &State,
    progress: &Progress,
) -> Result<(), String> {
    config.validate().map_err(|error| error.to_string())?;
    state.validate().map_err(str::to_owned)?;
    if state.final_benchmark.is_some() {
        return Err("final benchmark receipt already exists".into());
    }
    if !progress.is_completed(PHASE_THREE_DRIVER_INSTALL) {
        return Err("final benchmark requires completed P3:1".into());
    }
    if !final_benchmark_stage_is_ready(progress) {
        return Err("final benchmark requires resolved P3:2-P3:12 and unresolved P3:13".into());
    }
    Ok(())
}

fn final_benchmark_stage_is_ready(progress: &Progress) -> bool {
    let earlier_steps_are_resolved = !has_unresolved_phase_three_engine_steps(progress);
    let final_step_is_unresolved = !progress.is_completed(PHASE_THREE_FINAL_BENCHMARK)
        && !progress.is_skipped(PHASE_THREE_FINAL_BENCHMARK);
    earlier_steps_are_resolved && final_step_is_unresolved
}

fn has_unresolved_phase_three_engine_steps(progress: &Progress) -> bool {
    phase_three_engine_steps()
        .any(|step| !progress.is_completed(step.id) && !progress.is_skipped(step.id))
}

fn authorized_phase_three_transaction_id(state: &State) -> Result<TransactionId, String> {
    let transaction = state
        .active_reboot_transaction
        .as_ref()
        .ok_or("final benchmark requires an active reboot transaction")?;
    if !transaction.is_authorized_at(&RebootStage::PhaseThreeArmed) {
        return Err("final benchmark requires an authorized Phase 3 transaction".into());
    }
    transaction
        .transaction_id
        .clone()
        .ok_or_else(|| "authorized reboot transaction is missing its id".into())
}

fn history_has_final_benchmark_collision(
    history: &[BenchmarkRecord],
    receipt_id: &TransactionId,
    transaction_id: &TransactionId,
) -> bool {
    history.iter().any(|record| {
        let receipt_is_replayed = record.receipt_id.as_ref() == Some(receipt_id);
        let transaction_is_replayed = record.transaction_id.as_ref() == Some(transaction_id);
        receipt_is_replayed || transaction_is_replayed
    })
}

/// Validate the complete persisted bundle after all independent file writes.
/// This readback condition does not authorize removal of the native handoff.
pub fn validate_persisted_final_benchmark(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
) -> Result<FinalBenchmarkReceipt, String> {
    state.validate().map_err(str::to_owned)?;
    validate_benchmark_run_evidence(history)?;
    let receipt = state
        .final_benchmark
        .as_ref()
        .ok_or("persisted state has no final benchmark receipt")?;
    let transaction = state
        .active_reboot_transaction
        .as_ref()
        .ok_or("persisted state has no reboot transaction")?;
    if !transaction.is_authorized_at(&RebootStage::PhaseThreeComplete) {
        return Err("persisted final benchmark transaction is not Phase 3 complete".into());
    }
    receipt
        .validate_for_transaction(transaction)
        .map_err(str::to_owned)?;
    if !persisted_final_benchmark_progress_is_complete(progress, receipt) {
        return Err("persisted final benchmark progress is incomplete or skipped".into());
    }
    if has_unresolved_phase_three_engine_steps(progress) {
        return Err("persisted final benchmark has unresolved earlier Phase 3 work".into());
    }
    if !persisted_fps_state_matches_receipt(state, receipt) {
        return Err("persisted FPS state does not match the final benchmark receipt".into());
    }
    matching_final_record(receipt, history)?;
    Ok(receipt.clone())
}

fn persisted_final_benchmark_progress_is_complete(
    progress: &Progress,
    receipt: &FinalBenchmarkReceipt,
) -> bool {
    let driver_install_is_complete = progress.is_completed(PHASE_THREE_DRIVER_INSTALL);
    let final_step_is_complete = progress.is_completed(PHASE_THREE_FINAL_BENCHMARK);
    let final_step_is_not_skipped = !progress.is_skipped(PHASE_THREE_FINAL_BENCHMARK);
    let timestamp_matches = progress.timestamps.get("3-13") == Some(&receipt.captured_utc);
    driver_install_is_complete
        && final_step_is_complete
        && final_step_is_not_skipped
        && timestamp_matches
}

fn persisted_fps_state_matches_receipt(state: &State, receipt: &FinalBenchmarkReceipt) -> bool {
    let cap_matches = state.fps_cap == receipt.fps_cap;
    let average_matches = state.avg_fps == receipt.avg_fps;
    let percentile_matches = state.p1_fps == Some(receipt.p1_fps);
    let capture_date_matches = state.cap_date.as_deref() == Some(receipt.captured_utc.as_str());
    cap_matches && average_matches && percentile_matches && capture_date_matches
}

fn matching_final_record<'a>(
    receipt: &FinalBenchmarkReceipt,
    history: &'a [BenchmarkRecord],
) -> Result<&'a BenchmarkRecord, String> {
    let matching = history
        .iter()
        .filter(|record| record_matches_receipt_or_transaction(record, receipt))
        .collect::<Vec<_>>();
    if matching.len() != 1 || !receipt.matches_history_record(matching[0]) {
        return Err("persisted benchmark history is missing, duplicated, or inconsistent".into());
    }
    Ok(matching[0])
}

fn record_matches_receipt_or_transaction(
    record: &&BenchmarkRecord,
    receipt: &FinalBenchmarkReceipt,
) -> bool {
    let receipt_matches = record.receipt_id.as_ref() == Some(&receipt.receipt_id);
    let transaction_matches = record.transaction_id.as_ref() == Some(&receipt.transaction_id);
    receipt_matches || transaction_matches
}

fn valid_capture_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 19
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            10 => *byte == b' ',
            13 | 16 => *byte == b':',
            _ => byte.is_ascii_digit(),
        })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkRecord {
    pub timestamp: String,
    pub avg_fps: f64,
    pub p1_fps: f64,
    pub label: String,
    #[serde(default = "one_run")]
    pub runs: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_evidence: Option<BenchmarkRunEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<TransactionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<TransactionId>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

/// Decodes the compatible array or legacy singleton history representation.
#[must_use]
pub fn decode_benchmark_history(value: Value) -> Vec<BenchmarkRecord> {
    match value {
        Value::Array(records) => records
            .into_iter()
            .filter_map(|record| serde_json::from_value(record).ok())
            .collect(),
        Value::Object(_) => serde_json::from_value(value).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Adapter-facing compatible decoder. Malformed legacy records retain the
/// historical drop behavior, while an explicitly present run-evidence field
/// must decode and validate instead of disappearing before a rewrite.
pub fn decode_benchmark_history_checked(value: Value) -> Result<Vec<BenchmarkRecord>, String> {
    let values = match value {
        Value::Array(records) => records,
        Value::Object(_) => vec![value],
        _ => return Ok(Vec::new()),
    };
    let mut history = Vec::new();
    for value in values {
        let has_run_evidence = value
            .as_object()
            .and_then(|record| record.get("runEvidence"))
            .is_some_and(|evidence| !evidence.is_null());
        match serde_json::from_value(value) {
            Ok(record) => history.push(record),
            Err(error) if has_run_evidence => {
                return Err(format!("benchmark run evidence is malformed: {error}"));
            }
            Err(_) => {}
        }
    }
    validate_benchmark_run_evidence(&history)?;
    Ok(history)
}

const fn one_run() -> u32 {
    1
}

/// Appends one record to supplied history and retains the newest 200.
#[must_use]
pub fn append_benchmark_record(
    history: &[BenchmarkRecord],
    record: BenchmarkRecord,
) -> Vec<BenchmarkRecord> {
    let mut history = history.to_vec();
    history.push(record);
    if history.len() > MAX_BENCHMARK_HISTORY {
        history.drain(..history.len() - MAX_BENCHMARK_HISTORY);
    }
    history
}
