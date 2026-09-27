use super::*;

/// Reproduces an aggregate-only history/state crash prefix after proving that
/// the supplied original records contain that exact legacy prefix.
#[doc(hidden)]
pub fn prepare_final_benchmark_legacy_retry(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
    config: &Config,
    capture: BenchmarkCapture,
) -> Result<FinalBenchmarkCommit, String> {
    config.validate().map_err(|error| error.to_string())?;
    baseline::validate_history(history)?;
    let transaction_id = state
        .active_reboot_transaction
        .as_ref()
        .and_then(|transaction| transaction.transaction_id.as_ref())
        .ok_or("legacy final benchmark retry has no active transaction")?;
    let expected_cap = crate::fps::legacy_aggregate_fps_cap(config.fps_cap.strategy(), capture)
        .ok_or("legacy final benchmark prefix does not support the configured cap")?;
    let mut final_records = Vec::new();
    for (index, record) in history.iter().enumerate() {
        let has_final_identity = record.receipt_id.is_some() || record.transaction_id.is_some();
        if record.label == FINAL_BENCHMARK_LABEL || has_final_identity {
            final_records.push((index, record));
        }
    }
    if final_records.len() > 1 {
        return Err("legacy final benchmark history contains conflicting prefixes".into());
    }
    let history_prefix = final_records.first().copied();
    let state_prefix = state.final_benchmark.as_ref();
    if history_prefix.is_none() && state_prefix.is_none() {
        return Err("legacy final benchmark retry requires an existing persisted prefix".into());
    }

    if let Some(receipt) = state_prefix {
        validate_receipt_prefix(state, receipt, transaction_id, capture, expected_cap)?;
    }
    if let Some((_, record)) = history_prefix {
        validate_history_prefix(record, transaction_id, capture)?;
    }
    if let (Some(receipt), Some((_, record))) = (state_prefix, history_prefix)
        && !receipt.matches_history_record(record)
    {
        return Err("legacy final benchmark history and state prefixes disagree".into());
    }

    let (receipt_id, captured_utc) = legacy_identity(state_prefix, history_prefix)?;
    let mut source_state = state.clone();
    if state_prefix.is_some() {
        source_state.final_benchmark = None;
        source_state
            .active_reboot_transaction
            .as_mut()
            .ok_or("legacy final benchmark retry lost its transaction")?
            .stage = RebootStage::PhaseThreeArmed;
    }
    let mut source_history = history.to_vec();
    if let Some((index, _)) = history_prefix {
        source_history.remove(index);
    }
    prepare_final_benchmark_commit_inner(FinalBenchmarkCommitInput {
        state: &source_state,
        progress,
        history: &source_history,
        config,
        receipt_id,
        captured_utc,
        capture,
        run_evidence: None,
        legacy_retry: true,
    })
}

fn validate_receipt_prefix(
    state: &State,
    receipt: &FinalBenchmarkReceipt,
    transaction_id: &TransactionId,
    capture: BenchmarkCapture,
    expected_cap: u32,
) -> Result<(), String> {
    receipt.validate().map_err(str::to_owned)?;
    let BenchmarkCapture {
        average_fps: expected_average,
        p1_fps: expected_p1,
        runs: expected_runs,
    } = capture;
    let transaction_is_complete = state
        .active_reboot_transaction
        .as_ref()
        .is_some_and(|transaction| transaction.stage == RebootStage::PhaseThreeComplete);
    let receipt_capture_matches = receipt.avg_fps == expected_average
        && receipt.p1_fps == expected_p1
        && receipt.runs == expected_runs;
    let persisted_state_matches = state.fps_cap == receipt.fps_cap
        && state.avg_fps == receipt.avg_fps
        && state.p1_fps == Some(receipt.p1_fps)
        && state.cap_date.as_deref() == Some(&receipt.captured_utc);
    let identity_matches = &receipt.transaction_id == transaction_id
        && receipt.fps_cap == expected_cap
        && receipt.run_evidence.is_none();
    if !(receipt_capture_matches
        && persisted_state_matches
        && identity_matches
        && transaction_is_complete)
    {
        return Err("legacy final benchmark receipt prefix is not an exact match".into());
    }
    Ok(())
}

fn validate_history_prefix(
    record: &BenchmarkRecord,
    transaction_id: &TransactionId,
    capture: BenchmarkCapture,
) -> Result<(), String> {
    if record.run_evidence.is_some()
        || record.receipt_id.is_none()
        || record.transaction_id.as_ref() != Some(transaction_id)
        || record.avg_fps != capture.average_fps
        || record.p1_fps != capture.p1_fps
        || record.runs != capture.runs
        || record.label != FINAL_BENCHMARK_LABEL
    {
        return Err("legacy final benchmark history prefix is not an exact match".into());
    }
    Ok(())
}

fn legacy_identity(
    receipt: Option<&FinalBenchmarkReceipt>,
    history: Option<(usize, &BenchmarkRecord)>,
) -> Result<(TransactionId, String), String> {
    if let Some(receipt) = receipt {
        return Ok((receipt.receipt_id.clone(), receipt.captured_utc.clone()));
    }
    let record = history
        .map(|(_, record)| record)
        .ok_or("legacy final benchmark prefix has no identity")?;
    Ok((
        record
            .receipt_id
            .clone()
            .ok_or("legacy final benchmark history prefix has no receipt id")?,
        record.timestamp.clone(),
    ))
}
