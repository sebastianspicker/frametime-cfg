use crate::application_support::{authorize_write, json_error};
use crate::{
    ApplyReceipt, BackendBundle, NorthclockError, OperationPlan, OperationRequest, OperationTarget,
    Result, RollbackReceipt, SafetyPolicy,
};
use serde_json::Value;

pub(crate) fn preview<B: BackendBundle>(
    backend: &B,
    safety: SafetyPolicy,
    request: OperationRequest,
) -> Result<Value> {
    safety.validate(&request)?;
    let mut plan = backend_preview(backend, &request)?;
    validate_preview_contract(&plan, &request)?;
    plan.bounds_validated = true;
    serde_json::to_value(plan).map_err(json_error)
}

pub(crate) fn apply<B: BackendBundle>(
    backend: &B,
    safety: SafetyPolicy,
    plan: OperationPlan,
    experimental: bool,
    should_apply: bool,
    risk_acknowledgement: Option<&str>,
) -> Result<Value> {
    let request = request_from_plan(&plan);
    validate_apply_plan(&plan, &request, safety)?;

    let current = backend_preview(backend, &request)?;
    validate_preview_contract(&current, &request)?;
    if current.target != plan.target
        || current.backend != plan.backend
        || current.requested_changes != plan.requested_changes
        || current.captured_state != plan.captured_state
        || current.hardware_verified != plan.hardware_verified
    {
        return Err(NorthclockError::PermissionOrSafety(
            "preview backend, contract, or captured hardware state changed; create a new preview"
                .into(),
        ));
    }

    authorize_write(backend, experimental, should_apply, risk_acknowledgement)?;
    let receipt = backend_apply(backend, &plan)?;
    validate_receipt_identity(&receipt, &plan)?;
    if !receipt.validation_passed || receipt.readback != receipt.requested_changes {
        return rollback_after_failed_readback(backend, &receipt);
    }
    serde_json::to_value(receipt).map_err(json_error)
}

pub(crate) fn rollback<B: BackendBundle>(
    backend: &B,
    safety: SafetyPolicy,
    receipt: ApplyReceipt,
    experimental: bool,
    should_apply: bool,
    risk_acknowledgement: Option<&str>,
) -> Result<Value> {
    validate_rollback_input(&receipt, safety)?;
    let request = OperationRequest {
        target: receipt.target,
        changes: receipt.requested_changes.clone(),
    };
    let current = backend_preview(backend, &request)?;
    validate_preview_contract(&current, &request)?;
    if current.target != receipt.target
        || current.backend != receipt.backend
        || current.requested_changes != receipt.requested_changes
        || current.captured_state != receipt.readback
        || current.hardware_verified != receipt.hardware_verified
    {
        return Err(NorthclockError::PermissionOrSafety(
            "backend, operation contract, or hardware state changed after apply; refusing stale rollback"
                .into(),
        ));
    }

    authorize_write(backend, experimental, should_apply, risk_acknowledgement)?;
    let rollback = backend_rollback(backend, &receipt)?;
    validate_rollback_receipt(&rollback, &receipt)?;
    serde_json::to_value(rollback).map_err(json_error)
}

fn request_from_plan(plan: &OperationPlan) -> OperationRequest {
    OperationRequest {
        target: plan.target,
        changes: plan.requested_changes.clone(),
    }
}

fn validate_apply_plan(
    plan: &OperationPlan,
    request: &OperationRequest,
    safety: SafetyPolicy,
) -> Result<()> {
    if !plan.bounds_validated {
        return Err(NorthclockError::PermissionOrSafety(
            "apply requires a validated preview".into(),
        ));
    }
    safety.validate(request)?;
    validate_preview_contract(plan, request)
}

fn validate_preview_contract(plan: &OperationPlan, request: &OperationRequest) -> Result<()> {
    if plan.id.trim().is_empty()
        || plan.backend.trim().is_empty()
        || plan.target != request.target
        || plan.requested_changes != request.changes
        || plan.captured_state.is_empty()
        || plan.captured_state.keys().ne(plan.requested_changes.keys())
    {
        return Err(NorthclockError::PermissionOrSafety(
            "backend preview did not preserve the requested target, changes, or complete captured state"
                .into(),
        ));
    }
    Ok(())
}

