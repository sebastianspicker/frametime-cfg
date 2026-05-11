# Architecture and Maintainer Orientation

This document is for a competent programmer or IT operator who needs to understand
how the suite is wired before changing it. User-facing behavior remains documented
in [README.md](../README.md); optimization rationale lives in the topic-specific
deep dives under `docs/`.

## Purpose

The repository ships an administrator-run Windows PowerShell suite for CS2
performance tuning. The core design constraints are:

- prefer evidence-backed changes over folklore;
- make every risky write reversible through `backup.json`;
- keep the three reboot phases resumable;
- avoid external optimization tools, except for downloading the NVIDIA driver
  installer from NVIDIA;
- keep dry-run mode close to the live path while preventing persistent writes.

## Entrypoints

`START.bat` is the terminal menu most users run. It launches the PowerShell
entrypoints with `-ExecutionPolicy Bypass` after elevating itself.

`START-GUI.bat` launches `CS2-Optimize-GUI.ps1`, the WPF dashboard. The GUI is
for analysis, backup review, benchmarking, network diagnostics, storage checks,
and settings. Full optimization still runs through the terminal phase scripts.

`Run-Optimize.ps1` is Phase 1. It loads configuration and helpers, then
dot-sources the phase scripts in order:

- `Setup-Profile.ps1`;
- `Optimize-SystemBase.ps1`;
- `Optimize-Hardware.ps1`;
- `Optimize-RegistryTweaks.ps1`;
- `Optimize-GameConfig.ps1`.

Those phase scripts execute as they are dot-sourced. They are not passive module
imports, so ordering is workflow ordering.

`SafeMode-DriverClean.ps1` is Phase 2. It runs in Safe Mode via `RunOnce`,
removes the Safe Mode boot flag first, then performs native GPU driver removal,
then registers Phase 3.

`PostReboot-Setup.ps1` is Phase 3. It runs in normal boot after Phase 2 and
installs the clean driver, applies driver/device settings, writes final CS2
configuration, and guides final benchmarking.

`Boot-SafeMode.ps1` is a shortcut for re-entering Phase 2 after Phase 1 has
already prepared the runtime payload and marked `state.json` as Safe Mode-ready.

`Cleanup.ps1`, `FpsCap-Calculator.ps1`, and `Verify-Settings.ps1` are standalone
operator tools that load saved state when available and otherwise use safe
defaults.

## Runtime State

The runtime directory is `C:\CS2_OPTIMIZE`. The suite copies the scripts needed
for Phase 2 and Phase 3 there before a reboot so `RunOnce` can execute a stable
payload even if the original checkout moves.

Important files:

- `state.json`: profile, mode, GPU choice, FPS inputs, downloaded driver path,
  and handoff fields such as `phase1SafeModeReady`;
- `progress.json`: resume state for steps;
- `backup.json`: captured original values for rollback;
- `backup.lock`: advisory lock preventing two optimization windows from writing
  backup state at the same time;
- `Logs/optimize_current.log`: current run log.

`progress.json` uses keys shaped like `P{phase}:{step}`. Do not replace this
with bare step numbers. Phase 1 and Phase 3 both have step 5, step 10, and so on;
bare numbers would collide during resume.

`backup.json` is intentionally append-like from the operator's perspective. The
first captured value for a step is the rollback target. Re-runs should not
replace the original value with a value already changed by this suite.

## Helper Boundaries

`helpers.ps1` dot-sources common helper modules into the caller's script scope.
The helpers share `$SCRIPT:` state such as `Profile`, `DryRun`,
`CurrentStepTitle`, and phase counters.

Core helper responsibilities:

- `helpers/logging.ps1`: console output, logging, banners, phase counters;
- `helpers/tier-system.ps1`: profile, tier, risk, dry-run, and step execution
  policy;
- `helpers/step-state.ps1`: `progress.json` read/write and resume prompts;
- `helpers/system-utils.ps1`: JSON writes, ACL hardening, registry/BCD wrappers,
  RunOnce registration, runtime payload copy, compatibility checks;
