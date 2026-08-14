use northclock_core::{
    CommandEnvelope, CommandStatus, ErrorCategory, NorthclockError, Result, WorkloadReport,
    SCHEMA_VERSION,
};
use std::ffi::OsString;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_WORKER_OUTPUT_BYTES: usize = 1024 * 1024;

pub(crate) fn run_isolated(
    adapter: Option<&str>,
    bytes: u64,
    timeout: Duration,
) -> Result<WorkloadReport> {
    let executable =
        worker_executable_for_unelevated_process(super::is_elevated, worker_executable)?;
    let mut command = Command::new(&executable);
    command
        .arg("--bytes")
        .arg(bytes.to_string())
        .arg("--timeout-ms")
        .arg(timeout.as_millis().min(u128::from(u64::MAX)).to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(adapter) = adapter {
        command.arg("--adapter").arg(adapter);
    }
    let mut child = command.spawn().map_err(|error| {
        NorthclockError::Unavailable(format!(
            "could not start isolated VRAM worker {}: {error}",
            executable.display()
        ))
    })?;
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| NorthclockError::InvalidUsage("VRAM timeout overflowed".into()))?;
    let exit_status = loop {
        match child.try_wait().map_err(process_error)? {
            Some(status) => break status,
            None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            None => {
                child.kill().map_err(process_error)?;
                let _ = child.wait();
                return Err(NorthclockError::HardwareOperation(format!(
                    "isolated VRAM worker exceeded the total {} ms timeout and was terminated",
                    timeout.as_millis()
                )));
            }
        }
    };
    let stdout = read_bounded(child.stdout.take(), "stdout")?;
    let stderr = read_bounded(child.stderr.take(), "stderr")?;
    let envelope: CommandEnvelope = serde_json::from_slice(&stdout).map_err(|error| {
        NorthclockError::HardwareOperation(format!(
            "isolated VRAM worker returned invalid JSON: {error}; stderr: {}",
            String::from_utf8_lossy(&stderr)
        ))
    })?;
    validate_envelope(&envelope, exit_status.code())?;
    let data = envelope.data.ok_or_else(|| {
        NorthclockError::HardwareOperation("VRAM worker success omitted result data".into())
    })?;
    serde_json::from_value(data).map_err(|error| {
        NorthclockError::HardwareOperation(format!(
            "VRAM worker returned an invalid workload report: {error}"
        ))
    })
}

fn worker_executable_for_unelevated_process<DetectElevation, ResolveWorker>(
    detect_elevation: DetectElevation,
    resolve_worker: ResolveWorker,
) -> Result<std::path::PathBuf>
where
    DetectElevation: FnOnce() -> Result<bool>,
    ResolveWorker: FnOnce() -> Result<std::path::PathBuf>,
{
    worker_launch_allowed(detect_elevation()?)?;
    resolve_worker()
}

fn worker_launch_allowed(elevated: bool) -> Result<()> {
    if elevated {
        return Err(NorthclockError::PermissionOrSafety(
            "isolated VRAM worker launch is disabled from an elevated Northclock process".into(),
        ));
    }
    Ok(())
}

fn worker_executable() -> Result<std::path::PathBuf> {
    let current = std::env::current_exe().map_err(process_error)?;
    let directory = current.parent().ok_or_else(|| {
        NorthclockError::Unavailable("current executable has no parent directory".into())
    })?;
    let mut name = OsString::from("northclock-vram-worker");
    name.push(std::env::consts::EXE_SUFFIX);
    let worker = directory.join(name);
    if !worker.is_file() {
        return Err(NorthclockError::Unavailable(format!(
            "isolated VRAM worker is not installed beside {}",
            current.display()
        )));
    }
    Ok(worker)
}

fn read_bounded<R: Read>(reader: Option<R>, stream: &str) -> Result<Vec<u8>> {
    let reader = reader.ok_or_else(|| {
        NorthclockError::Internal(format!("VRAM worker {stream} pipe was unavailable"))
    })?;
    let mut output = Vec::new();
    reader
        .take((MAX_WORKER_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut output)
        .map_err(process_error)?;
    if output.len() > MAX_WORKER_OUTPUT_BYTES {
        return Err(NorthclockError::HardwareOperation(format!(
            "VRAM worker {stream} exceeded {MAX_WORKER_OUTPUT_BYTES} bytes"
        )));
    }
    Ok(output)
}

fn validate_envelope(envelope: &CommandEnvelope, exit_code: Option<i32>) -> Result<()> {
    if envelope.schema_version != SCHEMA_VERSION || envelope.command != "memory.vram_test" {
        return Err(NorthclockError::HardwareOperation(
            "VRAM worker returned an incompatible command envelope".into(),
        ));
    }
    if exit_code != Some(i32::from(envelope.exit_code())) {
        return Err(NorthclockError::HardwareOperation(
            "VRAM worker exit status disagreed with its command envelope".into(),
        ));
    }
    if envelope.status == CommandStatus::Success {
        return Ok(());
    }
    let error = envelope.error.as_ref().ok_or_else(|| {
        NorthclockError::HardwareOperation("VRAM worker failure omitted error details".into())
    })?;
    Err(match error.category {
        ErrorCategory::Internal => NorthclockError::Internal(error.message.clone()),
        ErrorCategory::InvalidUsage => NorthclockError::InvalidUsage(error.message.clone()),
        ErrorCategory::Unavailable => NorthclockError::Unavailable(error.message.clone()),
        ErrorCategory::PermissionOrSafety => {
            NorthclockError::PermissionOrSafety(error.message.clone())
        }
        ErrorCategory::HardwareOperation => {
            NorthclockError::HardwareOperation(error.message.clone())
        }
    })
}

fn process_error(error: std::io::Error) -> NorthclockError {
    NorthclockError::HardwareOperation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        validate_envelope, worker_executable_for_unelevated_process, worker_launch_allowed,
    };
    use northclock_core::{CommandEnvelope, CommandStatus, NorthclockError};
    use serde_json::json;
    use std::cell::Cell;
    use std::path::PathBuf;

    #[test]
    fn accepts_only_matching_success_envelope_and_exit() {
        let envelope = CommandEnvelope::success("memory.vram_test", None, json!({}));
        assert!(validate_envelope(&envelope, Some(0)).is_ok());
        assert!(validate_envelope(&envelope, Some(5)).is_err());
        let wrong = CommandEnvelope::success("doctor", None, json!({}));
        assert!(validate_envelope(&wrong, Some(0)).is_err());
        assert_eq!(envelope.status, CommandStatus::Success);
    }

    #[test]
    fn elevated_process_cannot_resolve_or_launch_the_worker() {
        let resolver_calls = Cell::new(0);
        let error = worker_executable_for_unelevated_process(
            || Ok(true),
            || {
                resolver_calls.set(resolver_calls.get() + 1);
                Ok(PathBuf::from("northclock-vram-worker.exe"))
            },
        )
        .expect_err("an elevated parent must fail before worker resolution or spawning");

        assert_eq!(resolver_calls.get(), 0);
        assert!(matches!(error, NorthclockError::PermissionOrSafety(_)));
        assert!(worker_launch_allowed(false).is_ok());
        assert!(matches!(
            worker_launch_allowed(true),
            Err(NorthclockError::PermissionOrSafety(_))
        ));
    }
}
