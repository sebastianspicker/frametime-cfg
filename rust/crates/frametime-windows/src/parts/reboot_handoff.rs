// Native, retry-safe reboot handoff transactions. This module deliberately
// owns only the fixed BCD/registry/state/progress boundary; Engine actions
// remain responsible for the driver work between these durable checkpoints.

use frametime_core::RebootTransaction;

const RUNONCE_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\RunOnce";
const RUN_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";

/// Arm P1:38 only after the immutable selected runtime, exact HKLM RunOnce
/// value, and Safe Mode BCD setting have each been read back.
pub fn arm_safe_mode_handoff(work_dir: &Path) -> Result<(), String> {
    let runtime = retain_selected_runtime(work_dir)?;
    let sid = current_token_user_sid().map_err(|error| format!("P1:38 TokenUser: {error}"))?;
    let phase2 = handoff_command(runtime.executable_path(), "phase2 --yes")?;
    let _trusted = TrustedWorkDir::acquire(work_dir)?;
    let _lock = WorkLock::acquire(work_dir)?;
    let (state, progress) = read_handoff_session(work_dir)?;
    let transaction = match &state.active_reboot_transaction {
        Some(existing)
            if existing.is_authorized_at(&RebootStage::PhaseOneSafeModeArmed)
                && existing.runtime.as_ref() == Some(runtime.record())
                && existing.initiator_user_sid.as_deref() == Some(sid.as_str()) =>
        {
            existing.clone()
        }
        Some(_) => {
            return Err("P1:38 active reboot transaction differs; retain it for recovery".into())
        }
        None => new_transaction(runtime.record().clone(), sid)?,
    };
    ensure_handoff_value(
        Hive::LocalMachine,
        RUNONCE_KEY,
        PHASE2_HANDOFF,
        &phase2,
        "P1:38",
    )?;
    set_safeboot_minimal()?;
    let mut next_state = state;
    next_state.phase1_safe_mode_ready = true;
    next_state.active_reboot_transaction = Some(transaction);
    persist_state_readback(work_dir, &next_state, "P1:38")?;
    let mut next_progress = progress;
    next_progress.complete(1, 38, timestamp());
    persist_progress_readback(work_dir, &next_progress, "P1:38")
}

/// Finish P2:1 as one lock-held checkpoint. Safe Boot must be absent before
/// `phase2SafeMode` and P2:1 progress can be persisted.
pub fn complete_phase_two_safe_boot_clear(work_dir: &Path) -> Result<(), String> {
    let runtime = retain_selected_runtime(work_dir)?;
    let _trusted = TrustedWorkDir::acquire(work_dir)?;
    let _lock = WorkLock::acquire(work_dir)?;
    let (mut state, progress) = read_handoff_session(work_dir)?;
    let transaction = state
        .active_reboot_transaction
        .as_mut()
        .ok_or("P2:1 reboot transaction is missing")?;
    require_runtime_and_stage(
        transaction,
        runtime.record(),
        RebootStage::PhaseOneSafeModeArmed,
        "P2:1",
    )?;
    require_exact_handoff(
        Hive::LocalMachine,
        RUNONCE_KEY,
        PHASE2_HANDOFF,
        &handoff_command(runtime.executable_path(), "phase2 --yes")?,
        "P2:1",
    )?;
    clear_safeboot()?;
    transaction
        .transition_to(RebootStage::PhaseTwoSafeMode)
        .map_err(|error| format!("P2:1 stage: {error}"))?;
    persist_state_readback(work_dir, &state, "P2:1")?;
    let mut next_progress = progress;
    next_progress.complete(2, 1, timestamp());
    persist_progress_readback(work_dir, &next_progress, "P2:1")
}

