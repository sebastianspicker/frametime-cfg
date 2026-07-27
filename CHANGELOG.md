# Changelog

This file records user-visible repository changes. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and version identifiers
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Proposed release identifier: `v3.0.0-alpha.1`.

This candidate has not been tagged or published. Alpha status means that
interfaces, configuration, and runtime state may change in later releases.

### Added

- Strict `-FullDryRun` lifecycle preview covering all three phases and four
  mutually exclusive GPU branches without persistent initialization.
- External WPF layout in `ui/frametime-gui.xaml` with assessment, setup,
  verification, benchmark, network, video, and recovery task areas.
- Immutable Phase 2 and Phase 3 runtime generations with a fixed payload list,
  SHA-256 manifest validation, and a current-generation pointer.
- Structured operation results for security-sensitive writes and phase
  transitions.
- Backup entries for registry, service, power plan, boot configuration,
  scheduled task, NIC property, QoS/URO, pagefile, DNS, and NVIDIA DRS state,
  plus restore compatibility for older Defender exclusion entries.
- Network latency history, storage-health reporting, GUI contract tests, and
  process-level entrypoint tests.

### Changed

- Project identity and live working directory changed to `frametime.cfg` and
  `C:\FRAMETIME_CFG`.
- The terminal workflow remains the owner of normal-mode, Safe Mode, and
  post-reboot operations. The WPF interface launches and observes those
  workflows instead of hiding reboot boundaries.
- GPU package removal, NVIDIA setup, boot handoff, ACL, power-plan, service,
  task, network, and recovery paths use narrower validation and explicit result
  handling.
- Driver cleanup and several former external-tool operations now use native
  Windows APIs, PowerShell, or documented manual steps.
- Third-party implementation references and unresolved source-provenance checks
  are recorded in `THIRD_PARTY_NOTICES.md`.
- Development validation is routed through repository scripts used by CI.
- The WPF panel controller is separated into shared, network, and video
  controller files without changing its external entry point.

### Fixed

- Per-executable AppCompatFlags and DirectX GPU-preference registry writes now
  permit only a validated local `cs2.exe` path at the two intended keys.
- The same narrow validation permits supported rollback of those path-named
  values while rejecting UNC, traversal, sibling-key, and other executable
  names.
- Phase 1 no longer reports the affected registry steps as complete when their
  required write fails.
- NVIDIA driver preparation no longer reports completion after a failed
  download, failed signature check, failed state persistence, or manual
  deferral.
- NVIDIA driver lookup resolves the current result-page response through the
  NVIDIA details service before applying the existing host allowlist.
- The Safe Mode RunOnce value requests both Safe Mode execution and deferred
  deletion, and the WPF controller initializes asynchronous operation state
  before StrictMode event-handler reads.
- Required service-change failures prevent Step 37 from being recorded as
  complete.
- Registry, boot-configuration, selected service, scheduled-task, power-plan,
  and NVIDIA DRS mutations now require a durable restore record before the
  corresponding change begins.
- Restore All now replays individual records in strict reverse capture order,
  including overlapping targets written by more than one step. Failed or
  rejected records remain in `backup.json` for retry.
- Persisted registry recovery accepts exact path and value-name pairs plus
  narrowly shaped CS2, adapter, display-class, and device-interrupt targets. It
  no longer accepts arbitrary values under broad Windows registry subtrees.
- Scheduled-task rollback verifies that a suite-created task is absent before
  deleting its recovery record, and recovery messages are limited to recorded
  supported settings.
- Step 14 startup-entry cleanup now restores only the configured value names,
  blocks deletion when capture or persistence fails, and verifies removal.
- AMD cleanup no longer removes AMD-wide application and registry roots or the
  Ryzen Master driver service. Display-package and exact cache cleanup remain.
- Phase handoff and progress code distinguish applied, skipped, partial, and
  failed outcomes in the paths covered by focused tests.
- Phase scripts no longer print internal Boolean step results in terminal or
  strict dry-run output.
- A verified NVIDIA driver installation now reports failed optional registry or
  service changes as a partial post-install result.

### Removed

- Unused Process Lasso configuration link and stale external-tool positioning.
- Fixed-version NVIDIA rollback guidance based on a stale driver threshold;
  Step 5 now reports the installed version without classifying it.
- Active Defender exclusions whose backup ordering and success reporting were
  unsuitable for the alpha. Existing recorded exclusion entries remain
  removable through Recovery.
- Ten partially decoded or unknown NVIDIA DRS entries whose stated provenance
  depended on a leaked internal settings database.
- Unused `$CFG_TimerResolution_Desired` configuration value, which had no
  runtime caller.

### Migration

- State, backups, and armed phase handoffs from the older
  `C:\CS2_OPTIMIZE` layout are not migrated. The current launcher refuses to
  start while a v2.3 Phase 2 or Phase 3 handoff is still armed.
- Complete or roll back an armed v2.3 workflow with its matching checkout. Do
  not copy old state or backup files into `C:\FRAMETIME_CFG`.

## [2.2] - 2026-04-18

The `v2.2` Git tag is the latest historical tag present in this checkout.
Detailed source history for that version remains available through Git. The
current changelog does not reconstruct claims that cannot be verified from the
tagged repository state.
