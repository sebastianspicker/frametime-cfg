use crate::*;
mod baseline;
mod final_reconciliation;
pub(crate) use baseline::*;
use final_reconciliation::*;
/// Load progress only from the fixed live location.  Corrupt data is never
/// rewritten by a read operation.
pub fn load_progress() -> Result<Progress, String> {
    load_progress_at(TrustedWorkDir::acquire_fixed()?.path())
}

pub(crate) fn load_progress_at(work_dir: &Path) -> Result<Progress, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let work_dir = trusted.path();
    let path = work_dir.join(PROGRESS_FILE);
    if !path.exists() {
        return Ok(Progress::default());
    }
    read_json_trusted(&trusted, PROGRESS_FILE).map_err(|error| format!("read progress: {error}"))
}

/// Load and validate persisted state only from the fixed live location.
pub fn load_state() -> Result<State, String> {
    load_state_at(TrustedWorkDir::acquire_fixed()?.path())
}

pub(crate) fn load_state_at(work_dir: &Path) -> Result<State, String> {
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

/// Reads the compatible benchmark history only through the trusted fixed root.
pub fn load_benchmark_history() -> Result<Vec<BenchmarkRecord>, String> {
    load_benchmark_history_at(TrustedWorkDir::acquire_fixed()?.path())
}

pub(crate) fn load_benchmark_history_at(work_dir: &Path) -> Result<Vec<BenchmarkRecord>, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let path = trusted.path().join("benchmark_history.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value = read_json_trusted(&trusted, "benchmark_history.json")
        .map_err(|error| format!("read benchmark history: {error}"))?;
    frametime_domain::benchmark::decode_benchmark_history_checked(value)
}

/// Reads recovery state only through the trusted fixed root; absent data is
/// represented as the legacy empty backup document.
pub fn load_backup() -> Result<BackupFile, String> {
    load_backup_at(TrustedWorkDir::acquire_fixed()?.path())
}

pub(crate) fn load_backup_at(work_dir: &Path) -> Result<BackupFile, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let path = trusted.path().join(BACKUP_FILE);
    if !path.exists() {
        return Ok(BackupFile {
            entries: Vec::new(),
            created: String::new(),
            unknown: BTreeMap::new(),
        });
    }
    read_json_trusted(&trusted, BACKUP_FILE).map_err(|error| format!("read backup: {error}"))
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
    _package: &AuthenticatedPackage,
    profile: Profile,
    dry_run: bool,
) -> Result<State, String> {
    let trusted = TrustedWorkDir::acquire_fixed()?;
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
    _package: &AuthenticatedPackage,
    cap: u32,
    capture: BenchmarkCapture,
    run_evidence: Option<BenchmarkRunEvidence>,
    label: String,
) -> Result<State, String> {
    let trusted = TrustedWorkDir::acquire_fixed()?;
    let work_dir = trusted.path();
    if !capture.average_fps.is_finite()
        || capture.average_fps <= 0.0
        || !capture.p1_fps.is_finite()
        || capture.p1_fps < 0.0
    {
        return Err("invalid FPS capture".into());
    }
    if let Some(evidence) = &run_evidence {
        evidence.validate_against(capture).map_err(str::to_owned)?;
    }
    if cap > 0 {
        let evidence = run_evidence
            .as_ref()
            .ok_or("a nonzero FPS cap requires per-run benchmark evidence")?;
        if evidence.observations.len() < MIN_CAP_AUTHORIZATION_RUNS
            || evidence.failing_runs(cap) != 0
        {
            return Err("per-run benchmark evidence does not sustain the nonzero FPS cap".into());
        }
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
        validate_benchmark_run_evidence(&history)?;
        history.push(BenchmarkRecord {
            timestamp: captured_at,
            avg_fps: capture.average_fps,
            p1_fps: capture.p1_fps,
            label,
            runs: capture.runs,
            run_evidence,
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
    _package: &AuthenticatedPackage,
    capture: &ValidatedBenchmarkCapture,
) -> Result<State, String> {
    let trusted = TrustedWorkDir::acquire_fixed()?;
    let work_dir = trusted.path();
    let _lock = WorkLock::acquire(work_dir)?;
    let state = read_state_for_baseline(&trusted, work_dir)?;
    let progress = read_progress_for_baseline(&trusted, work_dir)?;
    let history = read_history_for_baseline(&trusted, work_dir)?;
    let commit = prepare_baseline_benchmark_commit_with_evidence(
        &state,
        &progress,
        &history,
        timestamp(),
        capture,
    )?;
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
    _runtime: &VerifiedSelectedRuntime,
    capture: &ValidatedBenchmarkCapture,
) -> Result<FinalBenchmarkReceipt, String> {
    // This must be first: on non-Windows `acquire` rejects before this API can
    // create a lock, load configuration, or mutate a persistence file.
    let trusted = TrustedWorkDir::acquire_fixed()?;
    let work_dir = trusted.path();
    let _lock = WorkLock::acquire(work_dir)?;
    let state = read_state_for_final(&trusted, work_dir)?;
    let progress = read_progress_for_final(&trusted, work_dir)?;
    let history = read_history_for_final(&trusted, work_dir)?;
    let reconciliation = reconcile_final_benchmark(
        FinalBenchmarkReconciliationInput {
            state: &state,
            progress: &progress,
            history: &history,
            config: _runtime.config().value(),
            captured_utc: timestamp(),
            capture: capture.aggregate(),
            run_evidence: Some(capture.run_evidence()),
        },
        || fresh_final_receipt_id(&state, &history),
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

/// Execute only the bounded cleanup actions selected by the CLI.
pub fn cleanup_quick(package: &AuthenticatedPackage) -> Result<CleanupReport, String> {
    let trusted = TrustedWorkDir::acquire_fixed()?;
    require_elevation()?;
    Ok(cleanup_native::run(
        frametime_domain::cleanup::CleanupMode::Quick,
        trusted.path(),
        package.config(),
    ))
}

/// Execute the complete safe local-cleanup set.  It intentionally excludes
/// driver packages and user-owned game content.
pub fn cleanup_full(package: &AuthenticatedPackage) -> Result<CleanupReport, String> {
    let trusted = TrustedWorkDir::acquire_fixed()?;
    require_elevation()?;
    Ok(cleanup_native::run(
        frametime_domain::cleanup::CleanupMode::Full,
        trusted.path(),
        package.config(),
    ))
}

/// Prepare the driver-cleanup transaction by validating the prepared NVIDIA
/// evidence, publishing an immutable runtime, and verifying the P1:38 handoff.
pub fn arm_driver_cleanup(package: &AuthenticatedPackage) -> Result<CleanupReport, String> {
    let trusted = TrustedWorkDir::acquire_fixed()?;
    Ok(cleanup_native::run_driver(trusted.path(), package))
}

/// Collect native, read-only setting evidence.  This API never constructs an
/// engine, lock, progress file, or persistence target.
pub fn verify_settings() -> Result<VerificationReport, String> {
    let trusted = TrustedWorkDir::acquire_fixed()?;
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
