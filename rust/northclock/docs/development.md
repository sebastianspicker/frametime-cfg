# Development

Northclock requires Rust 1.92 or newer and pins Rust 1.96 for development. From
the project root:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test --workspace --all-features
cargo check --workspace --target x86_64-pc-windows-msvc
cargo run -p xtask -- hygiene
cargo run -p xtask -- docs
cargo deny check
cargo audit
```

The driver protocol has its own gates:

```powershell
cargo fmt --manifest-path driver/Cargo.toml --all -- --check
cargo clippy --manifest-path driver/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path driver/Cargo.toml --workspace
cargo check --manifest-path driver/Cargo.toml --workspace --all-features
cargo doc --manifest-path driver/Cargo.toml --workspace --no-deps
```

These commands compile and test protocol code. They do not build a KMDF binary,
create a driver package, sign it, install it, or run it on hardware.

## Test rules

The compact suite targets public protocol and persistence contracts with direct
inputs. Production code must fail closed when a backend is unavailable rather
than substituting test-only values.

The Windows system-status adapter is cross-compiled. Its Task Scheduler COM,
`Win32_DeviceGuard` WMI, Tool Help, Service Control Manager, SetupAPI, and
Configuration Manager calls have not been run on a Windows 11 host in the
current acceptance lane. Cross-compilation does not establish runtime
permissions, provider availability, or hardware behavior.

## Public tree

`cargo xtask hygiene` rejects binaries, dumps, generated build directories,
legacy product code, private records, internal work records, and Rust source
files over 600 lines. It skips the canonical Cargo output directory `target/`
for local checks to keep the working tree small.
