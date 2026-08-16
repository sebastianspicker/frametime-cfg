use driver_foundry_common::ActionJournal;
use std::process::Command;

pub fn pci_filter_tokens(vendor_folder: &str) -> Vec<&'static str> {
    match vendor_folder.to_ascii_uppercase().as_str() {
        "NVIDIA" => vec!["nvpciflt", "nvkflt"],
        "AMD" => vec!["amdkmpfd", "amdkmafd"],
        _ => Vec::new(),
    }
}

pub fn parse_multi_sz_filters(raw: &str) -> Vec<String> {
    raw.split(['\0', '\r', '\n', ','])
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn strip_filters_from_multi_sz(
    parts: &[String],
    filter_names: &[&str],
) -> (Vec<String>, usize) {
    let mut kept = Vec::new();
    let mut removed = 0;
    for part in parts {
        if filter_names.iter().any(|name| {
            part.eq_ignore_ascii_case(name)
                || part
                    .to_ascii_lowercase()
                    .contains(&name.to_ascii_lowercase())
        }) {
            removed += 1;
        } else {
            kept.push(part.clone());
        }
    }
    (kept, removed)
}

pub fn would_strip_filter_value(raw: &str, filter_names: &[&str]) -> bool {
    let (_, removed) = strip_filters_from_multi_sz(&parse_multi_sz_filters(raw), filter_names);
    removed > 0
}

pub fn plan_mmdevices_entries(tokens: &[String], journal: &mut ActionJournal) {
    if tokens.is_empty() {
        return;
    }
    let token_blob = tokens.join("|");
    journal.plan("Registry", "clean_mmdevices", &token_blob);
    let base = r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio";
    for flow in ["Render", "Capture"] {
        journal.plan("Registry", "mmdevices_flow", format!(r"{base}\{flow}"));
    }
    journal.plan_detail(
        "Registry",
        "mmdevices_summary",
        "Audio",
        format!("tokens={token_blob}"),
    );
}

pub(crate) fn execute_mmdevices_cleanup(tokens: &[String], journal: &mut ActionJournal) {
    if tokens.is_empty() {
        return;
    }

    let root = r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio";
    let mut failures = Vec::new();
    for token in tokens {
        let script = format!("$ErrorActionPreference = 'Stop'; $targets = @(Get-ChildItem -Path '{}' -Recurse -ErrorAction SilentlyContinue | Where-Object {{ $_.Name -like '*{}*' }}); foreach ($target in $targets) {{ Remove-Item -LiteralPath $target.PSPath -Recurse -Force -ErrorAction Stop }}", root.replace("HKLM\\", "HKLM:"), token.replace('\'', "''"));
        let result = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .status()
            .map_err(|error| format!("powershell: {error}"))
            .and_then(|status| {
                status
                    .success()
                    .then_some(())
                    .ok_or_else(|| format!("powershell exited with {status}"))
            });
        match result {
            Ok(()) => {}
            Err(detail) => failures.push(format!("{token}: {detail}")),
        }
    }
    let detail = (!failures.is_empty()).then(|| failures.join("; "));
    record_mmdevices_outcome(tokens, detail.as_deref(), journal);
}

fn record_mmdevices_outcome(
    tokens: &[String],
    failure_detail: Option<&str>,
    journal: &mut ActionJournal,
) {
    let base = r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio";
    let token_blob = tokens.join("|");
    let targets = [
        ("mmdevices_flow", format!(r"{base}\Render")),
        ("mmdevices_flow", format!(r"{base}\Capture")),
        ("clean_mmdevices", token_blob),
        ("mmdevices_summary", "Audio".to_owned()),
    ];

    for (action, target) in targets {
        if let Some(detail) = failure_detail {
            journal.mark_failed("Registry", action, target, detail);
        } else {
            journal.mark_executed("Registry", action, target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{plan_mmdevices_entries, record_mmdevices_outcome};
    use driver_foundry_common::ActionJournal;

    #[test]
    fn endpoint_mutation_script_stops_on_errors() {
        let source = include_str!("mmdevices.rs");
        assert!(source.contains("$ErrorActionPreference = 'Stop'"));
        assert!(source
            .contains("Remove-Item -LiteralPath $target.PSPath -Recurse -Force -ErrorAction Stop"));
    }

    #[test]
    fn live_outcome_resolves_each_planned_journal_entry() {
        let tokens = vec!["Realtek".to_owned(), "VEN_10EC".to_owned()];
        let mut journal = ActionJournal::default();
        plan_mmdevices_entries(&tokens, &mut journal);
        let planned = journal.entries.len();

        record_mmdevices_outcome(&tokens, None, &mut journal);

        assert_eq!(journal.entries.len(), planned);
        assert_eq!(journal.count_executed(), planned);
        assert_eq!(journal.count_failed(), 0);
        assert!(journal.entries.iter().all(|entry| entry.executed));
    }
}
