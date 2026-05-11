# Contributing

Thanks for your interest in improving the CS2 Optimization Suite.

## Ground Rules

1. **Evidence required.** Every optimization must have a source — benchmarks, xperf traces, CapFrameX logs, or at minimum a reproducible community finding. "I read it on Reddit" is not sufficient; "djdallmann tested it with xperf and showed X" is.

2. **No external tools.** The suite runs on pure PowerShell with zero downloads (except NVIDIA driver .exe). If your change requires an external binary, it needs a native PowerShell reimplementation.

3. **Backup everything.** Any registry, service, or boot config change must integrate with `helpers/backup-restore.ps1`. Users must be able to roll back.

4. **DRY-RUN must work.** All state-changing operations must respect `$SCRIPT:DryRun`. Test with the DRY-RUN profile.

5. **Don't break the tier system.** Changes go through `Invoke-TieredStep` with appropriate `-Risk`, `-Depth`, `-Tier`, and `-Improvement` metadata.

## Before Submitting

```powershell
# Lint check (must pass clean; mirrors CI exclusion)
$pssaPaths = Get-ChildItem -Recurse -Filter "*.ps1" |
    Where-Object { $_.FullName -notlike "*tests/helpers/_TestInit.ps1" }
$results = foreach ($file in $pssaPaths) {
    Invoke-ScriptAnalyzer -Path $file.FullName -Settings .\PSScriptAnalyzerSettings.psd1
}
if ($results) {
    $results | Format-Table -AutoSize Severity, ScriptName, Line, RuleName, Message
    throw "$($results.Count) PSScriptAnalyzer issue(s) found"
}

# Parse check (must show zero errors)
Get-ChildItem -Recurse -Filter "*.ps1" | ForEach-Object {
    $e = $null
    $null = [System.Management.Automation.Language.Parser]::ParseFile($_.FullName, [ref]$null, [ref]$e)
    if ($e) { $e | ForEach-Object { "$($_.Extent.StartLineNumber): $($_.Message)" } }
}

# Test gate
Invoke-Pester tests -CI

# Process-level E2E smoke gate
Invoke-Pester tests/e2e -CI

# Smoke at least the changed entry point locally; CI runs the full matrix on Windows
pwsh -NoProfile -ExecutionPolicy Bypass -File .\CS2-Optimize-GUI.ps1 -SmokeTest
```

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
