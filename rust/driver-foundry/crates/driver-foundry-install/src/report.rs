//! Machine-readable installation run report.

use serde::Serialize;
use std::io::Write;
use std::path::Path;

use crate::InstallError;

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub product: String,
    pub version: String,
    pub dry_run_install: bool,
    pub force_install: bool,
    pub preset: String,
    pub package_source: String,
    pub package_root: String,
    pub prepared_root: String,
    pub kept_components: Vec<String>,
    pub stripped_components: Vec<String>,
    pub setup_arguments: Vec<String>,
    pub not_whql: bool,
    pub stages: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_command: Option<String>,
}

impl RunReport {
    pub fn write_to(&self, path: &Path) -> Result<(), InstallError> {
        let json = serde_json::to_string_pretty(self)?;
        let mut file = crate::copy::create_new_output(path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }
}
