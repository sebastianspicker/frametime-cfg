use crate::*;
/// Shared by the baseline and final-benchmark readers; `label` only prefixes error messages.
pub(crate) fn read_state_labeled(trusted: &TrustedWorkDir, label: &str) -> Result<State, String> {
    let Some(state): Option<State> = read_json_if_present(trusted, STATE_FILE)
        .map_err(|error| format!("read {label} state: {error}"))?
    else {
        return Ok(State::default());
    };
    state.validate().map_err(str::to_owned)?;
    if !state.work_dir.eq_ignore_ascii_case(WINDOWS_WORK_DIR) {
        return Err("state workDir must be C:\\FRAMETIME_CFG".into());
    }
    Ok(state)
}

pub(crate) fn read_progress_labeled(
    trusted: &TrustedWorkDir,
    label: &str,
) -> Result<Progress, String> {
    Ok(read_json_if_present(trusted, PROGRESS_FILE)
        .map_err(|error| format!("read {label} progress: {error}"))?
        .unwrap_or_default())
}

/// The raw typed read is deliberate: the core validator runs in prepare and persisted
/// validation, and normalizing corrupt history into an empty vector would hide it.
pub(crate) fn read_history_labeled(
    trusted: &TrustedWorkDir,
    label: &str,
) -> Result<Vec<BenchmarkRecord>, String> {
    Ok(read_json_if_present(trusted, "benchmark_history.json")
        .map_err(|error| format!("read {label} benchmark history: {error}"))?
        .unwrap_or_default())
}

pub(crate) fn read_state_for_baseline(
    trusted: &TrustedWorkDir,
    _work_dir: &Path,
) -> Result<State, String> {
    read_state_labeled(trusted, "baseline")
}

pub(crate) fn read_progress_for_baseline(
    trusted: &TrustedWorkDir,
    _work_dir: &Path,
) -> Result<Progress, String> {
    read_progress_labeled(trusted, "baseline")
}

pub(crate) fn read_history_for_baseline(
    trusted: &TrustedWorkDir,
    _work_dir: &Path,
) -> Result<Vec<BenchmarkRecord>, String> {
    read_history_labeled(trusted, "baseline")
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
