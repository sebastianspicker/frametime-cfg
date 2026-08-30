# Driver Foundry

Driver Foundry is a native Rust tool for planning or performing Windows GPU and related-driver cleanup, then preparing a minimal vendor driver installation. The sole command is `dfoundry`.

## Quick start

```powershell
cargo build --release
cargo run -- --help
cargo run -- clean --vendor nvidia
cargo run -- install --work $env:TEMP\dfoundry-demo
cargo test --workspace
```

Release binary: `target\release\dfoundry.exe`

## Safety

- `clean` plans changes by default. Live cleanup requires `--execute` and administrator elevation.
- `install` filters and dry-runs by default. Launching a vendor installer requires the explicit force-install path.
- Safe Mode BCD changes and live OEM-driver deletion are separately guarded by `DFOUNDRY_*` environment variables; see [Security](SECURITY.md).
- Do not run live cleanup or force-install on a machine you need without a backup and a recovery path.

## Requirements

- Windows 10 or 11 for real driver work
- [Rust](https://rustup.rs/)

## Project layout

```text
crates/  workspace crates
data/    versioned catalogs and optional local helpers
```

## Attribution and non-affiliation

Vendor catalog text may originate from Display Driver Uninstaller community settings. Driver Foundry is not affiliated with NVIDIA, AMD, Intel, Wagnardsoft, or TechPowerUp, and is not a drop-in clone of proprietary tools or branding.

## License

MIT. See [LICENSE](LICENSE).
