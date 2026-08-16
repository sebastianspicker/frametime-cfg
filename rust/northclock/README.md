# Northclock

Northclock is a Rust application for measured hardware diagnostics on Windows
11 x64. It targets modern AMD Ryzen processors and AMD Radeon or NVIDIA GeForce
graphics adapters. The CLI and egui interface call the same typed application
service.

The default build is read-only. It reports unavailable data instead of
inventing measurements or treating a missing backend as success. The current
CPU write artifact is an isolated protocol validation core, and no GPU write
backend is registered. Neither is release-ready.

## Commands

```text
northclock doctor [--json]
northclock cpu identity|measure|workload|curve-optimizer-preview
northclock gpu list|measure
northclock memory system-test|vram-test
northclock power list
northclock system status
northclock process affinity preview|apply|rollback
northclock events whea
northclock frames capture
northclock rom inspect <path>
northclock settings show|set
northclock profiles list|import-ini
northclock operation preview|apply|rollback
northclock-gui [--overlay]
```

JSON commands return `schema_version`, `command`, `capability`, `status`,
`data`, and `error`. Exit codes distinguish internal failure, invalid usage,
unavailable support, safety rejection, and failed hardware validation.

## Build

```powershell
cargo build --workspace
cargo test --workspace
cargo run -p northclock-cli -- --json doctor
cargo run -p northclock-gui
```

User settings and profiles use versioned TOML under
`%LOCALAPPDATA%\Northclock`. A legacy INI file can be imported once without
modifying the original. History is JSONL and measurements are CSV; neither is
written without real backend input data. Persistence is disabled when
Northclock is elevated, so an administrator token never follows a user-owned
`%LOCALAPPDATA%` path. Elevated measurements remain available without history;
persistent settings, profiles, and imports require an unelevated session.

System-memory results include a bounded native WHEA Event Log correlation
window. If Event Log access is unavailable, the workload result remains intact
and the correlation object contains the backend error.

The CPU workload reports requested and elapsed duration, thread count,
validated work units, arithmetic validation errors, and measured iterations per
second. It is a software stress and benchmark workload, not proof of thermal or
overclock stability.

`northclock system status` uses documented Windows APIs to inspect the
Northclock Task Scheduler folder, the `Win32_DeviceGuard` VBS runtime status,
and a bounded set of potential overlapping hardware-control components. Device
findings require Windows PnP Code 12; process and service matches remain only
potential overlap signals. Each subsystem reports its own source and failure
state; the command performs no system mutation.

The experimental driver workspace is excluded from normal builds. Its current
crates validate a narrow protocol but do not form a packaged, signed, installed,
or hardware-qualified KMDF driver.

## Documentation

- [Architecture](docs/architecture.md)
- [Hardware support](docs/hardware-support.md)
- [Safety](docs/safety.md)
- [Development](docs/development.md)
- [Driver protocol](docs/driver-protocol.md)

Northclock is licensed under the [MIT License](LICENSE).
