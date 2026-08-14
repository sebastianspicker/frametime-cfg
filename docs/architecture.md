# Architecture and Maintainer Orientation

This document describes the runtime structure and maintainer boundaries.
User-facing behavior is documented in [README.md](../README.md), with the
preview contract in [dry-run.md](dry-run.md). Optimization rationale lives in
the topic-specific documents under `docs/`.

## Purpose

The repository contains an administrator-run Windows PowerShell suite for CS2
system and game configuration. The core design constraints are:

- prefer changes with documented evidence and tradeoffs;
- capture rollback data for supported setting types before live mutation and
  document operations that use separate or incomplete recovery;
- keep the three reboot phases resumable;
- avoid external optimization tools, except for downloading the NVIDIA driver
  installer from NVIDIA;
- keep dry-run mode close to the live path while preventing persistent writes.

## Entrypoints

`START.bat` is the portable strict-preview launcher. Live routes fail closed
because the repository has no installed or independently signed launcher that
authenticates source before elevation. `START.bat dry-run [1|2|3|4]` launches
one strict GPU preview. `START.bat dry-run all` runs four isolated previews and
stops on the first nonzero result.

Portable menu, restore, and manual Phase 3 actions are unavailable.
`Launcher-Action.ps1` is a fail-closed legacy menu-action surface. It rejects
every action before importing configuration, helpers, or portable content.

`START-GUI.bat` is fail-closed. The WPF source remains for development and
smoke validation but is not a live portable entrypoint.

`Run-Optimize.ps1` is Phase 1. It loads configuration and helpers, then
dot-sources the phase scripts in order:

- `Setup-Profile.ps1`;
- `Optimize-SystemBase.ps1`;
- `Optimize-Hardware.ps1`;
- `Optimize-RegistryTweaks.ps1`;
- `Optimize-GameConfig.ps1`.

Those phase scripts execute as they are dot-sourced. They are not passive module
imports, so ordering is workflow ordering.

`SafeMode-DriverClean.ps1` is Phase 2. It runs from the validated published
runtime payload in Safe Mode via `RunOnce`, removes the Safe Mode boot flag
first, then performs native GPU driver removal, then registers Phase 3.

`PostReboot-Setup.ps1` is Phase 3. It runs from the validated published runtime
payload in normal boot after Phase 2, automates NVIDIA driver installation when
a validated package is available, presents manual AMD or Intel driver guidance,
applies applicable driver and device settings, writes final CS2 configuration,
and guides final benchmarking. Its handoff remains registered until the final
benchmark is captured and saved.

`Boot-SafeMode.ps1` is a shortcut for re-entering Phase 2 after Phase 1 has
already prepared the runtime payload and marked `state.json` as Safe Mode-ready.
Its `-DryRun` switch previews the shortcut transaction without elevation or
state/boot changes.

`Cleanup.ps1`, `FpsCap-Calculator.ps1`, and `Verify-Settings.ps1` are standalone
operator tools that load saved state when available and otherwise use safe
defaults.

## Unified DRY-RUN Lifecycle

`Run-Optimize.ps1 -FullDryRun` is the public zero-prompt preview contract. The
exact Windows PowerShell form is:

```powershell
powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass `
    -File .\Run-Optimize.ps1 -FullDryRun -DryRunGpu 2
