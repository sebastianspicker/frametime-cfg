//! Portable policy helpers for the live backend's Windows handle-backed JSON.

use serde::Serialize;

pub(super) const MAX_TRUSTED_JSON_BYTES: usize = 64 * 1024 * 1024;
const TRUSTED_JSON_CHILDREN: [&str; 8] = [
    "backup.json",
    "progress.json",
    "state.json",
    "benchmark_history.json",
    "audit.json",
    "evidence.json",
    // The native runtime publisher uses this fixed selector as its final,
    // atomic commit point. It is never caller-selected.
    "runtime-current.json",
    // Driver lifecycle evidence is independently durable. It must never be
    // folded into mutable profile state or selected by a caller path.
    "driver-transaction.json",
];

pub(super) fn is_allowed_child(name: &str) -> bool {
    TRUSTED_JSON_CHILDREN.contains(&name)
}

pub(super) fn ensure_bounded_size(length: usize) -> Result<(), String> {
    if length > MAX_TRUSTED_JSON_BYTES {
        return Err(format!(
            "trusted suite JSON exceeds the {} MiB limit",
            MAX_TRUSTED_JSON_BYTES / (1024 * 1024)
        ));
    }
    Ok(())
}

pub(super) fn serialize_json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("serialize trusted suite JSON: {error}"))?;
    bytes.push(b'\n');
    ensure_bounded_size(bytes.len())?;
    Ok(bytes)
}

pub(super) fn temporary_leaf(parent: &str, nonce: u64) -> Result<String, String> {
    if !is_allowed_child(parent) {
        return Err("suite child identity is not allowlisted".into());
    }
    Ok(format!(".frametime-{parent}-{nonce:016x}.tmp"))
}

pub(super) fn is_temporary_leaf(leaf: &str) -> bool {
    let Some(body) = leaf
        .strip_prefix(".frametime-")
        .and_then(|value| value.strip_suffix(".tmp"))
    else {
        return false;
    };
    let Some((parent, nonce)) = body.rsplit_once('-') else {
        return false;
    };
    is_allowed_child(parent)
        && nonce.len() == 16
        && nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_TRUSTED_JSON_BYTES, ensure_bounded_size, is_allowed_child, is_temporary_leaf,
        serialize_json, temporary_leaf,
    };

    #[test]
    fn every_persisted_trusted_child_is_allowlisted() {
        for name in [
            "backup.json",
            "progress.json",
            "state.json",
            "benchmark_history.json",
            "audit.json",
            "evidence.json",
            "runtime-current.json",
            "driver-transaction.json",
        ] {
            assert!(is_allowed_child(name), "{name}");
        }
    }

    #[test]
    fn child_allowlist_rejects_paths_and_lock_files() {
        for name in ["", "backup.json.tmp", "backup.json\\evil", "backup.lock"] {
            assert!(!is_allowed_child(name), "{name}");
        }
    }

    #[test]
    fn temporary_leaf_is_bound_to_an_allowlisted_parent() {
        let leaf = temporary_leaf("state.json", 0x12ab).expect("allowlisted parent");
        assert_eq!(leaf, ".frametime-state.json-00000000000012ab.tmp");
        assert!(is_temporary_leaf(&leaf));
        assert!(!is_temporary_leaf(
            ".frametime-other.json-00000000000012ab.tmp"
        ));
        assert!(!is_temporary_leaf(".frametime-state.json-not-hex.tmp"));
    }

    #[test]
    fn size_limit_is_inclusive_and_rejects_one_extra_byte() {
        assert!(ensure_bounded_size(MAX_TRUSTED_JSON_BYTES).is_ok());
        assert!(ensure_bounded_size(MAX_TRUSTED_JSON_BYTES + 1).is_err());
    }

    #[test]
    fn serialization_stays_pretty_and_newline_terminated() {
        let bytes = serialize_json(&serde_json::json!({"fpsCap": 240})).expect("serialize");
        assert_eq!(bytes, b"{\n  \"fpsCap\": 240\n}\n");
    }
}
