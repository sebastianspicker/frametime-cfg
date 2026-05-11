# Safe Dry-Run Audit

Date: 2026-05-11

This companion document records a safe audit pass over the CS2 Windows
optimization scripts. The scripts were developed from a macOS-oriented workflow
but target Windows, so this audit separates checks that are safe to run on the
current checkout from checks that still need an elevated disposable Windows
environment.

## Repository Orientation

The repository is a Windows PowerShell optimization suite with a three-phase
workflow and several standalone operator tools.

- `START.bat` is the terminal launcher. It elevates, then dispatches the main
  scripts with `-ExecutionPolicy Bypass`.
- `START-GUI.bat` launches `CS2-Optimize-GUI.ps1`, the WPF dashboard.
- `Run-Optimize.ps1` is Phase 1. It dot-sources `Setup-Profile.ps1`,
  `Optimize-SystemBase.ps1`, `Optimize-Hardware.ps1`,
  `Optimize-RegistryTweaks.ps1`, and `Optimize-GameConfig.ps1`.
- `SafeMode-DriverClean.ps1` is Phase 2. It is intended to run in Safe Mode,
  clears the Safe Mode boot flag first, then performs driver cleanup.
- `PostReboot-Setup.ps1` is Phase 3. It runs after normal boot and performs
  driver install, device settings, DNS/profile work, and final benchmarking.
- `Boot-SafeMode.ps1`, `Cleanup.ps1`, `FpsCap-Calculator.ps1`, and
  `Verify-Settings.ps1` are standalone entrypoints.
- `helpers.ps1` loads helper modules from `helpers/`; the most important dry-run
  and write boundaries are in `helpers/system-utils.ps1`,
  `helpers/tier-system.ps1`, `helpers/backup-restore.ps1`, and
  `helpers/step-state.ps1`.

Runtime state is configured in `config.env.ps1` and defaults to
`C:\CS2_OPTIMIZE`. That directory can contain `state.json`, `progress.json`,
`backup.json`, `backup.lock`, logs, benchmark history, and handoff payloads.

## Environment Snapshot

Commands were run from `C:\Users\sebastian\Desktop\git\cs2-opt`.

| Surface | Observed value |
| --- | --- |
| Git status before doc edit | `## main...origin/main` |
| Host OS from `pwsh` | `Microsoft Windows 10.0.28000` |
| `pwsh` | PowerShell 7.6.1, Core, `Win32NT` |
| Windows PowerShell | PowerShell 5.1.28000.1830, Desktop |
| Pester module discovered before fixes | 3.4.0 at `C:\Program Files\WindowsPowerShell\Modules\Pester\3.4.0\Pester.psd1` |
| Pester module after bootstrap | 5.7.1 at `C:\Users\sebastian\Documents\PowerShell\Modules\Pester\5.7.1\Pester.psm1` |
| Elevation | Current shell is not elevated; live entrypoints now enforce elevation with explicit Administrator guards |

## Checks Run

The audit intentionally avoided live optimizer execution and did not write real
registry, BCD, service, task, driver, or network settings.

