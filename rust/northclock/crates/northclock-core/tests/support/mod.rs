use northclock_core::*;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scenario {
    Healthy,
    MissingDrivers,
    UnsupportedGeneration,
    PermissionDenied,
    ThermalAbort,
    WheaFault,
    DeviceRemoval,
    ReadbackMismatch,
    RollbackFailure,
    Timeout,
    MalformedBackendContract,
}

#[derive(Clone, Debug)]
pub struct MockBackend {
    scenario: Scenario,
}

impl MockBackend {
    pub fn new(scenario: Scenario) -> Self {
        Self { scenario }
    }

    pub fn capability(&self, name: &str) -> CapabilityReport {
        let state = match self.scenario {
            Scenario::MissingDrivers => CapabilityState::Unsupported,
            Scenario::UnsupportedGeneration => CapabilityState::Unsupported,
            Scenario::PermissionDenied => CapabilityState::PermissionRequired,
            Scenario::Healthy
            | Scenario::ThermalAbort
            | Scenario::WheaFault
            | Scenario::DeviceRemoval
            | Scenario::ReadbackMismatch
            | Scenario::RollbackFailure
            | Scenario::Timeout
            | Scenario::MalformedBackendContract => CapabilityState::Available,
        };
        CapabilityReport::new(
            name,
            state,
            "mock-test-only",
            format!("{:?}", self.scenario),
        )
    }

    pub fn cpu_device() -> DeviceIdentity {
        DeviceIdentity::new("cpu", "mock-cpu", "Injected mock CPU", Some("AMD".into()))
    }

    pub fn gpu_device() -> DeviceIdentity {
        DeviceIdentity::new(
            "gpu",
            "mock-gpu",
            "Injected mock GPU",
            Some("NVIDIA".into()),
        )
    }

    pub fn plan(&self, request: &OperationRequest, backend: &str) -> OperationPlan {
        let mut captured = BTreeMap::new();
        for name in request.changes.keys() {
            captured.insert(name.clone(), 0);
        }
        let mut plan = OperationPlan::new(
            request.target,
            format!("mock-test-only-{backend}"),
            request.changes.clone(),
            captured,
        );
        if self.scenario == Scenario::MalformedBackendContract {
            plan.backend.clear();
        }
        plan
    }

    pub fn apply(&self, plan: &OperationPlan) -> Result<ApplyReceipt> {
        match self.scenario {
            Scenario::PermissionDenied => Err(NorthclockError::PermissionOrSafety(
                "injected permission denial".into(),
            )),
            Scenario::ThermalAbort => Err(NorthclockError::HardwareOperation(
                "injected thermal abort".into(),
            )),
            Scenario::WheaFault => Err(NorthclockError::HardwareOperation(
                "injected WHEA fault".into(),
            )),
            _ => {
                let readback = if self.scenario == Scenario::ReadbackMismatch {
                    plan.captured_state.clone()
                } else {
                    plan.requested_changes.clone()
                };
                Ok(ApplyReceipt {
                    plan_id: plan.id.clone(),
                    target: plan.target,
                    captured_state: plan.captured_state.clone(),
                    requested_changes: plan.requested_changes.clone(),
                    readback,
                    validation_passed: self.scenario != Scenario::ReadbackMismatch,
                    rollback_available: true,
                    backend: plan.backend.clone(),
                    hardware_verified: false,
                })
            }
        }
    }

    pub fn rollback(&self, receipt: &ApplyReceipt) -> Result<RollbackReceipt> {
        if self.scenario == Scenario::RollbackFailure {
            return Err(NorthclockError::HardwareOperation(
                "injected rollback failure".into(),
            ));
        }
        Ok(RollbackReceipt {
            plan_id: receipt.plan_id.clone(),
            restored_state: receipt.captured_state.clone(),
            readback: receipt.captured_state.clone(),
            validation_passed: true,
            backend: receipt.backend.clone(),
            hardware_verified: false,
        })
    }
}

