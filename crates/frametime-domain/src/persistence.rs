//! Pure persisted-format helpers.
//!
//! Trusted file access, atomic replacement, corruption preservation, and
//! temporary-name generation are Windows adapter responsibilities.

use std::path::{Component, Path};

use serde::{Serialize, de::DeserializeOwned};

#[must_use]
pub fn safe_relative_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    if text.is_empty()
        || text.starts_with(['/', '\\'])
        || text.contains(['\0', ':'])
        || text
            .split(['/', '\\'])
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return false;
    }
    !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

pub fn decode_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|error| error.to_string())
}

pub fn encode_json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        assert!(safe_relative_path(Path::new("cfgs/a.cfg")));
        assert!(!safe_relative_path(Path::new("../a.cfg")));
        assert!(!safe_relative_path(Path::new("/a.cfg")));
        assert!(!safe_relative_path(Path::new(r"C:\a.cfg")));
        assert!(!safe_relative_path(Path::new(r"..\a.cfg")));
        assert!(!safe_relative_path(Path::new(r"\\server\share\a.cfg")));
    }

    #[test]
    fn pretty_encoding_is_newline_terminated() {
        assert_eq!(
            encode_json(&serde_json::json!({"value": 1})).unwrap(),
            b"{\n  \"value\": 1\n}\n"
        );
    }
}
