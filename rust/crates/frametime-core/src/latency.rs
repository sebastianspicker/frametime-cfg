use std::{collections::BTreeMap, io, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::persistence::{read_json_preserving_corrupt, write_json_atomic};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct LatencyResult {
    #[serde(default)]
    pub region_code: String,
    pub target_label: String,
    pub resolved_endpoint: String,
    pub protocol_used: String,
    #[serde(default)]
    pub sample_count: u32,
    #[serde(default)]
    pub successful_samples: u32,
    #[serde(default)]
    pub min_rtt_ms: Option<f64>,
    #[serde(default)]
    pub median_rtt_ms: Option<f64>,
    #[serde(default)]
    pub avg_rtt_ms: Option<f64>,
    #[serde(default)]
    pub timeout_count: u32,
    #[serde(default)]
    pub fallback_used: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub provenance: String,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct LatencyRun {
    pub run_id: String,
    pub kind: String,
    pub timestamp: String,
    #[serde(default)]
    pub adapter_name: String,
    #[serde(default)]
    pub adapter_type: String,
    #[serde(default)]
    pub dns_provider: String,
    #[serde(default)]
    pub dns_servers: Vec<String>,
    #[serde(default)]
    pub disclaimer: String,
    #[serde(default)]
    pub results: Vec<LatencyResult>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct LatencyHistory {
    #[serde(default = "history_version")]
    pub version: u8,
    #[serde(default)]
    pub runs: Vec<LatencyRun>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

impl Default for LatencyHistory {
    fn default() -> Self {
        Self {
            version: history_version(),
            runs: Vec::new(),
            unknown: BTreeMap::new(),
        }
    }
}

const fn history_version() -> u8 {
    1
}

pub fn load_latency_history(path: &Path) -> Result<LatencyHistory, String> {
    if !path.exists() {
        return Ok(LatencyHistory::default());
    }
    read_json_preserving_corrupt(path).map_err(|error| format!("read latency history: {error}"))
}

pub fn append_latency_run(path: &Path, run: LatencyRun) -> io::Result<LatencyHistory> {
    let mut history = load_latency_history(path).map_err(io::Error::other)?;
    history.runs.push(run);
    write_json_atomic(path, &history)?;
    let persisted: LatencyHistory = read_json_preserving_corrupt(path)?;
    if persisted != history {
        return Err(io::Error::other("latency history verification failed"));
    }
    Ok(history)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_history_has_legacy_version_and_empty_runs() {
        let directory = tempfile::tempdir().expect("directory");
        let history = load_latency_history(&directory.path().join("missing.json")).expect("load");
        assert_eq!(history.version, 1);
        assert!(history.runs.is_empty());
    }

    #[test]
    fn unknown_fields_survive_round_trip() {
        let raw = r#"{
          "Version":1,
          "Runs":[{
            "RunId":"one","Kind":"baseline","Timestamp":"2026-08-10 12:00:00",
            "Results":[{"TargetLabel":"Frankfurt","ResolvedEndpoint":"127.0.0.1","ProtocolUsed":"ICMP","Future":true}],
            "FutureRun":7
          }],
          "FutureRoot":"kept"
        }"#;
        let history: LatencyHistory = serde_json::from_str(raw).expect("history");
        assert_eq!(history.unknown["FutureRoot"], "kept");
        assert_eq!(history.runs[0].unknown["FutureRun"], 7);
        assert_eq!(history.runs[0].results[0].unknown["Future"], true);
    }
}
