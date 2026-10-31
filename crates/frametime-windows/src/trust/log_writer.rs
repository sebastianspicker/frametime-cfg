// Portable record assembly and rotation policy for the append-only current log.

use frametime_domain::logging::{format_log_record, log_start_record};

use crate::LogEntry;

/// Rotate before the current log grows past half of the bounded `show-log`
/// read limit, so the file stays displayable after the next append.
pub(crate) const LOG_ROTATE_BYTES: u64 = 8 * 1024 * 1024;

/// Whether appending `incoming_bytes` should first archive the current log.
/// An empty log is never rotated, so one oversized batch cannot loop.
pub(crate) fn rotation_needed(existing_bytes: u64, incoming_bytes: usize) -> bool {
    let incoming = u64::try_from(incoming_bytes).unwrap_or(u64::MAX);
    existing_bytes != 0 && existing_bytes.saturating_add(incoming) > LOG_ROTATE_BYTES
}

/// Host names scrubbed from every record before it reaches the file.
pub(crate) struct HostNames {
    pub(crate) computer: Option<String>,
    pub(crate) user: Option<String>,
}

/// Builds the exact bytes to append. The start record leads when the caller
/// begins a new session or the file is fresh (empty or just rotated).
pub(crate) fn assemble_log_text(
    entries: &[LogEntry],
    timestamp: &str,
    host: &HostNames,
    include_start: bool,
) -> String {
    let mut text = if include_start {
        log_start_record(timestamp)
    } else {
        String::new()
    };
    for entry in entries {
        text.push_str(&format_log_record(
            timestamp,
            entry.level,
            &entry.message,
            host.computer.as_deref(),
            host.user.as_deref(),
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> HostNames {
        HostNames {
            computer: Some("HOST".into()),
            user: Some("Alice".into()),
        }
    }

    fn entry(level: &'static str, message: &str) -> LogEntry {
        LogEntry {
            level,
            message: message.into(),
        }
    }

    #[test]
    fn rotation_never_triggers_for_an_empty_log() {
        assert!(!rotation_needed(0, usize::MAX));
    }

    #[test]
    fn rotation_triggers_only_when_the_append_would_cross_the_threshold() {
        assert!(!rotation_needed(LOG_ROTATE_BYTES - 10, 10));
        assert!(rotation_needed(LOG_ROTATE_BYTES - 10, 11));
        assert!(rotation_needed(LOG_ROTATE_BYTES, 1));
    }

    #[test]
    fn fresh_file_starts_with_the_start_record_then_redacted_entries() {
        let text = assemble_log_text(
            &[
                entry("INFO", "command optimize"),
                entry("ERROR", r"HOST Alice C:\Users\Alice\x failed"),
            ],
            "2026-08-10 12:00:00",
            &host(),
            true,
        );
        let lines = text.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "frametime.cfg log started 2026-08-10 12:00:00");
        assert_eq!(lines[1], "[2026-08-10 12:00:00] [INFO] command optimize");
        assert!(lines[2].starts_with("[2026-08-10 12:00:00] [ERROR] "));
        assert!(!text.contains("HOST") && !text.contains("Alice"));
    }

    #[test]
    fn continuing_a_session_in_an_existing_file_writes_only_entry_records() {
        let text = assemble_log_text(
            &[entry("INFO", "step P1:3 satisfied")],
            "2026-08-10 12:00:00",
            &host(),
            false,
        );
        assert_eq!(text, "[2026-08-10 12:00:00] [INFO] step P1:3 satisfied\n");
    }
}
