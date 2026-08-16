/// Load progress only from the fixed live location.  Corrupt data is never
/// rewritten by a read operation.
pub fn load_progress(work_dir: &Path) -> Result<Progress, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let work_dir = trusted.path();
    let path = work_dir.join(PROGRESS_FILE);
    if !path.exists() {
        return Ok(Progress::default());
    }
    read_json_trusted(&trusted, PROGRESS_FILE).map_err(|error| format!("read progress: {error}"))
}

/// Load and validate persisted state only from the fixed live location.
pub fn load_state(work_dir: &Path) -> Result<State, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let work_dir = trusted.path();
    let path = work_dir.join(STATE_FILE);
    if !path.exists() {
        return Ok(State::default());
    }
    let state: State =
        read_json_trusted(&trusted, STATE_FILE).map_err(|error| format!("read state: {error}"))?;
    state.validate().map_err(str::to_owned)?;
    if !state.work_dir.eq_ignore_ascii_case(WINDOWS_WORK_DIR) {
        return Err("state workDir must be C:\\FRAMETIME_CFG".into());
    }
    Ok(state)
}

/// Copy UTF-16 text through the native Windows clipboard.  Ownership of the
/// allocated global block transfers to Windows only after `SetClipboardData`
/// succeeds; every earlier failure frees or unlocks the owned allocation.
pub fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    clipboard::write(text)
}

/// Read Unicode text from the native Windows clipboard without retaining a
/// borrowed global-memory pointer after the clipboard is closed.
pub fn read_text_from_clipboard() -> Result<String, String> {
    clipboard::read()
}

/// Persist a selected profile as one fixed-root, lock-held transaction.  The
/// state model preserves forward-compatible fields through serde flattening.
pub fn configure_profile(
    work_dir: &Path,
    profile: Profile,
    dry_run: bool,
) -> Result<State, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let work_dir = trusted.path();
    let _lock = WorkLock::acquire(work_dir)?;
    let path = work_dir.join(STATE_FILE);
    let mut state = if path.exists() {
        read_json_trusted(&trusted, STATE_FILE).map_err(|error| format!("read state: {error}"))?
    } else {
        State::default()
    };
    state.validate().map_err(str::to_owned)?;
    state.profile = profile;
    state.mode = if dry_run {
        "DRY-RUN"
    } else {
        match profile {
            Profile::Safe | Profile::Recommended => "AUTO",
            Profile::Competitive => "CONTROL",
            Profile::Custom => "INFORMED",
            Profile::Yolo => "YOLO",
        }
    }
    .into();
    state.work_dir = WINDOWS_WORK_DIR.into();
    write_json_atomic_trusted(&trusted, STATE_FILE, &state)
        .map_err(|error| format!("persist profile state: {error}"))?;
    let persisted: State = read_json_trusted(&trusted, STATE_FILE)
        .map_err(|error| format!("read back profile state: {error}"))?;
    persisted.validate().map_err(str::to_owned)?;
    if persisted != state {
        return Err("profile-state readback verification failed".into());
    }
    Ok(persisted)
}

