//! Shared Driver Foundry product identity, journal, elevation, and path helpers.

use std::path::{Path, PathBuf};

pub mod elevation;
pub mod i18n;
pub mod pnputil;

pub const PRODUCT_NAME: &str = "Driver Foundry";
pub const COMMAND_NAME: &str = "dfoundry";
pub const PRODUCT_TAGLINE: &str = "Remove cleanly. Install only what you need.";
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn version_line() -> String {
    format!("{PRODUCT_NAME} {PRODUCT_VERSION}")
}

/// One planned or executed host action (dry-run journals planned only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub surface: String,
    pub action: String,
    pub target: String,
    pub executed: bool,
    pub detail: String,
}

/// Action journal: dry-run keeps `executed == false` for mutations.
#[derive(Debug, Default, Clone)]
pub struct ActionJournal {
    pub entries: Vec<JournalEntry>,
}

impl ActionJournal {
    pub fn plan(
        &mut self,
        surface: impl Into<String>,
        action: impl Into<String>,
        target: impl Into<String>,
    ) {
        self.entries.push(JournalEntry {
            surface: surface.into(),
            action: action.into(),
            target: target.into(),
            executed: false,
            detail: String::new(),
        });
    }

    pub fn plan_detail(
        &mut self,
        surface: impl Into<String>,
        action: impl Into<String>,
        target: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.entries.push(JournalEntry {
            surface: surface.into(),
            action: action.into(),
            target: target.into(),
            executed: false,
            detail: detail.into(),
        });
    }

    /// Mark the most recent matching planned entry as executed, or append executed entry.
    pub fn mark_executed(
        &mut self,
        surface: impl Into<String>,
        action: impl Into<String>,
        target: impl Into<String>,
    ) {
        let surface = surface.into();
        let action = action.into();
        let target = target.into();
        if let Some(e) = self.entries.iter_mut().rev().find(|e| {
            !e.executed && e.surface == surface && e.action == action && e.target == target
        }) {
            e.executed = true;
            return;
        }
        self.entries.push(JournalEntry {
            surface,
            action,
            target,
            executed: true,
            detail: String::new(),
        });
    }

    /// Record why a planned action did not complete without counting it as executed.
    pub fn mark_failed(
        &mut self,
        surface: impl Into<String>,
        action: impl Into<String>,
        target: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let surface = surface.into();
        let action = action.into();
        let target = target.into();
        let detail = format!("failed: {}", detail.into());
        if let Some(entry) = self.entries.iter_mut().rev().find(|entry| {
            !entry.executed
                && entry.surface == surface
                && entry.action == action
                && entry.target == target
        }) {
            entry.detail = detail;
            return;
        }
        self.entries.push(JournalEntry {
            surface,
            action,
            target,
            executed: false,
            detail,
        });
    }

    pub fn count_planned(&self) -> usize {
        self.entries.len()
    }

    pub fn count_executed(&self) -> usize {
        self.entries.iter().filter(|e| e.executed).count()
    }

    pub fn count_failed(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| !entry.executed && entry.detail.starts_with("failed:"))
            .count()
    }

    pub fn surface_counts(&self) -> Vec<(String, usize)> {
        let mut map: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for e in &self.entries {
            *map.entry(e.surface.clone()).or_default() += 1;
        }
        map.into_iter().collect()
    }
}

/// Resolve data root: env DFOUNDRY_DATA_DIR, beside exe `data/`, or walk-up for `data/`.
pub fn resolve_data_root() -> PathBuf {
    if let Ok(p) = std::env::var("DFOUNDRY_DATA_DIR") {
        let pb = PathBuf::from(p);
        if pb.is_dir() {
            return pb;
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join("data");
            if beside.is_dir() {
                return beside;
            }
            if let Some(ws) = walk_for_data(dir) {
                return ws;
            }
        }
    }

    let manifest_candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data"),
        PathBuf::from("data"),
    ];
    for c in manifest_candidates {
        if let Ok(c) = c.canonicalize() {
            if c.is_dir() {
                return c;
            }
        } else if c.is_dir() {
            return c;
        }
    }

    PathBuf::from("data")
}

