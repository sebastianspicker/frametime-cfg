#![no_std]
#![forbid(unsafe_code)]

//! Protocol validation shared with a future KMDF entry point.
//!
//! This crate is deliberately not a loadable driver. It provides the bounded
//! request gate that a separately reviewed `windows-drivers-rs` KMDF adapter
//! must call. CI compilation is not driver packaging or hardware validation.

use northclock_driver_protocol::{CurveOptimizerRequest, ProtocolStatus};

/// Decodes and validates one complete IOCTL input buffer.
///
/// A future KMDF queue must call this byte-oriented entry point before any
/// semantic dispatch. It intentionally rejects truncated and extended buffers.
#[must_use]
pub fn validate_curve_optimizer_request_bytes(bytes: &[u8]) -> ProtocolStatus {
    let Ok(request) = CurveOptimizerRequest::decode(bytes) else {
        return ProtocolStatus::InvalidBuffer;
    };
    validate_curve_optimizer_request(&request)
}

#[must_use]
pub fn validate_curve_optimizer_request(request: &CurveOptimizerRequest) -> ProtocolStatus {
    if !request
        .header
        .valid_for_size(northclock_driver_protocol::CURVE_OPTIMIZER_REQUEST_WIRE_SIZE)
    {
        return ProtocolStatus::InvalidHeader;
    }
    if !request.has_supported_command() {
        return ProtocolStatus::UnsupportedCommand;
    }
    if !request.processor.has_registered_protocol_table() {
        return ProtocolStatus::UnsupportedProcessor;
    }
    if !request.values_are_bounded() {
        return ProtocolStatus::ValueOutOfBounds;
    }
    ProtocolStatus::Success
}

#[cfg(test)]
mod tests {
    use super::*;
    use northclock_driver_protocol::{Command, ProcessorModel, ProtocolHeader};

    #[test]
    fn rejects_unsupported_processor_before_dispatch() {
        let request = CurveOptimizerRequest {
            header: ProtocolHeader::new(
                northclock_driver_protocol::CURVE_OPTIMIZER_REQUEST_WIRE_SIZE,
                7,
            ),
            command: Command::ApplyCurveOptimizer.wire_value(),
            processor: ProcessorModel {
                vendor: *b"GenuineIntel",
                family: 6,
                model: 0,
                stepping: 0,
                protocol_table_version: 1,
            },
            logical_core_index: 0,
            offset_steps: -10,
            watchdog_timeout_ms: 5_000,
        };
        assert_eq!(
            validate_curve_optimizer_request(&request),
            ProtocolStatus::UnsupportedProcessor
        );
    }

    fn valid_request() -> CurveOptimizerRequest {
        CurveOptimizerRequest {
            header: ProtocolHeader::new(
                northclock_driver_protocol::CURVE_OPTIMIZER_REQUEST_WIRE_SIZE,
                8,
            ),
            command: Command::ApplyCurveOptimizer.wire_value(),
            processor: northclock_driver_protocol::ProcessorModel {
                vendor: *b"AuthenticAMD",
                family: 0x19,
                model: 0x61,
                stepping: 2,
                protocol_table_version: 1,
            },
            logical_core_index: 0,
            offset_steps: -10,
            watchdog_timeout_ms: 5_000,
        }
    }

    #[test]
    fn byte_gate_reports_each_pre_dispatch_failure_category() {
        assert_eq!(
            validate_curve_optimizer_request_bytes(&[0_u8; 41]),
            ProtocolStatus::InvalidBuffer
        );

        let mut request = valid_request();
        request.header.magic = 0;
        assert_eq!(
            validate_curve_optimizer_request_bytes(&request.encode()),
            ProtocolStatus::InvalidHeader
        );

        request = valid_request();
        request.command = u16::MAX;
        assert_eq!(
            validate_curve_optimizer_request_bytes(&request.encode()),
            ProtocolStatus::UnsupportedCommand
        );

        request = valid_request();
        request.logical_core_index = u16::MAX;
        assert_eq!(
            validate_curve_optimizer_request_bytes(&request.encode()),
            ProtocolStatus::ValueOutOfBounds
        );

        assert_eq!(
            validate_curve_optimizer_request_bytes(&valid_request().encode()),
            ProtocolStatus::Success
        );
    }

    #[test]
    fn header_fields_are_independently_rejected() {
        for mutate in [
            |request: &mut CurveOptimizerRequest| request.header.magic = 0,
            |request: &mut CurveOptimizerRequest| request.header.version = 0,
            |request: &mut CurveOptimizerRequest| request.header.structure_size = 0,
            |request: &mut CurveOptimizerRequest| request.header.sequence = 0,
        ] {
            let mut request = valid_request();
            mutate(&mut request);
            assert_eq!(
                validate_curve_optimizer_request_bytes(&request.encode()),
                ProtocolStatus::InvalidHeader
            );
        }
    }

    #[test]
    fn processor_whitelist_fields_are_independently_rejected() {
        let mut requests = [valid_request(); 4];
        requests[0].processor.vendor = *b"GenuineIntel";
        requests[1].processor.family = 0x1a;
        requests[2].processor.model = 0x60;
        requests[3].processor.protocol_table_version = 2;
        for request in requests {
            assert_eq!(
                validate_curve_optimizer_request_bytes(&request.encode()),
                ProtocolStatus::UnsupportedProcessor
            );
        }
    }

    #[test]
    fn every_bounded_field_accepts_edges_and_rejects_neighbors() {
        for (core, offset, watchdog, expected) in [
            (0, -50, 1_000, ProtocolStatus::Success),
            (511, 50, 60_000, ProtocolStatus::Success),
            (512, 0, 5_000, ProtocolStatus::ValueOutOfBounds),
            (0, -51, 5_000, ProtocolStatus::ValueOutOfBounds),
            (0, 51, 5_000, ProtocolStatus::ValueOutOfBounds),
            (0, 0, 999, ProtocolStatus::ValueOutOfBounds),
            (0, 0, 60_001, ProtocolStatus::ValueOutOfBounds),
        ] {
            let mut request = valid_request();
            request.logical_core_index = core;
            request.offset_steps = offset;
            request.watchdog_timeout_ms = watchdog;
            assert_eq!(
                validate_curve_optimizer_request_bytes(&request.encode()),
                expected
            );
        }
    }
}
