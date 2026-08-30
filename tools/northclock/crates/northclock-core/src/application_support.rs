use crate::{
    CapabilityBackend, CapabilityReport, EventObservationBackend, Measurement, MemoryTestReport,
    NorthclockError, ProcessAffinityPlan, WriteAuthorization,
};
use serde_json::{json, Value};
use std::time::Duration;

pub(crate) type ExecutionResult = std::result::Result<
    (Option<CapabilityReport>, Value),
    (Option<CapabilityReport>, NorthclockError),
>;

pub(crate) fn unusable_capability_error(
    capability: &Option<CapabilityReport>,
) -> Option<NorthclockError> {
    capability.as_ref().and_then(|report| {
        (!report.is_usable())
            .then(|| NorthclockError::Unavailable(format!("{}: {}", report.name, report.detail)))
    })
}

pub(crate) fn validate_affinity_plan(plan: &ProcessAffinityPlan) -> crate::Result<()> {
    if plan.id.is_empty()
        || plan.process_id == 0
        || plan.requested_mask == 0
        || plan.captured_mask == 0
        || plan.system_mask == 0
        || plan.requested_mask & !plan.system_mask != 0
        || plan.captured_mask & !plan.system_mask != 0
    {
        return Err(NorthclockError::PermissionOrSafety(
            "process affinity plan contains invalid identity, mask, or system bounds".into(),
        ));
    }
    Ok(())
}

pub(crate) fn authorize_write<B: CapabilityBackend>(
    backend: &B,
    experimental: bool,
    apply: bool,
    risk_acknowledgement: Option<&str>,
) -> crate::Result<()> {
    WriteAuthorization {
        experimental,
        apply,
        elevated: backend.is_elevated()?,
        risk_acknowledgement,
    }
    .validate()
}

pub(crate) fn map_result(
    capability: Option<CapabilityReport>,
    result: crate::Result<Value>,
) -> ExecutionResult {
    match result {
        Ok(value) => Ok((capability, value)),
        Err(error) => Err((capability, error)),
    }
}

pub(crate) fn require_measurements<T>(values: &[Measurement<T>]) -> crate::Result<()> {
    if values.is_empty() {
        return Err(NorthclockError::Unavailable(
            "backend returned no measurements; no value was fabricated".into(),
        ));
    }
    Ok(())
}

pub(crate) fn json_error(error: serde_json::Error) -> NorthclockError {
    NorthclockError::Internal(error.to_string())
}

pub(crate) fn memory_report_with_whea<B: EventObservationBackend>(
    backend: &B,
    report: MemoryTestReport,
) -> crate::Result<Value> {
    let duration_ms = report.elapsed_ms.clamp(1, u128::from(u64::MAX)) as u64;
    let correlation = match backend.observe_whea(Duration::from_millis(duration_ms)) {
        Ok(events) => json!({
            "status": "success",
            "backend": "Windows Event Log API",
            "events": events,
            "error": null,
        }),
        Err(error) => json!({
            "status": "unavailable",
            "backend": "Windows Event Log API",
            "events": null,
            "error": {
                "category": error.category(),
                "message": error.to_string(),
                "exit_code": error.exit_code(),
            },
        }),
    };
    let mut value = serde_json::to_value(report).map_err(json_error)?;
    value
        .as_object_mut()
        .ok_or_else(|| NorthclockError::Internal("memory report was not a JSON object".into()))?
        .insert("whea_correlation".into(), correlation);
    Ok(value)
}
