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
  unpublished `repo-checks` workspace member. Correction: those checks are no
  longer part of the published workspace, so `cargo test --workspace` does not
  enforce them; they are contributor rules. `repo-checks` runs only the
  batch-label resolution check for `scripts/*.cmd` and the CS2 cfg CVar and
  video-key snapshot checks.
- Narrowed library surfaces: `frametime-domain` items are reached only through
  their owning module, and `frametime-windows` exposes one explicit API that is
  identical on every target.
- Suite timestamps are now recorded in UTC, matching the `*Utc` field names.
- `configure --dry-run` and the GUI "Dry run preference" checkbox were removed;
  they only stored a label. `dry-run` remains the read-only preview.
- `cleanup quick` / `cleanup full` skip Temp and Prefetch files that another
  program holds open and report how many were skipped, instead of failing.
- A `[VProf] FPS:` result line that cannot be parsed now rejects the capture
  instead of being dropped silently.
- `fps-cap` rejects an empty or reserved label and options that belong to the
  other strategy, and writes nothing until its input is valid.

### Fixed

Source-level fixes from the November 2026 audit; Windows hardware
qualification is still open.

- NVIDIA driver steps P2:2 and P3:1: audit records now read back intact, and
  the work lock is re-entrant and held for a whole run, so the steps no longer
  fail on their own lock and a second CLI or GUI process cannot interleave.
- P1:38 publishes the reboot runtime (handles request WRITE_DAC/WRITE_OWNER),
  saves the reboot transaction before arming RunOnce and Safe Boot, parses
  `bcdedit` Safe Boot output case-insensitively, and refuses when the NVIDIA
  approval expires within two hours. `optimize` is no longer refused forever
  after a runtime was published.
- P2:1 checks the signed-in user before clearing Safe Boot; P2:2 validates the
  approval before saving its capture.
- Hardware discovery skips non-PCI devices (HD Audio functions, WAN Miniports,
  VMBus), GPU detection uses SetupAPI instead of localized `pnputil` text, and
  P1:20 evidence no longer compares an observation timestamp.
- P1:16 accepts REG_SZ NIC keywords and its RSS-only inventory; P3:3 creates
  and later removes a missing interrupt Affinity Policy key.
- `restore`: pagefile size changes can be undone, an already-absent registry
  value no longer blocks restore, and the user's NVIDIA CS2 profile from
  before the baseline is kept and restored. The DRS merge no longer overwrites
  settings changed since the snapshot and keeps full application bindings.
- The CS2 GPU preference is written under the plain drive path, and the GUI
  applies the same reboot-state checks and Phase 1 confirmation as the CLI.

### Notes

- `frametime.exe` and `frametime-gui.exe` require an authenticated package for
  any state-changing operation. A source checkout can build, test, and run the
  strict `dry-run` preview only.
