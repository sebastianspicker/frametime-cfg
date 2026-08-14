// Final-benchmark platform support is kept separate so the public persistence
// surface remains within the repository's production-file size limit.

#[cfg(windows)]
fn random_final_receipt_id() -> Result<TransactionId, String> {
    use std::ffi::c_void;

    unsafe extern "system" {
        fn BCryptGenRandom(
            algorithm: *mut c_void,
            buffer: *mut u8,
            buffer_length: u32,
            flags: u32,
        ) -> i32;
    }

    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
    let mut bytes = [0_u8; 16];
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            u32::try_from(bytes.len()).map_err(|_| "receipt id buffer is too large")?,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return Err(format!(
            "generate final benchmark receipt id: NTSTATUS {status}"
        ));
    }
    let value = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    TransactionId::parse(value).map_err(str::to_owned)
}

#[cfg(not(windows))]
fn random_final_receipt_id() -> Result<TransactionId, String> {
    Err("the live backend is supported only on supported Windows x64".into())
}

fn read_state_for_final(trusted: &TrustedWorkDir, work_dir: &Path) -> Result<State, String> {
    if !work_dir.join(STATE_FILE).exists() {
        return Ok(State::default());
    }
    let state: State = read_json_trusted(trusted, STATE_FILE)
        .map_err(|error| format!("read final benchmark state: {error}"))?;
    state.validate().map_err(str::to_owned)?;
    if !state.work_dir.eq_ignore_ascii_case(WINDOWS_WORK_DIR) {
        return Err("state workDir must be C:\\FRAMETIME_CFG".into());
    }
    Ok(state)
}

fn read_progress_for_final(trusted: &TrustedWorkDir, work_dir: &Path) -> Result<Progress, String> {
    if !work_dir.join(PROGRESS_FILE).exists() {
        return Ok(Progress::default());
    }
    read_json_trusted(trusted, PROGRESS_FILE)
        .map_err(|error| format!("read final benchmark progress: {error}"))
}

fn read_history_for_final(
    trusted: &TrustedWorkDir,
    work_dir: &Path,
) -> Result<Vec<BenchmarkRecord>, String> {
    if !work_dir.join("benchmark_history.json").exists() {
        return Ok(Vec::new());
    }
    read_json_trusted(trusted, "benchmark_history.json")
        .map_err(|error| format!("read final benchmark history: {error}"))
}

/// Read-only result of checking the three independently persisted P3:13
/// records.  Only `final-benchmark` is allowed to create or repair them.
#[derive(Debug, Clone, PartialEq)]
pub enum FinalBenchmarkStatus {
    Absent,
    Coherent(FinalBenchmarkReceipt),
    Incoherent(String),
}

/// Inspect the P3:13 receipt under the suite lock without writing benchmark
/// state, history, progress, or the Phase 3 handoff.
pub fn final_benchmark_status(work_dir: &Path) -> Result<FinalBenchmarkStatus, String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let work_dir = trusted.path();
    let _lock = WorkLock::acquire(work_dir)?;
    let state = match read_state_for_final(&trusted, work_dir) {
        Ok(state) => state,
        Err(error) => return Ok(FinalBenchmarkStatus::Incoherent(error)),
    };
    let progress = match read_progress_for_final(&trusted, work_dir) {
        Ok(progress) => progress,
        Err(error) => return Ok(FinalBenchmarkStatus::Incoherent(error)),
    };
    let history = match read_history_for_final(&trusted, work_dir) {
        Ok(history) => history,
        Err(error) => return Ok(FinalBenchmarkStatus::Incoherent(error)),
    };
    Ok(final_benchmark_status_from_records(&state, &progress, &history))
}

fn final_benchmark_status_from_records(
    state: &State,
    progress: &Progress,
    history: &[BenchmarkRecord],
) -> FinalBenchmarkStatus {
    let completion_key = Progress::key(3, 13);
    let has_receipt_evidence = state.final_benchmark.is_some()
        || matches!(
            state.active_reboot_transaction.as_ref().map(|transaction| &transaction.stage),
            Some(RebootStage::PhaseThreeComplete)
        )
        || progress.completed_steps.contains(&completion_key)
        || progress.skipped_steps.contains(&completion_key)
        || progress.timestamps.contains_key("3-13")
        || history.iter().any(|record| {
            record.label == FINAL_BENCHMARK_LABEL
                || record.receipt_id.is_some()
                || record.transaction_id.is_some()
        });
    if !has_receipt_evidence {
        return FinalBenchmarkStatus::Absent;
    }
    match validate_persisted_final_benchmark(state, progress, history) {
        Ok(receipt) => FinalBenchmarkStatus::Coherent(receipt),
        Err(error) => FinalBenchmarkStatus::Incoherent(error),
    }
}
