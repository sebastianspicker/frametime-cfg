# Full DRY-RUN Guide

Full DRY-RUN is the supported, non-elevated preview of the three-phase
frametime.cfg lifecycle. It executes the Phase 1 preview path, simulates the
transition to Safe Mode for Phase 2, then simulates the return to Normal Mode
for Phase 3. It does not reboot Windows.

## Quick start

Launch from a normal, non-elevated Windows terminal:

```powershell
cmd.exe /d /c START.bat dry-run
```

That command is non-interactive and defaults to GPU branch `2` (other NVIDIA).
Select another branch with a second argument:

```powershell
cmd.exe /d /c START.bat dry-run 1
cmd.exe /d /c START.bat dry-run 2
cmd.exe /d /c START.bat dry-run 3
cmd.exe /d /c START.bat dry-run 4
```

Run every mutually exclusive GPU branch as four isolated previews:

```powershell
cmd.exe /d /c START.bat dry-run all
```

The same single-branch contract can be launched directly:

```powershell
powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass `
    -File .\Run-Optimize.ps1 -FullDryRun -DryRunGpu 2
```

The direct `START.bat dry-run ...` route is the only enabled portable launcher
path and is the documented zero-prompt form. Other routes fail closed without
requesting administrator elevation.

## GPU branch selector

`-DryRunGpu` is a synthetic branch selector, not hardware detection. It lets a
maintainer or user inspect code that cannot all apply to the same machine.

| Value | Preview branch |
|---|---|
| `1` | NVIDIA RTX 5000, including its scaling guidance |
| `2` | Other NVIDIA; default |
| `3` | AMD Radeon cleanup, driver, and Adrenalin guidance |
| `4` | Intel Arc cleanup, driver, and settings guidance |

`-DryRunGpu` is valid only with `-FullDryRun`. One PowerShell invocation covers
all tiers and phases for one selected branch. Use `START.bat dry-run all` when
the goal is source-level coverage of all four mutually exclusive branches.

## Persistence contract

Full DRY-RUN performs read-only discovery and renders operation plans to the
console. It does not:

- request administrator rights or create the `C:\FRAMETIME_CFG` work tree;
- create or update state, progress, logs, backups, benchmark history, or
  runtime payloads;
- write registry values, BCD options, Run or RunOnce entries, services, tasks, power
  plans, MSI/NIC settings, DNS configuration, or CS2 files;
- download, install, remove, or launch a driver installer;
- remove Driver Store packages or AppX packages;
- write to the clipboard; or
- restart Windows or arm a later reboot handoff.

The simulated boot switches are private orchestration inputs. The preview does
not spoof `SAFEBOOT_OPTION`. Launching Full DRY-RUN while Windows reports a real
Safe Mode environment fails closed before Phase 2 begins.

Existing suite files are read-only inputs. A strict preview does not repair,
rotate, quarantine, or rewrite corrupt state. No rollback artifact is created
because nothing is applied.

## What is covered

Full DRY-RUN forces CUSTOM/all-tier scope, verbose plan rendering, FPS `0`, no
step confirmations, and one explicit GPU branch. It previews:

- all 38 Phase 1 steps, including benchmark, driver, payload, RunOnce, BCD, and
  restart plans;
- Phase 2 Safe Mode driver cleanup and the Phase 3 handoff plan;
- all 13 Phase 3 steps, including driver install, AppX cleanup, MSI/NIC, DRS or
  vendor guidance, VBS, DNS, process priority, video guidance, and benchmark
  capture; and
- both reboot boundaries as in-memory lifecycle transitions.

The interactive Phase 1 `[D]` choice also supports SAFE, RECOMMENDED,
COMPETITIVE, and CUSTOM scoped previews. Those scopes retain profile inclusion
and risk filtering, but included actions are rendered without live
confirmations. They still simulate all three phases. Use Full DRY-RUN when a
repeatable, zero-prompt validation contract is required.

## Reading the result

A clean single-branch preview has all of these properties:

- process exit code `0`;
- empty standard error;
- `PHASE 1 PREVIEW COMPLETE`, `PHASE 2 PREVIEW COMPLETE`,
  `PHASE 3 PREVIEW COMPLETE`, and `ALL 3 PHASES PREVIEW COMPLETE` markers;
- no `preview issue (DRY-RUN)` or `FATAL ERROR` marker; and
- an unchanged `C:\FRAMETIME_CFG` tree.

Preview action failures are accumulated. The final summary reports
`COMPLETE WITH ... ISSUE(S)`, and Full DRY-RUN exits nonzero so automation
cannot mistake a partial preview for success.

The console output is intentionally not persisted by the suite. Redirecting it
to a user-chosen file is an explicit caller action:

```powershell
powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass `
    -File .\Run-Optimize.ps1 -FullDryRun -DryRunGpu 2 *> .\dry-run.txt
```

That redirection creates `dry-run.txt`; the suite's no-persistence contract does
not include an output path explicitly requested by the caller.

## Boundaries

DRY-RUN validates control flow, operation planning, branch coverage, and write
guards. It cannot prove that a later elevated action will succeed against the
current ACLs, firmware, Driver Store, vendor installer, hardware, network, or
reboot environment. It is not a replacement for a restore plan, benchmark, or
live release-candidate test on a disposable Windows system.

The GUI Preview checkbox is separate: it is a persisted setup preference and
therefore writes `C:\FRAMETIME_CFG\state.json`. Until the GUI launches the strict
CLI contract directly, use `START.bat dry-run` or `-FullDryRun` when zero
persistence is required.

`Boot-SafeMode.ps1 -DryRun` is narrower still. It previews only the Safe Mode
shortcut transaction; it does not run the full three-phase lifecycle.

## Maintainer verification

After changing any stateful feature, run the direct native contract suite plus
the strict process preview:

```powershell
cargo test --manifest-path rust/Cargo.toml --workspace --all-targets --all-features --locked

START.bat dry-run all
```

Every new mutating path needs a direct DRY-RUN guard and useful `Would ...`
output. Preview-only structured results must not be treated as evidence that a
live action was applied.
