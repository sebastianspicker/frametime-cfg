use crate::*;
#[cfg(test)]
use frametime_domain::handoff::{RebootTransaction, RuntimeRecord};

pub(super) struct FinalBenchmarkReconciliationInput<'a> {
    pub(super) state: &'a State,
    pub(super) progress: &'a Progress,
    pub(super) history: &'a [BenchmarkRecord],
    pub(super) config: &'a Config,
    pub(super) captured_utc: String,
    pub(super) capture: BenchmarkCapture,
    pub(super) run_evidence: Option<&'a BenchmarkRunEvidence>,
}

pub(super) enum FinalBenchmarkReconciliation {
    Complete(FinalBenchmarkReceipt),
    Pending(Box<FinalBenchmarkCommit>),
}

struct PartialFinalState {
    source_state: State,
    state_receipt: Option<FinalBenchmarkReceipt>,
}

struct FinalBenchmarkCommitInputs {
    receipt_id: TransactionId,
    captured_utc: String,
    source_history: Vec<BenchmarkRecord>,
    run_evidence: Option<BenchmarkRunEvidence>,
    legacy_prefix: bool,
}

pub(super) trait FreshReceiptId {
    fn generate(self) -> Result<TransactionId, String>;
}

impl FreshReceiptId for TransactionId {
    fn generate(self) -> Result<TransactionId, String> {
        Ok(self)
    }
}

impl<F> FreshReceiptId for F
where
    F: FnOnce() -> Result<TransactionId, String>,
{
    fn generate(self) -> Result<TransactionId, String> {
        self()
    }
}

pub(super) fn reconcile_final_benchmark<Id>(
    input: FinalBenchmarkReconciliationInput<'_>,
    fresh_receipt_id: Id,
) -> Result<FinalBenchmarkReconciliation, String>
where
    Id: FreshReceiptId,
{
    if let Some(receipt) = completed_reconciliation(&input)? {
        return Ok(FinalBenchmarkReconciliation::Complete(receipt));
    }
    let partial_history = final_history_prefix(input.history)?;
    let partial_state = prepare_partial_source_state(input.state, input.capture)?;
    let commit_inputs = select_commit_inputs(
        &input,
        partial_history,
        partial_state.state_receipt.as_ref(),
        fresh_receipt_id,
    )?;
    let FinalBenchmarkCommitInputs {
        receipt_id,
        captured_utc,
        source_history,
        run_evidence,
        legacy_prefix,
    } = commit_inputs;
    let commit = match run_evidence {
        Some(evidence) => {
            evidence
                .validate_against(input.capture)
                .map_err(str::to_owned)?;
            let validated = ValidatedBenchmarkCapture::new(evidence).map_err(str::to_owned)?;
            prepare_final_benchmark_commit_with_evidence(
                &partial_state.source_state,
                input.progress,
                &source_history,
                input.config,
                receipt_id,
                captured_utc,
                &validated,
            )?
        }
        None if legacy_prefix => prepare_final_benchmark_legacy_retry(
            input.state,
            input.progress,
            input.history,
            input.config,
            input.capture,
        )?,
        None => prepare_final_benchmark_commit(
            &partial_state.source_state,
            input.progress,
            &source_history,
            input.config,
            receipt_id,
            captured_utc,
            input.capture,
        )?,
    };
    validate_reproduced_prefixes(&commit, partial_state.state_receipt.as_ref(), input.history)?;
    Ok(FinalBenchmarkReconciliation::Pending(Box::new(commit)))
}

fn completed_reconciliation(
    input: &FinalBenchmarkReconciliationInput<'_>,
) -> Result<Option<FinalBenchmarkReceipt>, String> {
    let completion_key = Progress::key(3, 13);
    if input.progress.completed_steps.contains(&completion_key) {
        let receipt =
            validate_persisted_final_benchmark(input.state, input.progress, input.history)?;
        if !receipt_matches_capture(&receipt, input.capture, input.run_evidence) {
            return Err("completed final benchmark conflicts with the requested capture".into());
        }
        return Ok(Some(receipt));
    }
    if input.progress.skipped_steps.contains(&completion_key) {
        return Err("final benchmark progress was skipped and cannot be retried".into());
    }
    Ok(None)
}

