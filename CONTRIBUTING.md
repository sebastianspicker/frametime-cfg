# Contributing

Contributions must preserve the repository's preview, persistence, phase
handoff, and recovery boundaries. Keep each change small enough for a Windows
or PowerShell maintainer to review without unrelated refactoring.

## Before starting

Use a current source checkout and review:

- [`README.md`](README.md) for supported hosts, commands, and limitations;
- [`docs/architecture.md`](docs/architecture.md) for phase and helper ownership;
- [`docs/dry-run.md`](docs/dry-run.md) for the no-persistence preview contract;
- [`docs/backup-restore.md`](docs/backup-restore.md) for recovery coverage; and
- [`docs/evidence.md`](docs/evidence.md) before adding or changing a tuning
  recommendation.

Do not infer a general performance effect from a registry value, a community
guide, or one machine. New system changes need a reproducible defect,
authoritative platform documentation, or measurements that can be reviewed.

## Development requirements

Use PowerShell 7 for the main local validation commands. Keep Windows
PowerShell 5.1 installed for live-entry-point compatibility checks. CI uses:

- Pester 5.7.1; and
- PSScriptAnalyzer 1.24.0.

`scripts/Invoke-LocalTests.ps1` accepts any installed Pester release from 5.0.0
through 5.99.99. Without `-SkipInstall`, it installs a compatible release in
the current-user scope. The analyzer runner installs its pinned version when it
is missing. Both installations require PowerShell Gallery access.

The repository has no build, formatter, type checker, package manager,
documentation generator, or release automation.

## Implementation rules

For a state-changing operation:

1. route execution through the existing tier, risk, validation, and structured
   result conventions;
2. render an explicit Full DRY-RUN action without persistent side effects;
3. capture supported original state through
   `helpers/backup-restore.ps1` before mutation;
4. validate persisted paths, value names, identifiers, and commands as
   untrusted input at restore and handoff boundaries;
5. stop the step when a required capture, write, or postcondition fails;
6. do not call `Complete-Step` after an unsuccessful required operation; and
7. document operations that require separate or manual recovery.

Preserve Windows PowerShell 5.1 compatibility in live entry points and helpers.
Avoid new binary dependencies. If a binary is necessary, document its source,
license, integrity verification, execution boundary, and removal procedure.

When step behavior or metadata changes, update the phase implementation,
`helpers/step-catalog.ps1`, focused tests, and the relevant operational
documentation in the same pull request.

## Testing

Run a focused test while developing:

```powershell
pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\scripts\Invoke-LocalTests.ps1 `
    -Path .\tests\helpers\system-utils.Tests.ps1 `
    -SkipInstall
```

Run the full Pester suite before submitting:

```powershell
pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\scripts\Invoke-LocalTests.ps1 -SkipInstall
```

Run the repository validation scripts from the repository root:

```powershell
pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\.github\scripts\verify-syntax.ps1

powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\.github\scripts\verify-syntax.ps1

pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\.github\scripts\run-psscriptanalyzer.ps1

pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\.github\scripts\verify-launcher-contracts.ps1

pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\.github\scripts\smoke-entrypoints.ps1 -Engine pwsh

pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\.github\scripts\smoke-entrypoints.ps1 -Engine powershell

pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\frametime-gui.ps1 -SmokeTest
```

Changes to live execution, registry, boot configuration, phase handoffs,
services, tasks, files, drivers, AppX, NVIDIA DRS, power, network, clipboard, or
reboot behavior also require the four-branch preview:

```powershell
cmd.exe /d /c START.bat dry-run all
```

Each branch must exit with code `0`, print completion markers for Phase 1,
Phase 2, Phase 3, and the full lifecycle, contain no `preview issue (DRY-RUN)`
or `FATAL ERROR` marker, and leave `C:\FRAMETIME_CFG` unchanged.

Add tests for success, failure, refusal, invalid persisted input, and dry-run
behavior where those paths exist. Do not weaken an assertion or suppress an
error to make a gate pass.

Desktop-interface changes also require the GUI design contract, XAML parsing,
the GUI smoke marker, and the manual Windows checks in
[`docs/frontend.md`](docs/frontend.md).

## Documentation

Documentation must describe the checked-in implementation and current
validation status. Keep commands, paths, profile behavior, configuration names,
recovery limits, and screenshots synchronized with the source. Remove obsolete
instructions instead of retaining them as history.

Do not add performance estimates without committed reproducible artifacts. Do
not publish raw logs, state, machine diagnostics, or screenshots that have not
completed the manual interface checks in [`docs/frontend.md`](docs/frontend.md).

## Repository hygiene

Do not commit runtime state, logs, test reports, personal paths, credentials,
private system information, local recordings, draft screenshots, or local
analysis workspaces.

Before submitting, inspect tracked and ignored state:

```powershell
git status --short
git ls-files -ci --exclude-standard
```

The second command must produce no paths. Tracked source must not depend on an
ignore rule to remain visible.

## Pull requests

Use one pull request for one behavior or documentation objective. Include:

- the problem and scope;
- runtime, privilege, persistence, and recovery effects;
- tests added or changed;
- exact validation commands and results; and
- platform checks or live behavior that could not be reproduced.

Use sanitized excerpts when output is needed for review. Do not attach complete
logs, state files, backups, or system reports.

Report vulnerabilities through the process in
[`.github/SECURITY.md`](.github/SECURITY.md), not through a public issue with
technical details.
