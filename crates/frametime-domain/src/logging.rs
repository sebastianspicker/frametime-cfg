//! Pure log-record formatting and redaction policy.

pub const CURRENT_LOG_NAME: &str = "frametime_current.log";

/// Formats one immutable record. The clock and destination are host-owned.
#[must_use]
pub fn format_log_record(
    timestamp: &str,
    level: &str,
    message: &str,
    computer_name: Option<&str>,
    user_name: Option<&str>,
) -> String {
    format!(
        "[{timestamp}] [{level}] {}\n",
        redact_message(message, computer_name, user_name)
    )
}

#[must_use]
pub fn log_start_record(timestamp: &str) -> String {
    format!("frametime.cfg log started {timestamp}\n")
}

#[must_use]
pub fn archive_log_name(timestamp: &str) -> String {
    format!("frametime_{}.log", timestamp.replace([':', ' '], "-"))
}

#[must_use]
pub fn redact_message(
    message: &str,
    computer_name: Option<&str>,
    user_name: Option<&str>,
) -> String {
    let mut redacted = message.to_owned();
    for value in [computer_name, user_name].into_iter().flatten() {
        if !value.is_empty() {
            redacted = redacted.replace(value, "<redacted>");
        }
    }
    redact_user_paths(&redacted)
}

fn redact_user_paths(input: &str) -> String {
    let mut output = input.to_owned();
    let lower = output.to_ascii_lowercase();
    let marker = r"c:\users\";
    if let Some(start) = lower.find(marker) {
        let tail = &output[start + marker.len()..];
        let end = tail
            .find(['\\', '/', ' ', '\t', '\r', '\n'])
            .unwrap_or(tail.len());
        output.replace_range(start..start + marker.len() + end, r"C:\Users\<redacted>");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_redacts_host_user_and_user_path() {
        let record = format_log_record(
            "2026-08-10 12:00:00",
            "INFO",
            r"HOST Alice C:\Users\Alice\file",
            Some("HOST"),
            Some("Alice"),
        );
        assert!(!record.contains("HOST"));
        assert!(!record.contains("Alice"));
        assert!(record.contains(r"C:\Users\<redacted>"));
    }

    #[test]
    fn archive_name_preserves_legacy_shape() {
        assert_eq!(
            archive_log_name("2026-08-10 12:00:00"),
            "frametime_2026-08-10-12-00-00.log"
        );
    }
}