fn validate_receipt_identity(receipt: &ApplyReceipt, plan: &OperationPlan) -> Result<()> {
    if receipt.plan_id != plan.id
        || receipt.target != plan.target
        || receipt.backend != plan.backend
        || receipt.captured_state != plan.captured_state
        || receipt.requested_changes != plan.requested_changes
        || !receipt.rollback_available
        || receipt.hardware_verified != plan.hardware_verified
    {
        return Err(NorthclockError::HardwareOperation(
            "backend returned a receipt inconsistent with the previewed operation".into(),
        ));
    }
    Ok(())
}

fn validate_rollback_input(receipt: &ApplyReceipt, safety: SafetyPolicy) -> Result<()> {
    if receipt.plan_id.trim().is_empty()
        || receipt.backend.trim().is_empty()
        || !receipt.rollback_available
        || !receipt.validation_passed
        || receipt.captured_state.is_empty()
        || receipt.requested_changes.is_empty()
        || receipt.readback != receipt.requested_changes
        || receipt
            .captured_state
            .keys()
            .ne(receipt.requested_changes.keys())
    {
        return Err(NorthclockError::PermissionOrSafety(
            "rollback requires a complete validated apply receipt".into(),
        ));
    }
    safety.validate(&OperationRequest {
        target: receipt.target,
        changes: receipt.requested_changes.clone(),
    })?;
    safety.validate(&OperationRequest {
        target: receipt.target,
        changes: receipt.captured_state.clone(),
    })
}

fn validate_rollback_receipt(rollback: &RollbackReceipt, receipt: &ApplyReceipt) -> Result<()> {
    if rollback.plan_id != receipt.plan_id
        || rollback.backend != receipt.backend
        || rollback.hardware_verified != receipt.hardware_verified
        || rollback.restored_state != receipt.captured_state
        || !rollback.validation_passed
        || rollback.readback != rollback.restored_state
    {
        return Err(NorthclockError::HardwareOperation(
            "rollback readback did not match captured state".into(),
        ));
    }
    Ok(())
}

fn rollback_after_failed_readback<B: BackendBundle>(
    backend: &B,
    receipt: &ApplyReceipt,
) -> Result<Value> {
    match backend_rollback(backend, receipt) {
        Ok(rollback) if validate_rollback_receipt(&rollback, receipt).is_ok() => {
            Err(NorthclockError::HardwareOperation(
                "readback validation failed; captured state was restored".into(),
            ))
        }
        Ok(_) => Err(NorthclockError::HardwareOperation(
            "readback validation and rollback validation both failed".into(),
        )),
        Err(error) => Err(NorthclockError::HardwareOperation(format!(
            "readback validation failed and rollback failed: {error}"
        ))),
    }
}

fn backend_preview<B: BackendBundle>(
    backend: &B,
    request: &OperationRequest,
) -> Result<OperationPlan> {
    match request.target {
        OperationTarget::CpuCurveOptimizer => backend.preview_cpu_operation(request),
        OperationTarget::GpuTuning => backend.preview_gpu_operation(request),
    }
}

fn backend_apply<B: BackendBundle>(backend: &B, plan: &OperationPlan) -> Result<ApplyReceipt> {
    match plan.target {
        OperationTarget::CpuCurveOptimizer => backend.apply_cpu_operation(plan),
        OperationTarget::GpuTuning => backend.apply_gpu_operation(plan),
    }
}

fn backend_rollback<B: BackendBundle>(
    backend: &B,
    receipt: &ApplyReceipt,
) -> Result<RollbackReceipt> {
    match receipt.target {
        OperationTarget::CpuCurveOptimizer => backend.rollback_cpu_operation(receipt),
        OperationTarget::GpuTuning => backend.rollback_gpu_operation(receipt),
    }
}
