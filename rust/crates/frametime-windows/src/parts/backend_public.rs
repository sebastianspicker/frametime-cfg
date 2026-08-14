/// Prints persisted recovery-record counts without running an external tool.
pub fn backup_summary(work_dir: &Path) -> Result<(), String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let backup: BackupFile = read_json_trusted(&trusted, BACKUP_FILE)
        .map_err(|error| format!("read backup: {error}"))?;
    let mut counts = BTreeMap::<&str, usize>::new();
    for entry in &backup.entries {
        *counts.entry(backup_entry_kind(entry)).or_default() += 1;
    }
    if counts.is_empty() {
        println!("No backup entries found.");
    } else {
        for (kind, count) in counts {
            println!("{kind}: {count}");
        }
    }
    Ok(())
}

/// Prints the current file-backed log without executing a command.
pub fn show_log(work_dir: &Path) -> Result<(), String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    #[cfg(windows)]
    {
        print!("{}", trusted_io_windows::read_current_log(&trusted)?);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = trusted;
        Err("log display requires supported Windows x64".into())
    }
}

/// Run only the catalogued P1:16 NIC latency transaction through the normal
/// engine. The GUI supplies an explicit confirmation, but does not receive a
/// lower-level network host or any path that could bypass backup persistence,
/// readback verification, or progress ordering.
pub fn run_network_stack_transaction(
    work_dir: &Path,
    confirmed: bool,
) -> Result<frametime_core::RunReport, String> {
    if !confirmed {
        return Err("P1:16 requires explicit operator confirmation".into());
    }
    let step = network_stack_step()?;
    let state = load_state(work_dir)?;
    let progress = load_progress(work_dir)?;
    let backend = LiveBackend::new(work_dir.to_path_buf())?;
    let mut engine = frametime_core::Engine::new(backend, progress);
    engine
        .run_with_consent(&[step], state.profile, |_| true)
        .map_err(|error| error.to_string())
}

fn network_stack_step() -> Result<frametime_core::Step, String> {
    let mut matches = frametime_core::step_catalog()
        .iter()
        .filter(|step| step.phase == frametime_core::Phase::One && step.number == 16)
        .copied();
    let step = matches
        .next()
        .ok_or("compiled catalog is missing P1:16 NIC Latency Stack")?;
    if matches.next().is_some() {
        return Err("compiled catalog contains duplicate P1:16 steps".into());
    }
    Ok(step)
}

/// Replace progress atomically after the platform, path, and exclusive lock gates.
pub fn reset_progress(work_dir: &Path) -> Result<(), String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let work_dir = trusted.path();
    let _lock = WorkLock::acquire(work_dir)?;
    write_json_atomic_trusted(&trusted, PROGRESS_FILE, &Progress::default())
        .map_err(|error| format!("reset progress: {error}"))
}

/// Restore all recognised recovery records in reverse capture order.
///
/// Invalid, unknown, and failed entries remain in `backup.json`; successful
/// entries are removed atomically, so a subsequent run is a bounded retry.
pub fn restore_all(work_dir: &Path) -> Result<(), String> {
    restore_matching(work_dir, |_| true)
}

/// Restore only records captured for an exact catalog step title. Records for
/// other steps, unknown records, and failed restores remain byte-compatible in
/// the active backup file for later retry.
pub fn restore_selected(work_dir: &Path, step_title: &str) -> Result<(), String> {
    if step_title.is_empty() || step_title.len() > 128 {
        return Err("restore step title is invalid".into());
    }
    restore_matching(work_dir, |entry| entry.step() == Some(step_title))
}

fn restore_matching(
    work_dir: &Path,
    selected: impl Fn(&BackupEntry) -> bool,
) -> Result<(), String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let work_dir = trusted.path();
    require_elevation()?;
    // A backup is not recovery authority for config-selected identities. Bind
    // every restore attempt to the validated executable-adjacent config.
    let config = config_beside_executable()?;
    let _lock = WorkLock::acquire(work_dir)?;
    let mut backup: BackupFile = read_json_trusted(&trusted, BACKUP_FILE)
        .map_err(|error| format!("read backup: {error}"))?;
    let mut retained = Vec::new();
    let mut failures = Vec::new();
    for entry in backup.restore_order() {
        if !selected(entry) {
            retained.push(entry.clone());
            continue;
        }
        match restore_entry(entry, &config) {
            Ok(()) => {}
            Err(error) => {
                retained.push(entry.clone());
                failures.push(error);
            }
        }
    }
    retained.reverse();
    backup.entries = retained;
    write_json_atomic_trusted(&trusted, BACKUP_FILE, &backup)
        .map_err(|error| format!("persist restored backup: {error}"))?;
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} recovery entries retained: {}",
            failures.len(),
            failures.join("; ")
        ))
    }
}

/// Clear recovery records only after the caller's explicit confirmation. The
/// operation is lock-protected and atomically replaces the active JSON file.
pub fn clear_backup(work_dir: &Path) -> Result<(), String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    let work_dir = trusted.path();
    require_elevation()?;
    let _lock = WorkLock::acquire(work_dir)?;
    let backup: BackupFile = read_json_trusted(&trusted, BACKUP_FILE)
        .map_err(|error| format!("read backup before clear: {error}"))?;
    let cleared = BackupFile {
        entries: Vec::new(),
        created: backup.created,
        unknown: backup.unknown,
    };
    write_json_atomic_trusted(&trusted, BACKUP_FILE, &cleared)
        .map_err(|error| format!("clear backup: {error}"))
}

/// Export the exact active backup bytes after a constrained fixed-root read
/// and byte-for-byte destination verification.
pub fn export_backup(work_dir: &Path, destination: &Path) -> Result<(), String> {
    let trusted = TrustedWorkDir::acquire(work_dir)?;
    #[cfg(windows)]
    {
        // An existing destination is accepted only after it is opened as the
        // exact regular file with no sharing. It is then truncated and
        // replaced through that retained handle, never through a later path.
        trusted_io_windows::export_backup(&trusted, destination)
    }
    #[cfg(not(windows))]
    {
        let _ = (trusted, destination);
        Err("backup export requires supported Windows x64".into())
    }
}

#[cfg(test)]
mod backend_public_tests {
    use super::{network_stack_step, run_network_stack_transaction};
    use std::path::Path;

    #[test]
    fn network_facade_selects_only_p1_16() {
        let step = network_stack_step().expect("compiled network step");
        assert_eq!(step.phase as u8, 1);
        assert_eq!(step.number, 16);
        assert_eq!(step.title, "NIC Latency Stack");
    }

    #[test]
    fn network_facade_rejects_missing_confirmation_before_backend_construction() {
        let error = run_network_stack_transaction(Path::new("not-the-live-root"), false)
            .expect_err("confirmation must fail before any platform operation");
        assert!(error.contains("explicit operator confirmation"));
    }
}
