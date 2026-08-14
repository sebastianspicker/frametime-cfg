use serde::{Deserialize, Serialize};

/// Stable schema marker for every native diagnostic response.
pub const DIAGNOSTIC_SCHEMA_VERSION: &str = "frametime.hardware/v1";
const MAX_ETW_CAPTURE_MS: u32 = 60_000;
const MAX_WHEA_RECORDS: u16 = 128;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Available,
    Unsupported,
    PermissionRequired,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiagnosticCapability {
    pub name: String,
    pub state: CapabilityState,
    pub backend: String,
    pub detail: String,
    /// This is always false until a separate hardware-validation campaign proves it.
    pub hardware_verified: bool,
}

impl DiagnosticCapability {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        state: CapabilityState,
        backend: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            state,
            backend: backend.into(),
            detail: detail.into(),
            hardware_verified: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DoctorReport {
    pub platform: String,
    pub capabilities: Vec<DiagnosticCapability>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CpuIdentity {
    pub display_name: String,
    pub vendor: Option<String>,
    pub family: Option<u8>,
    pub model: Option<u8>,
    pub logical_processors: u32,
    pub physical_cores: Option<u32>,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GpuAdapter {
    pub stable_id: String,
    pub display_name: String,
    pub vendor: Option<String>,
    pub vendor_id: u32,
    pub device_id: u32,
    pub subsystem_id: u32,
    pub revision: u32,
    pub is_software: bool,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GpuInventory {
    pub adapters: Vec<GpuAdapter>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SystemStatus {
    pub architecture: String,
    pub logical_processors: u32,
    pub total_physical_memory_bytes: Option<u64>,
    pub available_physical_memory_bytes: Option<u64>,
    pub uptime_ms: u64,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WheaEvent {
    pub provider: String,
    pub event_id: u32,
    pub timestamp_utc: Option<String>,
    /// OS-rendered event XML, bounded by the platform adapter before retention.
    pub detail_xml: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FrameSample {
    pub process_id: u32,
    pub present_start_unix_ms: u64,
    pub frame_time_us: u64,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EtwFrameCaptureRequest {
    pub duration_ms: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WheaEventsRequest {
    pub max_records: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum DiagnosticCommand {
    Doctor,
    CpuIdentity,
    GpuInventory,
    SystemStatus,
    WheaEvents(WheaEventsRequest),
    EtwFrameCapture(EtwFrameCaptureRequest),
}

impl DiagnosticCommand {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Doctor => "doctor",
            Self::CpuIdentity => "cpu.identity",
            Self::GpuInventory => "gpu.inventory",
            Self::SystemStatus => "system.status",
            Self::WheaEvents(_) => "events.whea",
            Self::EtwFrameCapture(_) => "frames.etw_capture",
        }
    }

    pub fn validate(&self) -> Result<(), DiagnosticError> {
        match self {
            Self::WheaEvents(request) if request.max_records == 0 => Err(DiagnosticError::invalid(
                "WHEA event retrieval requires at least one record",
            )),
            Self::WheaEvents(request) if request.max_records > MAX_WHEA_RECORDS => {
                Err(DiagnosticError::invalid(format!(
                    "WHEA event retrieval is limited to {MAX_WHEA_RECORDS} records"
                )))
            }
            Self::EtwFrameCapture(request) if request.duration_ms == 0 => Err(
                DiagnosticError::invalid("ETW capture requires a non-zero duration"),
            ),
            Self::EtwFrameCapture(request) if request.duration_ms > MAX_ETW_CAPTURE_MS => {
                Err(DiagnosticError::invalid(format!(
                    "ETW capture is limited to {MAX_ETW_CAPTURE_MS} ms"
                )))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStatus {
    Success,
    Failure,
    Unavailable,
    Rejected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticErrorCode {
    InvalidRequest,
    PlatformUnsupported,
    PermissionDenied,
    SystemApiFailure,
    Internal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiagnosticError {
    pub code: DiagnosticErrorCode,
    pub message: String,
}

impl DiagnosticError {
    #[must_use]
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: DiagnosticErrorCode::InvalidRequest,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            code: DiagnosticErrorCode::PlatformUnsupported,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn system(message: impl Into<String>) -> Self {
        Self {
            code: DiagnosticErrorCode::SystemApiFailure,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DiagnosticPayload {
    Doctor(DoctorReport),
    CpuIdentity(CpuIdentity),
    GpuInventory(GpuInventory),
    SystemStatus(SystemStatus),
    WheaEvents(Vec<WheaEvent>),
    EtwFrameCapture(Vec<FrameSample>),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiagnosticEnvelope {
    pub schema_version: String,
    pub command: String,
    pub status: DiagnosticStatus,
    pub data: Option<DiagnosticPayload>,
    pub error: Option<DiagnosticError>,
}

impl DiagnosticEnvelope {
    #[must_use]
    pub fn success(command: &DiagnosticCommand, data: DiagnosticPayload) -> Self {
        Self {
            schema_version: DIAGNOSTIC_SCHEMA_VERSION.into(),
            command: command.name().into(),
            status: DiagnosticStatus::Success,
            data: Some(data),
            error: None,
        }
    }

    #[must_use]
    pub fn failure(command: &DiagnosticCommand, error: DiagnosticError) -> Self {
        let status = match error.code {
            DiagnosticErrorCode::PlatformUnsupported => DiagnosticStatus::Unavailable,
            DiagnosticErrorCode::InvalidRequest | DiagnosticErrorCode::PermissionDenied => {
                DiagnosticStatus::Rejected
            }
            DiagnosticErrorCode::SystemApiFailure | DiagnosticErrorCode::Internal => {
                DiagnosticStatus::Failure
            }
        };
        Self {
            schema_version: DIAGNOSTIC_SCHEMA_VERSION.into(),
            command: command.name().into(),
            status,
            data: None,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_is_versioned_and_typed() {
        let command = DiagnosticCommand::Doctor;
        let envelope = DiagnosticEnvelope::success(
            &command,
            DiagnosticPayload::Doctor(DoctorReport {
                platform: "test".into(),
                capabilities: vec![],
            }),
        );
        assert_eq!(envelope.schema_version, DIAGNOSTIC_SCHEMA_VERSION);
        assert_eq!(envelope.command, "doctor");
        assert!(matches!(envelope.data, Some(DiagnosticPayload::Doctor(_))));
    }

    #[test]
    fn validation_bounds_untrusted_capture_requests() {
        assert!(
            DiagnosticCommand::EtwFrameCapture(EtwFrameCaptureRequest { duration_ms: 0 })
                .validate()
                .is_err()
        );
        assert!(
            DiagnosticCommand::WheaEvents(WheaEventsRequest { max_records: 129 })
                .validate()
                .is_err()
        );
    }

    #[test]
    fn unsupported_error_maps_to_unavailable() {
        let envelope = DiagnosticEnvelope::failure(
            &DiagnosticCommand::GpuInventory,
            DiagnosticError::unavailable("Windows only"),
        );
        assert_eq!(envelope.status, DiagnosticStatus::Unavailable);
    }
}