| Check | Command | Result |
| --- | --- | --- |
| Git state | `git status --short --branch` | Clean before this document was added. |
| File inventory | `rg --files` | Confirmed PowerShell entrypoints, helpers, docs, CFGs, and Pester tests. |
| Parser check | PowerShell parser over every `*.ps1` file | Passed: all `.ps1` files parse cleanly under `pwsh`. |
| Pester version probe | `Get-Module -ListAvailable Pester` | Only Pester 3.4.0 was available locally. |
| Pester dry-run suite attempt before fixes | `Invoke-Pester tests/integration/dryrun-compliance.Tests.ps1 -CI` | Blocked: local Pester 3.4.0 does not support `-CI`. |
| Pester configuration attempt before fixes | `New-PesterConfiguration` with `Invoke-Pester -Configuration` | Blocked: local Pester 3.4.0 does not provide Pester 5 configuration APIs. |
| Entrypoint smoke attempt before fixes | `pwsh -File <script> -SmokeTest` for shipped entrypoints | Blocked for every public entrypoint by `#Requires -RunAsAdministrator` in a non-elevated shell. |
| Entrypoint smoke after fixes | `pwsh -File <script> -SmokeTest` for shipped entrypoints | Passed for all eight public entrypoints from a non-elevated shell. |
| Admin guard after fixes | `pwsh -File .\Run-Optimize.ps1` from non-elevated shell | Failed fast with the explicit Administrator guard before live execution. |
| Focused Pester gates after fixes | `.\scripts\Invoke-LocalTests.ps1 -Path .\tests\integration\dryrun-compliance.Tests.ps1 .\tests\e2e\entrypoints.Tests.ps1` | Passed: 26 tests, 0 failed. |
| Full Pester suite after remediation | `.\scripts\Invoke-LocalTests.ps1` | Passed: 717 tests, 0 failed, 1 skipped. |
| Repeated full Pester confidence sweep | `.\scripts\Invoke-LocalTests.ps1` | Passed again: 717 tests, 0 failed, 1 skipped. |
| Windows PowerShell 5.1 smoke matrix | `powershell -File <script> -SmokeTest` for shipped entrypoints | Passed for all eight public entrypoints with no error records. |
| PSScriptAnalyzer | CI-equivalent `Invoke-ScriptAnalyzer` over `*.ps1` except `_TestInit.ps1` | Passed: no findings. |
| Windows PowerShell Pester bootstrap before wrapper fix | `powershell -File .\scripts\Invoke-LocalTests.ps1 -Path .\tests\e2e\entrypoints.Tests.ps1` | Initially blocked by PowerShellGet publisher mismatch between built-in Pester 3.4 and gallery Pester 5. |
| Windows PowerShell E2E harness before compatibility fix | `powershell -File .\scripts\Invoke-LocalTests.ps1 -Path .\tests\e2e\entrypoints.Tests.ps1` | After bootstrap, initially failed because `.NET Framework` `ProcessStartInfo` lacks the modern `ArgumentList` property. |
| Windows PowerShell E2E after compatibility fixes | `powershell -File .\scripts\Invoke-LocalTests.ps1 -Path .\tests\e2e\entrypoints.Tests.ps1` | Passed: 2 tests, 0 failed. |
| Windows PowerShell targeted contracts after compatibility fixes | `powershell -File .\scripts\Invoke-LocalTests.ps1 -Path .\tests\e2e\entrypoints.Tests.ps1 .\tests\workflow-contracts.Tests.ps1` | Passed: 11 tests, 0 failed. |
| Secret scan | Local equivalent of `.github/workflows/security.yml` secret patterns | Passed: clean. |
| PowerShell safety scan | Local equivalent of dangerous pattern checks | Passed: clean. |
| Launcher safety scan | Local equivalent of launcher safety checks | Passed: clean. |
| Workflow integrity scan | Local equivalent of workflow integrity checks | Passed: clean. |
| Retired reference scan | CI retired-symbol cross-reference | Passed: retired surface is clean. |
| Diff hygiene | `git diff --check` | Passed. |

The entrypoint smoke attempt covered:

- `Run-Optimize.ps1`
- `Cleanup.ps1`
- `Boot-SafeMode.ps1`
- `SafeMode-DriverClean.ps1`
- `PostReboot-Setup.ps1`
- `FpsCap-Calculator.ps1`
- `Verify-Settings.ps1`
- `CS2-Optimize-GUI.ps1`

Before the fix, each failed before reaching its `-SmokeTest` body with the
expected PowerShell permission error for `#Requires -RunAsAdministrator`. After
the fix, each reaches its smoke-test body from a non-elevated shell; normal
execution remains protected by an explicit Administrator guard.

## Findings and Fix Proposals

