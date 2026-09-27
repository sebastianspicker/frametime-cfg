# Changelog

All notable changes to the root five-crate workspace are recorded here. The
project is alpha: no release has been qualified on Windows hardware. See
[compatibility and qualification](docs/native/compatibility.md).

## Unreleased

### Added

- Native Rust workspace with `frametime.exe` (CLI) and `frametime-gui.exe`
  (Win32 desktop) entrypoints over a shared `frametime-app` layer.
- Authenticated package authority with manifest, catalog, publisher-pin, and
  retained-handle verification that fails closed for unsigned input.
- A compiled five-setting CS2 profile baseline, benchmark evidence records, and
  guarded three-phase normal-boot / Safe Mode / normal-boot orchestration.
- Canonical TOML and asset inputs, protected `C:\FRAMETIME_CFG` persistence,
  and identity-bound recovery.

### Changed

- The product surface is now native Rust only; the legacy PowerShell runtime was
  retired.
- Northclock and Driver Foundry are archived reference workspaces, excluded from
  active CI, Dependabot, issue routing, and packaging.
- Repository policy checks (dependency direction, domain purity, frontend
  boundary, native package surface, file-size cap, clone detection) moved from
  two bash scripts, inline workflow steps, and domain tests into the
  unpublished `repo-checks` workspace member, so `cargo test --workspace` and
  `scripts\verify.cmd` enforce them on every platform.
- Narrowed library surfaces: `frametime-domain` items are reached only through
  their owning module, and `frametime-windows` exposes one explicit API that is
  identical on every target.

### Notes

- `frametime.exe` and `frametime-gui.exe` require an authenticated package for
  any state-changing operation. A source checkout can build, test, and run the
  strict `dry-run` preview only.
