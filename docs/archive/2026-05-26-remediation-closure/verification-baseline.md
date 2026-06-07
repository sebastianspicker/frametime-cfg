# Verification Baseline

Generated: 2026-05-26

Host: macOS with PowerShell 7.5.4 (`pwsh`). Windows PowerShell 5.1
(`powershell`) is not available on this host.

Overall status: PARTIAL. The local PowerShell 7 parser, PSScriptAnalyzer, unit
test, e2e smoke, entrypoint smoke, and security grep checks below were run. Full
release confidence is blocked by Windows-only compatibility/runtime checks that
cannot run on this macOS host.

The worktree was dirty before this baseline was written. This task changed only
`docs/verification-baseline.md`.

## Commands Discovered

Authoritative sources inspected:
- `README.md`
- `CONTRIBUTING.md`
- `.github/workflows/lint.yml`
- `.github/workflows/security.yml`
- `PSScriptAnalyzerSettings.psd1`

Dependency installation:
- No project package manifest or lockfile was found.
- CI installs missing PowerShell modules with:
  - `Install-Module Pester -Scope CurrentUser -Force -SkipPublisherCheck`
  - `Install-Module PSScriptAnalyzer -Scope CurrentUser -Force`
- Local modules already available:
  - `Pester` 5.7.1
  - `PSScriptAnalyzer` 1.24.0

Build:
- No separate build command was discovered. This is a PowerShell script repo;
  verification is through parser, lint, tests, and smoke checks.

Syntax/static analysis:
- Parse all `.ps1` files with
  `[System.Management.Automation.Language.Parser]::ParseFile(...)`.
- Run PSScriptAnalyzer against `.ps1` files except `_TestInit.ps1` using
  `PSScriptAnalyzerSettings.psd1`.
- Compatibility checking is configured through PSScriptAnalyzer rules including
  Windows PowerShell 5.1 and PowerShell 7.4 syntax targets.

Unit tests:
- `pwsh -NoProfile -Command "Invoke-Pester tests -CI"`
- CI also has a configuration path that writes `./test-results.xml`.

Integration/e2e tests:
- `pwsh -NoProfile -Command "Invoke-Pester tests/e2e -CI"`

Entrypoint smoke checks:
- CI smoke-tests these scripts with `-SmokeTest`:
  - `Run-Optimize.ps1`
  - `Cleanup.ps1`
  - `Boot-SafeMode.ps1`
  - `SafeMode-DriverClean.ps1`
  - `PostReboot-Setup.ps1`
  - `FpsCap-Calculator.ps1`
  - `Verify-Settings.ps1`
  - `CS2-Optimize-GUI.ps1`

Additional CI checks:
- Retired-surface cross-reference grep.
- Secret pattern scan.
- PowerShell dangerous-command scan.
- Batch launcher safety scan.
- Workflow integrity scan.
- Windows PowerShell 5.1 compatibility job.

Format checks:
- No formatter or dedicated format-check command was discovered.
- `git diff --check` is the only discovered whitespace check.

Migrations, generated code, snapshots, fixtures, and services:
- No migrations, generated-code directory, snapshot directory, fixture directory,
  package build output, or service dependency manifest was discovered.
- Tests use temporary directories under the host temp path.
- `testResults.xml` is present as an ignored local output file; it was not a new
  tracked change from this task.
- Strong runtime verification depends on Windows services, registry, BCD,
  drivers, GPU/NIC state, WPF, reboot/Safe Mode behavior, and administrative
  privileges.

## Commands Run