/// Arm P2:3 after P2:2 has durably completed. The current token user must
/// still be the initiating SID and the Run value is exact, not merely present.
pub fn arm_phase_three_handoff(work_dir: &Path) -> Result<(), String> {
    let runtime = retain_selected_runtime(work_dir)?;
    let sid = current_token_user_sid().map_err(|error| format!("P2:3 TokenUser: {error}"))?;
    let phase3 = handoff_command(runtime.executable_path(), "phase3-handoff")?;
    let _trusted = TrustedWorkDir::acquire(work_dir)?;
    let _lock = WorkLock::acquire(work_dir)?;
    let driver = load_driver_transaction(work_dir)?
        .ok_or("P2:3 requires durable P1:18/P1:19 NVIDIA transaction evidence")?;
    if !driver.removal_complete() {
        return Err("P2:3 requires coherent P2:2 removal evidence; retain the Phase 2 handoff".into());
    }
    let (mut state, progress) = read_handoff_session(work_dir)?;
    if !progress.completed_steps.contains(&Progress::key(2, 2)) {
        return Err("P2:3 requires completed P2:2; retain the Phase 2 handoff".into());
    }
    let transaction = state
        .active_reboot_transaction
        .as_mut()
        .ok_or("P2:3 reboot transaction is missing")?;
    require_runtime_and_stage(
        transaction,
        runtime.record(),
        RebootStage::PhaseTwoSafeMode,
        "P2:3",
    )?;
    if transaction.initiator_user_sid.as_deref() != Some(sid.as_str()) {
        return Err("P2:3 TokenUser differs from the initiating handoff user".into());
    }
    ensure_handoff_value(Hive::CurrentUser, RUN_KEY, PHASE3_HANDOFF, &phase3, "P2:3")?;
    transaction
        .transition_to(RebootStage::PhaseThreeArmed)
        .map_err(|error| format!("P2:3 stage: {error}"))?;
    persist_state_readback(work_dir, &state, "P2:3")?;
    let mut next_progress = progress;
    next_progress.complete(2, 3, timestamp());
    persist_progress_readback(work_dir, &next_progress, "P2:3")
}

/// Validate the retained P3 Run handoff and visibly elevate the exact selected
/// executable. ShellExecuteExW receives the executable path directly; no
/// command interpreter or hidden window is involved.
pub fn relaunch_phase_three_handoff(work_dir: &Path) -> Result<(), String> {
    let runtime = retain_selected_runtime(work_dir)?;
    let _trusted = TrustedWorkDir::acquire(work_dir)?;
    let _lock = WorkLock::acquire(work_dir)?;
    let (state, progress) = read_handoff_session(work_dir)?;
    let transaction = state
        .active_reboot_transaction
        .as_ref()
        .ok_or("phase3-handoff reboot transaction is missing")?;
    require_runtime_and_stage(
        transaction,
        runtime.record(),
        RebootStage::PhaseThreeArmed,
        "phase3-handoff",
    )?;
    let sid =
        current_token_user_sid().map_err(|error| format!("phase3-handoff TokenUser: {error}"))?;
    if transaction.initiator_user_sid.as_deref() != Some(sid.as_str()) {
        return Err("phase3-handoff TokenUser differs from the initiating handoff user".into());
    }
    if !progress.completed_steps.contains(&Progress::key(2, 3)) {
        return Err("phase3-handoff requires completed P2:3".into());
    }
    require_exact_handoff(
        Hive::CurrentUser,
        RUN_KEY,
        PHASE3_HANDOFF,
        &handoff_command(runtime.executable_path(), "phase3-handoff")?,
        "phase3-handoff",
    )?;
    shell_execute_phase_three(runtime.executable_path())
}

/// Clear only the exact P3 Run value after a coherent persisted P3:13 receipt
/// has advanced the active transaction to `phase3Complete`.
pub fn clear_phase_three_handoff(work_dir: &Path) -> Result<(), String> {
    let runtime = retain_selected_runtime(work_dir)?;
    let _trusted = TrustedWorkDir::acquire(work_dir)?;
    let _lock = WorkLock::acquire(work_dir)?;
    let (state, progress) = read_handoff_session(work_dir)?;
    if !progress.completed_steps.contains(&Progress::key(3, 13))
        || !matches!(
            final_benchmark_status_locked(work_dir, &state, &progress),
            Ok(())
        )
    {
        return Err(
            "P3 completion requires a coherent persisted final benchmark; handoff retained".into(),
        );
    }
    let transaction = state
        .active_reboot_transaction
        .as_ref()
        .ok_or("P3 completion reboot transaction is missing")?;
    require_runtime_and_stage(
        transaction,
        runtime.record(),
        RebootStage::PhaseThreeComplete,
        "P3 completion",
    )?;
    let sid =
        current_token_user_sid().map_err(|error| format!("P3 completion TokenUser: {error}"))?;
    if transaction.initiator_user_sid.as_deref() != Some(sid.as_str()) {
        return Err("P3 completion TokenUser differs from the initiating handoff user".into());
    }
    let phase3 = handoff_command(runtime.executable_path(), "phase3-handoff")?;
    require_exact_handoff(
        Hive::CurrentUser,
        RUN_KEY,
        PHASE3_HANDOFF,
        &phase3,
        "P3 completion",
    )?;
    registry_delete(Hive::CurrentUser, RUN_KEY, PHASE3_HANDOFF)
        .map_err(|error| format!("P3 completion clear Run handoff: {error}"))?;
    match read_handoff_value(Hive::CurrentUser, RUN_KEY, PHASE3_HANDOFF)? {
        None => Ok(()),
        Some(_) => Err("P3 completion Run handoff delete readback failed; retry recovery".into()),
    }
}