```

Phase 1 creates configuration only in memory. The orchestrator then dot-sources
the Phase 2 and Phase 3 implementations and calls their private preview seams
with injected state plus explicit `-SimulateSafeMode` and
`-SimulateNormalBoot` switches.

The simulation does not spoof `SAFEBOOT_OPTION`, persist state or progress,
publish a runtime payload, register RunOnce, change BCD, initialize logs or
backups, download or install a driver, remove packages, or restart Windows. Dry-run
results remain `Status = DryRun` and `Applied = false`; orchestration accepts
them only as permission to continue the preview, not as proof of application.
Launching the unified preview from real Safe Mode fails closed. Action
exceptions increment a lifecycle preview-issue counter; the final process exits
nonzero rather than printing an unconditional success result.

Phase-owned interactive features have deterministic preview renderers. That
includes benchmark capture/history, the CS2 video guide, NVIDIA package install
and DRS profile, MSI/NIC planning, VBS, DNS selection, and both reboot handoffs.
Mutually exclusive GPU paths are selected with `-DryRunGpu 1..4`. Automated E2E
uses branch 2 for the full no-mutation process contract; Windows release
validation runs the launcher `all` matrix so the four branches remain isolated
rather than pretending they can all apply to one machine.

The lifecycle invariants are:

- preview selection happens before elevation and work-directory initialization;
- injected Phase 2/3 state is accepted only with explicit simulated boot
  switches while `Mode = DRY-RUN`;
- intermediate phase summaries do not claim application or final success;
- no preview result completes persistent step state; and
- only the final overall summary offers the live-run next action.

## Runtime State

The state and log directory is `C:\FRAMETIME_CFG`. Each published Phase 2/3
payload is an immutable `C:\FRAMETIME_CFG\runtime-generations\<id>` directory;
`runtime-current.json` atomically selects the newest verified generation for
manual launch. Before a reboot, the suite stages a fixed file set, writes a
SHA-256 manifest, verifies the exact file set and hashes, renames it to a new
generation, and commits the pointer before `RunOnce` is registered. Existing
generations are retained so publishing a newer payload does not remove an
already-armed handoff's target.

The Phase 2 and Phase 3 entrypoints validate the published payload manifest
before administrator validation or helper loading. A missing, extra, or
hash-mismatched file stops the handoff rather than running it.

Important files:

- `state.json`: profile, mode, GPU choice, FPS inputs, downloaded driver path,
  and handoff fields such as `phase1SafeModeReady`;
- `progress.json`: resume state for steps;
- `backup.json`: captured original values for rollback;
- `backup.lock`: advisory lock preventing two optimization windows from writing
  backup state at the same time;
- `Logs/frametime_current.log`: current run log.

`progress.json` uses keys shaped like `P{phase}:{step}`. Do not replace this
with bare step numbers. Phase 1 and Phase 3 both have step 5, step 10, and so on;
bare numbers would collide during resume.

`backup.json` is append-like from the operator's perspective. Registry, service,
scheduled-task, BCD, power-plan, NIC, and DNS identities are deduplicated within
a step so a re-run retains the first captured rollback value. See
[backup-restore.md](backup-restore.md) for all eleven entry types and their
recovery boundaries.

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

GUI-only helpers are loaded after the WPF window and shared GUI primitives are
available:

- `helpers/gui-panels.ps1`: shared panel state plus Overview, Assess, Setup,
  Recovery, and Benchmark controllers;
- `helpers/gui-network.ps1`: Network panel mapping and event handlers, loaded
  only by `gui-panels.ps1`;
- `helpers/gui-video.ps1`: Video settings mapping and event handlers, loaded
  only by `gui-panels.ps1`;
- `helpers/step-catalog.ps1`: data-only step catalog for Setup and verify;
- `helpers/system-analysis.ps1`: non-destructive checks for Assess.

## Phase Handoff

Phase 1 Step 38 prepares the reboot handoff:

1. stage the fixed Phase 2/3 file set in a unique directory and write its
   SHA-256 manifest;
2. verify the exact set and hashes, rename it to a new immutable generation,
   and atomically commit `runtime-current.json` without removing older targets;
3. register the validated Phase 2 runtime entrypoint in `HKLM\...\RunOnce`;
4. set and verify `bcdedit safeboot minimal`;
5. mark `state.json` with `phase1SafeModeReady`;
6. prompt for restart.

The machine-level Phase 2 value is stored in `HKLM\...\RunOnce`. It uses `*` to
request execution in Safe Mode and `!` to defer deletion until the command has
run. Windows processes the machine-level key only after an administrator
account signs in. These semantics are represented by source contracts, but the
combined handoff still requires a live interrupted-run test on the release
candidate.

Phase 2 removes the Safe Mode boot flag before driver removal. This is the key
crash-safety invariant: if driver cleanup fails later, the next boot should be
normal mode rather than a Safe Mode loop.

After driver cleanup, Phase 2 registers Phase 3 in the current account's
`HKCU\...\Run` key. This is a durable per-user handoff, not a machine-level
RunOnce value. The same administrator account must sign in after the normal
reboot. Signing in with another account does not start Phase 3.

Phase 3 refuses to run in Safe Mode. If it detects Safe Mode, it attempts to
clear the flag, re-registers its validated runtime entrypoint in the same
per-user Run key, and asks for a normal reboot. The handoff is retained when the
final benchmark has not produced a saved result and is removed after Phase 3
completes.

## Backup and Restore Rules

Supported mutations capture rollback data before modification:

- registry writes go through `Set-RegistryValue`;
- boot configuration writes go through `Set-BootConfig`;
- service/task/device-specific code calls the matching `Backup-*` helper before
  changing state.

Backup functions own their own dry-run guard. Callers can request backup capture
unconditionally when they have enough context.

`Flush-BackupBuffer` is the normal step-boundary persistence point.
`Complete-Step` flushes before writing progress; a flush failure prevents the
progress record from being saved. Power-plan capture is flushed and verified
before the plan changes.

Restore code treats `backup.json` as untrusted input. Keep restore allowlists and
identity checks no broader than the live write path. AppX removal, GPU driver
package removal, CS2 file edits, and several partial restore cases are not fully
represented in `backup.json`; maintain those limits in
[backup-restore.md](backup-restore.md).

## Adding or Changing a Step

For a new optimization step, update the same surfaces together:

1. phase script with the `Invoke-TieredStep` call or explicit step block;
2. `helpers/step-catalog.ps1` so Setup and verify mirrors the workflow;
3. the README capability or usage section and any relevant topic document;
4. focused tests under `tests/helpers/` or `tests/integration/`;
5. workflow contract tests if the change affects entrypoints or CI behavior.

Any state-changing addition must also render a useful Full DRY-RUN plan, add a
focused no-mutation regression, and propagate preview exceptions to the shared
issue counter. Do not treat a `DryRun` result as successful application.

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
  smoke and strict Full DRY-RUN paths are intentionally allowed from
  non-elevated shells; portable live execution fails before loading mutable
  configuration or helper code.

For a Windows dry-run release gate, also run `START.bat dry-run all` and require
exit code zero, empty stderr for each PowerShell child, all four lifecycle
completion markers per branch, no preview-issue/fatal marker, and an unchanged
`C:\FRAMETIME_CFG` tree.

CI mirrors those surfaces in `.github/workflows/lint.yml` and adds security
checks in `.github/workflows/security.yml`.

## Desktop frontend

The WPF shell is split between `frametime-gui.ps1`,
`ui/frametime-gui.xaml`, and the three `helpers/gui-*.ps1` controllers. The
entrypoint owns runtime lifecycle and asynchronous operation cleanup, the XAML
owns layout and shared presentation resources, and the controllers map domain
data and interactions. See [frontend.md](frontend.md) for component,
accessibility, responsive, content, and Windows validation conventions.

On macOS, Windows-specific behavior cannot be fully reproduced. Treat local
Pester, parse, analyzer, and smoke results as useful gates, but keep Windows
PowerShell 5.1 CI as the final compatibility authority.