| Severity | Finding | Evidence | Impact | Proposed fix |
| --- | --- | --- | --- | --- |
| Medium | Local test commands require Pester 5, but this machine only has Pester 3.4.0. | `Invoke-Pester ... -CI` and `New-PesterConfiguration` both failed; module probe found only 3.4.0. | Maintainers following README commands on a fresh or older Windows install may see command-shape failures before any tests run. | Fixed by adding `scripts/Invoke-LocalTests.ps1`, which installs/imports Pester 5.x in CurrentUser scope before running tests. |
| Medium | Non-elevated `-SmokeTest` cannot reach smoke-test code because `#Requires -RunAsAdministrator` is evaluated first. | Every shipped entrypoint with `-SmokeTest` failed with the admin `#Requires` permission error. | CI can pass when elevated or when run in a compatible context, but local smoke tests are not usable from a normal shell. | Fixed by replacing top-level `#Requires -RunAsAdministrator` with an explicit Administrator guard after the `-SmokeTest` early return. Live execution remains admin-gated. |
| Low | README and docs mention `Invoke-Pester tests -CI` as the local verification command without stating the Pester 5 prerequisite. | README and `docs/architecture.md` listed `-CI`; local Pester 3.4.0 rejects it. | Documentation was technically correct for CI's intended module version but brittle for default Windows PowerShell installations. | Fixed by documenting `scripts/Invoke-LocalTests.ps1` as the local verification entrypoint. |
| Low | Parser validation passes under `pwsh`, but target compatibility still depends on Windows PowerShell 5.1. | Parser check passed with `pwsh`; repo README states the full optimization path targets Windows PowerShell 5.1. | A clean `pwsh` parser pass is useful but not sufficient for target-runtime confidence. | Keep parser checks under `pwsh`, but run smoke and workflow tests under both `pwsh` and Windows PowerShell 5.1 where possible. |
| Info | Dry-run write interception has focused tests already. | `tests/integration/dryrun-compliance.Tests.ps1` checks registry, BCD, backup, and progress behavior under `$SCRIPT:DryRun = $true`. | The safety surface is testable, but this audit could not execute it until Pester 5 is available. | Treat this suite as the primary automated dry-run safety gate after Pester 5 bootstrap. |
| Low | Non-elevated sandbox tests could lock themselves out when ACL hardening attempted an Administrators/SYSTEM-only ACL. | Full Pester initially failed on `Set-SecureAcl` owner/DACL operations in temp test directories. | Local non-admin validation was brittle even though live scripts require elevation. | Fixed by making `Set-SecureAcl` skip ACL mutation in non-elevated sessions; elevated live runs still apply the restrictive ACL. |
| Low | `Test-TrustedLocalPath` used an incompatible `Split-Path -LiteralPath ... -Parent` call. | Full Pester exposed parameter binding failures in hardware and GUI path-trust tests. | Path-trust checks could fail under PowerShell versions where that parameter set is invalid. | Fixed by using `Split-Path -Path ... -Parent` on the already-resolved filesystem path. |
| Low | Windows PowerShell 5.1 Pester bootstrap needed publisher supersedence handling. | PowerShellGet rejected installing gallery Pester 5 over built-in Microsoft-signed Pester 3.4 without `-SkipPublisherCheck`. | The local wrapper worked under `pwsh` but could fail for Windows PowerShell users, which is the target runtime. | Fixed by passing `-SkipPublisherCheck` to the wrapper's `Install-Module Pester` call. |
| Low | E2E smoke harness used a `ProcessStartInfo.ArgumentList` API that is unavailable in Windows PowerShell 5.1. | Windows PowerShell E2E run failed with `PropertyNotFoundException: The property 'ArgumentList' cannot be found`. | The test harness could not validate target-runtime smoke behavior even though CI's `pwsh` E2E path passed. | Fixed by building the compatible `ProcessStartInfo.Arguments` string explicitly. |

## Safe Dry-Run Procedure

Use this order for future audit passes.

1. Start with local non-mutating checks:
   - `git status --short --branch`
   - PowerShell parser pass over all `*.ps1` files
   - Pester version probe
   - entrypoint `-SmokeTest` attempts
2. Install or import Pester 5 only if it is not already available:
   - `pwsh -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-LocalTests.ps1`
3. Run focused Pester gates:
   - `pwsh -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-LocalTests.ps1 -Path .\tests\integration\dryrun-compliance.Tests.ps1`
   - `pwsh -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-LocalTests.ps1 -Path .\tests\e2e\entrypoints.Tests.ps1`
4. Run elevated smoke tests on Windows:
   - launch an elevated shell;
   - execute each public entrypoint with `-SmokeTest`;
   - confirm exit code `0`, no error records, and `SMOKE TEST OK` in output.
5. Use Windows Sandbox first for interactive DRY-RUN:
   - copy this repo into the sandbox;
   - open an elevated PowerShell session inside the sandbox;
   - run `Run-Optimize.ps1`;
   - select `D`, then preview at least `SAFE` and `RECOMMENDED` scopes;
   - verify DRY-RUN output includes `Would set` style messages for registry and
     BCD actions;
   - inspect `C:\CS2_OPTIMIZE` for expected logs/temp runtime files;
   - discard the sandbox after the audit.

Do not continue into real Safe Mode reboot testing unless the environment is a
throwaway VM or snapshot and the audit explicitly includes reboot handoff
validation.

## Manual Windows Sandbox Checks Still Pending

This pass did not perform an elevated interactive run in Windows Sandbox or a
disposable VM. The following items remain pending:

- interactive `Run-Optimize.ps1` DRY-RUN previews for `SAFE` and `RECOMMENDED`;
- inspection of generated `C:\CS2_OPTIMIZE` runtime files after sandbox dry-run;
- confirmation that no registry, BCD, service, scheduled-task, driver, or DNS
  change is applied during DRY-RUN preview.

## Acceptance Criteria for Closing the Audit

The audit can be considered complete when:

- parser checks still pass for all `.ps1` files;
- Pester 5 full suite passes in the local safe harness;
- non-elevated smoke tests reach each `-SmokeTest` body and report `SMOKE TEST OK`;
- sandbox DRY-RUN previews show intended "would change" output without live
  writes;
- any runtime artifacts under `C:\CS2_OPTIMIZE` are explained in this document
  or in a follow-up audit note.
