use std::{collections::BTreeMap, io, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    config::Config,
    fps::BenchmarkCapture,
    fps::recommended_cap,
    handoff::{RebootStage, RebootTransaction, TransactionId},
    persistence::{read_json_tolerant, write_json_atomic},
    state::{Progress, State},
};

pub const MAX_BENCHMARK_HISTORY: usize = 200;
pub const FINAL_BENCHMARK_SCHEMA_VERSION: u8 = 1;
pub const FINAL_BENCHMARK_LABEL: &str = "After all optimizations";
pub const BASELINE_BENCHMARK_LABEL: &str = "Baseline (before optimizations)";

/// Coherent in-memory target for the P1:17 observation bundle. The fixed
/// write order is platform-owned: history, state, then progress.
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineBenchmarkCommit {
    pub state: State,
    pub progress: Progress,
    pub history: Vec<BenchmarkRecord>,
    pub captured_utc: String,
    pub idempotent: bool,
}

/// Prepare the P1:17 baseline observation without filesystem I/O. A retry may
/// repair a history/state crash prefix only when it supplies exactly the same
/// validated VProf aggregate; conflicting or skipped/completed state fails
/// closed.
pub fn prepare_baseline_benchmark_commit(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
    captured_utc: String,
    capture: BenchmarkCapture,
) -> Result<BaselineBenchmarkCommit, String> {
    state.validate().map_err(str::to_owned)?;
    validate_history(history)?;
    validate_complete_capture(capture)?;
    if !valid_capture_timestamp(&captured_utc) {
        return Err("baseline benchmark timestamp is invalid".into());
    }
    let key = Progress::key(1, 17);
    if progress.skipped_steps.contains(&key) {
        return Err("baseline benchmark was skipped and cannot be captured".into());
    }
    let matching = history
        .iter()
        .filter(|record| record.label == BASELINE_BENCHMARK_LABEL)
        .collect::<Vec<_>>();
    if matching.len() > 1 {
        return Err("baseline benchmark history is duplicated".into());
    }
    let existing = matching.first().copied();
    if let Some(record) = existing
        && !record_matches_capture(record, capture)
    {
        return Err("baseline benchmark conflicts with existing history".into());
    }
    let state_has_baseline = state.baseline_avg > 0.0 || state.baseline_p1.is_some();
    if state_has_baseline {
        let Some(record) = existing else {
            return Err("baseline state has no matching benchmark history".into());
        };
        if state.baseline_avg != capture.average_fps || state.baseline_p1 != Some(capture.p1_fps) {
            return Err("baseline state conflicts with requested capture".into());
        }
        if progress.completed_steps.contains(&key) {
            validate_persisted_baseline_benchmark(state, progress, history)?;
            return Ok(BaselineBenchmarkCommit {
                state: state.clone(),
                progress: progress.clone(),
                history: history.to_vec(),
                captured_utc: record.timestamp.clone(),
                idempotent: true,
            });
        }
    } else if progress.completed_steps.contains(&key) {
        return Err("baseline benchmark progress precedes its state".into());
    }

    let record_timestamp = existing.map_or(captured_utc, |record| record.timestamp.clone());
    let mut next_state = state.clone();
    next_state.baseline_avg = capture.average_fps;
    next_state.baseline_p1 = Some(capture.p1_fps);
    next_state.validate().map_err(str::to_owned)?;
    let mut next_history = history.to_vec();
    if existing.is_none() {
        next_history.push(BenchmarkRecord {
            timestamp: record_timestamp.clone(),
            avg_fps: capture.average_fps,
            p1_fps: capture.p1_fps,
            label: BASELINE_BENCHMARK_LABEL.into(),
            runs: capture.runs,
            receipt_id: None,
            transaction_id: None,
            unknown: BTreeMap::new(),
        });
        if next_history.len() > MAX_BENCHMARK_HISTORY {
            next_history.drain(..next_history.len() - MAX_BENCHMARK_HISTORY);
        }
    }
    let mut next_progress = progress.clone();
    next_progress.complete(1, 17, record_timestamp.clone());
    Ok(BaselineBenchmarkCommit {
        state: next_state,
        progress: next_progress,
        history: next_history,
        captured_utc: record_timestamp,
        idempotent: false,
    })
}