fn new_transaction(runtime: RuntimeRecord, sid: String) -> Result<RebootTransaction, String> {
    let transaction = RebootTransaction {
        schema_version: 1,
        transaction_id: Some(random_transaction_id()?),
        initiator_user_sid: Some(sid),
        stage: RebootStage::PhaseOneSafeModeArmed,
        runtime: Some(runtime),
        driver_package: None,
        created_utc: Some(timestamp()),
        updated_utc: Some(timestamp()),
        unknown: Default::default(),
    };
    transaction
        .is_authorized_at(&RebootStage::PhaseOneSafeModeArmed)
        .then_some(transaction)
        .ok_or_else(|| "P1:38 generated invalid reboot transaction".to_owned())
}

fn require_runtime_and_stage(
    transaction: &RebootTransaction,
    runtime: &RuntimeRecord,
    stage: RebootStage,
    prefix: &str,
) -> Result<(), String> {
    if !transaction.is_authorized_at(&stage) || transaction.runtime.as_ref() != Some(runtime) {
        return Err(format!(
            "{prefix} transaction runtime or stage differs; retain handoff for recovery"
        ));
    }
    Ok(())
}

fn read_handoff_session(work_dir: &Path) -> Result<(State, Progress), String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let state = read_state_for_handoff(&trusted, work_dir)?;
    let progress = read_progress_for_handoff(&trusted, work_dir)?;
    Ok((state, progress))
}

fn read_state_for_handoff(trusted: &TrustedWorkDir, work_dir: &Path) -> Result<State, String> {
    if !work_dir.join(STATE_FILE).exists() {
        return Ok(State::default());
    }
    let state: State = read_json_trusted(trusted, STATE_FILE)
        .map_err(|error| format!("read reboot transaction state: {error}"))?;
    state.validate().map_err(str::to_owned)?;
    state
        .work_dir
        .eq_ignore_ascii_case(WINDOWS_WORK_DIR)
        .then_some(state)
        .ok_or("state workDir must be C:\\FRAMETIME_CFG".into())
}

fn read_progress_for_handoff(
    trusted: &TrustedWorkDir,
    work_dir: &Path,
) -> Result<Progress, String> {
    if !work_dir.join(PROGRESS_FILE).exists() {
        return Ok(Progress::default());
    }
    read_json_trusted(trusted, PROGRESS_FILE)
        .map_err(|error| format!("read reboot handoff progress: {error}"))
}

fn persist_state_readback(work_dir: &Path, state: &State, prefix: &str) -> Result<(), String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    write_json_atomic_trusted(&trusted, STATE_FILE, state)
        .map_err(|error| format!("{prefix} persist state: {error}"))?;
    let persisted = read_state_for_handoff(&trusted, work_dir)
        .map_err(|error| format!("{prefix} read back state: {error}"))?;
    (persisted == *state)
        .then_some(())
        .ok_or_else(|| format!("{prefix} state readback differs; retry recovery"))
}

fn persist_progress_readback(
    work_dir: &Path,
    progress: &Progress,
    prefix: &str,
) -> Result<(), String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    write_json_atomic_trusted(&trusted, PROGRESS_FILE, progress)
        .map_err(|error| format!("{prefix} persist progress: {error}"))?;
    let persisted = read_progress_for_handoff(&trusted, work_dir)
        .map_err(|error| format!("{prefix} read back progress: {error}"))?;
    (persisted == *progress)
        .then_some(())
        .ok_or_else(|| format!("{prefix} progress readback differs; retry recovery"))
}

fn handoff_command(executable: &Path, arguments: &str) -> Result<String, String> {
    let path = executable
        .to_str()
        .ok_or("selected runtime path is not Unicode")?;
    if path.contains(['\0', '"', '\r', '\n']) || arguments.contains(['\0', '\r', '\n']) {
        return Err("selected runtime command is unsafe".into());
    }
    Ok(format!("\"{path}\" {arguments}"))
}

fn handoff_change(
    hive: Hive,
    key: &'static str,
    name: &'static str,
    value: &str,
) -> RegistryChange {
    RegistryChange {
        hive,
        key,
        name,
        value: RegValue::String(Box::leak(value.to_owned().into_boxed_str())),
    }
}

fn read_handoff_value(
    hive: Hive,
    key: &'static str,
    name: &'static str,
) -> Result<Option<String>, String> {
    match registry_read_exact(&handoff_change(hive, key, name, ""))? {
        Some(RegValue::String(value)) => Ok(Some(value.to_owned())),
        None => Ok(None),
        _ => Err("handoff registry value is not a string".into()),
    }
}

