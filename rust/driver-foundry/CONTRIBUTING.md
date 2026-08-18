# Contributing to Driver Foundry

Driver Foundry is one root-level Rust workspace. Keep product changes within `crates/`, versioned inputs under `data/`, and current documentation under `docs/`.

`archive/legacy/` preserves legacy .NET and reverse-engineering material. It is not part of the runtime or CI. Do not add product dependencies, build steps, or command paths that reach into it.

## Build checks

From the repository root:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release
```

The first three commands are the cross-platform quality gate. The release build and controlled live-adapter validation run on Windows. Use the release command as `dfoundry`. For locally repeatable checks that need data outside the executable layout, set `DFOUNDRY_DATA_DIR` to the root `data/` directory.

## Safety

- Cleanup must retain its dry-run default; live mutation needs an explicit execute flag and administrator elevation.
- Installation must not launch `setup.exe` unless the explicit force-install path is selected.
- Keep Safe Mode, live OEM-driver deletion, and UAC-relaunch overrides opt-in through their documented `DFOUNDRY_*` controls.
- Do not add auto-reboot behavior or unsupported WHQL claims.
- Do not commit proprietary driver-tool binaries, private keys, certificates, or closed helper dumps. See `.gitignore`.

## Style and documentation

- Keep cohesive Rust source files at or below 600 physical lines when splitting does not weaken the domain boundary.
- Prefer small, reviewable changes that preserve safety boundaries.
- Match existing Rust module style and add focused tests for behavior changes.
- Update [README.md](README.md) when user-visible behavior or a safety boundary changes.
- Preserve catalog attribution and the project's non-affiliation statement.
