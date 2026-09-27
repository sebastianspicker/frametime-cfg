# Development

> **Archived and unsupported.** These commands are retained only to inspect or
> reproduce the preserved source and lockfiles. They are not active CI,
> Dependabot, packaging, or contributor requirements. Report vulnerabilities
> through the repository
> [security policy](../../../.github/SECURITY.md).

Northclock declares Rust 1.92 or newer and pins Rust 1.96.0 with `rustfmt`,
Clippy, and the Windows MSVC target for repository development.

## Historical user-mode workspace commands

Run from `tools/northclock`:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -W clippy::too_many_lines -W clippy::cognitive_complexity
cargo test --workspace --all-targets --all-features --locked
cargo check --workspace --all-targets --all-features --target x86_64-pc-windows-msvc --locked
cargo run -p xtask --locked -- hygiene
cargo run -p xtask --locked -- docs
cargo doc --workspace --no-deps --locked
cargo deny check all
cargo audit --file Cargo.lock --deny warnings
```

CI additionally runs `typos .` with `typos-cli` 1.49.1. `cargo-deny`,
`cargo-audit`, and `typos` are external prerequisites; the repository does not
provide an installer for them.

For a Windows release build of the user-mode workspace:

```powershell
cargo build --release --workspace --all-features --locked
```

This creates binaries only. The repository has no Northclock packaging, signing,
installation, or publication command.

## Historical driver protocol workspace commands

The excluded `driver/` directory is a separate Cargo workspace. Run from
`tools/northclock`:

```sh
cargo fmt --manifest-path driver/Cargo.toml --all -- --check
cargo clippy --manifest-path driver/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo test --manifest-path driver/Cargo.toml --workspace --all-targets --all-features --locked
cargo check --manifest-path driver/Cargo.toml --workspace --all-targets --all-features --locked
cargo audit --file driver/Cargo.lock --deny warnings
```

These commands validate protocol and driver-facing Rust code. They do not build a
loadable KMDF binary, create a driver package, sign or install it, implement a
watchdog, or exercise physical hardware.

## Historical test and evidence notes

Tests use direct inputs and mocks for public protocol, safety, and persistence
contracts. Production code must report an unavailable backend rather than
substituting test values. Cross-compilation does not establish Windows runtime
permissions, provider availability, ABI behavior, or hardware results.

Windows Task Scheduler, WMI, Tool Help, Service Control Manager, SetupAPI,
Configuration Manager, DXGI, D3D12, ETW, Event Log, and NVAPI behavior remains
hardware-unverified until recorded on a representative Windows 11 x64 system.

## Public-tree hygiene

`cargo run -p xtask --locked -- hygiene` rejects binaries, dumps, generated build
directories, legacy product code, private work records, and Rust source files
over 600 lines. It deliberately skips the canonical root `target/` directory.
`cargo run -p xtask --locked -- docs` checks repository-relative Markdown links.
