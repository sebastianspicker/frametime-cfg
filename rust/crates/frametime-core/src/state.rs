use crate::{
    benchmark::FinalBenchmarkReceipt,
    handoff::{RebootStage, RebootTransaction},
    policy::Profile,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

mod decode;
mod progress;

use decode::StateFields;
pub use progress::AdvisoryResolution;

const MAX_PAGEFILE_MB: u64 = 1_048_576;
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_profile")]
    pub profile: Profile,
    #[serde(default)]
    pub fps_cap: u32,
    #[serde(default)]
    pub avg_fps: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p1_fps: Option<f64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub baseline_avg: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_p1: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_input: Option<String>,
    #[serde(default, rename = "pagefileMB")]
    pub pagefile_mb: u64,
    #[serde(default = "default_work_dir")]
    pub work_dir: String,
    #[serde(default)]
    pub script_root: String,
    #[serde(default)]
    pub phase1_safe_mode_ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_reboot_transaction: Option<RebootTransaction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_benchmark: Option<FinalBenchmarkReceipt>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            log_level: default_log_level(),
            profile: default_profile(),
            fps_cap: 0,
            avg_fps: 0.0,
            p1_fps: None,
            baseline_avg: 0.0,
            baseline_p1: None,
            cap_date: None,
            gpu_input: None,
            pagefile_mb: 0,
            work_dir: default_work_dir(),
            script_root: String::new(),
            phase1_safe_mode_ready: false,
            active_reboot_transaction: None,
            final_benchmark: None,
            unknown: BTreeMap::new(),
        }
    }
}
impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = BTreeMap::<String, Value>::deserialize(deserializer)?;
        let mut fields = StateFields::new(fields);
        let profile = fields
            .string("profile")
            .as_deref()
            .and_then(parse_profile)
            .unwrap_or_else(default_profile);
        let mode = fields
            .string("mode")
            .filter(|value| {
                matches!(
                    value.as_str(),
                    "AUTO" | "CONTROL" | "INFORMED" | "YOLO" | "DRY-RUN"
                )
            })
            .unwrap_or_else(|| mode_for_profile(profile).to_owned());
        let log_level = fields
            .string("logLevel")
            .filter(|value| matches!(value.as_str(), "MINIMAL" | "NORMAL" | "VERBOSE"))
            .unwrap_or_else(default_log_level);
        let fps_cap = fields.u32("fpsCap").unwrap_or_default();
        let avg_fps = fields.finite_f64("avgFps").unwrap_or_default();
        let p1_fps = fields.nonnegative_f64("p1Fps");
        let baseline_avg = fields.nonnegative_f64("baselineAvg").unwrap_or_default();
        let baseline_p1 = fields.nonnegative_f64("baselineP1");
        let cap_date = fields.string("capDate");
        let gpu_input = fields
            .string("gpuInput")
            .filter(|value| matches!(value.as_str(), "1" | "2" | "3" | "4"));
        let pagefile_mb = fields
            .u64("pagefileMB")
            .filter(|value| *value == 0 || *value <= MAX_PAGEFILE_MB)
            .unwrap_or_default();
        let work_dir = fields.string("workDir").unwrap_or_else(default_work_dir);
        let script_root = fields.string("scriptRoot").unwrap_or_default();
        let phase1_safe_mode_ready = fields.is_true("phase1SafeModeReady");
        // Malformed transaction data stays in the flattened unknown map so
        // migration and recovery fail closed instead of treating it as absent.
        let active_reboot_transaction =
            fields.typed_preserving_malformed("activeRebootTransaction");
        let final_benchmark = fields.typed_preserving_malformed("finalBenchmark");
        Ok(Self {
            mode,
            log_level,
            profile,
            fps_cap,
            avg_fps,
            p1_fps,
            baseline_avg,
            baseline_p1,
            cap_date,
            gpu_input,
            pagefile_mb,
            work_dir,
            script_root,
            phase1_safe_mode_ready,
            active_reboot_transaction,
            final_benchmark,
            unknown: fields.into_unknown(),
        })
    }
}
impl State {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.unknown.contains_key("activeRebootTransaction") {
            return Err("activeRebootTransaction is malformed");
        }
        if self.unknown.contains_key("finalBenchmark") {
            return Err("finalBenchmark is malformed");
        }
        if let Some(value) = &self.gpu_input
            && !matches!(value.as_str(), "1" | "2" | "3" | "4")
        {
            return Err("gpuInput must be one of 1, 2, 3, or 4");
        }
        if self.pagefile_mb != 0 && self.pagefile_mb > MAX_PAGEFILE_MB {
            return Err("pagefileMB must be 0 or between 1 and 1048576");
        }
        match self.baseline_p1 {
            Some(p1)
                if self.baseline_avg.is_finite()
                    && self.baseline_avg > 0.0
                    && p1.is_finite()
                    && p1 > 0.0
                    && p1 <= self.baseline_avg => {}
            None if self.baseline_avg == 0.0 => {}
            _ => return Err("baselineAvg and baselineP1 must be a complete valid capture"),
        }
        if let Some(receipt) = &self.final_benchmark {
            let transaction = self
                .active_reboot_transaction
                .as_ref()
                .ok_or("finalBenchmark requires an active reboot transaction")?;
            let authorized_phase_three = match transaction.stage {
                RebootStage::PhaseThreeArmed => {
                    transaction.is_authorized_at(&RebootStage::PhaseThreeArmed)
                }
                RebootStage::PhaseThreeComplete => {
                    transaction.is_authorized_at(&RebootStage::PhaseThreeComplete)
                }
                _ => false,
            };
            if !authorized_phase_three {
                return Err("finalBenchmark requires a Phase 3 reboot transaction");
            }
            receipt.validate_for_transaction(transaction)?;
        }
        Ok(())
    }
}
fn is_zero(value: &f64) -> bool {
    *value == 0.0
}
fn default_mode() -> String {
    "CONTROL".into()
}
fn default_log_level() -> String {
    "NORMAL".into()
}
const fn default_profile() -> Profile {
    Profile::Recommended
}
fn default_work_dir() -> String {
    r"C:\FRAMETIME_CFG".into()
}
fn parse_profile(value: &str) -> Option<Profile> {
    match value.to_ascii_uppercase().as_str() {
        "SAFE" => Some(Profile::Safe),
        "RECOMMENDED" => Some(Profile::Recommended),
        "COMPETITIVE" => Some(Profile::Competitive),
        "CUSTOM" => Some(Profile::Custom),
        "YOLO" => Some(Profile::Yolo),
        _ => None,
    }
}
const fn mode_for_profile(profile: Profile) -> &'static str {
    match profile {
        Profile::Safe | Profile::Recommended => "AUTO",
        Profile::Competitive => "CONTROL",
        Profile::Custom => "INFORMED",
        Profile::Yolo => "YOLO",
    }
}
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    #[serde(default)]
    pub phase: u8,
    #[serde(default)]
    pub last_completed_step: u8,
    #[serde(default)]
    pub last_skipped_step: u8,
    #[serde(default)]
    pub completed_steps: BTreeSet<String>,
    #[serde(default)]
    pub skipped_steps: BTreeSet<String>,
    #[serde(default)]
    pub timestamps: BTreeMap<String, String>,
    /// Check-only steps acknowledged without an authoritative observation.
    /// These are deliberately distinct from both completed and skipped steps.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub advisories: BTreeMap<String, AdvisoryResolution>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

