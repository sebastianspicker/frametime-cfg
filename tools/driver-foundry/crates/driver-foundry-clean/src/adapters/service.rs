use super::ServiceProbe;
use std::process::Command;

/// Non-destructive service query via `sc query`.
pub fn probe_service_windows(name: &str) -> ServiceProbe {
    match Command::new("sc").args(["query", name]).output() {
        Ok(output) => {
            let raw = String::from_utf8_lossy(&output.stdout).into_owned()
                + &String::from_utf8_lossy(&output.stderr);
            let exists = output.status.success()
                && !raw.to_ascii_lowercase().contains("does not exist")
                && !raw.to_ascii_lowercase().contains("1060");
            let state = if raw.contains("RUNNING") {
                "RUNNING"
            } else if raw.contains("STOPPED") {
                "STOPPED"
            } else if exists {
                "UNKNOWN"
            } else {
                "ABSENT"
            };
            ServiceProbe {
                name: name.into(),
                exists,
                state: state.into(),
                raw,
            }
        }
        Err(error) => ServiceProbe {
            name: name.into(),
            exists: false,
            state: "query-failed".into(),
            raw: error.to_string(),
        },
    }
}
