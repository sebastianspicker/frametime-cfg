use crate::{
    benchmark::FinalBenchmarkReceipt,
    handoff::{RebootStage, RebootTransaction},
    policy::Profile,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use super::decode::StateFields;

pub(crate) const MAX_PAGEFILE_MB: u64 = 1_048_576;
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