/// Persist an FPS capture under one suite lock.  State and history are
/// independently atomic and read back before the lock is released; malformed
/// persisted JSON is preserved by the core reader rather than overwritten.
pub fn persist_fps_capture(
    work_dir: &Path,
    cap: u32,
    capture: BenchmarkCapture,
    label: String,
) -> Result<State, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let work_dir = trusted.path();
    if cap == 0
        || !capture.average_fps.is_finite()
        || capture.average_fps <= 0.0
        || !capture.p1_fps.is_finite()
        || capture.p1_fps < 0.0
    {
        return Err("invalid FPS capture".into());
    }
    let _lock = WorkLock::acquire(work_dir)?;
    let state_path = work_dir.join(STATE_FILE);
    let mut state = if state_path.exists() {
        read_json_trusted(&trusted, STATE_FILE).map_err(|error| format!("read state: {error}"))?
    } else {
        State::default()
    };
    state.validate().map_err(str::to_owned)?;
    if state.final_benchmark.is_some() {
        return Err(
            "advisory FPS persistence cannot replace a completed Phase 3 benchmark receipt".into(),
        );
    }
    let captured_at = timestamp();
    state.fps_cap = cap;
    state.avg_fps = capture.average_fps;
    state.p1_fps = (capture.p1_fps > 0.0).then_some(capture.p1_fps);
    state.cap_date = Some(captured_at.clone());
    state.work_dir = WINDOWS_WORK_DIR.into();
    write_json_atomic_trusted(&trusted, STATE_FILE, &state)
        .map_err(|error| format!("persist FPS state: {error}"))?;
    let persisted: State = read_json_trusted(&trusted, STATE_FILE)
        .map_err(|error| format!("verify FPS state: {error}"))?;
    persisted.validate().map_err(str::to_owned)?;
    if persisted != state {
        return Err("FPS state readback verification failed".into());
    }
    if capture.p1_fps > 0.0 {
        let history_path = work_dir.join("benchmark_history.json");
        let mut history: Vec<BenchmarkRecord> = if history_path.exists() {
            read_json_trusted(&trusted, "benchmark_history.json")
                .map_err(|error| format!("read benchmark history: {error}"))?
        } else {
            Vec::new()
        };
        history.push(BenchmarkRecord {
            timestamp: captured_at,
            avg_fps: capture.average_fps,
            p1_fps: capture.p1_fps,
            label,
            runs: capture.runs,
            receipt_id: None,
            transaction_id: None,
            unknown: BTreeMap::new(),
        });
        if history.len() > MAX_BENCHMARK_HISTORY {
            history.drain(..history.len() - MAX_BENCHMARK_HISTORY);
        }
        write_json_atomic_trusted(&trusted, "benchmark_history.json", &history)
            .map_err(|error| format!("persist benchmark history: {error}"))?;
        let verified: Vec<BenchmarkRecord> = read_json_trusted(&trusted, "benchmark_history.json")
            .map_err(|error| format!("verify benchmark history: {error}"))?;
        if verified != history {
            return Err("benchmark history readback verification failed".into());
        }
    }
    Ok(persisted)
}

/// Persist P1:17 only after a complete VProf capture. This observation never
/// writes FPS-cap fields, final receipts, or backups. The ordered writes make
/// crash prefixes safe: incomplete history/state can be reconciled by an exact
/// retry, while progress is always last.
pub fn persist_baseline_benchmark(
    work_dir: &Path,
    capture: BenchmarkCapture,
) -> Result<State, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let work_dir = trusted.path();
    let _lock = WorkLock::acquire(work_dir)?;
    let state = read_state_for_baseline(&trusted, work_dir)?;
    let progress = read_progress_for_baseline(&trusted, work_dir)?;
    let history = read_history_for_baseline(&trusted, work_dir)?;
    let commit =
        prepare_baseline_benchmark_commit(&state, &progress, &history, timestamp(), capture)?;
    if commit.idempotent {
        return Ok(commit.state);
    }
    if commit.history != history {
        write_json_atomic_trusted(&trusted, "benchmark_history.json", &commit.history)
            .map_err(|error| format!("persist baseline benchmark history: {error}"))?;
        let verified: Vec<BenchmarkRecord> = read_json_trusted(&trusted, "benchmark_history.json")
            .map_err(|error| format!("verify baseline benchmark history: {error}"))?;
        if verified != commit.history {
            return Err("baseline benchmark history readback verification failed".into());
        }
    }
    if commit.state != state {
        write_json_atomic_trusted(&trusted, STATE_FILE, &commit.state)
            .map_err(|error| format!("persist baseline benchmark state: {error}"))?;
        let verified: State = read_json_trusted(&trusted, STATE_FILE)
            .map_err(|error| format!("verify baseline benchmark state: {error}"))?;
        verified.validate().map_err(str::to_owned)?;
        if verified != commit.state {
            return Err("baseline benchmark state readback verification failed".into());
        }
    }
    if commit.progress != progress {
        write_json_atomic_trusted(&trusted, PROGRESS_FILE, &commit.progress)
            .map_err(|error| format!("persist baseline benchmark progress: {error}"))?;
        let verified: Progress = read_json_trusted(&trusted, PROGRESS_FILE)
            .map_err(|error| format!("verify baseline benchmark progress: {error}"))?;
        if verified != commit.progress {
            return Err("baseline benchmark progress readback verification failed".into());
        }
    }
    validate_persisted_baseline_benchmark(&commit.state, &commit.progress, &commit.history)?;
    Ok(commit.state)
}