/// Require all three persisted P1:17 records to agree before the catalog can
/// treat the observation as satisfied.
pub fn validate_persisted_baseline_benchmark(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
) -> Result<BenchmarkRecord, String> {
    state.validate().map_err(str::to_owned)?;
    validate_history(history)?;
    let baseline_p1 = state.baseline_p1.unwrap_or_default();
    if !state.baseline_avg.is_finite()
        || state.baseline_avg <= 0.0
        || !baseline_p1.is_finite()
        || baseline_p1 <= 0.0
        || baseline_p1 > state.baseline_avg
    {
        return Err("baseline state is incomplete or invalid".into());
    }
    let key = Progress::key(1, 17);
    if !progress.completed_steps.contains(&key) || progress.skipped_steps.contains(&key) {
        return Err("baseline benchmark progress is incomplete or skipped".into());
    }
    let matching = history
        .iter()
        .filter(|record| {
            record.label == BASELINE_BENCHMARK_LABEL
                && record.avg_fps == state.baseline_avg
                && record.p1_fps == baseline_p1
                && record.runs > 0
                && record.receipt_id.is_none()
                && record.transaction_id.is_none()
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 || progress.timestamps.get("1-17") != Some(&matching[0].timestamp) {
        return Err("baseline state, history, and progress are not coherent".into());
    }
    Ok(matching[0].clone())
}

fn validate_complete_capture(capture: BenchmarkCapture) -> Result<(), String> {
    if !capture.average_fps.is_finite()
        || capture.average_fps <= 0.0
        || !capture.p1_fps.is_finite()
        || capture.p1_fps <= 0.0
        || capture.p1_fps > capture.average_fps
        || capture.runs == 0
    {
        return Err("baseline benchmark requires complete VProf Avg, P1, and runs".into());
    }
    Ok(())
}

fn validate_history(history: &[BenchmarkRecord]) -> Result<(), String> {
    if history.len() > MAX_BENCHMARK_HISTORY {
        return Err("benchmark history exceeds its retention limit".into());
    }
    if history.iter().any(|record| {
        !record.avg_fps.is_finite()
            || record.avg_fps < 0.0
            || !record.p1_fps.is_finite()
            || record.p1_fps < 0.0
            || record.runs == 0
            || record.label.is_empty()
            || !valid_capture_timestamp(&record.timestamp)
    }) {
        return Err("benchmark history contains an invalid record".into());
    }
    Ok(())
}

fn record_matches_capture(record: &BenchmarkRecord, capture: BenchmarkCapture) -> bool {
    record.avg_fps == capture.average_fps
        && record.p1_fps == capture.p1_fps
        && record.runs == capture.runs
        && record.receipt_id.is_none()
        && record.transaction_id.is_none()
}

/// Durable evidence for the Phase 3 final benchmark. It is deliberately
/// distinct from an advisory `fps-cap` calculation: a final receipt requires
/// a complete VProf capture and is bound to one reboot transaction.
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

impl FinalBenchmarkReceipt {
    pub fn new(
        receipt_id: TransactionId,
        transaction_id: TransactionId,
        captured_utc: String,
        capture: BenchmarkCapture,
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
        if !self.avg_fps.is_finite()
            || self.avg_fps <= 0.0
            || !self.p1_fps.is_finite()
            || self.p1_fps <= 0.0
            || self.p1_fps > self.avg_fps
            || self.runs == 0
            || self.fps_cap == 0
        {
            return Err("final benchmark capture is incomplete or invalid");
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
        record.timestamp == self.captured_utc
            && record.avg_fps == self.avg_fps
            && record.p1_fps == self.p1_fps
            && record.label == self.label
            && record.runs == self.runs
            && record.receipt_id.as_ref() == Some(&self.receipt_id)
            && record.transaction_id.as_ref() == Some(&self.transaction_id)
    }
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
    config.validate().map_err(|error| error.to_string())?;
    state.validate().map_err(str::to_owned)?;
    if state.final_benchmark.is_some() {
        return Err("final benchmark receipt already exists".into());
    }
    if !progress.completed_steps.contains(&Progress::key(3, 1)) {
        return Err("final benchmark requires completed P3:1".into());
    }
    if (2..13).any(|step| {
        let key = Progress::key(3, step);
        !progress.completed_steps.contains(&key) && !progress.skipped_steps.contains(&key)
    }) || progress.completed_steps.contains(&Progress::key(3, 13))
        || progress.skipped_steps.contains(&Progress::key(3, 13))
    {
        return Err("final benchmark requires resolved P3:2-P3:12 and unresolved P3:13".into());
    }
    let transaction = state
        .active_reboot_transaction
        .as_ref()
        .ok_or("final benchmark requires an active reboot transaction")?;
    if !transaction.is_authorized_at(&RebootStage::PhaseThreeArmed) {
        return Err("final benchmark requires an authorized Phase 3 transaction".into());
    }
    let transaction_id = transaction
        .transaction_id
        .clone()
        .ok_or("authorized reboot transaction is missing its id")?;
    if history.iter().any(|record| {
        record.receipt_id.as_ref() == Some(&receipt_id)
            || record.transaction_id.as_ref() == Some(&transaction_id)
    }) {
        return Err("final benchmark receipt or reboot transaction was already recorded".into());
    }
    let fps_cap = recommended_cap(
        capture.average_fps,
        config.fps_cap.percent,
        config.fps_cap.minimum,
    );
    let receipt = FinalBenchmarkReceipt::new(
        receipt_id,
        transaction_id,
        captured_utc.clone(),
        capture,
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
    next_progress.complete(3, 13, captured_utc.clone());
    let mut next_history = history.to_vec();
    next_history.push(BenchmarkRecord {
        timestamp: captured_utc,
        avg_fps: receipt.avg_fps,
        p1_fps: receipt.p1_fps,
        label: receipt.label.clone(),
        runs: receipt.runs,
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

/// Validate the complete persisted bundle after all independent file writes.
/// This readback condition does not authorize removal of the native handoff.
pub fn validate_persisted_final_benchmark(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
) -> Result<FinalBenchmarkReceipt, String> {
    state.validate().map_err(str::to_owned)?;
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
    if !progress.completed_steps.contains(&Progress::key(3, 1))
        || !progress.completed_steps.contains(&Progress::key(3, 13))
        || progress.skipped_steps.contains(&Progress::key(3, 13))
        || progress.timestamps.get("3-13") != Some(&receipt.captured_utc)
    {
        return Err("persisted final benchmark progress is incomplete or skipped".into());
    }
    if (2..13).any(|step| {
        let key = Progress::key(3, step);
        !progress.completed_steps.contains(&key) && !progress.skipped_steps.contains(&key)
    }) {
        return Err("persisted final benchmark has unresolved earlier Phase 3 work".into());
    }
    if state.fps_cap != receipt.fps_cap
        || state.avg_fps != receipt.avg_fps
        || state.p1_fps != Some(receipt.p1_fps)
        || state.cap_date.as_deref() != Some(receipt.captured_utc.as_str())
    {
        return Err("persisted FPS state does not match the final benchmark receipt".into());
    }
    let matching = history
        .iter()
        .filter(|record| {
            record.receipt_id.as_ref() == Some(&receipt.receipt_id)
                || record.transaction_id.as_ref() == Some(&receipt.transaction_id)
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 || !receipt.matches_history_record(matching[0]) {
        return Err("persisted benchmark history is missing, duplicated, or inconsistent".into());
    }
    Ok(receipt.clone())
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
    pub receipt_id: Option<TransactionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<TransactionId>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

/// Missing or malformed legacy history is treated as an empty history.
/// A dry run deliberately avoids even reading the file.
pub fn load_benchmark_history(path: &Path, dry_run: bool) -> Vec<BenchmarkRecord> {
    if dry_run {
        return Vec::new();
    }
    let Ok(value) = read_json_tolerant::<Value>(path) else {
        return Vec::new();
    };
    match value {
        Value::Array(records) => records
            .into_iter()
            .filter_map(|record| serde_json::from_value(record).ok())
            .collect(),
        Value::Object(_) => serde_json::from_value(value).into_iter().collect(),
        _ => Vec::new(),
    }
}

const fn one_run() -> u32 {
    1
}

/// Appends one record and retains the newest 200. A dry run performs no I/O.
pub fn append_benchmark_record(
    path: &Path,
    record: BenchmarkRecord,
    dry_run: bool,
) -> io::Result<Vec<BenchmarkRecord>> {
    if dry_run {
        return Ok(Vec::new());
    }
    let mut history = load_benchmark_history(path, false);
    history.push(record);
    if history.len() > MAX_BENCHMARK_HISTORY {
        history.drain(..history.len() - MAX_BENCHMARK_HISTORY);
    }
    write_json_atomic(path, &history)?;
    let persisted = load_benchmark_history(path, false);
    if persisted != history {
        return Err(io::Error::other("benchmark history verification failed"));
    }
    Ok(history)
}

#[cfg(test)]
mod tests;
