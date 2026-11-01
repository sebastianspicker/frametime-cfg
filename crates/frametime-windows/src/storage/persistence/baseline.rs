use crate::*;
pub(crate) fn read_state_for_baseline(
    trusted: &TrustedWorkDir,
    work_dir: &Path,
) -> Result<State, String> {
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

pub(crate) fn read_progress_for_baseline(
    trusted: &TrustedWorkDir,
    work_dir: &Path,
) -> Result<Progress, String> {
    if !work_dir.join(PROGRESS_FILE).exists() {
        return Ok(Progress::default());
    }
    read_json_trusted(trusted, PROGRESS_FILE)
        .map_err(|error| format!("read baseline progress: {error}"))
}

pub(crate) fn read_history_for_baseline(
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

/// `Err` means the saved files could not be read; `Ok(false)` means no coherent baseline.
pub(crate) fn baseline_benchmark_is_persisted(
    work_dir: &Path,
    trusted: &TrustedWorkDir,
) -> Result<bool, String> {
    let state = read_state_for_baseline(trusted, work_dir)?;
    let progress = read_progress_for_baseline(trusted, work_dir)?;
    let history = read_history_for_baseline(trusted, work_dir)?;
    Ok(validate_persisted_baseline_benchmark(&state, &progress, &history).is_ok())
}
