use crate::{NorthclockError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const RISK_ACKNOWLEDGEMENT: &str = "NORTHCLOCK-HARDWARE-WRITES-UNVERIFIED";

static NEXT_PLAN_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationTarget {
    CpuCurveOptimizer,
    GpuTuning,
}

impl OperationTarget {
    #[must_use]
    pub const fn capability_name(self) -> &'static str {
        match self {
            Self::CpuCurveOptimizer => "cpu.tuning",
            Self::GpuTuning => "gpu.tuning",
        }
    }
}

impl fmt::Display for OperationTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CpuCurveOptimizer => "cpu_curve_optimizer",
            Self::GpuTuning => "gpu_tuning",
        })
    }
}

impl FromStr for OperationTarget {
    type Err = NorthclockError;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "cpu_curve_optimizer" | "cpu.curve_optimizer" => Ok(Self::CpuCurveOptimizer),
            "gpu_tuning" | "gpu.tuning" => Ok(Self::GpuTuning),
            _ => Err(NorthclockError::InvalidUsage(format!(
                "unsupported operation target {value}; expected cpu_curve_optimizer or gpu_tuning"
            ))),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationRequest {
    pub target: OperationTarget,
    pub changes: BTreeMap<String, i64>,
}

impl OperationRequest {
    #[must_use]
    pub fn cpu_curve_optimizer(offset: i64) -> Self {
        Self {
            target: OperationTarget::CpuCurveOptimizer,
            changes: BTreeMap::from([("curve_optimizer".into(), offset)]),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationPlan {
    pub id: String,
    pub target: OperationTarget,
    pub backend: String,
    pub requested_changes: BTreeMap<String, i64>,
    pub captured_state: BTreeMap<String, i64>,
    pub bounds_validated: bool,
    pub hardware_verified: bool,
}

impl OperationPlan {
    #[must_use]
    pub fn new(
        target: OperationTarget,
        backend: impl Into<String>,
        requested_changes: BTreeMap<String, i64>,
        captured_state: BTreeMap<String, i64>,
    ) -> Self {
        Self {
            id: next_plan_id(),
            target,
            backend: backend.into(),
            requested_changes,
            captured_state,
            bounds_validated: false,
            hardware_verified: false,
        }
    }
}

fn next_plan_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    let sequence = NEXT_PLAN_ID.fetch_add(1, Ordering::Relaxed);
    format!("plan-{millis}-{sequence}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_target_parsing_is_closed_and_typed() {
        assert!(matches!(
            "cpu_curve_optimizer".parse(),
            Ok(OperationTarget::CpuCurveOptimizer)
        ));
        assert!(matches!(
            "gpu.tuning".parse(),
            Ok(OperationTarget::GpuTuning)
        ));
        assert!("cpu_anything".parse::<OperationTarget>().is_err());
    }

    #[cfg(not(feature = "experimental-hardware-writes"))]
    #[test]
    fn default_build_cannot_authorize_a_write() {
        let authorization = WriteAuthorization {
            experimental: true,
            apply: true,
            elevated: true,
            risk_acknowledgement: Some(RISK_ACKNOWLEDGEMENT),
        };
        assert_eq!(
            authorization
                .validate()
                .err()
                .map(|error| error.exit_code()),
            Some(4)
        );
    }

    #[cfg(feature = "experimental-hardware-writes")]
    #[test]
    fn write_feature_still_requires_every_runtime_gate() {
        for authorization in [
            WriteAuthorization {
                experimental: false,
                apply: true,
                elevated: true,
                risk_acknowledgement: Some(RISK_ACKNOWLEDGEMENT),
            },
            WriteAuthorization {
                experimental: true,
                apply: false,
                elevated: true,
                risk_acknowledgement: Some(RISK_ACKNOWLEDGEMENT),
            },
            WriteAuthorization {
                experimental: true,
                apply: true,
                elevated: false,
                risk_acknowledgement: Some(RISK_ACKNOWLEDGEMENT),
            },
            WriteAuthorization {
                experimental: true,
                apply: true,
                elevated: true,
                risk_acknowledgement: Some("wrong"),
            },
        ] {
            assert!(authorization.validate().is_err());
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApplyReceipt {
    pub plan_id: String,
    pub target: OperationTarget,
    pub captured_state: BTreeMap<String, i64>,
    pub requested_changes: BTreeMap<String, i64>,
    pub readback: BTreeMap<String, i64>,
    pub validation_passed: bool,
    pub rollback_available: bool,
    pub backend: String,
    pub hardware_verified: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RollbackReceipt {
    pub plan_id: String,
    pub restored_state: BTreeMap<String, i64>,
    pub readback: BTreeMap<String, i64>,
    pub validation_passed: bool,
    pub backend: String,
    pub hardware_verified: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessAffinityPlan {
    pub id: String,
    pub process_id: u32,
    pub requested_mask: u64,
    pub captured_mask: u64,
    pub system_mask: u64,
    pub bounds_validated: bool,
}

impl ProcessAffinityPlan {
    #[must_use]
    pub fn new(process_id: u32, requested_mask: u64, captured_mask: u64, system_mask: u64) -> Self {
        Self {
            id: next_plan_id(),
            process_id,
            requested_mask,
            captured_mask,
            system_mask,
            bounds_validated: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessAffinityReceipt {
    pub plan_id: String,
    pub process_id: u32,
    pub captured_mask: u64,
    pub requested_mask: u64,
    pub readback_mask: u64,
    pub validation_passed: bool,
    pub rollback_available: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessAffinityRollbackReceipt {
    pub plan_id: String,
    pub process_id: u32,
    pub restored_mask: u64,
    pub readback_mask: u64,
    pub validation_passed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteAuthorization<'a> {
    pub experimental: bool,
    pub apply: bool,
    pub elevated: bool,
    pub risk_acknowledgement: Option<&'a str>,
}

impl WriteAuthorization<'_> {
    pub fn validate(&self) -> Result<()> {
        if !cfg!(feature = "experimental-hardware-writes") {
            return Err(NorthclockError::PermissionOrSafety(
                "binary was built without experimental-hardware-writes".into(),
            ));
        }
        if !self.experimental || !self.apply {
            return Err(NorthclockError::PermissionOrSafety(
                "hardware writes require both --experimental and --apply".into(),
            ));
        }
        if !self.elevated {
            return Err(NorthclockError::PermissionOrSafety(
                "hardware writes require an elevated process".into(),
            ));
        }
        if self.risk_acknowledgement != Some(RISK_ACKNOWLEDGEMENT) {
            return Err(NorthclockError::PermissionOrSafety(format!(
                "risk acknowledgement must exactly match {RISK_ACKNOWLEDGEMENT}"
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SafetyPolicy;

impl SafetyPolicy {
    pub fn validate(&self, request: &OperationRequest) -> Result<()> {
        if request.changes.is_empty() {
            return Err(NorthclockError::InvalidUsage(
                "an operation must request at least one change".into(),
            ));
        }
        for (name, value) in &request.changes {
            let valid = match (request.target, name.as_str()) {
                (OperationTarget::CpuCurveOptimizer, "curve_optimizer") => {
                    (-50..=50).contains(value)
                }
                (OperationTarget::GpuTuning, "gpu_power_limit_percent") => {
                    (-50..=20).contains(value)
                }
                (OperationTarget::GpuTuning, "gpu_core_offset_mhz") => (-500..=500).contains(value),
                (OperationTarget::GpuTuning, "gpu_memory_offset_mhz") => {
                    (-1000..=1500).contains(value)
                }
                _ => false,
            };
            if !valid {
                return Err(NorthclockError::PermissionOrSafety(format!(
                    "{name} value {value} is unsupported or outside the fixed safety bounds"
                )));
            }
        }
        Ok(())
    }
}