fn ensure_handoff_value(
    hive: Hive,
    key: &'static str,
    name: &'static str,
    expected: &str,
    prefix: &str,
) -> Result<(), String> {
    match read_handoff_value(hive, key, name)? {
        Some(value) if value == expected => Ok(()),
        Some(_) => Err(format!(
            "{prefix} existing handoff value differs; retain it for recovery"
        )),
        None => {
            registry_write(&handoff_change(hive, key, name, expected))
                .map_err(|error| format!("{prefix} register handoff: {error}"))?;
            require_exact_handoff(hive, key, name, expected, prefix)
        }
    }
}

fn require_exact_handoff(
    hive: Hive,
    key: &'static str,
    name: &'static str,
    expected: &str,
    prefix: &str,
) -> Result<(), String> {
    (read_handoff_value(hive, key, name)?.as_deref() == Some(expected))
        .then_some(())
        .ok_or_else(|| format!("{prefix} handoff readback is absent or differs; retry recovery"))
}

fn set_safeboot_minimal() -> Result<(), String> {
    CommandVector::new(
        CommandName::Bcdedit,
        &["/set", "{current}", "safeboot", "minimal"],
    )
    .and_then(|command| command.run())
    .map_err(|error| format!("P1:38 set safeboot: {error}"))?;
    match safeboot_evidence() {
        SafebootEvidence::Configured(value) if value == "minimal" => Ok(()),
        _ => Err("P1:38 safeboot readback differs; retry recovery".into()),
    }
}

fn clear_safeboot() -> Result<(), String> {
    CommandVector::new(
        CommandName::Bcdedit,
        &["/deletevalue", "{current}", "safeboot"],
    )
    .and_then(|command| command.run())
    .map_err(|error| format!("P2:1 clear safeboot: {error}"))?;
    matches!(safeboot_evidence(), SafebootEvidence::Absent)
        .then_some(())
        .ok_or("P2:1 safeboot still configured or unreadable; retry recovery".into())
}

fn final_benchmark_status_locked(
    work_dir: &Path,
    state: &State,
    progress: &Progress,
) -> Result<(), String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let history = read_history_for_final(&trusted, work_dir)?;
    validate_persisted_final_benchmark(state, progress, &history)
        .map(|_| ())
        .map_err(|error| format!("P3 completion final benchmark: {error}"))
}

#[cfg(windows)]
fn random_transaction_id() -> Result<TransactionId, String> {
    use std::ffi::c_void;
    unsafe extern "system" {
        fn BCryptGenRandom(
            algorithm: *mut c_void,
            buffer: *mut u8,
            buffer_length: u32,
            flags: u32,
        ) -> i32;
    }
    let mut bytes = [0_u8; 16];
    let status = unsafe { BCryptGenRandom(std::ptr::null_mut(), bytes.as_mut_ptr(), 16, 2) };
    if status < 0 {
        return Err(format!("P1:38 generate transaction id: NTSTATUS {status}"));
    }
    TransactionId::parse(
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
    .map_err(str::to_owned)
}

#[cfg(not(windows))]
fn random_transaction_id() -> Result<TransactionId, String> {
    Err("P1:38 transaction IDs require supported Windows x64".into())
}

#[cfg(windows)]
fn shell_execute_phase_three(executable: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::UI::Shell::{
            ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
        },
    };
    let file = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let verb = "runas".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let parameters = "phase3 --yes"
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut execute = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(std::mem::size_of::<SHELLEXECUTEINFOW>())
            .map_err(|_| "ShellExecuteEx structure too large")?,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: 1,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut execute) }
        .map_err(|error| format!("phase3-handoff ShellExecuteExW(runas): {error}"))
}

#[cfg(not(windows))]
fn shell_execute_phase_three(_: &Path) -> Result<(), String> {
    Err("phase3-handoff ShellExecuteExW requires supported Windows x64".into())
}

#[cfg(test)]
mod reboot_handoff_tests {
    use super::*;

    #[test]
    fn exact_handoff_command_quotes_the_selected_executable_without_a_shell() {
        assert_eq!(handoff_command(Path::new(r"C:\FRAMETIME_CFG\runtime-generations\0123456789abcdef0123456789abcdef\frametime.exe"), "phase2 --yes").unwrap(), r#""C:\FRAMETIME_CFG\runtime-generations\0123456789abcdef0123456789abcdef\frametime.exe" phase2 --yes"#);
        assert!(handoff_command(Path::new("bad\n.exe"), "phase2 --yes").is_err());
    }
}
