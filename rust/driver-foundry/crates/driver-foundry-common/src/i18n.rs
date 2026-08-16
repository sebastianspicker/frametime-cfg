//! XML language-pack loader.

use std::fs;
use std::path::{Path, PathBuf};

/// List available language pack basenames (without .xml) under settings/Languages.
pub fn list_language_packs(settings_root: &Path) -> Vec<String> {
    let dir = settings_root.join("Languages");
    let Ok(rd) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("_For translators - ReadMe.txt") {
            continue;
        }
        if name.to_ascii_lowercase().ends_with(".xml") {
            names.push(
                name.trim_end_matches(".xml")
                    .trim_end_matches(".XML")
                    .to_string(),
            );
        }
    }
    names.sort();
    names
}

/// Load raw XML language pack text if present.
pub fn load_language_pack(settings_root: &Path, name: &str) -> Option<String> {
    let dir = settings_root.join("Languages");
    let candidates = [
        dir.join(format!("{name}.xml")),
        dir.join(format!("{name}.XML")),
    ];
    for c in candidates {
        if c.is_file() {
            return fs::read_to_string(c).ok();
        }
    }
    // Case-insensitive scan
    let Ok(rd) = fs::read_dir(&dir) else {
        return None;
    };
    for entry in rd.flatten() {
        let fname = entry.file_name().to_string_lossy().into_owned();
        let stem = fname.trim_end_matches(".xml").trim_end_matches(".XML");
        if stem.eq_ignore_ascii_case(name) {
            return fs::read_to_string(entry.path()).ok();
        }
    }
    None
}

pub fn languages_dir(settings_root: &Path) -> PathBuf {
    settings_root.join("Languages")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{resolve_data_root, settings_root};

    #[test]
    fn lists_shipped_language_packs_when_present() {
        let root = settings_root(&resolve_data_root());
        let packs = list_language_packs(&root);
        // Optional polish: if Languages shipped, must be non-empty XML set
        if languages_dir(&root).is_dir() {
            assert!(
                !packs.is_empty(),
                "Languages dir exists but no packs found under {}",
                root.display()
            );
            // English-family or German commonly present in DDU set
            let joined = packs.join("|").to_ascii_lowercase();
            assert!(
                joined.contains("german")
                    || joined.contains("french")
                    || joined.contains("english")
                    || packs.len() > 5,
                "unexpected pack set: {packs:?}"
            );
        }
    }

    #[test]
    fn load_pack_returns_xml_when_present() {
        let root = settings_root(&resolve_data_root());
        let packs = list_language_packs(&root);
        if let Some(first) = packs.first() {
            let text = load_language_pack(&root, first).expect("load pack");
            assert!(
                text.contains('<') && text.len() > 20,
                "language pack should be XML content"
            );
        }
    }
}
