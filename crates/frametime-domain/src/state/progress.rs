use crate::catalog::Phase;
use crate::catalog::StepId;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Persisted, acknowledged uncertainty for a check-only catalog step.
///
/// `reason` is stable, user-visible evidence explaining why the step could not
/// be proven. Unknown fields are retained to keep newer writers compatible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdvisoryResolution {
    pub reason: String,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

impl Progress {
    pub fn acknowledge_step_advisory(&mut self, id: StepId, reason: String) {
        self.acknowledge_advisory(id.phase.number(), id.number, reason);
    }

    pub fn acknowledge_advisory(&mut self, phase: u8, step: u8, reason: String) {
        let key = Self::key(phase, step);
        self.completed_steps.remove(&key);
        self.skipped_steps.remove(&key);
        self.advisories.insert(
            key,
            AdvisoryResolution {
                reason,
                unknown: BTreeMap::new(),
            },
        );
        self.phase = phase;
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
    StepId::from_progress_key(key).is_some_and(|id| crate::catalog::step_by_id(id).is_some())
}

impl Progress {
    #[must_use]
    pub fn key(phase: u8, step: u8) -> String {
        format!("P{phase}:{step}")
    }

    #[must_use]
    pub fn key_for(id: StepId) -> String {
        id.progress_key()
    }

    #[must_use]
    pub fn is_completed(&self, id: StepId) -> bool {
        self.completed_steps.contains(&Self::key_for(id))
    }

    #[must_use]
    pub fn is_skipped(&self, id: StepId) -> bool {
        self.skipped_steps.contains(&Self::key_for(id))
    }

    #[must_use]
    pub fn has_advisory(&self, id: StepId) -> bool {
        self.advisories.contains_key(&Self::key_for(id))
    }

    #[must_use]
    pub fn resolved_count_in_phase(&self, phase: Phase) -> usize {
        crate::catalog::step_catalog()
            .iter()
            .filter(|step| step.id.phase == phase)
            .filter(|step| self.is_completed(step.id) || self.is_skipped(step.id))
            .count()
    }

    #[must_use]
    pub fn has_resolved_in_phase(&self, phase: Phase) -> bool {
        self.resolved_count_in_phase(phase) > 0
    }

    pub fn complete_step(&mut self, id: StepId, timestamp: String) {
        self.complete(id.phase.number(), id.number, timestamp);
    }

    pub fn skip_step(&mut self, id: StepId) {
        self.skip(id.phase.number(), id.number);
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
