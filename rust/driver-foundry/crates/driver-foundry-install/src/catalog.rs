//! Package catalog and component-selection presets.

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::InstallError;

const CATALOG_SCHEMA: &str = "driver-foundry.catalog/v1";

#[derive(Debug, Clone, Deserialize)]
pub struct PackageDefinition {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogFile {
    pub schema: String,
    pub packages: Vec<PackageDefinition>,
}

#[derive(Debug, Clone)]
pub struct PackageCatalog {
    pub packages: BTreeMap<String, PackageDefinition>,
}

impl PackageCatalog {
    pub fn load_from_file(path: &Path) -> Result<Self, InstallError> {
        let text = fs::read_to_string(path)?;
        Self::load_from_json(&text)
    }

    pub fn load_from_json(json: &str) -> Result<Self, InstallError> {
        let file: CatalogFile = serde_json::from_str(json)?;
        if file.schema != CATALOG_SCHEMA {
            return Err(InstallError::Other(format!(
                "unsupported package catalog schema: {:?}",
                file.schema
            )));
        }
        let mut map = BTreeMap::new();
        let mut folded_ids = BTreeSet::new();
        for p in file.packages {
            validate_component_id(&p.id)?;
            let folded = p.id.to_ascii_lowercase();
            if !folded_ids.insert(folded) {
                return Err(InstallError::Other(format!(
                    "duplicate or case-fold-colliding package id: {}",
                    p.id
                )));
            }
            if map.contains_key(&p.id) {
                return Err(InstallError::Other(format!(
                    "duplicate package id: {}",
                    p.id
                )));
            }
            map.insert(p.id.clone(), p);
        }
        let catalog = Self { packages: map };
        catalog.validate_dependencies()?;
        Ok(catalog)
    }

    pub fn required_ids(&self) -> Vec<String> {
        self.packages
            .values()
            .filter(|p| p.required)
            .map(|p| p.id.clone())
            .collect()
    }

    /// Resolve selection + required packages + dependencies (order-stable).
    pub fn resolve_with_deps(&self, selected: &BTreeSet<String>) -> Vec<String> {
        let mut out = BTreeSet::new();
        for id in self.required_ids() {
            out.insert(id);
        }
        for id in selected {
            self.add_with_deps(id, &mut out);
        }
        let mut v: Vec<_> = out.into_iter().collect();
        v.sort();
        // Ensure Display.Driver first-ish: keep sorted is fine for scaffold
        v
    }

    fn add_with_deps(&self, id: &str, out: &mut BTreeSet<String>) {
        if out.contains(id) {
            return;
        }
        if let Some(pkg) = self.packages.get(id) {
            for dep in &pkg.dependencies {
                self.add_with_deps(dep, out);
            }
            out.insert(id.to_string());
        }
    }

    fn validate_dependencies(&self) -> Result<(), InstallError> {
        for (id, package) in &self.packages {
            for dependency in &package.dependencies {
                validate_component_id(dependency)?;
                if !self.packages.contains_key(dependency) {
                    return Err(InstallError::Other(format!(
                        "package {id} depends on unknown component {dependency}"
                    )));
                }
            }
        }
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        for id in self.packages.keys() {
            self.visit_dependency(id, &mut visiting, &mut visited)?;
        }
        Ok(())
    }

    fn visit_dependency(
        &self,
        id: &str,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> Result<(), InstallError> {
        if visited.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id.to_string()) {
            return Err(InstallError::Other(format!(
                "package dependency cycle detected at {id}"
            )));
        }
        for dependency in &self.packages[id].dependencies {
            self.visit_dependency(dependency, visiting, visited)?;
        }
        visiting.remove(id);
        visited.insert(id.to_string());
        Ok(())
    }
}

/// Windows-safe leaf component names used to derive directories and filenames.
fn validate_component_id(id: &str) -> Result<(), InstallError> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.ends_with(['.', ' '])
        || id.bytes().any(|byte| byte < 0x20)
        || id.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || Path::new(id).file_name().is_none()
    {
        return Err(InstallError::Other(format!(
            "unsafe package component id: {id:?}"
        )));
    }
    let stem = id
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        return Err(InstallError::Other(format!(
            "reserved Windows package component id: {id:?}"
        )));
    }
    Ok(())
}

/// Named component-selection presets.
pub struct SelectionPresets;

impl SelectionPresets {
    pub const MINIMAL: &'static str = "minimal";
    pub const CLEAN: &'static str = "clean";
    pub const RECOMMENDED: &'static str = "recommended";
    pub const NOTEBOOK: &'static str = "notebook";
    pub const GAMING: &'static str = "gaming";
    pub const FULL: &'static str = "full";

