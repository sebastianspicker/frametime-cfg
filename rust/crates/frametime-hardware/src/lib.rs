#![forbid(unsafe_code)]

//! Versioned, platform-neutral contracts for native hardware diagnostics.
//!
//! This crate is deliberately read-only. Platform adapters produce typed
//! envelopes; callers can serialize them directly without parsing console text.

mod contracts;

pub use contracts::{
    CapabilityState, CpuIdentity, DIAGNOSTIC_SCHEMA_VERSION, DiagnosticCapability,
    DiagnosticCommand, DiagnosticEnvelope, DiagnosticError, DiagnosticErrorCode, DiagnosticPayload,
    DiagnosticStatus, DoctorReport, EtwFrameCaptureRequest, FrameSample, GpuAdapter, GpuInventory,
    SystemStatus, WheaEvent, WheaEventsRequest,
};