fn prepare_partial_source_state(
    state: &State,
    capture: BenchmarkCapture,
) -> Result<PartialFinalState, String> {
    let mut source_state = state.clone();
    let state_receipt = source_state.final_benchmark.clone();
    if let Some(receipt) = &state_receipt {
        validate_partial_final_state(&source_state, receipt, capture)?;
        source_state.final_benchmark = None;
        let transaction = source_state
            .active_reboot_transaction
            .as_mut()
            .ok_or("final benchmark partial state lost its reboot transaction")?;
        transaction.stage = RebootStage::PhaseThreeArmed;
    } else {
        state.validate().map_err(str::to_owned)?;
    }
    Ok(PartialFinalState {
        source_state,
        state_receipt,
    })
}

fn select_commit_inputs<Id>(
    input: &FinalBenchmarkReconciliationInput<'_>,
    partial_history: Option<(usize, &BenchmarkRecord)>,
    state_receipt: Option<&FinalBenchmarkReceipt>,
    fresh_receipt_id: Id,
) -> Result<FinalBenchmarkCommitInputs, String>
where
    Id: FreshReceiptId,
{
    match partial_history {
        Some((index, record)) => history_prefix_commit_inputs(input, state_receipt, index, record),
        None => state_or_fresh_commit_inputs(input, state_receipt, fresh_receipt_id),
    }
}

fn history_prefix_commit_inputs(
    input: &FinalBenchmarkReconciliationInput<'_>,
    state_receipt: Option<&FinalBenchmarkReceipt>,
    index: usize,
    record: &BenchmarkRecord,
) -> Result<FinalBenchmarkCommitInputs, String> {
    validate_partial_final_record(record, input.state, input.capture)?;
    validate_existing_run_evidence(record.run_evidence.as_ref(), input.run_evidence)?;
    if let Some(receipt) = state_receipt
        && !receipt.matches_history_record(record)
    {
        return Err("final benchmark history and state prefixes disagree".into());
    }
    let mut source_history = input.history.to_vec();
    source_history.remove(index);
    Ok(FinalBenchmarkCommitInputs {
        receipt_id: receipt_id_from_record(record)?,
        captured_utc: record.timestamp.clone(),
        source_history,
        run_evidence: record.run_evidence.clone(),
        legacy_prefix: record.run_evidence.is_none(),
    })
}

fn state_or_fresh_commit_inputs<Id>(
    input: &FinalBenchmarkReconciliationInput<'_>,
    state_receipt: Option<&FinalBenchmarkReceipt>,
    fresh_receipt_id: Id,
) -> Result<FinalBenchmarkCommitInputs, String>
where
    Id: FreshReceiptId,
{
    if let Some(receipt) = state_receipt {
        validate_existing_run_evidence(receipt.run_evidence.as_ref(), input.run_evidence)?;
    }
    let (receipt_id, captured_utc, run_evidence) = match state_receipt {
        Some(receipt) => (
            receipt.receipt_id.clone(),
            receipt.captured_utc.clone(),
            receipt.run_evidence.clone(),
        ),
        None => (
            fresh_receipt_id.generate()?,
            input.captured_utc.clone(),
            input.run_evidence.cloned(),
        ),
    };
    Ok(FinalBenchmarkCommitInputs {
        receipt_id,
        captured_utc,
        source_history: input.history.to_vec(),
        run_evidence,
        legacy_prefix: state_receipt.is_some_and(|receipt| receipt.run_evidence.is_none()),
    })
}

fn validate_existing_run_evidence(
    existing: Option<&BenchmarkRunEvidence>,
    requested: Option<&BenchmarkRunEvidence>,
) -> Result<(), String> {
    if existing.is_some() && existing != requested {
        return Err("final benchmark run evidence conflicts with the requested capture".into());
    }
    Ok(())
}

fn validate_reproduced_prefixes(
    commit: &FinalBenchmarkCommit,
    state_receipt: Option<&FinalBenchmarkReceipt>,
    history: &[BenchmarkRecord],
) -> Result<(), String> {
    if let Some(receipt) = state_receipt
        && commit.receipt != *receipt
    {
        return Err("final benchmark partial state does not reproduce its receipt".into());
    }
    if let Some((_, record)) = final_history_prefix(history)?
        && !commit.receipt.matches_history_record(record)
    {
        return Err("final benchmark partial history does not reproduce its receipt".into());
    }
    Ok(())
}

