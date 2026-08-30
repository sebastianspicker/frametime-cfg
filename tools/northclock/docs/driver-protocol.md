# Driver protocol

The experimental driver workspace is isolated from all normal application
builds. Microsoft describes Rust Windows driver support as early-stage, so the
workspace is not part of releases.

`northclock-driver-protocol` is `no_std` and uses a versioned, padding-free,
little-endian wire layout. Requests and responses are decoded from exact-length
byte slices before validation; the driver must not cast an IOCTL buffer to a
Rust or C structure. The Curve Optimizer request is 42 bytes and its response is
28 bytes. Commands are limited to capability query, Curve Optimizer state
capture, bounded apply, captured-state restore, and watchdog query. There is no
generic request that accepts an address, register, bus operation, or arbitrary
command number.

The protocol reports distinct invalid-buffer, invalid-header,
unsupported-command, unsupported-processor, and value-bounds failures. It
validates the exact length, magic value, version, structure size, nonzero
sequence, command allowlist, processor-family and model whitelist,
protocol-table version, core index, Curve Optimizer bounds, and watchdog
interval before dispatch. The only registered table is the hardware-unverified AMD family
`0x19`, model `0x61`, table version 1 tuple. Registration prevents generic
dispatch and is not evidence that a physical write is safe. Responses carry
explicit permission, rate-limit, readback, and restore failures.

`northclock-kmdf-driver` currently implements the request validation core only.
It is not a loadable KMDF driver. A future adapter based on Microsoft's
`windows-drivers-rs` still needs reviewed IOCTL definitions, queue and
cancellation handling, model-specific command tables, a watchdog, packaging,
test signing, installation tests, and physical validation.

A future IOCTL adapter must retrieve an exact input buffer length, copy or map
it only for the duration guaranteed by KMDF, decode it with
`validate_curve_optimizer_request_bytes`, enforce the device ACL and caller
authorization independently, reject replayed sequences, rate-limit every
accepted request, arm restoration before applying a value, validate hardware
readback, and return only an encoded response. It must never dispatch from a raw
pointer, enum discriminant, C-layout cast, unchecked length, or caller-supplied
address. These are adapter requirements, not claims about the current
validation-only crate.
