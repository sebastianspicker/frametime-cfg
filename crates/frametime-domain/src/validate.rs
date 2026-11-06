//! Shared field checks; each caller keeps its own error type and message.

/// Non-empty, at most 512 bytes, no control characters.
pub(crate) fn text_ok(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}

/// Non-empty, at most 64 bytes, no control characters.
pub(crate) fn timestamp_ok(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 64 && !value.chars().any(char::is_control)
}

/// Exactly `len` lowercase hexadecimal digits.
pub(crate) fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