| Command | Result |
| --- | --- |
| `command -v pwsh` | Found `/usr/local/bin/pwsh`. |
| `command -v powershell` | No command found. Windows PowerShell 5.1 is unavailable on this host. |
| `pwsh -NoProfile -Command '$PSVersionTable.PSVersion.ToString(); Get-Module -ListAvailable Pester ...; Get-Module -ListAvailable PSScriptAnalyzer ...'` | PowerShell 7.5.4, Pester 5.7.1, PSScriptAnalyzer 1.24.0. |
| PowerShell parser pass over all `.ps1` files | Exit 0. Output: `Syntax check: all files parse cleanly`. |
| PSScriptAnalyzer over `.ps1` files except `_TestInit.ps1` | Exit 0. Output: `PSScriptAnalyzer: all clean`. |
| `pwsh -NoProfile -Command 'Invoke-Pester tests -CI'` | Exit 0. Discovery found 729 tests. Completed in 148.01s. 728 passed, 0 failed, 1 skipped. |
| `pwsh -NoProfile -Command 'Invoke-Pester tests/e2e -CI'` | Exit 0. Discovery found 2 tests. Completed in 1.8s. 2 passed, 0 failed, 0 skipped. |
| `pwsh -NoProfile -ExecutionPolicy Bypass -File ./CS2-Optimize-GUI.ps1 -SmokeTest` | Exit 0. Output: `SMOKE TEST OK: CS2-Optimize-GUI`. |
| `pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass -File ./Run-Optimize.ps1 -SmokeTest` | Exit 0. Output: `SMOKE TEST OK: Run-Optimize`. |
| `pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass -File ./Verify-Settings.ps1 -SmokeTest` | Exit 0. Output: `SMOKE TEST OK: Verify-Settings`. |
| Full CI-style PowerShell 7 entrypoint smoke matrix | Exit 0. All eight listed entrypoints printed `SMOKE TEST OK`. |
| Retired-surface cross-reference grep | Exit 0. Output: `retired surface is clean`. |
| CI-style secret pattern scan | Exit 0. Output: `Secret scan: clean`. |
| CI-style PowerShell script safety scan | Exit 0. Output: `Script safety check: clean`. |
| CI-style launcher safety scan | Exit 0. Output: `Launcher safety check: clean`. |
| CI-style workflow integrity scan | Exit 0. Output: `Workflow integrity check: clean`. |
| `git diff --check` | Exit 0. No whitespace errors reported in tracked diffs. |
| `git diff --check -- docs/verification-baseline.md` | Exit 0. No whitespace errors reported. |
| `git diff --no-index --check /dev/null docs/verification-baseline.md` | Exit 1 because the file differs from `/dev/null`; no whitespace errors were printed. |
| `grep -n '[[:blank:]]$' docs/verification-baseline.md` | Exit 1 with no output, which means no trailing whitespace was found. |

Long-form commands that were run from the repository root:

```powershell
pwsh -NoProfile -Command '$errors = 0; Get-ChildItem -Recurse -Filter "*.ps1" | ForEach-Object { $parseErrors = $null; $null = [System.Management.Automation.Language.Parser]::ParseFile($_.FullName, [ref]$null, [ref]$parseErrors); foreach ($e in $parseErrors) { Write-Error "$($_.FullName):$($e.Extent.StartLineNumber) - $($e.Message)"; $errors++ } }; if ($errors -gt 0) { throw "$errors parse error(s) found" }; Write-Host "Syntax check: all files parse cleanly"'
```

```powershell
pwsh -NoProfile -Command '$pssaPaths = Get-ChildItem -Recurse -Filter "*.ps1" | Where-Object { $_.Name -ne "_TestInit.ps1" }; if (-not $pssaPaths) { throw "No PowerShell files found for PSScriptAnalyzer" }; $results = @(); foreach ($file in $pssaPaths) { try { $results += Invoke-ScriptAnalyzer -Path $file.FullName -Settings .\PSScriptAnalyzerSettings.psd1 -ErrorAction Stop } catch { throw "PSScriptAnalyzer failed on $($file.FullName): $($_.Exception.Message)" } }; if ($results) { $results | Format-Table -AutoSize Severity, ScriptName, Line, RuleName, Message; throw "$($results.Count) PSScriptAnalyzer issue(s) found" }; Write-Host "PSScriptAnalyzer: all clean"'
```