    pub fn all_ids() -> &'static [&'static str] {
        &[
            Self::MINIMAL,
            Self::CLEAN,
            Self::RECOMMENDED,
            Self::NOTEBOOK,
            Self::GAMING,
            Self::FULL,
        ]
    }

    pub fn is_known(preset: &str) -> bool {
        let p = preset.trim().to_ascii_lowercase();
        Self::all_ids().iter().any(|id| *id == p)
    }

    pub fn optional_selections(preset: &str) -> Result<&'static [&'static str], InstallError> {
        match preset.trim().to_ascii_lowercase().as_str() {
            Self::MINIMAL => Ok(&[]),
            Self::CLEAN => Ok(&["HDAudio", "PhysX", "NGXCore"]),
            Self::RECOMMENDED => Ok(&["HDAudio", "PhysX", "NGXCore", "MSVCRT"]),
            Self::NOTEBOOK => Ok(&["HDAudio", "PhysX", "NGXCore", "MSVCRT", "Display.Optimus"]),
            Self::GAMING => Ok(&["HDAudio", "PhysX", "NGXCore", "MSVCRT", "NvCpl"]),
            Self::FULL => Ok(&[]), // special-cased
            other => Err(InstallError::UnknownPreset(other.into())),
        }
    }

    pub fn create_selection(
        catalog: &PackageCatalog,
        preset: &str,
    ) -> Result<BTreeSet<String>, InstallError> {
        let p = preset.trim().to_ascii_lowercase();
        if p == Self::FULL {
            let bloat = bloat_telemetry_ids();
            let mut set = BTreeSet::new();
            for pkg in catalog.packages.values() {
                if pkg.required {
                    continue;
                }
                if bloat.contains(pkg.id.as_str()) {
                    continue;
                }
                set.insert(pkg.id.clone());
            }
            return Ok(set);
        }
        let opts = Self::optional_selections(&p)?;
        let mut set = BTreeSet::new();
        for id in opts {
            if catalog.packages.contains_key(*id) {
                set.insert((*id).to_string());
            }
        }
        Ok(set)
    }
}

fn bloat_telemetry_ids() -> BTreeSet<&'static str> {
    [
        "GFExperience",
        "GFExperience.NvStreamSrv",
        "NvTelemetry",
        "NvBackend",
        "ShadowPlay",
        "Update.Core",
        "NvApp",
        "NvApp.MessageBus",
        "nodejs",
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use driver_foundry_common::{catalog_path, resolve_data_root};

    #[test]
    fn loads_shipped_catalog() {
        let path = catalog_path(&resolve_data_root());
        let cat = PackageCatalog::load_from_file(&path).expect("catalog");
        assert!(cat.packages.contains_key("Display.Driver"));
        assert!(cat.packages["Display.Driver"].required);
    }

    #[test]
    fn clean_preset_resolves_display_driver() {
        let path = catalog_path(&resolve_data_root());
        let cat = PackageCatalog::load_from_file(&path).unwrap();
        let sel = SelectionPresets::create_selection(&cat, "clean").unwrap();
        let resolved = cat.resolve_with_deps(&sel);
        assert!(resolved.iter().any(|id| id == "Display.Driver"));
        assert!(resolved.iter().any(|id| id == "HDAudio"));
    }

    #[test]
    fn rejects_unsafe_duplicate_unknown_and_cyclic_components() {
        for json in [
            r#"{"schema":"driver-foundry.catalog/v1","packages":[{"id":"../escape"}]}"#,
            r#"{"schema":"driver-foundry.catalog/v1","packages":[{"id":"CON"}]}"#,
            r#"{"schema":"driver-foundry.catalog/v1","packages":[{"id":"A"},{"id":"a"}]}"#,
            r#"{"schema":"driver-foundry.catalog/v1","packages":[{"id":"A","dependencies":["missing"]}]}"#,
            r#"{"schema":"driver-foundry.catalog/v1","packages":[{"id":"A","dependencies":["B"]},{"id":"B","dependencies":["A"]}]}"#,
        ] {
            assert!(
                PackageCatalog::load_from_json(json).is_err(),
                "catalog={json}"
            );
        }
    }

    #[test]
    fn rejects_missing_or_unrecognized_catalog_schema() {
        for json in [
            r#"{"packages":[]}"#,
            r#"{"schema":"driver-foundry.catalog/v2","packages":[]}"#,
        ] {
            assert!(
                PackageCatalog::load_from_json(json).is_err(),
                "catalog={json}"
            );
        }
    }
}
