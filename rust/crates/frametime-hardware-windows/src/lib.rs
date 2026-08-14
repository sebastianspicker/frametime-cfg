//! Read-only Windows implementation of [`frametime_hardware`] diagnostics.
//!
//! No command launches a shell, PowerShell, executable, or GUI. The ETW and
//! Event Log integrations are bounded native API calls and return typed data.

#[cfg(not(windows))]
use frametime_hardware::DiagnosticError;
#[cfg(windows)]
use frametime_hardware::DiagnosticPayload;
use frametime_hardware::{
    CapabilityState, DiagnosticCapability, DiagnosticCommand, DiagnosticEnvelope, DoctorReport,
};

#[cfg(windows)]
mod etw;
#[cfg(windows)]
mod system;
#[cfg(windows)]
mod whea;

/// Native, read-only implementation that a CLI or GUI can compose directly.
#[derive(Clone, Debug, Default)]
pub struct WindowsHardwareDiagnostics;

impl WindowsHardwareDiagnostics {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Executes exactly one bounded diagnostic command.
    #[must_use]
    pub fn execute(&self, command: DiagnosticCommand) -> DiagnosticEnvelope {
        if let Err(error) = command.validate() {
            return DiagnosticEnvelope::failure(&command, error);
        }
        #[cfg(windows)]
        {
            self.execute_windows(command)
        }
        #[cfg(not(windows))]
        {
            DiagnosticEnvelope::failure(
                &command,
                DiagnosticError::unavailable(
                    "frametime-hardware-windows only performs diagnostics on Windows",
                ),
            )
        }
    }

    #[cfg(windows)]
    fn execute_windows(&self, command: DiagnosticCommand) -> DiagnosticEnvelope {
        let result = match &command {
            DiagnosticCommand::Doctor => Ok(DiagnosticPayload::Doctor(self.doctor_report())),
            DiagnosticCommand::CpuIdentity => {
                system::cpu_identity().map(DiagnosticPayload::CpuIdentity)
            }
            DiagnosticCommand::GpuInventory => {
                system::gpu_inventory().map(DiagnosticPayload::GpuInventory)
            }
            DiagnosticCommand::SystemStatus => {
                system::system_status().map(DiagnosticPayload::SystemStatus)
            }
            DiagnosticCommand::WheaEvents(request) => {
                whea::read_whea_events(request.max_records).map(DiagnosticPayload::WheaEvents)
            }
            DiagnosticCommand::EtwFrameCapture(request) => {
                etw::capture_present_starts(request.duration_ms)
                    .map(DiagnosticPayload::EtwFrameCapture)
            }
        };
        match result {
            Ok(payload) => DiagnosticEnvelope::success(&command, payload),
            Err(error) => DiagnosticEnvelope::failure(&command, error),
        }
    }

    #[must_use]
    pub fn doctor_report(&self) -> DoctorReport {
        let platform = if cfg!(windows) {
            "windows"
        } else {
            "non_windows"
        };
        let state = if cfg!(windows) {
            CapabilityState::Available
        } else {
            CapabilityState::Unsupported
        };
        let detail = if cfg!(windows) {
            "native read-only Windows API adapter; no live hardware validation claim"
        } else {
            "Windows-only adapter; requests fail closed on this platform"
        };
        DoctorReport {
            platform: platform.into(),
            capabilities: [
                ("cpu.identity", "CPUID + Windows system information"),
                ("gpu.inventory", "DXGI adapter enumeration"),
                ("system.status", "Windows system information"),
                ("events.whea", "Windows Event Log"),
                ("frames.etw_capture", "DxgKrnl ETW Present_Start"),
            ]
            .into_iter()
            .map(|(name, backend)| DiagnosticCapability::new(name, state, backend, detail))
            .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(windows))]
    use frametime_hardware::{DiagnosticStatus, EtwFrameCaptureRequest};

    #[test]
    fn doctor_exposes_all_composable_commands() {
        let doctor = WindowsHardwareDiagnostics::new().doctor_report();
        assert_eq!(doctor.capabilities.len(), 5);
        assert!(
            doctor
                .capabilities
                .iter()
                .all(|item| !item.hardware_verified)
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_execution_fails_closed() {
        let envelope = WindowsHardwareDiagnostics::new().execute(
            DiagnosticCommand::EtwFrameCapture(EtwFrameCaptureRequest { duration_ms: 1 }),
        );
        assert_eq!(envelope.status, DiagnosticStatus::Unavailable);
        assert!(envelope.data.is_none());
    }
}
