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
- `assets/cfgs/meta_october_2026.cfg`, a complete October 2026 CVar meta
  configuration, and `assets/videosettings.txt`, the matching video-menu
  reference with pinned `setting.*` keys; both ship in the package for
  manual use.
- A repository check that every command in the shipped CS2 cfg assets is a
  player-settable name in a pinned CS2 convar snapshot
  (`repo-checks/data/cs2-console-names.txt`, build of 14 October 2026).

### Changed

- CS2 cfg assets updated to the October 2026 build. They no longer set any
  crosshair CVar (`cl_crosshair*`, `cl_grenadecrosshair*`, observer
  crosshair): crosshair conversion is owned by
  [Small Indie Crosshair Company](https://github.com/sebastianspicker/small-indie-crosshair-company).
  Competitive defaults to Natural EQ, voice 0.70, preset
  viewmodel position 1, and CMAA2 video guidance; the starter autoexec centers
  the radar and uses HUD scale 1.0. Comments now separate Valve defaults,
  measured facts, and guide opinion, and no longer present `snd_mixahead` or
  `snd_autodetect_latency` as latency tuning.
- Both full CS2 cfgs also mute the per-mode music sets Valve added on
  22 September 2026 (`_casual`, `_deathmatch`, `_armsrace`, `_rush`), show the
  loss/late/jitter breakdown only in poor network conditions
  (`cl_hud_telemetry_net_detailed 1`), and return
  `mm_dedicated_search_maxping` to Valve's default 150.
- Removed from the full cfgs: `snd_mixahead` and `snd_autodetect_latency`
  (defaults with no documented effect) and the unverifiable "Color mode"
  guidance, which the current video menu does not offer.

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
