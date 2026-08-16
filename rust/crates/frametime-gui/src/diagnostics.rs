//! Platform-neutral presentation for read-only hardware diagnostics.
//!
//! Native Windows calls remain in the platform adapter. This module only turns
//! versioned typed envelopes into accessible, visible table rows.

use frametime_hardware::{
    CapabilityState, DiagnosticCommand, DiagnosticEnvelope, DiagnosticPayload, DiagnosticStatus,
    EtwFrameCaptureRequest, WheaEventsRequest,
};

pub const ETW_CAPTURE_DURATION_MS: u32 = 5_000;
const WHEA_RECORD_LIMIT: u16 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticAction {
    Doctor,
    Cpu,
    Gpu,
    System,
    Whea,
    EtwFrames,
}

impl DiagnosticAction {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Doctor => "Hardware doctor",
            Self::Cpu => "CPU identity",
            Self::Gpu => "GPU inventory",
            Self::System => "System status",
            Self::Whea => "Read WHEA events",
            Self::EtwFrames => "Capture 5s ETW frames",
        }
    }

    pub const fn is_benchmark(self) -> bool {
        matches!(self, Self::EtwFrames)
    }

    pub fn command(self) -> DiagnosticCommand {
        match self {
            Self::Doctor => DiagnosticCommand::Doctor,
            Self::Cpu => DiagnosticCommand::CpuIdentity,
            Self::Gpu => DiagnosticCommand::GpuInventory,
            Self::System => DiagnosticCommand::SystemStatus,
            Self::Whea => DiagnosticCommand::WheaEvents(WheaEventsRequest {
                max_records: WHEA_RECORD_LIMIT,
            }),
            Self::EtwFrames => DiagnosticCommand::EtwFrameCapture(EtwFrameCaptureRequest {
                duration_ms: ETW_CAPTURE_DURATION_MS,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticPresentationKind {
    Complete,
    Warning,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticRow {
    pub item: String,
    pub value: String,
    pub state: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticPresentation {
    pub action: Option<DiagnosticAction>,
    pub kind: DiagnosticPresentationKind,
    pub detail: String,
    pub rows: Vec<DiagnosticRow>,
}

impl DiagnosticPresentation {
    pub fn empty() -> Self {
        Self {
            action: None,
            kind: DiagnosticPresentationKind::Complete,
            detail: "No hardware diagnostic has run. Results are read-only and do not change workflow progress.".into(),
            rows: Vec::new(),
        }
    }

    pub fn from_envelope(action: DiagnosticAction, envelope: DiagnosticEnvelope) -> Self {
        let version = envelope.schema_version;
        let command = envelope.command;
        match (envelope.status, envelope.data, envelope.error) {
            (DiagnosticStatus::Success, Some(payload), None) => Self {
                action: Some(action),
                kind: DiagnosticPresentationKind::Complete,
                detail: format!(
                    "Read-only typed result received: {command} ({version}). This is not workflow progress or a hardware-validation claim."
                ),
                rows: payload_rows(payload, &version),
            },
            (status, _, error) => {
                let message = error.map_or_else(
                    || "The adapter returned no typed result detail.".into(),
                    |error| format!("{:?}: {}", error.code, error.message),
                );
                let kind = match status {
                    DiagnosticStatus::Unavailable | DiagnosticStatus::Rejected => {
                        DiagnosticPresentationKind::Warning
                    }
                    DiagnosticStatus::Failure => DiagnosticPresentationKind::Failed,
                    DiagnosticStatus::Success => DiagnosticPresentationKind::Failed,
                };
                Self {
                    action: Some(action),
                    kind,
                    detail: format!(
                        "Read-only diagnostic {command} is {:?}: {message} ({version}). No workflow progress was recorded.",
                        status
                    ),
                    rows: vec![DiagnosticRow {
                        item: format!("{command} result"),
                        value: format!("{version}; {:?}", status),
                        state: message,
                    }],
                }
            }
        }
    }

    pub fn belongs_to_benchmark(&self) -> bool {
        self.action.is_some_and(DiagnosticAction::is_benchmark)
    }

    pub fn belongs_to_assess(&self) -> bool {
        self.action.is_some_and(|action| !action.is_benchmark())
    }
}

fn payload_rows(payload: DiagnosticPayload, version: &str) -> Vec<DiagnosticRow> {
    let mut rows = vec![DiagnosticRow {
        item: "Schema".into(),
        value: version.into(),
        state: "Versioned typed diagnostic envelope".into(),
    }];
    match payload {
        DiagnosticPayload::Doctor(report) => rows.extend(report.capabilities.into_iter().map(|item| {
            DiagnosticRow {
                item: item.name,
                value: capability_state(item.state).into(),
                state: format!(
                    "{}; {}; hardware validation not claimed",
                    item.backend, item.detail
                ),
            }
        })),
        DiagnosticPayload::CpuIdentity(cpu) => rows.extend([
            row("CPU", cpu.display_name, cpu.source),
            row("Vendor", cpu.vendor.unwrap_or_else(|| "Unavailable".into()), "CPUID value"),
            row(
                "Topology",
                format!("{} logical / {:?} physical cores", cpu.logical_processors, cpu.physical_cores),
                format!("family {:?}, model {:?}", cpu.family, cpu.model),
            ),
        ]),
        DiagnosticPayload::GpuInventory(inventory) => rows.extend(inventory.adapters.into_iter().map(|gpu| {
            row(
                "GPU",
                gpu.display_name,
                format!(
                    "{}; {}; {}",
                    gpu.stable_id,
                    gpu.source,
                    if gpu.is_software { "software adapter" } else { "hardware adapter" }
                ),
            )
        })),
        DiagnosticPayload::SystemStatus(system) => rows.extend([
            row("Architecture", system.architecture, system.source),
            row("Logical processors", system.logical_processors.to_string(), "Windows system information"),
            row(
                "Physical memory",
                format!(
                    "{} total / {} available",
                    bytes(system.total_physical_memory_bytes),
                    bytes(system.available_physical_memory_bytes)
                ),
                format!("uptime {} ms", system.uptime_ms),
            ),
        ]),
        DiagnosticPayload::WheaEvents(events) if events.is_empty() => rows.push(row(
            "WHEA events",
            "0 records",
            "No matching bounded Event Log records were returned.",
        )),
        DiagnosticPayload::WheaEvents(events) => rows.extend(events.into_iter().map(|event| {
            row(
                format!("WHEA event {}", event.event_id),
                event.timestamp_utc.unwrap_or_else(|| "Timestamp unavailable".into()),
                format!("{}; XML retained by the typed adapter", event.provider),
            )
        })),
        DiagnosticPayload::EtwFrameCapture(samples) if samples.is_empty() => rows.push(row(
            "ETW frame samples",
            "0 intervals",
            "No Present_Start intervals were observed during the bounded capture; no benchmark was claimed.",
        )),
        DiagnosticPayload::EtwFrameCapture(samples) => {
            let count = samples.len();
            let average = samples.iter().map(|sample| sample.frame_time_us).sum::<u64>() / count as u64;
            rows.push(row(
                "ETW Present_Start intervals",
                format!("{count} samples; avg {average} us"),
                "Bounded DxgKrnl observation, not presentation-completion or benchmark proof.",
            ));
            rows.extend(samples.into_iter().take(12).map(|sample| {
                row(
                    format!("PID {}", sample.process_id),
                    format!("{} us", sample.frame_time_us),
                    format!("{}; {} ms", sample.source, sample.present_start_unix_ms),
                )
            }));
        }
    }
    rows
}

fn row(
    item: impl Into<String>,
    value: impl Into<String>,
    state: impl Into<String>,
) -> DiagnosticRow {
    DiagnosticRow {
        item: item.into(),
        value: value.into(),
        state: state.into(),
    }
}

fn capability_state(state: CapabilityState) -> &'static str {
    match state {
        CapabilityState::Available => "Available",
        CapabilityState::Unsupported => "Unsupported",
        CapabilityState::PermissionRequired => "Permission required",
        CapabilityState::Unavailable => "Unavailable",
    }
}

fn bytes(value: Option<u64>) -> String {
    value.map_or_else(
        || "Unavailable".into(),
        |value| format!("{} MiB", value / 1_048_576),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use frametime_hardware::{DiagnosticError, DiagnosticPayload, DoctorReport};

    #[test]
    fn every_diagnostic_action_has_a_distinct_visible_label() {
        let actions = [
            DiagnosticAction::Doctor,
            DiagnosticAction::Cpu,
            DiagnosticAction::Gpu,
            DiagnosticAction::System,
            DiagnosticAction::Whea,
            DiagnosticAction::EtwFrames,
        ];
        let labels = actions.map(DiagnosticAction::label);
        assert!(labels.iter().all(|label| !label.is_empty()));
        assert_eq!(
            labels
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            actions.len()
        );

        let empty = DiagnosticPresentation::empty();
        assert!(!empty.belongs_to_benchmark());
        let benchmark = DiagnosticPresentation {
            action: Some(DiagnosticAction::EtwFrames),
            ..empty
        };
        assert!(benchmark.belongs_to_benchmark());
    }

    #[test]
    fn etw_action_is_bounded_and_kept_in_benchmark_surface() {
        let command = DiagnosticAction::EtwFrames.command();
        assert!(DiagnosticAction::EtwFrames.is_benchmark());
        assert!(
            matches!(command, DiagnosticCommand::EtwFrameCapture(ref request) if request.duration_ms == ETW_CAPTURE_DURATION_MS)
        );
        assert!(command.validate().is_ok());
    }

    #[test]
    fn unavailable_envelope_is_explicit_and_never_progress() {
        let command = DiagnosticAction::Gpu.command();
        let presentation = DiagnosticPresentation::from_envelope(
            DiagnosticAction::Gpu,
            DiagnosticEnvelope::failure(&command, DiagnosticError::unavailable("DXGI unavailable")),
        );
        assert_eq!(presentation.kind, DiagnosticPresentationKind::Warning);
        assert!(presentation.detail.contains("No workflow progress"));
        assert!(presentation.rows[0].value.contains("frametime.hardware/v1"));
    }

    #[test]
    fn successful_typed_result_renders_schema_and_no_hardware_claim() {
        let command = DiagnosticAction::Doctor.command();
        let presentation = DiagnosticPresentation::from_envelope(
            DiagnosticAction::Doctor,
            DiagnosticEnvelope::success(
                &command,
                DiagnosticPayload::Doctor(DoctorReport {
                    platform: "test".into(),
                    capabilities: vec![],
                }),
            ),
        );
        assert_eq!(presentation.kind, DiagnosticPresentationKind::Complete);
        assert_eq!(presentation.rows[0].value, "frametime.hardware/v1");
        assert!(presentation.detail.contains("not workflow progress"));
        assert!(presentation.belongs_to_assess());
    }
}
