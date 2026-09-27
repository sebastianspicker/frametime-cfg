# Northclock

> **Archived and unsupported.** Northclock is preserved in place for source,
> history, Cargo manifests and lockfiles, and the repository
> [security-reporting route](../../.github/SECURITY.md). It is excluded from
> active CI, Dependabot, issue routing, packaging, and contributor workflows.
> Everything below is historical reference, not a support commitment.

Northclock is a historical Rust application for measured hardware diagnostics on
Windows 11 x64. The CLI and egui interface call the same typed application
service in the preserved source.

The default build is read-only: it reports unavailable data instead of inventing
measurements or treating a missing backend as success. The CPU write artifact is
an isolated protocol-validation core, and no GPU write backend is registered.
Neither is release-ready.

## Historical commands

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

JSON commands return `schema_version`, `command`, `capability`, `status`, `data`,
and `error`. Exit codes distinguish internal failure, invalid usage, unavailable
support, safety rejection, and failed hardware validation.

## Historical local verification

These commands are retained for archive inspection only. They are not an active
contributor workflow or a supported distribution path.

```powershell
cargo build --workspace --locked
cargo test --workspace --all-targets --all-features --locked
cargo run -p northclock-cli --locked -- --json doctor
cargo run -p northclock-gui --locked
```

`memory vram-test` fails closed: the application has no authenticated capability
for locating and launching the `northclock-vram-worker` image. The worker
contains a bounded D3D12 test when run directly, but the repository has no
supported package or parent-launch path for it.

User settings and profiles use versioned TOML under `%LOCALAPPDATA%\Northclock`.
A legacy INI file can be imported once without modifying the original. History is
JSONL and measurements are CSV; neither is written without real backend input
data. Persistence is disabled while Northclock is elevated, so an administrator
token never follows a user-owned `%LOCALAPPDATA%` path. Elevated measurements
still work without history; persistent settings, profiles, and imports require an
unelevated session.

System-memory results include a bounded native WHEA Event Log correlation window.
If Event Log access is unavailable, the workload result stays intact and the
correlation object carries the backend error.

The CPU workload reports requested and elapsed duration, thread count, validated
work units, arithmetic validation errors, and measured iterations per second. It
is a software stress and benchmark workload, not proof of thermal or overclock
stability.

`northclock system status` uses documented Windows APIs to inspect the Northclock
Task Scheduler folder, the `Win32_DeviceGuard` VBS runtime status, and a bounded
set of potential overlapping hardware-control components. Device findings
require Windows PnP Code 12; process and service matches are only potential
overlap signals. Each subsystem reports its own source and failure state, and the
command performs no system mutation.

The experimental driver workspace is excluded from normal builds. Its current
crates validate a narrow protocol but do not form a packaged, signed, installed,
or hardware-qualified KMDF driver.

The repository provides release build commands but no packaging, signing,
installation, or publication workflow for Northclock.

## Documentation

- [Architecture](docs/architecture.md)
- [Hardware support](docs/hardware-support.md)
- [Safety](docs/safety.md)
- [Development](docs/development.md)
- [Driver protocol](docs/driver-protocol.md)

Northclock is licensed under the [MIT License](LICENSE).
