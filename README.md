# frametime.cfg

`frametime.cfg` is a native Rust Windows configuration workflow for Counter-Strike 2 and selected system settings. It ships two signed PE entrypoints: `frametime.exe` for terminal operation and `frametime-gui.exe` for the desktop interface. Both are release-only capabilities: state-changing operation requires authentication of the installed package.

## Scope and qualification

The workflow can inspect, plan, persist, and recover selected CS2, driver, networking, power, and Windows settings. It is intended for x64 Windows systems operated by an administrator who can review the proposed changes and recover the machine.

The repository does not claim universal FPS, frame-time, latency, image-quality, or stability gains. Host tests prove Rust contracts and fail-closed behavior. They do not qualify Windows APIs, UAC, Safe Mode, drivers, NVAPI, hardware, filesystem races, or a complete reboot sequence. Those require disposable Windows VMs and representative hardware.

## Source build and strict preview

Use a current Rust toolchain matching [`rust-toolchain.toml`](rust-toolchain.toml).

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -W clippy::too_many_lines -W clippy::cognitive_complexity
cargo test --workspace --all-targets --all-features --locked
cargo run -p frametime-cli -- dry-run all
```

`dry-run` is a strict source preview. It does not confer package authority, elevate, modify Windows, or qualify a live operation. Cross-check the Windows target before release work:

```sh
cargo check --workspace --all-targets --all-features --target x86_64-pc-windows-msvc --locked
```

## Package authority

A distributable package contains the two PE entrypoints, `frametime.toml`, canonical assets, documentation, licenses, a manifest, and a signed catalog. The executable authenticates the package root, catalog membership, file identities, hashes, publisher pin, and its own retained executable identity before state-changing work. An arbitrary source checkout, copied executable, unsigned package, or missing publisher pin fails closed.

The protected work root is fixed at `C:\FRAMETIME_CFG`; `frametime.toml` must name that exact path. State, backups, logs, driver artifacts, runtime generations, and reboot handoffs are stored beneath it through the Windows trusted-directory boundary. Do not make this root configurable without redesigning that boundary.

See [package security](docs/package-security.md) for the verification model.

## Architecture

The workspace contains five crates:

```text
frametime-cli ─┐
               ├─> frametime-app ─┬─> frametime-domain
frametime-gui ─┘                  └─> frametime-windows ─> frametime-domain
```

- `frametime-domain` contains pure policy, configuration validation, catalog, workflow, and persistence models.
- `frametime-windows` implements Windows-only adapters, trusted storage, package authentication, runtime publication, and native integrations.
- `frametime-app` coordinates domain workflows with the Windows adapter.
- `frametime-cli` builds `frametime.exe`; it also performs native process hardening and package authentication at startup.
- `frametime-gui` builds `frametime-gui.exe`; it retains only native window/process bootstrap calls outside the application layer.

`assets/` is the canonical embedded CFG and video-data source. `frametime.toml` is the canonical configuration input. `tools/northclock` and `tools/driver-foundry` are independent Rust workspaces; they are not dependencies of the five-crate application workspace.

## Reboot workflow

The workflow has three explicit stages:

1. Phase 1 runs in normal boot, records state, and may publish an authenticated runtime before arming the Safe Mode handoff.
2. Phase 2 runs in Safe Mode, verifies the selected runtime and handoff, performs its guarded work, clears Safe Boot, and arms the same-user normal-boot handoff.
3. Phase 3 runs after normal boot, verifies the retained runtime and user binding, completes guarded work, and clears the handoff only after coherent completion.

Each phase treats absent, inconsistent, or unverified evidence as a failure rather than inferring readiness. Details and recovery limits are in [native operations](docs/native/operations.md) and [recovery](docs/native/recovery.md).

## Repository map

- [Native architecture](docs/architecture.md)
- [Native operations](docs/native/operations.md)
- [Windows integrations](docs/native/integrations.md)
- [Recovery](docs/native/recovery.md)
- [GUI](docs/native/gui.md)
- [NVIDIA DRS](docs/native/nvidia-drs-settings.md)
- [Compatibility and qualification](docs/native/compatibility.md)
- [Research and evidence boundaries](docs/research/)
- [Contributing](CONTRIBUTING.md)
- [Security policy](.github/SECURITY.md)
