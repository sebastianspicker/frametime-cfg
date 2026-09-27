use super::*;
use crate::catalog::PHASE_ONE_BASELINE_BENCHMARK;

/// Coherent P1:17 target; platform-owned write order is history, state, then progress.
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineBenchmarkCommit {
    pub state: State,
    pub progress: Progress,
    pub history: Vec<BenchmarkRecord>,
    pub captured_utc: String,
    pub idempotent: bool,
}

/// Prepares P1:17 without I/O; exact retries repair a crash prefix and all conflicts fail closed.
pub fn prepare_baseline_benchmark_commit(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
    captured_utc: String,
    capture: BenchmarkCapture,
) -> Result<BaselineBenchmarkCommit, String> {
    prepare_baseline_benchmark_commit_inner(state, progress, history, captured_utc, capture, None)
}

pub fn prepare_baseline_benchmark_commit_with_evidence(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
    captured_utc: String,
    capture: &ValidatedBenchmarkCapture,
) -> Result<BaselineBenchmarkCommit, String> {
    prepare_baseline_benchmark_commit_inner(
        state,
        progress,
        history,
        captured_utc,
        capture.aggregate(),
        Some(capture.run_evidence()),
    )
}

fn prepare_baseline_benchmark_commit_inner(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
    captured_utc: String,
    capture: BenchmarkCapture,
    run_evidence: Option<&BenchmarkRunEvidence>,
) -> Result<BaselineBenchmarkCommit, String> {
    validate_baseline_commit_inputs(state, history, &captured_utc, capture)?;
    if let Some(evidence) = run_evidence {
        evidence.validate_against(capture).map_err(str::to_owned)?;
    }
    if progress.is_skipped(PHASE_ONE_BASELINE_BENCHMARK) {
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
        && !record_matches_capture(record, capture, run_evidence)
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
        if progress.is_completed(PHASE_ONE_BASELINE_BENCHMARK) {
            validate_persisted_baseline_benchmark(state, progress, history)?;
            return Ok(BaselineBenchmarkCommit {
                state: state.clone(),
                progress: progress.clone(),
                history: history.to_vec(),
                captured_utc: record.timestamp.clone(),
                idempotent: true,
            });
        }
    } else if progress.is_completed(PHASE_ONE_BASELINE_BENCHMARK) {
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
            run_evidence: run_evidence.cloned(),
            receipt_id: None,
            transaction_id: None,
            unknown: BTreeMap::new(),
        });
        if next_history.len() > MAX_BENCHMARK_HISTORY {
            next_history.drain(..next_history.len() - MAX_BENCHMARK_HISTORY);
        }
    }
    let mut next_progress = progress.clone();
    next_progress.complete_step(PHASE_ONE_BASELINE_BENCHMARK, record_timestamp.clone());
    Ok(BaselineBenchmarkCommit {
        state: next_state,
        progress: next_progress,
        history: next_history,
        captured_utc: record_timestamp,
        idempotent: false,
    })
}

fn validate_baseline_commit_inputs(
    state: &State,
    history: &[BenchmarkRecord],
    captured_utc: &str,
    capture: BenchmarkCapture,
) -> Result<(), String> {
    state.validate().map_err(str::to_owned)?;
    validate_history(history)?;
    validate_complete_capture(capture)?;
    if !valid_capture_timestamp(captured_utc) {
        return Err("baseline benchmark timestamp is invalid".into());
    }
    Ok(())
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
    if !baseline_state_is_complete(state, baseline_p1) {
        return Err("baseline state is incomplete or invalid".into());
    }
    if !baseline_progress_is_complete(progress) {
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

fn baseline_state_is_complete(state: &State, baseline_p1: f64) -> bool {
    let average_is_valid = state.baseline_avg.is_finite() && state.baseline_avg > 0.0;
    let percentile_is_valid = baseline_p1.is_finite() && baseline_p1 > 0.0;
    let percentile_is_not_above_average = baseline_p1 <= state.baseline_avg;
    average_is_valid && percentile_is_valid && percentile_is_not_above_average
}

fn baseline_progress_is_complete(progress: &Progress) -> bool {
    progress.is_completed(PHASE_ONE_BASELINE_BENCHMARK)
        && !progress.is_skipped(PHASE_ONE_BASELINE_BENCHMARK)
}

fn validate_complete_capture(capture: BenchmarkCapture) -> Result<(), String> {
    if !capture_is_complete(capture) {
        return Err("baseline benchmark requires complete VProf Avg, P1, and runs".into());
    }
    Ok(())
}

pub(super) fn capture_is_complete(capture: BenchmarkCapture) -> bool {
    let average_is_valid = capture.average_fps.is_finite() && capture.average_fps > 0.0;
    let percentile_is_valid = capture.p1_fps.is_finite() && capture.p1_fps > 0.0;
    let percentile_is_not_above_average = capture.p1_fps <= capture.average_fps;
    average_is_valid && percentile_is_valid && percentile_is_not_above_average && capture.runs > 0
}

pub(super) fn validate_history(history: &[BenchmarkRecord]) -> Result<(), String> {
    if history.len() > MAX_BENCHMARK_HISTORY {
        return Err("benchmark history exceeds its retention limit".into());
    }
    if history.iter().any(history_record_is_invalid) {
        return Err("benchmark history contains an invalid record".into());
    }
    Ok(())
}

fn history_record_is_invalid(record: &BenchmarkRecord) -> bool {
    let average_is_invalid = !record.avg_fps.is_finite() || record.avg_fps < 0.0;
    let percentile_is_invalid = !record.p1_fps.is_finite() || record.p1_fps < 0.0;
    let is_missing_required_data = record.runs == 0 || record.label.is_empty();
    let has_invalid_timestamp = !valid_capture_timestamp(&record.timestamp);
    let evidence_is_invalid = record.run_evidence.as_ref().is_some_and(|evidence| {
        evidence
            .validate_against(BenchmarkCapture {
                average_fps: record.avg_fps,
                p1_fps: record.p1_fps,
                runs: record.runs,
            })
            .is_err()
    });
    average_is_invalid
        || percentile_is_invalid
        || is_missing_required_data
        || has_invalid_timestamp
        || evidence_is_invalid
}

fn record_matches_capture(
    record: &BenchmarkRecord,
    capture: BenchmarkCapture,
    run_evidence: Option<&BenchmarkRunEvidence>,
) -> bool {
    let average_matches = record.avg_fps == capture.average_fps;
    let percentile_matches = record.p1_fps == capture.p1_fps;
    let run_count_matches = record.runs == capture.runs;
    let has_no_transaction_binding = record.receipt_id.is_none() && record.transaction_id.is_none();
    let evidence_matches = record
        .run_evidence
        .as_ref()
        .is_none_or(|existing| Some(existing) == run_evidence);
    average_matches
        && percentile_matches
        && run_count_matches
        && has_no_transaction_binding
        && evidence_matches
}
