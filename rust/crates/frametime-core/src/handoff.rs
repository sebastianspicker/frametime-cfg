//! Typed, forward-compatible records for an immutable reboot handoff.
//!
//! These records describe intent and observed immutable runtime data. They do
//! not create registry entries, change BCD, retain filesystem handles, or make
//! an Authenticode claim; those effects belong to the Windows adapter.

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

const REBOOT_TRANSACTION_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransactionId(String);

impl TransactionId {
    pub fn parse(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if value.len() == 32
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(value))
        } else {
            Err("transaction id must be 32 lowercase hexadecimal characters")
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TransactionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for TransactionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for TransactionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

/// The only stages the core recognizes. Unknown persisted stages are retained
/// but never authorize work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebootStage {
    PhaseOneSafeModeArmed,
    PhaseTwoSafeMode,
    PhaseThreeArmed,
    PhaseThreeComplete,
    RecoveryRequired,
    Unknown(String),
}

impl RebootStage {
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "phase1SafeModeArmed" => Self::PhaseOneSafeModeArmed,
            "phase2SafeMode" => Self::PhaseTwoSafeMode,
            "phase3Armed" => Self::PhaseThreeArmed,
            "phase3Complete" => Self::PhaseThreeComplete,
            "recoveryRequired" => Self::RecoveryRequired,
            other => Self::Unknown(other.to_owned()),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::PhaseOneSafeModeArmed => "phase1SafeModeArmed",
            Self::PhaseTwoSafeMode => "phase2SafeMode",
            Self::PhaseThreeArmed => "phase3Armed",
            Self::PhaseThreeComplete => "phase3Complete",
            Self::RecoveryRequired => "recoveryRequired",
            Self::Unknown(value) => value,
        }
    }

    #[must_use]
    pub const fn allows_transition_to(&self, next: &Self) -> bool {
        matches!(
            (self, next),
            (Self::PhaseOneSafeModeArmed, Self::PhaseTwoSafeMode)
                | (Self::PhaseTwoSafeMode, Self::PhaseThreeArmed)
                | (Self::PhaseThreeArmed, Self::PhaseThreeComplete)
                | (_, Self::RecoveryRequired)
        )
    }
}

impl Serialize for RebootStage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RebootStage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRecord {
    pub generation: String,
    pub manifest_sha256: String,
    pub payload_contract_hash: String,
    pub executable_path: String,
    pub executable_sha256: String,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

impl RuntimeRecord {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        valid_generation_id(&self.generation)
            && valid_sha256(&self.manifest_sha256)
            && valid_sha256(&self.payload_contract_hash)
            && valid_relative_path(&self.executable_path)
            && valid_sha256(&self.executable_sha256)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DriverPackageRecord {
    pub published_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RebootTransaction {
    pub schema_version: u8,
    pub transaction_id: Option<TransactionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initiator_user_sid: Option<String>,
    pub stage: RebootStage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_package: Option<DriverPackageRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_utc: Option<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

impl Default for RebootTransaction {
    fn default() -> Self {
        Self {
            schema_version: REBOOT_TRANSACTION_SCHEMA_VERSION,
            transaction_id: None,
            initiator_user_sid: None,
            stage: RebootStage::Unknown("missing".into()),
            runtime: None,
            driver_package: None,
            created_utc: None,
            updated_utc: None,
            unknown: BTreeMap::new(),
        }
    }
}

impl<'de> Deserialize<'de> for RebootTransaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut fields = BTreeMap::<String, Value>::deserialize(deserializer)?;
        let transaction_id = fields
            .remove("transactionId")
            .and_then(|value| value.as_str().map(str::to_owned))
            .and_then(|value| TransactionId::parse(value).ok());
        let initiator_user_sid = tolerant_string(fields.remove("initiatorUserSid"));
        let stage = fields
            .remove("stage")
            .and_then(|value| value.as_str().map(RebootStage::parse))
            .unwrap_or_else(|| RebootStage::Unknown("missing".into()));
        let runtime = tolerant_record(fields.remove("runtime"));
        let driver_package = tolerant_record(fields.remove("driverPackage"));
        Ok(Self {
            schema_version: fields
                .remove("schemaVersion")
                .as_ref()
                .and_then(Value::as_u64)
                .and_then(|value| u8::try_from(value).ok())
                .unwrap_or_default(),
            transaction_id,
            initiator_user_sid,
            stage,
            runtime,
            driver_package,
            created_utc: tolerant_string(fields.remove("createdUtc")),
            updated_utc: tolerant_string(fields.remove("updatedUtc")),
            unknown: fields,
        })
    }
}

impl RebootTransaction {
    #[must_use]
    pub fn is_authorized_at(&self, expected_stage: &RebootStage) -> bool {
        self.schema_version == REBOOT_TRANSACTION_SCHEMA_VERSION
            && self.transaction_id.is_some()
            && self
                .initiator_user_sid
                .as_deref()
                .is_some_and(valid_user_sid)
            && self
                .runtime
                .as_ref()
                .is_some_and(RuntimeRecord::is_complete)
            && &self.stage == expected_stage
    }

