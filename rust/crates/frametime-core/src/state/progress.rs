use super::Progress;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

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
