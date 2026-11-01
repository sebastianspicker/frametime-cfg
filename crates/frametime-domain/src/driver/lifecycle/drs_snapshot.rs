use std::collections::BTreeSet;

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

/// The optional fields were added after the first snapshots; they default so
/// executable-only snapshots still load.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrsApplicationSnapshot {
    pub executable: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub friendly_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub launcher: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file_in_folder: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command_line: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub flags: u32,
}

const fn is_zero(value: &u32) -> bool {
    *value == 0
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
                for text in [
                    &application.friendly_name,
                    &application.launcher,
                    &application.file_in_folder,
                    &application.command_line,
                ] {
                    if !text.is_empty() {
                        validate_drs_text(text, "applicationField")?;
                    }
                }
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
