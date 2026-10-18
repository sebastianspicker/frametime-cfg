//! Batch-script label resolution.
//!
//! `cmd.exe` resolves `call :label` and `goto label` only at run time, so a
//! missing label aborts a packaging lane long after the script started. This
//! check runs on every host instead.

use std::collections::BTreeSet;

/// The `call :label` and `goto label` targets in `source` that no `:label`
/// line defines, lower-cased and sorted. `:eof` is built in and never reported.
pub fn undefined_batch_labels(source: &str) -> Vec<String> {
    let mut defined = BTreeSet::new();
    let mut used = BTreeSet::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(label) = trimmed.strip_prefix(':') {
            if !label.starts_with(':') {
                defined.insert(first_word(label));
            }
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        for (keyword, needs_colon) in [("call ", true), ("goto ", false)] {
            let mut rest = lower.as_str();
            while let Some(index) = rest.find(keyword) {
                rest = &rest[index + keyword.len()..];
                let target = rest.trim_start();
                match target.strip_prefix(':') {
                    Some(label) => {
                        used.insert(first_word(label));
                    }
                    None if !needs_colon => {
                        used.insert(first_word(target));
                    }
                    None => {}
                }
            }
        }
    }
    used.remove("eof");
    used.remove("");
    used.difference(&defined).cloned().collect()
}

fn first_word(text: &str) -> String {
    text.split(|character: char| character.is_whitespace() || character == '"')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::undefined_batch_labels;
    use crate::{read_utf8, repository_root};

    #[test]
    fn reports_called_labels_without_a_definition() {
        let source = "call :present \"a\" || exit /b 1\r\nif x call :Missing\r\ngoto done\r\n:: comment\r\n:PRESENT\r\ngoto :eof\r\n:done\r\n";
        assert_eq!(undefined_batch_labels(source), vec!["missing".to_owned()]);
    }

    #[test]
    fn repository_batch_scripts_define_every_called_label() {
        let scripts = repository_root().join("scripts");
        let mut checked = 0;
        for entry in fs::read_dir(&scripts).expect("read scripts directory") {
            let path = entry.expect("read scripts entry").path();
            if path.extension().is_some_and(|extension| extension == "cmd") {
                checked += 1;
                assert_eq!(
                    undefined_batch_labels(&read_utf8(&path)),
                    Vec::<String>::new(),
                    "{} calls undefined labels",
                    path.display()
                );
            }
        }
        assert!(checked > 0, "no batch scripts found under scripts/");
    }
}