fn final_history_prefix(
    history: &[BenchmarkRecord],
) -> Result<Option<(usize, &BenchmarkRecord)>, String> {
    let records = history
        .iter()
        .enumerate()
        .filter(|(_, record)| {
            record.label == FINAL_BENCHMARK_LABEL
                || record.receipt_id.is_some()
                || record.transaction_id.is_some()
        })
        .collect::<Vec<_>>();
    match records.as_slice() {
        [] => Ok(None),
        [record] => Ok(Some(*record)),
        _ => Err("final benchmark history contains conflicting receipt prefixes".into()),
    }
}

fn validate_partial_final_state(
    state: &State,
    receipt: &FinalBenchmarkReceipt,
    capture: BenchmarkCapture,
) -> Result<(), String> {
    state.validate().map_err(str::to_owned)?;
    if !matches!(
        state
            .active_reboot_transaction
            .as_ref()
            .map(|transaction| &transaction.stage),
        Some(RebootStage::PhaseThreeComplete)
    ) {
        return Err("final benchmark partial state is not Phase 3 complete".into());
    }
    if !partial_state_matches_receipt(state, receipt, capture) {
        return Err("final benchmark partial state conflicts with the requested capture".into());
    }
    Ok(())
}

fn partial_state_matches_receipt(
    state: &State,
    receipt: &FinalBenchmarkReceipt,
    capture: BenchmarkCapture,
) -> bool {
    [
        receipt_matches_capture(receipt, capture, receipt.run_evidence.as_ref()),
        state.fps_cap == receipt.fps_cap,
        state.avg_fps == receipt.avg_fps,
        state.p1_fps == Some(receipt.p1_fps),
        state.cap_date.as_deref() == Some(&receipt.captured_utc),
    ]
    .into_iter()
    .all(|matches| matches)
}

fn validate_partial_final_record(
    record: &BenchmarkRecord,
    state: &State,
    capture: BenchmarkCapture,
) -> Result<(), String> {
    state.validate().map_err(str::to_owned)?;
    let transaction_id = state
        .active_reboot_transaction
        .as_ref()
        .and_then(|transaction| transaction.transaction_id.as_ref())
        .ok_or("final benchmark history has no active reboot transaction")?;
    if !partial_record_matches(record, transaction_id, capture) {
        return Err("final benchmark partial history conflicts with the requested capture".into());
    }
    Ok(())
}

fn partial_record_matches(
    record: &BenchmarkRecord,
    transaction_id: &TransactionId,
    capture: BenchmarkCapture,
) -> bool {
    [
        record.label == FINAL_BENCHMARK_LABEL,
        record.receipt_id.is_some(),
        record.transaction_id.as_ref() == Some(transaction_id),
        record.avg_fps == capture.average_fps,
        record.p1_fps == capture.p1_fps,
        record.runs == capture.runs,
    ]
    .into_iter()
    .all(|matches| matches)
}

fn receipt_matches_capture(
    receipt: &FinalBenchmarkReceipt,
    capture: BenchmarkCapture,
    run_evidence: Option<&BenchmarkRunEvidence>,
) -> bool {
    let average_matches = receipt.avg_fps == capture.average_fps;
    let p1_matches = receipt.p1_fps == capture.p1_fps;
    let run_count_matches = receipt.runs == capture.runs;
    let evidence_matches = receipt
        .run_evidence
        .as_ref()
        .is_none_or(|existing| Some(existing) == run_evidence);
    average_matches && p1_matches && run_count_matches && evidence_matches
}

fn receipt_id_from_record(record: &BenchmarkRecord) -> Result<TransactionId, String> {
    record
        .receipt_id
        .clone()
        .ok_or("final benchmark partial history has no receipt id".into())
}

pub(super) fn fresh_final_receipt_id(
    state: &State,
    history: &[BenchmarkRecord],
) -> Result<TransactionId, String> {
    let transaction_id = state
        .active_reboot_transaction
        .as_ref()
        .and_then(|transaction| transaction.transaction_id.as_ref());
    for _ in 0..8 {
        let candidate = random_final_receipt_id()?;
        if transaction_id != Some(&candidate)
            && history
                .iter()
                .all(|record| record.receipt_id.as_ref() != Some(&candidate))
        {
            return Ok(candidate);
        }
    }
    Err("could not generate a unique final benchmark receipt id".into())
}
