//! Vendor catalog loader for `settings/<VENDOR>/*.cfg` data.

use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("catalog not found: {0}")]
    NotFound(PathBuf),
    #[error("backup catalogs (.bak) are not loadable: {0}")]
    BackupRejected(String),
    #[error("unsafe catalog path component: {0}")]
    UnsafePathComponent(String),
    #[error("unsafe catalog entry in {file}: {entry:?}")]
    UnsafeEntry { file: String, entry: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Read catalog lines: non-empty, trimmed; rejects `.bak`.
pub fn load_lines(
    settings_root: &Path,
    vendor: &str,
    file_name: &str,
) -> Result<Vec<String>, CatalogError> {
    validate_path_component(vendor)?;
    validate_path_component(file_name)?;
    if file_name.to_ascii_lowercase().ends_with(".bak") {
        return Err(CatalogError::BackupRejected(file_name.into()));
    }
    let path = settings_root.join(vendor).join(file_name);
    if !path.is_file() {
        return Err(CatalogError::NotFound(path));
    }
    let text = fs::read_to_string(&path)?;
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            validate_entry(line).map_err(|_| CatalogError::UnsafeEntry {
                file: file_name.into(),
                entry: line.into(),
            })?;
            Ok(line.to_owned())
        })
        .collect()
}

fn validate_path_component(component: &str) -> Result<(), CatalogError> {
    if component.is_empty()
        || component.contains(['/', '\\'])
        || component.contains("..")
        || !component
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(CatalogError::UnsafePathComponent(component.into()));
    }
    Ok(())
}

/// Catalog values drive elevated deletion actions. Keep them as a small data language rather
/// than command text: no quoting, control bytes, PowerShell metacharacters, or traversal.
fn validate_entry(entry: &str) -> Result<(), ()> {
    if entry.len() > 260 || entry.contains("..") {
        return Err(());
    }
    entry
        .bytes()
        .all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b' ' | b'\\'
                        | b'/'
                        | b'.'
                        | b'_'
                        | b'-'
                        | b','
                        | b':'
                        | b'&'
                        | b'%'
                        | b'*'
                        | b'('
                        | b')'
                        | b'{'
                        | b'}'
                        | b'$'
                        | b'@'
                        | b'='
                        | b'+'
                )
        })
        .then_some(())
        .ok_or(())
}

pub fn try_load_lines(settings_root: &Path, vendor: &str, file_name: &str) -> Vec<String> {
    load_lines(settings_root, vendor, file_name).unwrap_or_default()
}

pub fn list_cfg_files(settings_root: &Path, vendor: &str) -> Result<Vec<String>, CatalogError> {
    let dir = settings_root.join(vendor);
    if !dir.is_dir() {
        return Err(CatalogError::NotFound(dir));
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".bak") {
            continue;
        }
        if lower.ends_with(".cfg") {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use driver_foundry_common::{resolve_data_root, settings_root};

    #[test]
    fn loads_nvidia_services() {
        let root = settings_root(&resolve_data_root());
        let lines = load_lines(&root, "NVIDIA", "services.cfg").expect("services.cfg");
        assert!(!lines.is_empty());
        assert!(lines.iter().any(|l| l.to_ascii_lowercase().contains("nv")));
    }

    #[test]
    fn loads_realtek_services() {
        let root = settings_root(&resolve_data_root());
        let lines = load_lines(&root, "REALTEK", "services.cfg").expect("realtek services");
        assert!(!lines.is_empty());
    }

    #[test]
    fn rejects_bak() {
        let root = settings_root(&resolve_data_root());
        let err = load_lines(&root, "NVIDIA", "gfedriverfiles.cfg.bak").unwrap_err();
        assert!(matches!(err, CatalogError::BackupRejected(_)));
    }

    #[test]
    fn rejects_traversal_and_command_injection_entries() {
        let root = std::env::temp_dir().join(format!("dfoundry-catalog-{}", std::process::id()));
        let vendor = root.join("NVIDIA");
        fs::create_dir_all(&vendor).unwrap();
        fs::write(
            vendor.join("services.cfg"),
            "nvsvc\nname; Remove-Item C:\\\nquoted'entry\nnewline\x01",
        )
        .unwrap();
        let error = load_lines(&root, "NVIDIA", "services.cfg").unwrap_err();
        assert!(matches!(error, CatalogError::UnsafeEntry { .. }));
        assert!(matches!(
            load_lines(&root, "../NVIDIA", "services.cfg"),
            Err(CatalogError::UnsafePathComponent(_))
        ));
        let _ = fs::remove_dir_all(root);
    }
}
