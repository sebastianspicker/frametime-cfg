# Driver Foundry

> **Archived and unsupported.** Driver Foundry is preserved in place for source,
> history, Cargo manifests and lockfiles, and its [security policy](SECURITY.md).
> It is excluded from active CI, Dependabot, issue routing, packaging, and
> contributor workflows. Everything below is historical reference, not a support
> commitment.

Driver Foundry is a historical Rust workspace for catalog-driven Windows
driver-cleanup planning and for preparing reduced vendor driver trees. The
`dfoundry` CLI also opens the egui interface.

The current implementation is safe for planning and package preparation only.
Live cleanup, live registry application, vendor-installer launch, embedded-helper
execution, and BCD mutation are blocked until authenticated catalog, helper, and
platform-signer capabilities exist. Passing `--execute`, `--force-install`, or an
environment variable does not bypass those checks.

## Historical requirements

- Rust 1.96.0, inherited from the repository toolchain.
- Windows 10 or 11 for Windows-specific observation and eventual live behavior.

Host tests and Windows compilation do not qualify live driver or registry work.

## Historical local verification

These commands are retained for archive inspection only. They are not an active
contributor workflow or a supported distribution path.

Run from `tools/driver-foundry`:

```powershell
cargo build --release --workspace --all-features --locked
cargo run --locked -- --help
cargo run --locked -- clean --vendor nvidia
cargo run --locked -- install --work $env:TEMP\dfoundry-demo
cargo test --workspace --all-targets --all-features --locked
```

The release CLI is `target\release\dfoundry.exe`. Cleanup supports catalog inputs
for NVIDIA, AMD, Intel, Lisuan, and Realtek. The bundled package catalog and
download index prepare NVIDIA-oriented component sets; the download index
contains no independent SHA-256 authorization pins.

## Historical commands and behavior

- `clean` validates vendor settings and writes a staged plan/journal. Dry-run is
  the supported public path; `--execute` fails before elevation or a live adapter
  is reached.
- `install` accepts local roots/archives, HTTPS sources, catalog-index sources,
  or a synthetic fixture, creates a new work directory, filters components,
  applies text transformations, and writes reports. ZIP processing is in-process
  and bounded.
- `--force-install`, live registry application, 7z/SFX helpers, signing-helper
  execution, and embedded-helper materialization fail closed.
- Safe Mode and power actions are journal entries only. No BCD change, reboot,
  restart, or shutdown is executed.

Remote acquisition uses HTTPS, refuses redirects and URL credentials, bounds a
download at 2 GiB, and verifies a caller-supplied SHA-256 value when present. A
digest supplied by the same caller is integrity evidence, not authorization to
launch an installer.

## Data and output

The data-root precedence is `DFOUNDRY_DATA_DIR`, `data/` beside the executable,
upward discovery, then development fallbacks. Treat `data/settings` and
`data/catalog` as versioned input. The default install workspace is a new
timestamped directory below the process temporary directory; explicit output
paths must not already exist.

The five crates separate common/elevation support, cleanup planning, install
preparation, GUI, and CLI concerns. See [CONTRIBUTING.md](CONTRIBUTING.md) for
historical validation context and [SECURITY.md](SECURITY.md) for the retained
security-reporting route.

## Attribution and license

Vendor catalog text may originate from Display Driver Uninstaller community
settings. Driver Foundry is not affiliated with NVIDIA, AMD, Intel, Wagnardsoft,
or TechPowerUp and is not a drop-in clone of proprietary tools or branding.

Driver Foundry is licensed under the [MIT License](LICENSE).