impl CapabilityBackend for MockBackend {
    fn capabilities(&self) -> Vec<CapabilityReport> {
        [
            "cpu.identity",
            "cpu.telemetry",
            "cpu.workload",
            "cpu.tuning",
            "gpu.inventory",
            "gpu.telemetry",
            "gpu.tuning",
            "memory.vram_test",
            "power.plans",
            "windows.system_status",
            "process.affinity",
            "events.whea",
            "frames.capture",
            "rom.inspect",
        ]
        .into_iter()
        .map(|name| self.capability(name))
        .collect()
    }

    fn is_elevated(&self) -> Result<bool> {
        Ok(self.scenario != Scenario::PermissionDenied)
    }
}

impl CpuTelemetryBackend for MockBackend {
    fn cpu_identity(&self) -> Result<CpuIdentity> {
        if self.scenario == Scenario::UnsupportedGeneration {
            return Err(NorthclockError::Unavailable(
                "injected unsupported CPU generation".into(),
            ));
        }
        Ok(CpuIdentity {
            device: Self::cpu_device(),
            family: Some(0x19),
            model: Some(0x61),
            physical_cores: 8,
            logical_processors: 16,
        })
    }

    fn cpu_measurements(&self) -> Result<Vec<Measurement<f64>>> {
        match self.scenario {
            Scenario::ThermalAbort => Ok(vec![Measurement::at(
                101.0,
                "C",
                1,
                Self::cpu_device(),
                "mock-test-only",
            )]),
            Scenario::MissingDrivers => Ok(Vec::new()),
            _ => Ok(vec![Measurement::at(
                45.0,
                "C",
                1,
                Self::cpu_device(),
                "mock-test-only",
            )]),
        }
    }
}

impl CpuTuningBackend for MockBackend {
    fn preview_cpu_operation(&self, request: &OperationRequest) -> Result<OperationPlan> {
        Ok(self.plan(request, "cpu"))
    }

    fn apply_cpu_operation(&self, plan: &OperationPlan) -> Result<ApplyReceipt> {
        self.apply(plan)
    }

    fn rollback_cpu_operation(&self, receipt: &ApplyReceipt) -> Result<RollbackReceipt> {
        self.rollback(receipt)
    }
}

impl GpuTelemetryBackend for MockBackend {
    fn gpu_devices(&self) -> Result<Vec<GpuDevice>> {
        Ok(vec![GpuDevice {
            device: Self::gpu_device(),
            dedicated_memory_bytes: Some(8 * 1024 * 1024 * 1024),
            driver_backend: "mock-test-only".into(),
        }])
    }

    fn gpu_measurements(&self, _stable_id: Option<&str>) -> Result<Vec<Measurement<f64>>> {
        if self.scenario == Scenario::MissingDrivers {
            return Ok(Vec::new());
        }
        Ok(vec![Measurement::at(
            120.0,
            "W",
            1,
            Self::gpu_device(),
            "mock-test-only",
        )])
    }
}

impl GpuTuningBackend for MockBackend {
    fn preview_gpu_operation(&self, request: &OperationRequest) -> Result<OperationPlan> {
        Ok(self.plan(request, "gpu"))
    }

    fn apply_gpu_operation(&self, plan: &OperationPlan) -> Result<ApplyReceipt> {
        self.apply(plan)
    }

    fn rollback_gpu_operation(&self, receipt: &ApplyReceipt) -> Result<RollbackReceipt> {
        self.rollback(receipt)
    }
}

impl WorkloadBackend for MockBackend {
    fn run_cpu_workload(&self, duration: Duration, threads: usize) -> Result<CpuWorkloadReport> {
        Ok(CpuWorkloadReport {
            requested_duration_ms: duration.as_millis() as u64,
            elapsed_ms: duration.as_millis(),
            threads,
            iterations: 10,
            iterations_per_second: 10.0,
            validation_checks: 1,
            validation_errors: u64::from(self.scenario == Scenario::WheaFault),
            timed_out: self.scenario == Scenario::Timeout,
            hardware_verified: false,
        })
    }