- `helpers/backup-restore.ps1`: backup capture, rollback, lock handling, restore
  validation;
- hardware/domain modules such as `nvidia-driver.ps1`, `nvidia-drs.ps1`,
  `msi-interrupts.ps1`, `power-plan.ps1`, and `process-priority.ps1`: narrow
  Windows or CS2 optimization surfaces.

GUI-only helpers are loaded by the GUI entrypoint:

- `helpers/gui-panels.ps1`: WPF panel builders and event handlers;
- `helpers/step-catalog.ps1`: data-only step catalog for the Optimize panel;
- `helpers/system-analysis.ps1`: non-destructive health checks for Analyze.

## Phase Handoff

Phase 1 Step 38 prepares the reboot handoff:

1. copy required scripts, helpers, CFGs, and reference docs to
   `C:\CS2_OPTIMIZE`;
2. register Phase 2 in `HKLM\...\RunOnce`;
3. set `bcdedit safeboot minimal`;
4. mark `state.json` with `phase1SafeModeReady`;
5. prompt for restart.

Phase 2 removes the Safe Mode boot flag before driver removal. This is the key
crash-safety invariant: if driver cleanup fails later, the next boot should be
normal mode rather than a Safe Mode loop.

Phase 3 refuses to run in Safe Mode. If it detects Safe Mode, it attempts to
clear the flag, re-registers itself, and asks for a normal reboot.

## Backup and Restore Rules

Every write wrapper should capture rollback data before modification:

- registry writes go through `Set-RegistryValue`;
- boot configuration writes go through `Set-BootConfig`;
- service/task/device-specific code calls the matching `Backup-*` helper before
  changing state.

Backup functions own their own dry-run guard. Callers can request backup capture
unconditionally when they have enough context.

`Flush-BackupBuffer` is the normal step-boundary persistence point. `Complete-Step`
flushes before writing progress so the suite never records a step as complete
without first trying to persist rollback information.

Restore code treats `backup.json` as untrusted input. Keep restore allowlists and
identity checks narrower than the live write path.

## Adding or Changing a Step

For a new optimization step, update the same surfaces together:

1. phase script with the `Invoke-TieredStep` call or explicit step block;
2. `helpers/step-catalog.ps1` so the GUI Optimize panel mirrors the workflow;
3. README Phase Breakdown and any relevant deep-dive doc;
4. focused tests under `tests/helpers/` or `tests/integration/`;
5. workflow contract tests if the change affects entrypoints or CI behavior.

For a changed domain assumption, prefer documenting the reason in the relevant
deep-dive doc and only add code comments where the invariant is local to the
implementation.

## Verification Map

The repository is PowerShell-first. The relevant local checks are:

- `pwsh -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-LocalTests.ps1 -Path .\tests\integration\dryrun-compliance.Tests.ps1 .\tests\e2e\entrypoints.Tests.ps1`
  for the normal-shell dry-run and entrypoint smoke gates with Pester 5.x
  bootstrapping;
- `pwsh -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-LocalTests.ps1`
  for the full test gate from an elevated shell or CI;
- `pwsh -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-LocalTests.ps1 -Path .\tests\e2e`
  for process-level E2E coverage of the public entrypoints;
- a parse check over `*.ps1` files after script edits;
- PSScriptAnalyzer for linting when available;
- entrypoint smoke checks using each shipped script's `-SmokeTest` switch. The
  smoke path is intentionally allowed from non-elevated shells; normal execution
  still performs an explicit Administrator guard.

CI mirrors those surfaces in `.github/workflows/lint.yml` and adds security
checks in `.github/workflows/security.yml`.

On macOS, Windows-specific behavior cannot be fully reproduced. Treat local
Pester, parse, analyzer, and smoke results as useful gates, but keep Windows
PowerShell 5.1 CI as the final compatibility authority.
