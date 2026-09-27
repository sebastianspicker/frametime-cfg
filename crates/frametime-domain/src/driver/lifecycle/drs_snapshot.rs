use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    DriverLifecycleError, MAX_DRS_BINARY_BYTES, MAX_DRS_ITEMS_PER_PROFILE, MAX_DRS_PROFILES,
    MAX_DRS_RECORDS, MAX_DRS_SERIALIZED_BYTES, MAX_DRS_UTF16_UNITS,
    NVIDIA_DRS_SNAPSHOT_SCHEMA_VERSION,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrsItemKey {
    pub profile: String,
    pub kind: DrsItemKind,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DrsItemKind {
    Application,
    Setting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum DrsValue {
    Dword(u32),
    Qword(u64),
    Binary(Vec<u8>),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrsApplicationSnapshot {
    pub executable: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrsSettingSnapshot {
    pub setting_id: u32,
    pub value: DrsValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrsProfileSnapshot {
    pub name: String,
    #[serde(default)]
    pub applications: Vec<DrsApplicationSnapshot>,
    #[serde(default)]
    pub settings: Vec<DrsSettingSnapshot>,
}

/// A lossless, portable DRS backup. It uses no opaque handles or host paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrsSnapshot {
    pub schema_version: u32,
    pub profiles: Vec<DrsProfileSnapshot>,
}

impl DrsSnapshot {
    pub fn validate(&self) -> Result<(), DriverLifecycleError> {
        if self.schema_version != NVIDIA_DRS_SNAPSHOT_SCHEMA_VERSION {
            return Err(DriverLifecycleError::InvalidDrsSnapshot("schemaVersion"));
        }
        if self.profiles.len() > MAX_DRS_PROFILES {
            return Err(DriverLifecycleError::InvalidDrsSnapshot("profiles"));
        }
        let mut profiles = BTreeSet::new();
        let mut records = self.profiles.len();
        for profile in &self.profiles {
            validate_drs_text(&profile.name, "profileName")?;
            if !profiles.insert(&profile.name)
                || profile.applications.len() > MAX_DRS_ITEMS_PER_PROFILE
                || profile.settings.len() > MAX_DRS_ITEMS_PER_PROFILE
            {
                return Err(DriverLifecycleError::InvalidDrsSnapshot("profileItems"));
            }
            records = records
                .checked_add(profile.applications.len())
                .and_then(|value| value.checked_add(profile.settings.len()))
                .ok_or(DriverLifecycleError::InvalidDrsSnapshot("records"))?;
            let mut applications = BTreeSet::new();
            for application in &profile.applications {
                validate_drs_text(&application.executable, "application")?;
                if !applications.insert(&application.executable) {
                    return Err(DriverLifecycleError::InvalidDrsSnapshot("applications"));
                }
            }
            let mut settings = BTreeSet::new();
            for setting in &profile.settings {
                if !settings.insert(setting.setting_id) {
                    return Err(DriverLifecycleError::InvalidDrsSnapshot("settings"));
                }
                match &setting.value {
                    DrsValue::Dword(_) | DrsValue::Qword(_) => {}
                    DrsValue::Binary(value) if value.len() <= MAX_DRS_BINARY_BYTES => {}
                    DrsValue::Binary(_) => {
                        return Err(DriverLifecycleError::InvalidDrsSnapshot("binary"));
                    }
                    DrsValue::String(value) => validate_drs_text(value, "settingValue")?,
                }
            }
        }
        if records > MAX_DRS_RECORDS {
            return Err(DriverLifecycleError::InvalidDrsSnapshot("records"));
        }
        let serialized =
            serde_json::to_vec(self).map_err(|_| DriverLifecycleError::Serialization)?;
        if serialized.len() > MAX_DRS_SERIALIZED_BYTES {
            return Err(DriverLifecycleError::InvalidDrsSnapshot("serialized"));
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<Vec<u8>, DriverLifecycleError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| DriverLifecycleError::Serialization)
    }

    pub fn from_json(input: &[u8]) -> Result<Self, DriverLifecycleError> {
        if input.len() > MAX_DRS_SERIALIZED_BYTES {
            return Err(DriverLifecycleError::InvalidDrsSnapshot("serialized"));
        }
        let snapshot: Self =
            serde_json::from_slice(input).map_err(|_| DriverLifecycleError::Serialization)?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    #[must_use]
    pub fn reconcile(&self, incoming: &Self) -> DrsReconciliation {
        let current = self.item_values();
        let proposed = incoming.item_values();
        let mut items = Vec::new();
        for key in current
            .keys()
            .chain(proposed.keys())
            .collect::<BTreeSet<_>>()
        {
            let status = match (current.get(key), proposed.get(key)) {
                (Some(left), Some(right)) if left == right => DrsReconciliationStatus::Equivalent,
                (Some(_), Some(_)) => DrsReconciliationStatus::Incompatible,
                (None, Some(_)) => DrsReconciliationStatus::IncomingOnly,
                (Some(_), None) => DrsReconciliationStatus::CurrentOnly,
                (None, None) => unreachable!("key came from a source map"),
            };
            items.push(DrsReconciliationItem {
                key: (*key).clone(),
                status,
            });
        }
        DrsReconciliation { items }
    }

    /// Merges incoming data only when every conflict has an explicit key-based
    /// acceptance. Accepted conflicts take the incoming value.
    pub fn merge_accepting(
        &self,
        incoming: &Self,
        accepted_incompatible: &BTreeSet<DrsItemKey>,
    ) -> Result<DrsMergeResult, DriverLifecycleError> {
        self.validate()?;
        incoming.validate()?;
        let reconciliation = self.reconcile(incoming);
        let missing = reconciliation
            .items
            .iter()
            .filter(|item| item.status == DrsReconciliationStatus::Incompatible)
            .filter(|item| !accepted_incompatible.contains(&item.key))
            .map(|item| item.key.clone())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(DriverLifecycleError::IncompatibleDrsItems(missing));
        }
        let mut merged = self.item_values();
        for (key, value) in incoming.item_values() {
            merged.insert(key, value);
        }
        let merged = Self::from_item_values(merged)?;
        Ok(DrsMergeResult {
            reconciliation,
            merged,
        })
    }

    fn item_values(&self) -> BTreeMap<DrsItemKey, DrsItemValue> {
        let mut values = BTreeMap::new();
        for profile in &self.profiles {
            for application in &profile.applications {
                values.insert(
                    DrsItemKey {
                        profile: profile.name.clone(),
                        kind: DrsItemKind::Application,
                        key: application.executable.clone(),
                    },
                    DrsItemValue::Application,
                );
            }
            for setting in &profile.settings {
                values.insert(
                    DrsItemKey {
                        profile: profile.name.clone(),
                        kind: DrsItemKind::Setting,
                        key: setting.setting_id.to_string(),
                    },
                    DrsItemValue::Setting(setting.value.clone()),
                );
            }
        }
        values
    }

    fn from_item_values(
        items: BTreeMap<DrsItemKey, DrsItemValue>,
    ) -> Result<Self, DriverLifecycleError> {
        let mut profiles = BTreeMap::<String, DrsProfileSnapshot>::new();
        for (key, value) in items {
            let profile =
                profiles
                    .entry(key.profile.clone())
                    .or_insert_with(|| DrsProfileSnapshot {
                        name: key.profile,
                        applications: Vec::new(),
                        settings: Vec::new(),
                    });
            match (key.kind, value) {
                (DrsItemKind::Application, DrsItemValue::Application) => {
                    profile.applications.push(DrsApplicationSnapshot {
                        executable: key.key,
                    });
                }
                (DrsItemKind::Setting, DrsItemValue::Setting(value)) => {
                    let setting_id = key
                        .key
                        .parse()
                        .map_err(|_| DriverLifecycleError::InvalidDrsSnapshot("settingId"))?;
                    profile
                        .settings
                        .push(DrsSettingSnapshot { setting_id, value });
                }
                _ => return Err(DriverLifecycleError::InvalidDrsSnapshot("itemKind")),
            }
        }
        let mut profiles = profiles.into_values().collect::<Vec<_>>();
        for profile in &mut profiles {
            profile
                .applications
                .sort_by(|left, right| left.executable.cmp(&right.executable));
            profile.settings.sort_by_key(|setting| setting.setting_id);
        }
        let snapshot = Self {
            schema_version: NVIDIA_DRS_SNAPSHOT_SCHEMA_VERSION,
            profiles,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
}

fn validate_drs_text(value: &str, field: &'static str) -> Result<(), DriverLifecycleError> {
    if value.is_empty()
        || value.chars().any(char::is_control)
        || value.encode_utf16().count() > MAX_DRS_UTF16_UNITS
    {
        Err(DriverLifecycleError::InvalidDrsSnapshot(field))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DrsItemValue {
    Application,
    Setting(DrsValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DrsReconciliationStatus {
    Equivalent,
    IncomingOnly,
    CurrentOnly,
    Incompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrsReconciliationItem {
    pub key: DrsItemKey,
    pub status: DrsReconciliationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrsReconciliation {
    pub items: Vec<DrsReconciliationItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrsMergeResult {
    pub reconciliation: DrsReconciliation,
    pub merged: DrsSnapshot,
}