    fn run_vram_test(
        &self,
        _adapter: Option<&str>,
        _bytes: u64,
        timeout: Duration,
    ) -> Result<WorkloadReport> {
        if self.scenario == Scenario::DeviceRemoval {
            return Err(NorthclockError::HardwareOperation(
                "injected DXGI device removal".into(),
            ));
        }
        Ok(WorkloadReport {
            duration_ms: timeout.as_millis(),
            iterations: 1,
            validation_errors: 0,
            timed_out: self.scenario == Scenario::Timeout,
            hardware_verified: false,
        })
    }
}

impl ProcessControlBackend for MockBackend {
    fn preview_process_affinity(&self, process_id: u32, mask: u64) -> Result<ProcessAffinityPlan> {
        Ok(ProcessAffinityPlan::new(process_id, mask, 1, u64::MAX))
    }

    fn apply_process_affinity(&self, plan: &ProcessAffinityPlan) -> Result<ProcessAffinityReceipt> {
        Ok(ProcessAffinityReceipt {
            plan_id: plan.id.clone(),
            process_id: plan.process_id,
            captured_mask: plan.captured_mask,
            requested_mask: plan.requested_mask,
            readback_mask: plan.requested_mask,
            validation_passed: true,
            rollback_available: true,
        })
    }

    fn rollback_process_affinity(
        &self,
        receipt: &ProcessAffinityReceipt,
    ) -> Result<ProcessAffinityRollbackReceipt> {
        Ok(ProcessAffinityRollbackReceipt {
            plan_id: receipt.plan_id.clone(),
            process_id: receipt.process_id,
            restored_mask: receipt.captured_mask,
            readback_mask: receipt.captured_mask,
            validation_passed: true,
        })
    }
}

impl PowerPlanBackend for MockBackend {
    fn power_plans(&self) -> Result<Vec<PowerPlan>> {
        Ok(vec![PowerPlan {
            guid: "00000000-0000-0000-0000-000000000000".into(),
            name: "Injected mock".into(),
            active: true,
        }])
    }
}

impl SystemStatusBackend for MockBackend {
    fn system_status(&self) -> Result<SystemStatusReport> {
        Ok(SystemStatusReport {
            task_scheduler: Observation::not_found("mock-test-only"),
            virtualization_based_security: Observation::observed(
                "mock-test-only",
                VbsStatus {
                    runtime_state: VbsRuntimeState::NotEnabled,
                    runtime_state_raw: 0,
                },
            ),
            potential_conflicts: Observation::observed("mock-test-only", Vec::new()),
        })
    }
}

impl FrameCaptureBackend for MockBackend {
    fn capture_frames(&self, _duration: Duration) -> Result<Vec<FrameSample>> {
        Ok(vec![FrameSample {
            process_id: 7,
            frame_time: Measurement::at(16.6, "ms", 1, Self::gpu_device(), "mock-test-only"),
        }])
    }
}

impl OverlayBackend for MockBackend {
    fn show_overlay(&self, _measurements: &[Measurement<f64>]) -> Result<()> {
        Ok(())
    }

    fn hide_overlay(&self) -> Result<()> {
        Ok(())
    }
}

impl EventObservationBackend for MockBackend {
    fn observe_whea(&self, _duration: Duration) -> Result<Vec<ObservedEvent>> {
        if self.scenario == Scenario::WheaFault {
            return Ok(vec![ObservedEvent {
                provider: "mock-test-only".into(),
                event_id: 18,
                timestamp_unix_ms: 1,
                detail: "injected cache hierarchy error".into(),
            }]);
        }
        Ok(Vec::new())
    }
}

impl RomInspectionBackend for MockBackend {
    fn read_rom(&self, _path: &Path) -> Result<Vec<u8>> {
        let mut bytes = vec![0_u8; 512];
        bytes[0] = 0x55;
        bytes[1] = 0xaa;
        bytes[2] = 1;
        let checksum = bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte));
        bytes[511] = 0_u8.wrapping_sub(checksum);
        Ok(bytes)
    }
}

pub fn curve_request(value: i64) -> OperationRequest {
    OperationRequest::cpu_curve_optimizer(value)
}
