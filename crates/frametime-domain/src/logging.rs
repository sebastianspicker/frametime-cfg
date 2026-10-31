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
    let message = redact_message(message, computer_name, user_name);
    // A continuation line is indented so it can never start with `[` and
    // pass for a separate record.
    let message = indent_continuation_lines(&message);
    format!("[{timestamp}] [{level}] {message}\n")
}

fn indent_continuation_lines(message: &str) -> String {
    let mut output = String::with_capacity(message.len());
    let mut chars = message.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\r' => {
                chars.next_if_eq(&'\n');
                output.push_str("\n    ");
            }
            '\n' => output.push_str("\n    "),
            other => output.push(other),
        }
    }
    output
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
        // Names shorter than three characters would corrupt unrelated text
        // ("a", "PC"); profile paths below are still redacted for them.
        if value.chars().count() >= MIN_NAME_REDACTION_CHARS {
            redacted = replace_ignore_ascii_case(&redacted, value, "<redacted>");
        }
    }
    redact_user_paths(&redact_account_sids(&redacted))
}

const MIN_NAME_REDACTION_CHARS: usize = 3;
const SID_PREFIX: &str = "s-1-5-21-";

/// Replaces every ASCII-case-insensitive occurrence of `needle`. ASCII
/// lowercasing keeps byte offsets identical to the original text.
fn replace_ignore_ascii_case(text: &str, needle: &str, replacement: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let needle = needle.to_ascii_lowercase();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(found) = lower[cursor..].find(&needle) {
        let start = cursor + found;
        output.push_str(&text[cursor..start]);
        output.push_str(replacement);
        cursor = start + needle.len();
    }
    output.push_str(&text[cursor..]);
    output
}

/// Masks the machine/domain part of Windows account SIDs.
fn redact_account_sids(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(found) = lower[cursor..].find(SID_PREFIX) {
        let start = cursor + found;
        let tail = &lower[start + SID_PREFIX.len()..];
        let length = tail
            .find(|c: char| !c.is_ascii_digit() && c != '-')
            .unwrap_or(tail.len());
        output.push_str(&input[cursor..start]);
        output.push_str("S-1-5-21-<redacted>");
        cursor = start + SID_PREFIX.len() + length;
    }
    output.push_str(&input[cursor..]);
    output
}

/// Redacts the profile name of every `<drive>:\Users\<name>` or
/// `<drive>:/Users/<name>` occurrence. The name runs to a separator, quote,
/// line break, or the end of the text, so names containing spaces are covered.
fn redact_user_paths(input: &str) -> String {
    let bytes = input.as_bytes();
    let lower = input.to_ascii_lowercase();
    let lower = lower.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    let mut index = 0;
    while index + USERS_PREFIX_BYTES <= bytes.len() {
        let separator = bytes[index + 2];
        let is_profile_root = bytes[index].is_ascii_alphabetic()
            && bytes[index + 1] == b':'
            && matches!(separator, b'\\' | b'/')
            && &lower[index + 3..index + 8] == b"users"
            && bytes[index + 8] == separator;
        if !is_profile_root {
            index += 1;
            continue;
        }
        let name_start = index + USERS_PREFIX_BYTES;
        let name_length = input[name_start..]
            .find(['\\', '/', '"', '\'', '\r', '\n'])
            .unwrap_or(input.len() - name_start);
        let separator = char::from(separator);
        output.push_str(&input[cursor..index]);
        output.push(char::from(bytes[index]));
        output.push(':');
        output.push(separator);
        output.push_str("Users");
        output.push(separator);
        output.push_str("<redacted>");
        cursor = name_start + name_length;
        index = cursor;
    }
    output.push_str(&input[cursor..]);
    output
}

const USERS_PREFIX_BYTES: usize = 9;

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

    fn redact(message: &str) -> String {
        redact_message(message, Some("HOST"), Some("Alice"))
    }

    #[test]
    fn every_profile_path_is_redacted_with_separator_style_kept() {
        assert_eq!(
            redact(r"a C:\Users\Bob\x and c:\USERS\Carol\y"),
            r"a C:\Users\<redacted>\x and c:\Users\<redacted>\y"
        );
        assert_eq!(
            redact("D:/Users/Bob/x then E:\\Users\\Dan"),
            r"D:/Users/<redacted>/x then E:\Users\<redacted>"
        );
        assert_eq!(redact(r"\\?\C:\Users\Bob\x"), r"\\?\C:\Users\<redacted>\x");
    }

    #[test]
    fn profile_names_with_spaces_end_at_separator_quote_or_line_break() {
        assert_eq!(redact(r"C:\Users\John Smith\x"), r"C:\Users\<redacted>\x");
        assert_eq!(
            redact(r#""C:\Users\J S" ok"#),
            r#""C:\Users\<redacted>" ok"#
        );
        assert_eq!(
            redact("C:\\Users\\Bob\r\nnext"),
            "C:\\Users\\<redacted>\r\nnext"
        );
        assert_eq!(redact(r"C:\Users"), r"C:\Users");
    }

    #[test]
    fn host_names_are_replaced_case_insensitively() {
        assert_eq!(
            redact("host HoSt ALICE alice"),
            "<redacted> <redacted> <redacted> <redacted>"
        );
    }

    #[test]
    fn names_shorter_than_three_characters_are_not_replaced() {
        assert_eq!(
            redact_message("PC a ab abc", Some("PC"), Some("a")),
            "PC a ab abc"
        );
        assert_eq!(
            redact_message("PC a ab abc", Some("ab"), Some("abc")),
            "PC a ab <redacted>"
        );
    }

    #[test]
    fn account_sids_are_masked() {
        assert_eq!(
            redact("owner S-1-5-21-111-222-333-1001, s-1-5-21-9 end"),
            "owner S-1-5-21-<redacted>, S-1-5-21-<redacted> end"
        );
        assert_eq!(redact("S-1-5-18 stays"), "S-1-5-18 stays");
    }

    #[test]
    fn multi_line_messages_are_indented_into_one_record() {
        let record = format_log_record(
            "T",
            "ERROR",
            "first\r\n[T] [INFO] forged\rthird\nfourth",
            None,
            None,
        );
        assert_eq!(
            record,
            "[T] [ERROR] first\n    [T] [INFO] forged\n    third\n    fourth\n"
        );
        assert_eq!(record.matches("\n[").count(), 0);
        assert!(record.ends_with("fourth\n"));
    }
}
