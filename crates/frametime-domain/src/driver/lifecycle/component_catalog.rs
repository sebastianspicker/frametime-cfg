use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{DriverLifecycleError, NVIDIA_COMPONENT_CATALOG_SCHEMA_VERSION};

const CANONICAL_NVIDIA_COMPONENTS: &str =
    include_str!("../../../../../assets/nvidia-components.v1.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum NvidiaCleanupScope {
    #[default]
    Default,
    NvidiaApp,
    GeforceExperience,
    Broadcast,
    ControlPanel,
    PhysX,
    HdAudio,
    Monitors,
    DriverSearchPolicy,
}

/// Stable built-in component selections. `Full` retains every catalog item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NvidiaComponentPreset {
    Minimal,
    Clean,
    Recommended,
    Notebook,
    Gaming,
    Full,
}

impl NvidiaComponentPreset {
    pub const ALL: [Self; 6] = [
        Self::Minimal,
        Self::Clean,
        Self::Recommended,
        Self::Notebook,
        Self::Gaming,
        Self::Full,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NvidiaComponentDefinition {
    pub id: String,
    pub title: String,
    pub required: bool,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub description: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NvidiaComponentCatalogFile {
    schema_version: u32,
    components: Vec<NvidiaComponentDefinition>,
}

/// A canonical, build-time compiled catalog. It cannot be redirected to an
/// ambient file or extended by a caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaComponentCatalog {
    components: BTreeMap<String, NvidiaComponentDefinition>,
}

impl NvidiaComponentCatalog {
    pub fn canonical() -> Result<Self, DriverLifecycleError> {
        Self::from_json(CANONICAL_NVIDIA_COMPONENTS)
    }

    pub fn from_json(input: &str) -> Result<Self, DriverLifecycleError> {
        let parsed: NvidiaComponentCatalogFile =
            serde_json::from_str(input).map_err(|_| DriverLifecycleError::InvalidCatalog)?;
        Self::validate_header(&parsed)?;
        let components = Self::insert_components(parsed.components)?;
        Self::validate_required_driver(&components)?;
        Self::validate_dependencies(&components)?;
        let catalog = Self { components };
        catalog.validate_cycles()?;
        Ok(catalog)
    }

    fn validate_header(file: &NvidiaComponentCatalogFile) -> Result<(), DriverLifecycleError> {
        if file.schema_version == NVIDIA_COMPONENT_CATALOG_SCHEMA_VERSION
            && file.components.len() == 35
        {
            Ok(())
        } else {
            Err(DriverLifecycleError::InvalidCatalog)
        }
    }

    fn insert_components(
        definitions: Vec<NvidiaComponentDefinition>,
    ) -> Result<BTreeMap<String, NvidiaComponentDefinition>, DriverLifecycleError> {
        let mut components = BTreeMap::new();
        let mut folded = BTreeSet::new();
        for component in definitions {
            Self::insert_component(&mut components, &mut folded, component)?;
        }
        Ok(components)
    }

    fn insert_component(
        components: &mut BTreeMap<String, NvidiaComponentDefinition>,
        folded: &mut BTreeSet<String>,
        component: NvidiaComponentDefinition,
    ) -> Result<(), DriverLifecycleError> {
        if !valid_component_directory(&component.id)
            || component.title.trim().is_empty()
            || component.description.trim().is_empty()
            || !folded.insert(component.id.to_ascii_lowercase())
            || components.insert(component.id.clone(), component).is_some()
        {
            return Err(DriverLifecycleError::InvalidCatalog);
        }
        Ok(())
    }

    fn validate_required_driver(
        components: &BTreeMap<String, NvidiaComponentDefinition>,
    ) -> Result<(), DriverLifecycleError> {
        if components
            .get("Display.Driver")
            .is_some_and(|component| component.required)
        {
            Ok(())
        } else {
            Err(DriverLifecycleError::InvalidCatalog)
        }
    }

    fn validate_dependencies(
        components: &BTreeMap<String, NvidiaComponentDefinition>,
    ) -> Result<(), DriverLifecycleError> {
        for component in components.values() {
            let mut dependencies = BTreeSet::new();
            if component.dependencies.iter().any(|dependency| {
                !valid_component_directory(dependency)
                    || !components.contains_key(dependency)
                    || !dependencies.insert(dependency)
            }) {
                return Err(DriverLifecycleError::InvalidCatalog);
            }
        }
        Ok(())
    }

    fn validate_cycles(&self) -> Result<(), DriverLifecycleError> {
        for id in self.components.keys() {
            self.ensure_acyclic(id, &mut BTreeSet::new(), &mut BTreeSet::new())?;
        }
        Ok(())
    }

    #[must_use]
    pub fn component(&self, id: &str) -> Option<&NvidiaComponentDefinition> {
        self.components.get(id)
    }

    pub fn components(&self) -> impl Iterator<Item = &NvidiaComponentDefinition> {
        self.components.values()
    }

    /// Resolves a preset plus repeated caller selections and deselections.
    /// Required components, `Display.Driver`, and selected dependency closure
    /// are retained even if they occur in `deselects`.
    pub fn resolve_selection(
        &self,
        preset: NvidiaComponentPreset,
        source_directories: &[String],
        selects: &[String],
        deselects: &[String],
    ) -> Result<NvidiaComponentSelection, DriverLifecycleError> {
        let source_components = self.classify_source_directories(source_directories)?;
        let mut direct = self.preset_selection(preset);
        for id in selects {
            self.require_component(id)?;
            direct.insert(id.clone());
        }
        let required = self.required_ids();
        for id in deselects {
            self.require_component(id)?;
            if !required.contains(id) {
                direct.remove(id);
            }
        }
        direct.extend(required.iter().cloned());
        let selected = self.dependency_closure(&direct)?;
        let mut protected = required;
        protected.insert("Display.Driver".to_owned());
        for id in &selected {
            if deselects.iter().any(|deselect| deselect == id) {
                protected.insert(id.clone());
            }
        }
        let required_unclassified = source_components
            .iter()
            .filter_map(|item| match item {
                SourceComponentClassification::RequiredUnclassified(item) => Some(item.clone()),
                SourceComponentClassification::Known(_) => None,
            })
            .collect();
        Ok(NvidiaComponentSelection {
            preset,
            selected,
            protected,
            source_components,
            required_unclassified,
        })
    }

    pub fn classify_source_directories(
        &self,
        directories: &[String],
    ) -> Result<Vec<SourceComponentClassification>, DriverLifecycleError> {
        let mut unique = BTreeSet::new();
        for directory in directories {
            if !valid_component_directory(directory) {
                return Err(DriverLifecycleError::UnsafeComponentDirectory(
                    directory.clone(),
                ));
            }
            unique.insert(directory.clone());
        }
        Ok(unique
            .into_iter()
            .map(|directory| {
                if self.components.contains_key(&directory) {
                    SourceComponentClassification::Known(directory)
                } else {
                    SourceComponentClassification::RequiredUnclassified(
                        RequiredUnclassifiedComponent { directory },
                    )
                }
            })
            .collect())
    }

    fn require_component(&self, id: &str) -> Result<(), DriverLifecycleError> {
        if self.components.contains_key(id) {
            Ok(())
        } else {
            Err(DriverLifecycleError::UnknownComponent(id.to_owned()))
        }
    }

    fn required_ids(&self) -> BTreeSet<String> {
        self.components
            .values()
            .filter(|component| component.required)
            .map(|component| component.id.clone())
            .collect()
    }

    fn preset_selection(&self, preset: NvidiaComponentPreset) -> BTreeSet<String> {
        let ids: &[&str] = match preset {
            NvidiaComponentPreset::Minimal => &[],
            NvidiaComponentPreset::Clean => &["HDAudio", "PhysX", "NGXCore"],
            NvidiaComponentPreset::Recommended => &["HDAudio", "PhysX", "NGXCore", "MSVCRT"],
            NvidiaComponentPreset::Notebook => {
                &["HDAudio", "PhysX", "NGXCore", "MSVCRT", "Display.Optimus"]
            }
            NvidiaComponentPreset::Gaming => &["HDAudio", "PhysX", "NGXCore", "MSVCRT", "NvCpl"],
            NvidiaComponentPreset::Full => return self.components.keys().cloned().collect(),
        };
        ids.iter()
            .filter(|id| self.components.contains_key(**id))
            .map(|id| (*id).to_owned())
            .collect()
    }

    fn dependency_closure(
        &self,
        selected: &BTreeSet<String>,
    ) -> Result<BTreeSet<String>, DriverLifecycleError> {
        let mut closure = BTreeSet::new();
        for id in selected {
            self.add_with_dependencies(id, &mut closure)?;
        }
        Ok(closure)
    }

    fn add_with_dependencies(
        &self,
        id: &str,
        closure: &mut BTreeSet<String>,
    ) -> Result<(), DriverLifecycleError> {
        if !closure.insert(id.to_owned()) {
            return Ok(());
        }
        let component = self
            .components
            .get(id)
            .ok_or_else(|| DriverLifecycleError::UnknownComponent(id.to_owned()))?;
        for dependency in &component.dependencies {
            self.add_with_dependencies(dependency, closure)?;
        }
        Ok(())
    }

    fn ensure_acyclic(
        &self,
        id: &str,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> Result<(), DriverLifecycleError> {
        if visited.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id.to_owned()) {
            return Err(DriverLifecycleError::InvalidCatalog);
        }
        for dependency in &self.components[id].dependencies {
            self.ensure_acyclic(dependency, visiting, visited)?;
        }
        visiting.remove(id);
        visited.insert(id.to_owned());
        Ok(())
    }
}

fn valid_component_directory(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= 128
        && !value.ends_with(['.', ' '])
        && !value.bytes().any(|byte| byte < 0x20)
        && !value.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiredUnclassifiedComponent {
    pub directory: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "classification", content = "value")]
pub enum SourceComponentClassification {
    Known(String),
    RequiredUnclassified(RequiredUnclassifiedComponent),
}

/// A deterministic selection result. All sets are sorted and therefore stable
/// for persisted preview and native plan input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NvidiaComponentSelection {
    pub preset: NvidiaComponentPreset,
    pub selected: BTreeSet<String>,
    pub protected: BTreeSet<String>,
    pub source_components: Vec<SourceComponentClassification>,
    pub required_unclassified: BTreeSet<RequiredUnclassifiedComponent>,
}
