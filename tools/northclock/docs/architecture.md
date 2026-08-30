# Architecture

`northclock-core` contains domain types, backend traits, command workflows,
safety rules, bounded memory and ROM algorithms, and persistence. It forbids
unsafe Rust and has no platform FFI.

`northclock-platform-windows` is the only user-mode crate allowed to contain
unsafe code. Its public API returns owned, validated Rust values. The current
implementation uses CPUID and Windows processor topology for CPU identity,
`GetSystemTimes` and `GlobalMemoryStatusEx` for measurements, and DXGI for GPU
inventory. It also uses the Event Log API for bounded WHEA snapshots, DxgKrnl
ETW present events for frame intervals, D3D12 for device-local VRAM
copy/readback validation, and the installed NVAPI runtime for read-only NVIDIA
load and temperature. AMD ADLX telemetry and all control surfaces remain
explicit unverified or unavailable capabilities until safe Rust ABI bindings
and live behavior are validated.

`northclock-cli` and `northclock-gui` translate user input into
`ApplicationCommand` values. `ApplicationService` performs capability checks,
safety validation, preview, apply, readback, and rollback. Neither interface has
its own hardware logic.

`northclock-vram-worker` is a separate process boundary. It returns the same
versioned command envelope as the main application. A successful run requires a
hardware DXGI adapter, completed D3D12 fence, and byte-for-byte readback.
The parent enforces a total wall-clock deadline, terminates an overrun worker,
and bounds and validates its output before accepting the report.

The application service owns the persistence workflow. On Windows, both
frontends configure versioned settings, profiles, history, and measurements
under `%LOCALAPPDATA%\Northclock`. Measurement files are appended only after a
backend returns nonempty measurements with an explicit source.

The backend interfaces cover CPU and GPU telemetry and tuning, workloads,
process control, power plans, frame capture, overlays, event observation, and
ROM reads. A system-status backend aggregates independent Task Scheduler, VBS,
and potential-conflict observations; one unavailable subsystem does not erase
the others. Tests inject implementations explicitly. Test measurements label
their source as test-only and are not compiled into the platform adapter.

## Operation lifecycle

```text
request -> bounds -> preview and capture -> re-capture -> authorization -> apply
        -> readback -> validate -> receipt
                         |
                         +-> rollback captured state on mismatch
```

`OperationPlan`, `ApplyReceipt`, and `RollbackReceipt` record the requested
changes, typed target, captured state, backend, readback, and validation result.
Apply rejects a changed backend or captured state and validates that the receipt
preserves the preview contract. A write build feature only exposes the
authorization path; it does not make a backend available or verified.

Process-affinity changes use their own preview, captured mask, apply receipt,
readback, stale-state rejection, and rollback contracts. They pass through the
same compile-time feature, elevation, runtime flags, and acknowledgement gate as
hardware changes.

## Driver isolation

`driver/` is a separate Cargo workspace. The `no_std`
`northclock-driver-protocol` crate exposes only versioned, bounded Curve
Optimizer requests. The companion crate currently contains protocol validation,
not a loadable driver. See [Driver protocol](driver-protocol.md).