fn walk_for_data(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    for _ in 0..8 {
        let d = dir?;
        let candidate = d.join("data");
        if candidate.join("settings").is_dir() || candidate.join("catalog").is_dir() {
            return Some(candidate);
        }
        if d.file_name().and_then(|s| s.to_str()) == Some("driver-foundry") {
            let c = d.join("data");
            if c.is_dir() {
                return Some(c);
            }
        }
        dir = d.parent();
    }
    None
}

pub fn settings_root(data: &Path) -> PathBuf {
    data.join("settings")
}

pub fn catalog_path(data: &Path) -> PathBuf {
    data.join("catalog").join("packages.v1.json")
}

pub fn driver_index_path(data: &Path) -> PathBuf {
    data.join("catalog").join("driver-index.v1.json")
}

/// Expand common env-style tokens in path strings.
pub fn expand_path_tokens(s: &str) -> PathBuf {
    let mut out = s.to_string();
    let replacements = [
        (
            "%LOCALAPPDATA%",
            std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
                std::env::var("USERPROFILE")
                    .map(|u| format!(r"{u}\AppData\Local"))
                    .unwrap_or_default()
            }),
        ),
        (
            "%APPDATA%",
            std::env::var("APPDATA").unwrap_or_else(|_| {
                std::env::var("USERPROFILE")
                    .map(|u| format!(r"{u}\AppData\Roaming"))
                    .unwrap_or_default()
            }),
        ),
        (
            "%TEMP%",
            std::env::temp_dir().to_string_lossy().into_owned(),
        ),
        (
            "%PROGRAMDATA%",
            std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into()),
        ),
        (
            "%PROGRAMFILES%",
            std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into()),
        ),
        (
            "%WINDIR%",
            std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into()),
        ),
        (
            "%SYSTEMROOT%",
            std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()),
        ),
    ];
    for (k, v) in replacements {
        out = out.replace(k, &v);
    }
    PathBuf::from(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_non_empty() {
        assert!(!PRODUCT_VERSION.is_empty());
        assert!(version_line().contains(PRODUCT_NAME));
    }

    #[test]
    fn journal_planned_not_executed() {
        let mut j = ActionJournal::default();
        j.plan("Service", "stop", "nvsvc");
        j.plan("File", "delete", r"C:\Windows\System32\nvapi64.dll");
        assert_eq!(j.count_planned(), 2);
        assert_eq!(j.count_executed(), 0);
    }

    #[test]
    fn journal_mark_executed() {
        let mut j = ActionJournal::default();
        j.plan("Service", "stop_delete", "nvlddmkm");
        j.mark_executed("Service", "stop_delete", "nvlddmkm");
        assert_eq!(j.count_planned(), 1);
        assert_eq!(j.count_executed(), 1);
    }

    #[test]
    fn journal_failure_keeps_action_unexecuted() {
        let mut journal = ActionJournal::default();
        journal.plan("Service", "stop_delete", "nvlddmkm");
        journal.mark_failed("Service", "stop_delete", "nvlddmkm", "access denied");
        assert_eq!(journal.count_planned(), 1);
        assert_eq!(journal.count_executed(), 0);
        assert_eq!(journal.count_failed(), 1);
        assert_eq!(journal.entries[0].detail, "failed: access denied");
    }

    #[test]
    fn data_root_resolves() {
        let root = resolve_data_root();
        assert!(
            root.join("settings").exists() || root.join("catalog").exists(),
            "data root should contain settings or catalog: {}",
            root.display()
        );
    }

    #[test]
    fn expand_temp_token() {
        let p = expand_path_tokens(r"%TEMP%\NVIDIA");
        assert!(p.to_string_lossy().contains("NVIDIA"));
    }
}
