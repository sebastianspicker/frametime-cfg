use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
};
use time::{OffsetDateTime, macros::format_description};

pub const CURRENT_LOG_NAME: &str = "frametime_current.log";

#[must_use]
pub fn legacy_timestamp_now() -> String {
    let value = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    value
        .format(format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .expect("static timestamp format")
}

pub fn initialize_log(logs_dir: &Path, timestamp: &str, max_files: usize) -> io::Result<PathBuf> {
    fs::create_dir_all(logs_dir)?;
    let current = logs_dir.join(CURRENT_LOG_NAME);
    if current.metadata().is_ok_and(|metadata| metadata.len() > 0) {
        let archive_stamp = timestamp.replace([':', ' '], "-");
        fs::rename(
            &current,
            logs_dir.join(format!("frametime_{archive_stamp}.log")),
        )?;
    }
    prune_archives(logs_dir, max_files)?;
    let mut output = fs::File::create(&current)?;
    writeln!(output, "frametime.cfg log started {timestamp}")?;
    output.sync_all()?;
    Ok(current)
}

pub fn append_log(
    path: &Path,
    timestamp: &str,
    level: &str,
    message: &str,
    computer_name: Option<&str>,
    user_name: Option<&str>,
) -> io::Result<()> {
    let mut redacted = message.to_owned();
    for value in [computer_name, user_name].into_iter().flatten() {
        if !value.is_empty() {
            redacted = redacted.replace(value, "<redacted>");
        }
    }
    redacted = redact_user_paths(&redacted);
    let mut output = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(output, "[{timestamp}] [{level}] {redacted}")?;
    output.sync_all()
}

fn prune_archives(logs_dir: &Path, max_files: usize) -> io::Result<()> {
    let mut archives = fs::read_dir(logs_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("frametime_") && name != CURRENT_LOG_NAME)
        })
        .collect::<Vec<_>>();
    archives.sort();
    let remove_count = archives.len().saturating_sub(max_files);
    for archive in archives.into_iter().take(remove_count) {
        fs::remove_file(archive)?;
    }
    Ok(())
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
    fn rotates_prunes_and_redacts() {
        let directory = tempfile::tempdir().expect("directory");
        let logs = directory.path();
        for stamp in ["2026-01-01-00-00-00", "2026-01-02-00-00-00"] {
            fs::write(logs.join(format!("frametime_{stamp}.log")), "old").expect("fixture");
        }
        fs::write(logs.join(CURRENT_LOG_NAME), "current").expect("fixture");
        let current = initialize_log(logs, "2026-08-10 12:00:00", 2).expect("initialize");
        append_log(
            &current,
            "now",
            "INFO",
            r"HOST Alice C:\Users\Alice\file",
            Some("HOST"),
            Some("Alice"),
        )
        .expect("append");
        let text = fs::read_to_string(current).expect("log");
        assert!(!text.contains("HOST"));
        assert!(!text.contains("Alice"));
        assert!(text.contains(r"C:\Users\<redacted>"));
        let archives = fs::read_dir(logs)
            .expect("read")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name() != CURRENT_LOG_NAME)
            .count();
        assert_eq!(archives, 2);
    }

    #[test]
    fn generated_timestamp_matches_legacy_shape() {
        let timestamp = legacy_timestamp_now();
        assert_eq!(timestamp.len(), 19);
        assert_eq!(&timestamp[4..5], "-");
        assert_eq!(&timestamp[10..11], " ");
        assert_eq!(&timestamp[13..14], ":");
    }
}