/// Persist the final P3:13 VProf observation as a single, lock-held durable
/// bundle. History and state are deliberately committed before progress, so a
/// power-loss prefix never marks P3:13 complete. An exact retry reuses a
/// receipt already present in either prefix; any disagreement fails closed.
pub fn persist_final_benchmark(
    work_dir: &Path,
    capture: BenchmarkCapture,
    config: &VerifiedConfig,
) -> Result<FinalBenchmarkReceipt, String> {
    // This must be first: on non-Windows `acquire` rejects before this API can
    // create a lock, load configuration, or mutate a persistence file.
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let work_dir = trusted.path();
    let _lock = WorkLock::acquire(work_dir)?;
    let state = read_state_for_final(&trusted, work_dir)?;
    let progress = read_progress_for_final(&trusted, work_dir)?;
    let history = read_history_for_final(&trusted, work_dir)?;
    let reconciliation = reconcile_final_benchmark(
        &state,
        &progress,
        &history,
        config.value(),
        || fresh_final_receipt_id(&state, &history),
        timestamp(),
        capture,
    )?;
    let FinalBenchmarkReconciliation::Pending(commit) = reconciliation else {
        let FinalBenchmarkReconciliation::Complete(receipt) = reconciliation else {
            unreachable!("final benchmark reconciliation has two variants")
        };
        return Ok(receipt);
    };

    if commit.history != history {
        write_json_atomic_trusted(&trusted, "benchmark_history.json", &commit.history)
            .map_err(|error| format!("persist final benchmark history: {error}"))?;
        let verified: Vec<BenchmarkRecord> = read_json_trusted(&trusted, "benchmark_history.json")
            .map_err(|error| format!("verify final benchmark history: {error}"))?;
        if verified != commit.history {
            return Err("final benchmark history readback verification failed".into());
        }
    }
    if commit.state != state {
        write_json_atomic_trusted(&trusted, STATE_FILE, &commit.state)
            .map_err(|error| format!("persist final benchmark state: {error}"))?;
        let verified: State = read_json_trusted(&trusted, STATE_FILE)
            .map_err(|error| format!("verify final benchmark state: {error}"))?;
        verified.validate().map_err(str::to_owned)?;
        if verified != commit.state {
            return Err("final benchmark state readback verification failed".into());
        }
    }
    // P3:13 is the durable completion marker and is therefore always last.
    if commit.progress != progress {
        write_json_atomic_trusted(&trusted, PROGRESS_FILE, &commit.progress)
            .map_err(|error| format!("persist final benchmark progress: {error}"))?;
        let verified: Progress = read_json_trusted(&trusted, PROGRESS_FILE)
            .map_err(|error| format!("verify final benchmark progress: {error}"))?;
        if verified != commit.progress {
            return Err("final benchmark progress readback verification failed".into());
        }
    }
    let state: State = read_json_trusted(&trusted, STATE_FILE)
        .map_err(|error| format!("final benchmark state reread: {error}"))?;
    let progress: Progress = read_json_trusted(&trusted, PROGRESS_FILE)
        .map_err(|error| format!("final benchmark progress reread: {error}"))?;
    let history: Vec<BenchmarkRecord> = read_json_trusted(&trusted, "benchmark_history.json")
        .map_err(|error| format!("final benchmark history reread: {error}"))?;
    validate_persisted_final_benchmark(&state, &progress, &history)
}