impl<'de> Deserialize<'de> for Progress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut fields = BTreeMap::<String, Value>::deserialize(deserializer)?;
        let phase = tolerant_u8(fields.remove("phase"))
            .filter(|value| *value <= 3)
            .unwrap_or_default();
        let last_completed_step =
            tolerant_u8(fields.remove("lastCompletedStep")).unwrap_or_default();
        let last_skipped_step = tolerant_u8(fields.remove("lastSkippedStep")).unwrap_or_default();
        let completed_steps = tolerant_keys(fields.remove("completedSteps"));
        let skipped_steps = tolerant_keys(fields.remove("skippedSteps"));
        let timestamps = fields
            .remove("timestamps")
            .and_then(|value| value.as_object().cloned())
            .map(|values| {
                values
                    .into_iter()
                    .filter_map(|(key, value)| value.as_str().map(|text| (key, text.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        let advisories = fields
            .remove("advisories")
            .and_then(|value| value.as_object().cloned())
            .map(|values| {
                values
                    .into_iter()
                    .filter(|(key, _)| valid_progress_key(key))
                    .filter_map(|(key, value)| {
                        serde_json::from_value::<AdvisoryResolution>(value)
                            .ok()
                            .filter(|advisory| !advisory.reason.is_empty())
                            .map(|advisory| (key, advisory))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            phase,
            last_completed_step,
            last_skipped_step,
            completed_steps,
            skipped_steps,
            timestamps,
            advisories,
            unknown: fields,
        })
    }
}

fn tolerant_u8(value: Option<Value>) -> Option<u8> {
    value
        .as_ref()
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
}

fn tolerant_keys(value: Option<Value>) -> BTreeSet<String> {
    value
        .and_then(|value| value.as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .filter(|key| valid_progress_key(key))
        .collect()
}

fn valid_progress_key(key: &str) -> bool {
    let Some((phase, step)) = key.split_once(':') else {
        return false;
    };
    let phase_valid = matches!(phase, "P1" | "P2" | "P3");
    phase_valid && step.parse::<u8>().is_ok_and(|value| value > 0)
}

impl Progress {
    #[must_use]
    pub fn key(phase: u8, step: u8) -> String {
        format!("P{phase}:{step}")
    }

    pub fn complete(&mut self, phase: u8, step: u8, timestamp: String) {
        let key = Self::key(phase, step);
        self.skipped_steps.remove(&key);
        self.advisories.remove(&key);
        self.completed_steps.insert(key.clone());
        self.timestamps.insert(format!("{phase}-{step}"), timestamp);
        self.phase = phase;
        self.last_completed_step = step;
    }

    pub fn skip(&mut self, phase: u8, step: u8) {
        let key = Self::key(phase, step);
        self.completed_steps.remove(&key);
        self.advisories.remove(&key);
        self.skipped_steps.insert(key.clone());
        self.phase = phase;
        self.last_skipped_step = self.last_skipped_step.max(step);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FinalBenchmarkReceipt, RuntimeRecord, TransactionId, fps::BenchmarkCapture};

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
    const RECEIPT_ID: &str = "fedcba9876543210fedcba9876543210";
    const OTHER_TRANSACTION_ID: &str = "11111111111111111111111111111111";
    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn phase_three_transaction(stage: RebootStage) -> RebootTransaction {
        RebootTransaction {
            schema_version: 1,
            transaction_id: Some(TransactionId::parse(TRANSACTION_ID).expect("transaction id")),
            initiator_user_sid: Some("S-1-5-21-1".into()),
            stage,
            runtime: Some(RuntimeRecord {
                generation: TRANSACTION_ID.into(),
                manifest_sha256: HASH.into(),
                payload_contract_hash: HASH.into(),
                executable_path: "frametime.exe".into(),
                executable_sha256: HASH.into(),
                unknown: BTreeMap::new(),
            }),
            driver_package: None,
            created_utc: None,
            updated_utc: None,
            unknown: BTreeMap::new(),
        }
    }

    fn final_receipt(transaction_id: &str) -> FinalBenchmarkReceipt {
        FinalBenchmarkReceipt::new(
            TransactionId::parse(RECEIPT_ID).expect("receipt id"),
            TransactionId::parse(transaction_id).expect("transaction id"),
            "2026-08-10 12:34:56".into(),
            BenchmarkCapture {
                average_fps: 300.0,
                p1_fps: 180.0,
                runs: 3,
            },
            273,
        )
        .expect("receipt")
    }

    #[test]
    fn unknown_fields_survive_state_round_trip() {
        let raw = r#"{"mode":"CONTROL","profile":"SAFE","future":{"x":1}}"#;
        let state: State = serde_json::from_str(raw).expect("state");
        assert_eq!(state.unknown["future"]["x"], 1);
        let value: Value =
            serde_json::from_str(&serde_json::to_string(&state).expect("json")).expect("value");
        assert_eq!(value["future"]["x"], 1);
    }

    #[test]
    fn typed_reboot_fields_are_tolerant_and_keep_other_extensions() {
        let state: State = serde_json::from_str(
            r#"{
              "phase1SafeModeReady":true,
              "activeRebootTransaction":{"schemaVersion":1,"stage":"future","futureTxn":null},
              "futureState":{"retained":true}
            }"#,
        )
        .expect("state");
        assert!(state.phase1_safe_mode_ready);
        assert_eq!(
            state.active_reboot_transaction.unwrap().unknown["futureTxn"],
            Value::Null
        );
        assert_eq!(state.unknown["futureState"]["retained"], true);

        let malformed: State = serde_json::from_str(
            r#"{"phase1SafeModeReady":"true","activeRebootTransaction":false,"future":1}"#,
        )
        .expect("state");
        assert!(!malformed.phase1_safe_mode_ready);
        assert!(malformed.active_reboot_transaction.is_none());
        assert_eq!(malformed.unknown["activeRebootTransaction"], false);
        assert_eq!(malformed.unknown["future"], 1);
        let round_trip = serde_json::to_value(malformed).expect("state round trip");
        assert_eq!(round_trip["activeRebootTransaction"], false);
    }

    #[test]
    fn progress_keys_are_phase_qualified() {
        let mut progress = Progress::default();
        progress.complete(1, 5, "now".into());
        assert!(progress.completed_steps.contains("P1:5"));
        progress.skip(1, 5);
        assert!(!progress.completed_steps.contains("P1:5"));
        assert!(progress.skipped_steps.contains("P1:5"));
    }

    #[test]
    fn malformed_known_fields_default_without_losing_unknown_fields() {
        let raw = r#"{"profile":"SAFE","logLevel":["VERBOSE"],"gpuInput":3,"future":true}"#;
        let state: State = serde_json::from_str(raw).expect("tolerant state");
        assert_eq!(state.profile, Profile::Safe);
        assert_eq!(state.mode, "AUTO");
        assert_eq!(state.log_level, "NORMAL");
        assert_eq!(state.gpu_input, None);
        assert_eq!(state.unknown["future"], true);
    }

    #[test]
    fn progress_tolerates_malformed_known_fields_and_drops_bare_keys() {
        let raw = r#"{
          "phase":"one",
          "lastCompletedStep":999,
          "completedSteps":["1","P1:2",3],
          "skippedSteps":false,
          "timestamps":{"P1:2":"now","3":"bad","P2:1":4},
          "future":{"kept":true}
        }"#;
        let progress: Progress = serde_json::from_str(raw).expect("tolerant progress");
        assert_eq!(progress.phase, 0);
        assert_eq!(progress.last_completed_step, 0);
        assert_eq!(progress.completed_steps, BTreeSet::from(["P1:2".into()]));
        assert!(progress.skipped_steps.is_empty());
        assert_eq!(progress.timestamps["P1:2"], "now");
        assert_eq!(progress.unknown["future"]["kept"], true);
    }

    #[test]
    fn serialization_matches_legacy_acronyms_and_timestamp_keys() {
        let state = State {
            pagefile_mb: 4096,
            ..State::default()
        };
        let state_json = serde_json::to_value(state).expect("state");
        assert_eq!(state_json["pagefileMB"], 4096);
        assert!(state_json.get("pagefileMb").is_none());

        let mut progress = Progress::default();
        progress.complete(1, 5, "now".into());
        progress.skip(1, 6);
        assert_eq!(progress.timestamps["1-5"], "now");
        assert!(!progress.timestamps.contains_key("1-6"));
        assert_eq!(progress.last_skipped_step, 6);
    }

    #[test]
    fn advisory_progress_round_trips_with_future_fields_without_completion() {
        let raw = r#"{
          "advisories": {
            "P1:2": {
              "reason": "XMP/EXPO observation requires authoritative SMBIOS memory-profile data",
              "futureDetail": {"source":"firmware"}
            }
          },
          "futureProgress": true
        }"#;
        let progress: Progress = serde_json::from_str(raw).expect("advisory progress");
        assert!(progress.completed_steps.is_empty());
        assert!(progress.skipped_steps.is_empty());
        assert_eq!(
            progress.advisories["P1:2"].reason,
            "XMP/EXPO observation requires authoritative SMBIOS memory-profile data"
        );
        assert_eq!(
            progress.advisories["P1:2"].unknown["futureDetail"]["source"],
            "firmware"
        );
        let serialized = serde_json::to_value(progress).expect("serialize advisory progress");
        assert_eq!(serialized["futureProgress"], true);
        assert_eq!(
            serialized["advisories"]["P1:2"]["futureDetail"]["source"],
            "firmware"
        );
    }

    #[test]
    fn pagefile_size_bounds_allow_unset_and_limit_values() {
        for pagefile_mb in [0, 1, MAX_PAGEFILE_MB] {
            State {
                pagefile_mb,
                ..State::default()
            }
            .validate()
            .expect("valid pagefile size");
        }
        assert_eq!(
            State {
                pagefile_mb: MAX_PAGEFILE_MB + 1,
                ..State::default()
            }
            .validate(),
            Err("pagefileMB must be 0 or between 1 and 1048576")
        );
    }

    #[test]
    fn invalid_pagefile_input_defaults_without_serializing_an_invalid_value() {
        let state: State = serde_json::from_str(r#"{"pagefileMB":1048577}"#).expect("state");
        assert_eq!(state.pagefile_mb, 0);
        assert_eq!(
            serde_json::to_value(state).expect("state JSON")["pagefileMB"],
            0
        );
    }

    #[test]
    fn final_benchmark_is_transaction_bound_and_unknown_tolerant() {
        let mut receipt = final_receipt(TRANSACTION_ID);
        receipt
            .unknown
            .insert("futureReceipt".into(), serde_json::json!({"keep": true}));
        let state = State {
            active_reboot_transaction: Some(phase_three_transaction(
                RebootStage::PhaseThreeComplete,
            )),
            final_benchmark: Some(receipt),
            ..State::default()
        };
        state.validate().expect("coherent final benchmark");
        let value = serde_json::to_value(&state).expect("state JSON");
        assert_eq!(value["finalBenchmark"]["futureReceipt"]["keep"], true);
        let round_trip: State = serde_json::from_value(value).expect("state round trip");
        round_trip.validate().expect("round-trip validation");

        let mismatched = State {
            active_reboot_transaction: Some(phase_three_transaction(RebootStage::PhaseThreeArmed)),
            final_benchmark: Some(final_receipt(OTHER_TRANSACTION_ID)),
            ..State::default()
        };
        assert!(mismatched.validate().is_err());
    }

    #[test]
    fn malformed_final_benchmark_is_preserved_and_fails_validation() {
        let state: State = serde_json::from_str(
            r#"{"finalBenchmark":{"schemaVersion":1,"receiptId":false},"future":1}"#,
        )
        .expect("tolerant state");
        assert!(state.final_benchmark.is_none());
        assert_eq!(state.unknown["finalBenchmark"]["receiptId"], false);
        assert_eq!(state.unknown["future"], 1);
        assert_eq!(state.validate(), Err("finalBenchmark is malformed"));
        let value = serde_json::to_value(state).expect("state JSON");
        assert_eq!(value["finalBenchmark"]["receiptId"], false);
    }
}