    pub fn transition_to(&mut self, next: RebootStage) -> Result<(), &'static str> {
        if !self.stage.allows_transition_to(&next) {
            return Err("reboot transaction transition is not allowed");
        }
        self.stage = next;
        Ok(())
    }
}

fn tolerant_record<T: for<'de> Deserialize<'de>>(value: Option<Value>) -> Option<T> {
    value.and_then(|value| serde_json::from_value(value).ok())
}

fn tolerant_string(value: Option<Value>) -> Option<String> {
    value.and_then(|value| value.as_str().map(str::to_owned))
}

fn valid_generation_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with(['/', '\\'])
        && !value.contains(['\\', ':', '\0'])
        && value
            .split('/')
            .all(|part| !part.is_empty() && !matches!(part, "." | ".."))
}

fn valid_user_sid(value: &str) -> bool {
    let mut parts = value.split('-');
    if parts.next() != Some("S") || parts.next() != Some("1") {
        return false;
    }
    let Some(authority) = parts.next() else {
        return false;
    };
    if !canonical_decimal(authority) || authority.parse::<u64>().is_err() {
        return false;
    }
    let subauthorities = parts.collect::<Vec<_>>();
    (1..=15).contains(&subauthorities.len())
        && subauthorities
            .iter()
            .all(|part| canonical_decimal(part) && part.parse::<u32>().is_ok())
}

fn canonical_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef0123456789abcdef";
    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn runtime() -> RuntimeRecord {
        RuntimeRecord {
            generation: ID.into(),
            manifest_sha256: HASH.into(),
            payload_contract_hash: HASH.into(),
            executable_path: "frametime.exe".into(),
            executable_sha256: HASH.into(),
            unknown: BTreeMap::new(),
        }
    }

    #[test]
    fn transaction_round_trips_unknown_fields_but_unknown_stages_never_authorize() {
        let raw = format!(
            r#"{{"schemaVersion":1,"transactionId":"{ID}","initiatorUserSid":"S-1-5-21-1","stage":"futureStage","runtime":{{"generation":"{ID}","manifestSha256":"{HASH}","payloadContractHash":"{HASH}","executablePath":"frametime.exe","executableSha256":"{HASH}","futureRuntime":true}},"futureTransaction":{{"retain":true}}}}"#
        );
        let transaction: RebootTransaction = serde_json::from_str(&raw).expect("transaction");
        assert_eq!(transaction.unknown["futureTransaction"]["retain"], true);
        assert_eq!(
            transaction.runtime.as_ref().unwrap().unknown["futureRuntime"],
            true
        );
        assert!(!transaction.is_authorized_at(&RebootStage::PhaseOneSafeModeArmed));
        assert_eq!(
            serde_json::to_value(transaction).unwrap()["stage"],
            "futureStage"
        );
    }

    #[test]
    fn only_forward_stages_or_recovery_are_allowed() {
        let mut transaction = RebootTransaction {
            transaction_id: Some(TransactionId::parse(ID).unwrap()),
            initiator_user_sid: Some("S-1-5-21-1".into()),
            stage: RebootStage::PhaseOneSafeModeArmed,
            runtime: Some(runtime()),
            ..RebootTransaction::default()
        };
        transaction
            .transition_to(RebootStage::PhaseTwoSafeMode)
            .unwrap();
        transaction
            .transition_to(RebootStage::PhaseThreeArmed)
            .unwrap();
        assert!(
            transaction
                .transition_to(RebootStage::PhaseOneSafeModeArmed)
                .is_err()
        );
        transaction
            .transition_to(RebootStage::RecoveryRequired)
            .unwrap();
        assert!(!transaction.is_authorized_at(&RebootStage::PhaseThreeArmed));
    }

    #[test]
    fn malformed_legacy_fields_deserialize_without_authorization() {
        let transaction: RebootTransaction = serde_json::from_str(
            r#"{"schemaVersion":"1","transactionId":"not-an-id","stage":3,"runtime":false,"future":null}"#,
        )
        .unwrap();
        assert_eq!(transaction.stage, RebootStage::Unknown("missing".into()));
        assert!(transaction.transaction_id.is_none());
        assert!(transaction.runtime.is_none());
        assert!(transaction.unknown["future"].is_null());
        assert!(!transaction.is_authorized_at(&RebootStage::PhaseOneSafeModeArmed));
    }

    #[test]
    fn authorization_requires_a_canonical_initiating_user_sid() {
        for sid in [None, Some("S-1-5-021"), Some("S-2-5-21"), Some("not-a-sid")] {
            let transaction = RebootTransaction {
                transaction_id: Some(TransactionId::parse(ID).unwrap()),
                initiator_user_sid: sid.map(str::to_owned),
                stage: RebootStage::PhaseOneSafeModeArmed,
                runtime: Some(runtime()),
                ..RebootTransaction::default()
            };
            assert!(!transaction.is_authorized_at(&RebootStage::PhaseOneSafeModeArmed));
        }
    }
}