```powershell
pwsh -NoProfile -Command '$scripts = @("Run-Optimize.ps1","Cleanup.ps1","Boot-SafeMode.ps1","SafeMode-DriverClean.ps1","PostReboot-Setup.ps1","FpsCap-Calculator.ps1","Verify-Settings.ps1","CS2-Optimize-GUI.ps1"); foreach ($script in $scripts) { $records = & pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass -File $script -SmokeTest 2>&1; $output = $records | Out-String; $errorRecords = @($records | Where-Object { $_ -is [System.Management.Automation.ErrorRecord] }); if ($LASTEXITCODE -ne 0) { throw "Smoke test failed for $script`n$output" }; if ($errorRecords.Count -gt 0) { throw "Smoke test emitted error records for $script`n$output" }; if ($output -notmatch "SMOKE TEST OK") { throw "Smoke test marker missing for $script`n$output" }; Write-Host "OK $script :: $($output.Trim())" }'
```

```bash
bash -lc '<retired-surface guard from .github/workflows/lint.yml>'
```

## Failures

No local verification command is currently recorded as failing in this baseline.
Full release confidence remains unavailable on this host because Windows-only
compatibility/runtime checks were skipped.

## Skipped Or Unavailable Checks

- Windows PowerShell 5.1 compatibility job: skipped because `powershell` is not
  installed on this macOS host.
- CI's Windows-hosted smoke matrix: only the PowerShell 7 form was run locally;
  the Windows `shell: powershell` job was not available.
- Real optimization/runtime flows: skipped because they require Windows,
  administrative privileges, registry/BCD/service access, reboot or Safe Mode
  behavior, GPU/NIC state, WPF, and machine-specific settings.
- Dependency installation: skipped because the required PowerShell modules were
  already installed locally.
- CI artifact upload and GitHub Actions runner behavior: not run locally.
- Migrations/generated-code checks: no such commands or directories were found.
- Format enforcement beyond whitespace: skipped because no formatter or
  format-check command was discovered.

## Suspicious Or Limited Tests

These tests are still useful, but they should not be treated as full runtime
proof:

- `tests/e2e/entrypoints.Tests.ps1` checks that entrypoints start with
  `-SmokeTest`, emit no error records, and print a smoke marker. It does not
  prove the real optimization paths work.
- `tests/PostReboot-Setup.Tests.ps1` has a `-SmokeTest` check skipped when
  `$IsWindows` is false, so it did not run on this host.
- `tests/workflow-contracts.Tests.ps1` checks workflow YAML text and expected
  job/target strings. It can catch accidental contract drift, but it cannot
  prove GitHub Actions semantics beyond those text expectations.
- `tests/config.Tests.ps1` includes hardcoded count and explanatory-string
  assertions, such as the documented CVar count. These protect documentation
  contracts but are partly implementation-trivial.
- `tests/helpers/nvidia-drs.Tests.ps1` verifies embedded C# source shape,
  constants, and declarations. It does not prove live NVAPI behavior.
- `tests/helpers/gpu-driver-clean.Tests.ps1` includes source-pattern checks for
  commands such as driver enumeration and cleanup. These are useful guards but
  not live driver-clean verification.

No tests looked meaningless enough to discard from this pass, but several are
best understood as contract or source-shape checks rather than end-to-end
behavior checks.

## Trust Level By Check

- Parser and PSScriptAnalyzer checks are trustworthy for syntax/static lint
  coverage, not runtime behavior.
- PowerShell 7 Pester results are trustworthy for the mocked/local behavior
  they cover, but they are partial because one Windows-only test was skipped and
  many Windows runtime effects are mocked.
- PowerShell 7 smoke checks prove that entrypoints can short-circuit cleanly
  with `-SmokeTest`; they do not prove the real optimization workflows.
- Grep-based security and compatibility checks are deterministic but
  pattern-based. They can produce false positives and false negatives. The
  retired-surface guard is workflow-blocking because it matches the CI job's
  exact grep contract.
- `git diff --check` only checks whitespace in tracked diffs. Untracked docs
  need explicit checks after creation.

## Current Verified State

Verified on this host:
- All `.ps1` files parsed cleanly under PowerShell 7.
- PSScriptAnalyzer reported no findings with the repository settings.
- `Invoke-Pester tests -CI` completed with 728 passed, 0 failed, 1 skipped.
- `Invoke-Pester tests/e2e -CI` completed with 2 passed, 0 failed, 0 skipped.
- All eight entrypoints completed the PowerShell 7 `-SmokeTest` matrix.
- CI-style retired-surface, secret, script-safety, launcher-safety, and
  workflow-integrity grep checks completed cleanly.

Not verified:
- Full Windows PowerShell 5.1 compatibility.
- Full GitHub Actions execution on Windows.
- Real runtime behavior for registry, services, BCD, reboot, Safe Mode, GPU,
  NIC, WPF GUI, and administrative actions.
- Live CS2, Steam, NVIDIA, driver, or network-device behavior.
- Formatting beyond whitespace.

Blocking stronger verification:
- `powershell` is unavailable on this host.
- Windows-only runtime surfaces are unavailable on this host.
- Administrative and reboot/Safe Mode actions are unsafe for a local baseline
  run.