enum FinalBenchmarkReconciliation {
    Complete(FinalBenchmarkReceipt),
    Pending(Box<FinalBenchmarkCommit>),
}

trait FreshReceiptId {
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

fn reconcile_final_benchmark<Id>(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
    config: &Config,
    fresh_receipt_id: Id,
    captured_utc: String,
    capture: BenchmarkCapture,
) -> Result<FinalBenchmarkReconciliation, String>
where
    Id: FreshReceiptId,
{
    let completion_key = Progress::key(3, 13);
    if progress.completed_steps.contains(&completion_key) {
        let receipt = validate_persisted_final_benchmark(state, progress, history)?;
        if !receipt_matches_capture(&receipt, capture) {
            return Err("completed final benchmark conflicts with the requested capture".into());
        }
        return Ok(FinalBenchmarkReconciliation::Complete(receipt));
    }
    if progress.skipped_steps.contains(&completion_key) {
        return Err("final benchmark progress was skipped and cannot be retried".into());
    }

    let partial_history = final_history_prefix(history)?;
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

    let (receipt_id, record_timestamp, source_history) = match partial_history {
        Some((index, record)) => {
            validate_partial_final_record(record, state, capture)?;
            if let Some(receipt) = &state_receipt
                && !receipt.matches_history_record(record)
            {
                return Err("final benchmark history and state prefixes disagree".into());
            }
            let receipt_id = receipt_id_from_record(record)?;
            let mut source_history = history.to_vec();
            source_history.remove(index);
            (receipt_id, record.timestamp.clone(), source_history)
        }
        None => match &state_receipt {
            Some(receipt) => (
                receipt.receipt_id.clone(),
                receipt.captured_utc.clone(),
                history.to_vec(),
            ),
            None => (fresh_receipt_id.generate()?, captured_utc, history.to_vec()),
        },
    };
    let commit = prepare_final_benchmark_commit(
        &source_state,
        progress,
        &source_history,
        config,
        receipt_id,
        record_timestamp,
        capture,
    )?;
    if let Some(receipt) = &state_receipt
        && commit.receipt != *receipt
    {
        return Err("final benchmark partial state does not reproduce its receipt".into());
    }
    if let Some((_, record)) = final_history_prefix(history)?
        && !commit.receipt.matches_history_record(record)
    {
        return Err("final benchmark partial history does not reproduce its receipt".into());
    }
    Ok(FinalBenchmarkReconciliation::Pending(Box::new(commit)))
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
    if !receipt_matches_capture(receipt, capture)
        || state.fps_cap != receipt.fps_cap
        || state.avg_fps != receipt.avg_fps
        || state.p1_fps != Some(receipt.p1_fps)
        || state.cap_date.as_deref() != Some(&receipt.captured_utc)
    {
        return Err("final benchmark partial state conflicts with the requested capture".into());
    }
    Ok(())
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
    if record.label != FINAL_BENCHMARK_LABEL
        || record.receipt_id.is_none()
        || record.transaction_id.as_ref() != Some(transaction_id)
        || record.avg_fps != capture.average_fps
        || record.p1_fps != capture.p1_fps
        || record.runs != capture.runs
    {
        return Err("final benchmark partial history conflicts with the requested capture".into());
    }
    Ok(())
}

fn receipt_matches_capture(receipt: &FinalBenchmarkReceipt, capture: BenchmarkCapture) -> bool {
    receipt.avg_fps == capture.average_fps
        && receipt.p1_fps == capture.p1_fps
        && receipt.runs == capture.runs
}

fn receipt_id_from_record(record: &BenchmarkRecord) -> Result<TransactionId, String> {
    record
        .receipt_id
        .clone()
        .ok_or("final benchmark partial history has no receipt id".into())
}

fn fresh_final_receipt_id(
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

fn read_state_for_baseline(trusted: &TrustedWorkDir, work_dir: &Path) -> Result<State, String> {
    if !work_dir.join(STATE_FILE).exists() {
        return Ok(State::default());
    }
    let state: State = read_json_trusted(trusted, STATE_FILE)
        .map_err(|error| format!("read baseline state: {error}"))?;
    state.validate().map_err(str::to_owned)?;
    if !state.work_dir.eq_ignore_ascii_case(WINDOWS_WORK_DIR) {
        return Err("state workDir must be C:\\FRAMETIME_CFG".into());
    }
    Ok(state)
}

fn read_progress_for_baseline(
    trusted: &TrustedWorkDir,
    work_dir: &Path,
) -> Result<Progress, String> {
    if !work_dir.join(PROGRESS_FILE).exists() {
        return Ok(Progress::default());
    }
    read_json_trusted(trusted, PROGRESS_FILE)
        .map_err(|error| format!("read baseline progress: {error}"))
}

fn read_history_for_baseline(
    trusted: &TrustedWorkDir,
    work_dir: &Path,
) -> Result<Vec<BenchmarkRecord>, String> {
    if !work_dir.join("benchmark_history.json").exists() {
        return Ok(Vec::new());
    }
    let history: Vec<BenchmarkRecord> = read_json_trusted(trusted, "benchmark_history.json")
        .map_err(|error| format!("read baseline benchmark history: {error}"))?;
    // The core validator is deliberately invoked by prepare and persisted
    // validation. Keeping the raw typed read here avoids normalizing corrupt
    // history into an empty vector.
    Ok(history)
}

fn baseline_benchmark_is_persisted(work_dir: &Path, trusted: &TrustedWorkDir) -> bool {
    let Ok(state) = read_state_for_baseline(trusted, work_dir) else {
        return false;
    };
    let Ok(progress) = read_progress_for_baseline(trusted, work_dir) else {
        return false;
    };
    let Ok(history) = read_history_for_baseline(trusted, work_dir) else {
        return false;
    };
    validate_persisted_baseline_benchmark(&state, &progress, &history).is_ok()
}

/// Execute only the bounded cleanup actions selected by the CLI.
pub fn cleanup_quick(
    work_dir: &Path,
    package: &AuthenticatedPackage,
) -> Result<CleanupReport, String> {
    let _trusted = TrustedWorkDir::acquire(work_dir)?;
    require_elevation()?;
    Ok(cleanup_native::run(frametime_core::CleanupMode::Quick, work_dir, package.config()))
}

/// Execute the complete safe local-cleanup set.  It intentionally excludes
/// driver packages and user-owned game content.
pub fn cleanup_full(
    work_dir: &Path,
    package: &AuthenticatedPackage,
) -> Result<CleanupReport, String> {
    let _trusted = TrustedWorkDir::acquire(work_dir)?;
    require_elevation()?;
    Ok(cleanup_native::run(frametime_core::CleanupMode::Full, work_dir, package.config()))
}

/// Prepare the driver-cleanup transaction by validating the prepared NVIDIA
/// evidence, publishing an immutable runtime, and verifying the P1:38 handoff.
pub fn arm_driver_cleanup(
    work_dir: &Path,
    package: &AuthenticatedPackage,
) -> Result<CleanupReport, String> {
    let _trusted = TrustedWorkDir::acquire(work_dir)?;
    Ok(cleanup_native::run_driver(work_dir, package))
}

/// Collect native, read-only setting evidence.  This API never constructs an
/// engine, lock, progress file, or persistence target.
pub fn verify_settings(work_dir: &Path) -> Result<VerificationReport, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let hardware = discover_hardware()?;
    let mut items = vec![VerificationItem {
        status: if hardware.display_adapters.is_empty() {
            VerificationStatus::Missing
        } else {
            VerificationStatus::Ok
        },
        name: "display adapters".into(),
        detail: if hardware.display_adapters.is_empty() {
            "native display inventory returned no adapters".into()
        } else {
            hardware.display_adapters.join("; ")
        },
    }];
    items.extend(hags_pending_verification_items(&trusted)?);
    Ok(VerificationReport { items })
}
