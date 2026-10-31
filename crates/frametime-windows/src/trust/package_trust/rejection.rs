//! Cause-and-remedy texts for package root rejections. Pure formatting only:
//! none of these helpers decides whether a package is accepted.

use std::collections::BTreeSet;

const REPARSE_REMEDY: &str = "is a reparse point (for example a OneDrive or cloud-synced folder); extract the package to a plain local folder such as C:\\frametime-cfg-rust";

pub(super) fn reparse_point(relative: &str) -> String {
    format!("package member {relative} {REPARSE_REMEDY}")
}

pub(super) fn wrong_member_type(name: &str) -> String {
    format!(
        "package member {name} is not the expected type (file or folder); re-extract the ZIP into an empty folder"
    )
}

pub(super) fn not_fixed_drive() -> String {
    "the package must be on a fixed local drive".into()
}

pub(super) fn identity_unavailable(error: &str) -> String {
    format!(
        "inspect package member identity: {error}; the package must be on an NTFS volume (FAT and exFAT volumes cannot provide stable file identities)"
    )
}

pub(super) fn revocation_unavailable() -> String {
    "signature revocation status is offline or unknown: package authentication checks revocation online, so it needs internet access to the signing certificate chain's CRL/OCSP endpoints; the self-signed alpha test certificate itself is unaffected because its root is excluded from the check".into()
}

/// Describe how the enumerated tree differs from the fixed inventory, naming
/// the first offending relative path. Returns `None` when the sets match.
pub(super) fn inventory_difference(
    actual: &BTreeSet<String>,
    expected: &BTreeSet<String>,
) -> Option<String> {
    let extra: Vec<_> = actual.difference(expected).collect();
    if let Some(first) = extra.first() {
        return Some(format!(
            "unexpected file {first} in the package{}; delete it or re-extract the ZIP into an empty folder",
            more(extra.len())
        ));
    }
    let missing: Vec<_> = expected.difference(actual).collect();
    missing.first().map(|first| {
        format!(
            "missing file {first} in the package{}; re-extract the ZIP into an empty folder",
            more(missing.len())
        )
    })
}

fn more(count: usize) -> String {
    match count {
        0 | 1 => String::new(),
        _ => format!(" (and {} more)", count - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn equal_inventories_have_no_difference() {
        assert_eq!(
            inventory_difference(&set(&["a", "b"]), &set(&["a", "b"])),
            None
        );
    }

    #[test]
    fn extra_file_is_named_with_remedy() {
        let message =
            inventory_difference(&set(&["a", "thumbs.db"]), &set(&["a"])).expect("difference");
        assert_eq!(
            message,
            "unexpected file thumbs.db in the package; delete it or re-extract the ZIP into an empty folder"
        );
    }

    #[test]
    fn extra_files_report_remaining_count() {
        let message =
            inventory_difference(&set(&["a", "x", "y"]), &set(&["a"])).expect("difference");
        assert!(message.contains("unexpected file x in the package (and 1 more);"));
    }

    #[test]
    fn missing_file_is_named() {
        let message = inventory_difference(&set(&["a"]), &set(&["a", "b"])).expect("difference");
        assert_eq!(
            message,
            "missing file b in the package; re-extract the ZIP into an empty folder"
        );
    }

    #[test]
    fn extra_file_takes_precedence_over_missing() {
        let message = inventory_difference(&set(&["x"]), &set(&["b"])).expect("difference");
        assert!(message.starts_with("unexpected file x"));
    }

    #[test]
    fn reparse_message_names_member_cause_and_remedy() {
        let message = reparse_point("bin/tool.exe");
        assert!(message.contains("bin/tool.exe is a reparse point"));
        assert!(message.contains("OneDrive"));
        assert!(message.contains("C:\\frametime-cfg-rust"));
    }

    #[test]
    fn volume_messages_state_the_remedy() {
        assert_eq!(
            not_fixed_drive(),
            "the package must be on a fixed local drive"
        );
        assert!(identity_unavailable("boom").contains("must be on an NTFS volume"));
        let revocation = revocation_unavailable();
        assert!(revocation.starts_with("signature revocation status is offline or unknown"));
        assert!(revocation.contains("internet access"));
        assert!(revocation.contains("CRL/OCSP"));
        assert!(revocation.contains("self-signed alpha test certificate"));
        assert!(wrong_member_type("x").contains("package member x"));
    }
}
