# Contributing

Thanks for your interest in improving the CS2 Optimization Suite.

## Ground Rules

1. **Evidence required.** Every optimization must have a source: benchmarks,
   xperf traces, CapFrameX logs, or at minimum a reproducible community finding.
   A concrete test result is evidence; an unattributed recommendation is not.

2. **No external tools.** The suite runs on PowerShell with no downloads except
   the NVIDIA driver. A new external binary requires strong justification and a
   native alternative assessment.

3. **Backup everything.** Registry, service, and boot configuration changes must
   integrate with `helpers/backup-restore.ps1`. Users must be able to roll back.

4. **DRY-RUN must work.** State-changing operations must respect
   `$SCRIPT:DryRun`. Test with the DRY-RUN profile.

5. **Preserve the tier system.** Changes go through `Invoke-TieredStep` with
   appropriate `-Risk`, `-Depth`, `-Tier`, and `-Improvement` metadata.

## Before Submitting

```powershell
# Lint check (must pass clean; mirrors CI exclusion)
$pssaPaths = Get-ChildItem -Recurse -Filter "*.ps1" |
    Where-Object { $_.FullName -notlike "*tests/helpers/_TestInit.ps1" }
$results = foreach ($file in $pssaPaths) {
    Invoke-ScriptAnalyzer -Path $file.FullName `
        -Settings .\PSScriptAnalyzerSettings.psd1
}
if ($results) {
    $results | Format-Table -AutoSize Severity, ScriptName, Line, RuleName, Message
    throw "$($results.Count) PSScriptAnalyzer issue(s) found"
}

# Parse check (must show zero errors)
Get-ChildItem -Recurse -Filter "*.ps1" | ForEach-Object {
    $e = $null
    $null = [System.Management.Automation.Language.Parser]::ParseFile(
        $_.FullName, [ref]$null, [ref]$e
    )
    if ($e) {
        $e | ForEach-Object {
            "$($_.Extent.StartLineNumber): $($_.Message)"
        }
    }
}

# Test gate (imports an installed Pester 5 release)
pwsh -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-LocalTests.ps1

# Process-level E2E smoke gate
pwsh -NoProfile -ExecutionPolicy Bypass `
    -File .\scripts\Invoke-LocalTests.ps1 -Path .\tests\e2e

# Smoke the changed entry point; CI runs the full matrix on Windows
pwsh -NoProfile -ExecutionPolicy Bypass -File .\CS2-Optimize-GUI.ps1 -SmokeTest
```

For desktop-interface changes, also validate
`ui/CS2-Optimize-GUI.xaml`, run
`tests/gui-design-contract.Tests.ps1`, and complete the Windows-only checks in
[`docs/frontend.md`](docs/frontend.md).

## Public repository boundary

Do not commit runtime state, logs, local analysis workspaces, agent ledgers,
personal paths, private system information, raw diagnostic exports, credentials,
or unvalidated screenshots. The root [`.gitignore`](.gitignore) defines the
local-only lanes. Durable source, tests, public documentation, workflow
configuration, `PRODUCT.md`, `DESIGN.md`, and
`.codacy/codacy.config.json` remain public.

Before submitting, inspect both:

```powershell
git status --short
git ls-files -ci --exclude-standard
```

The second command must produce no output; tracked files must never depend on an
ignore rule to stay private.

## What We're Looking For

- New optimizations with evidence (benchmarks, sources)
- AMD GPU support (currently NVIDIA-focused)
- Bug fixes with reproduction steps
- Documentation improvements with citations
- Translations

## What We Won't Merge

- Tweaks from the [Debunked list](docs/debunked.md) without new contradicting evidence
- Changes that add external tool dependencies
- TCP-only "optimizations" (CS2 uses UDP)
- Anything without backup/restore support
