mod support;

use northclock_core::*;
use std::collections::BTreeMap;
use std::time::Duration;
use support::{curve_request, MockBackend, Scenario};

#[test]
fn measurement_envelope_carries_actual_backend_source() {
    let service = ApplicationService::new(MockBackend::new(Scenario::Healthy));
    let envelope = service.execute(ApplicationCommand::CpuMeasurements);
    assert_eq!(envelope.status, CommandStatus::Success);
    let encoded = serde_json::to_string(&envelope)
        .unwrap_or_else(|error| panic!("serialization failed: {error}"));
    assert!(encoded.contains("mock-test-only"));
    assert!(!encoded.contains("synthetic"));
}

#[test]
fn missing_driver_never_fabricates_measurements() {
    let service = ApplicationService::new(MockBackend::new(Scenario::MissingDrivers));
    let envelope = service.execute(ApplicationCommand::GpuMeasurements { stable_id: None });
    assert_eq!(envelope.status, CommandStatus::Unavailable);
    assert!(envelope.data.is_none());
    assert_eq!(envelope.exit_code(), 3);
}

#[test]
fn mock_matrix_preserves_distinct_hardware_failures() {
    for scenario in [
        Scenario::UnsupportedGeneration,
        Scenario::PermissionDenied,
        Scenario::ThermalAbort,
        Scenario::WheaFault,
        Scenario::DeviceRemoval,
        Scenario::RollbackFailure,
    ] {
        let backend = MockBackend::new(scenario);
        match scenario {
            Scenario::UnsupportedGeneration => {
                assert_eq!(
                    ApplicationService::new(backend)
                        .execute(ApplicationCommand::CpuIdentity)
                        .exit_code(),
                    3
                );
            }
            Scenario::PermissionDenied => {
                assert!(!backend
                    .is_elevated()
                    .unwrap_or_else(|error| panic!("elevation mock failed: {error}")));
            }
            Scenario::ThermalAbort | Scenario::WheaFault => {
                let plan = backend.plan(&curve_request(-10), "cpu");
                assert_eq!(
                    backend.apply(&plan).err().map(|error| error.exit_code()),
                    Some(5)
                );
            }
            Scenario::DeviceRemoval => {
                assert_eq!(
                    backend
                        .run_vram_test(None, 1024, Duration::from_secs(1))
                        .err()
                        .map(|error| error.exit_code()),
                    Some(5)
                );
            }
            Scenario::RollbackFailure => {
                let plan = backend.plan(&curve_request(-10), "cpu");
                let receipt = backend
                    .apply(&plan)
                    .unwrap_or_else(|error| panic!("apply mock failed: {error}"));
                assert!(backend.rollback(&receipt).is_err());
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn timeout_is_explicit_data() {
    let service = ApplicationService::new(MockBackend::new(Scenario::Timeout));
    let envelope = service.execute(ApplicationCommand::VramTest {
        adapter: None,
        bytes: 1024,
        timeout_ms: 1,
    });
    assert_eq!(envelope.status, CommandStatus::Success);
    assert_eq!(
        envelope
            .data
            .as_ref()
            .and_then(|data| data.get("timed_out"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

#[test]
fn bounded_commands_use_their_typed_backends() {
    let service = ApplicationService::new(MockBackend::new(Scenario::Healthy));
    assert_eq!(
        service
            .execute(ApplicationCommand::CpuWorkload {
                duration_ms: 1,
                threads: 1,
            })
            .status,
        CommandStatus::Success
    );
    assert_eq!(
        service
            .execute(ApplicationCommand::ProcessAffinityPreview {
                process_id: 7,
                mask: 1,
            })
            .status,
        CommandStatus::Success
    );
    assert_eq!(
        service
            .execute(ApplicationCommand::WheaEvents { duration_ms: 1 })
            .status,
        CommandStatus::Success
    );
    assert_eq!(
        service.execute(ApplicationCommand::SystemStatus).status,
        CommandStatus::Success
    );
}

#[test]
fn oversized_vram_request_is_rejected_before_backend_allocation() {
    let service = ApplicationService::new(MockBackend::new(Scenario::Healthy));
    let envelope = service.execute(ApplicationCommand::VramTest {
        adapter: None,
        bytes: u64::MAX,
        timeout_ms: 1,
    });
    assert_eq!(envelope.exit_code(), 2);
}

#[test]
fn safety_bounds_apply_before_backend_preview() {
    let service = ApplicationService::new(MockBackend::new(Scenario::Healthy));
    let envelope = service.execute(ApplicationCommand::OperationPreview(curve_request(-51)));
    assert_eq!(envelope.status, CommandStatus::Rejected);
    assert_eq!(envelope.exit_code(), 4);
}

#[test]
fn target_rejects_changes_from_another_operation_domain() {
    let request = OperationRequest {
        target: OperationTarget::CpuCurveOptimizer,
        changes: BTreeMap::from([("gpu_power_limit_percent".into(), 5)]),
    };
    let envelope = ApplicationService::new(MockBackend::new(Scenario::Healthy))
        .execute(ApplicationCommand::OperationPreview(request));
    assert_eq!(envelope.exit_code(), 4);
}

#[test]
fn preview_captures_before_state() {
    let service = ApplicationService::new(MockBackend::new(Scenario::Healthy));
    let envelope = service.execute(ApplicationCommand::OperationPreview(curve_request(-10)));
    assert_eq!(envelope.status, CommandStatus::Success);
    let plan: OperationPlan = serde_json::from_value(
        envelope
            .data
            .unwrap_or_else(|| panic!("preview returned no plan")),
    )
    .unwrap_or_else(|error| panic!("invalid plan JSON: {error}"));
    assert!(plan.bounds_validated);
    assert_eq!(plan.captured_state.get("curve_optimizer"), Some(&0));
}

#[test]
fn preview_rejects_an_incomplete_backend_contract() {
    let service = ApplicationService::new(MockBackend::new(Scenario::MalformedBackendContract));
    let envelope = service.execute(ApplicationCommand::OperationPreview(curve_request(-10)));
    assert_eq!(envelope.status, CommandStatus::Rejected);
    assert_eq!(envelope.exit_code(), 4);
}

#[cfg(not(feature = "experimental-hardware-writes"))]
#[test]
fn default_build_rejects_every_write() {
    let backend = MockBackend::new(Scenario::Healthy);
    let mut plan = backend.plan(&curve_request(-10), "cpu");
    plan.bounds_validated = true;
    let service = ApplicationService::new(backend);
    let envelope = service.execute(ApplicationCommand::OperationApply {
        plan,
        experimental: true,
        apply: true,
        risk_acknowledgement: Some(RISK_ACKNOWLEDGEMENT.into()),
    });
    assert_eq!(envelope.status, CommandStatus::Rejected);
    assert_eq!(envelope.exit_code(), 4);
}

#[cfg(feature = "experimental-hardware-writes")]
#[test]
fn all_feature_build_requires_readback_and_rolls_back_mismatch() {
    let backend = MockBackend::new(Scenario::ReadbackMismatch);
    let mut plan = backend.plan(&curve_request(-10), "cpu");
    plan.bounds_validated = true;
    let service = ApplicationService::new(backend);
    let envelope = service.execute(ApplicationCommand::OperationApply {
        plan,
        experimental: true,
        apply: true,
        risk_acknowledgement: Some(RISK_ACKNOWLEDGEMENT.into()),
    });
    assert_eq!(envelope.status, CommandStatus::Failure);
    assert_eq!(envelope.exit_code(), 5);
    assert!(envelope
        .error
        .is_some_and(|error| error.message.contains("restored")));
}

#[cfg(feature = "experimental-hardware-writes")]
#[test]
fn all_feature_build_rejects_forged_captured_state() {
    let backend = MockBackend::new(Scenario::Healthy);
    let mut plan = backend.plan(&curve_request(-10), "cpu");
    plan.bounds_validated = true;
    plan.captured_state.insert("curve_optimizer".into(), 7);
    let envelope = ApplicationService::new(backend).execute(ApplicationCommand::OperationApply {
        plan,
        experimental: true,
        apply: true,
        risk_acknowledgement: Some(RISK_ACKNOWLEDGEMENT.into()),
    });
    assert_eq!(envelope.status, CommandStatus::Rejected);
    assert_eq!(envelope.exit_code(), 4);
}
