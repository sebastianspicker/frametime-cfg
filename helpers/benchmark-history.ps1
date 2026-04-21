# ==============================================================================
#  helpers/benchmark-history.ps1  —  Iterative Benchmark Tracking
# ==============================================================================

$CFG_BenchmarkFile    = "$CFG_WorkDir\benchmark_history.json"
$script:CFG_BenchmarkMaxEntries = 200   # Cap history size — prevents unbounded JSON growth

function Add-BenchmarkResult {
    <#
    .SYNOPSIS  Records a benchmark result with timestamp and optional label.
               Enables before/after comparison and tracking over time.
    .NOTES
        History is capped at $CFG_BenchmarkMaxEntries entries. When the cap is
        reached, the oldest entries are trimmed (FIFO). This prevents the JSON
        file from growing unboundedly on systems that benchmark frequently.
    #>
    param(
        [Parameter(Mandatory)]
        [double]$AvgFps,
        [Parameter(Mandatory)]
        [double]$P1Fps,
        [string]$Label = "",
        [int]$Runs = 1
    )

    # @() wrapper ensures $history is always an array, even when Get-BenchmarkHistory
    # returns $null (empty/missing file). Without it, $null += $entry yields a bare
    # Hashtable whose .Count equals its key count, causing the trim logic to misfire.
    $history = @(Get-BenchmarkHistory)

    $entry = @{
        # Local time (timezone not tracked — acceptable for gaming benchmarks)
        timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        avgFps    = $AvgFps
        p1Fps     = $P1Fps
        label     = $Label
        runs      = $Runs
    }

    $history += $entry

    # Trim oldest entries if history exceeds cap
    if ($history.Count -gt $script:CFG_BenchmarkMaxEntries) {
        $history = $history[($history.Count - $script:CFG_BenchmarkMaxEntries)..($history.Count - 1)]
    }

    Save-JsonAtomic -Data $history -Path $CFG_BenchmarkFile

    return $entry
}

function Get-BenchmarkHistory {
    <#  Returns all recorded benchmark results as an array.
        Handles: missing file, corrupted JSON, empty JSON array (PS 5.1 returns $null
        for "[]"), single-object JSON (not wrapped in array).  #>
    if (-not (Test-Path $CFG_BenchmarkFile)) { return @() }
    try {
        $data = Get-Content $CFG_BenchmarkFile -Raw -ErrorAction Stop | ConvertFrom-Json
        # PS 5.1: ConvertFrom-Json returns $null for empty arrays ("[]")
        # Use $null -eq (not -not) to avoid false positives on valid falsy values (0, "")
        if ($null -eq $data) { return @() }
        if ($data -is [array]) { return $data }
        return @($data)
    } catch { return @() }
}

function Show-BenchmarkComparison {
    <#
    .SYNOPSIS  Displays a comparison table of all benchmark results,
               showing improvement/degradation between each run.
    #>
    $history = @(Get-BenchmarkHistory)

    if ($history.Count -eq 0) {
        Write-Info "No benchmark results recorded yet."
        return
    }

    Write-Blank
    Write-Host "  ╔══════════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
    Write-Host "  ║  BENCHMARK HISTORY                                              ║" -ForegroundColor Cyan
    Write-Host "  ╠══════════════════════════════════════════════════════════════════╣" -ForegroundColor Cyan
    Write-Host "  ║  #   Date        Time      Avg FPS   1% Low   Δ Avg   Δ 1%     ║" -ForegroundColor Cyan
    Write-Host "  ╠══════════════════════════════════════════════════════════════════╣" -ForegroundColor Cyan

    for ($i = 0; $i -lt $history.Count; $i++) {
        $entry = $history[$i]
        $num = ($i + 1).ToString().PadLeft(2)
        $ts = if ($null -ne $entry.timestamp) { [string]$entry.timestamp } else { "" }
        if ($ts.Length -ge 16) {
            $date = $ts.Substring(0, 10)
            $time = $ts.Substring(11, 5)
        } else {
            $date = $ts.PadRight(10)
            $time = "??:??"
        }
        $avg = if ($null -ne $entry.avgFps) { $entry.avgFps.ToString("F1", [System.Globalization.CultureInfo]::InvariantCulture).PadLeft(7) } else { "    N/A" }
        $p1  = if ($null -ne $entry.p1Fps) { $entry.p1Fps.ToString("F1", [System.Globalization.CultureInfo]::InvariantCulture).PadLeft(7) } else { "    N/A" }

        $avgDiffStr = "   —  "
        $p1DiffStr  = "   —  "
        $color = "White"

        if ($i -gt 0) {
            $prev = $history[$i - 1]
            if ($null -ne $entry.avgFps -and $null -ne $prev.avgFps -and $null -ne $entry.p1Fps -and $null -ne $prev.p1Fps) {
                $avgDiff = [math]::Round($entry.avgFps - $prev.avgFps, 1)
                $p1Diff  = [math]::Round($entry.p1Fps - $prev.p1Fps, 1)
                $avgDiffStr = "$(if($avgDiff -ge 0){'+'}else{''})$($avgDiff.ToString('F1', [System.Globalization.CultureInfo]::InvariantCulture))".PadLeft(6)
                $p1DiffStr  = "$(if($p1Diff -ge 0){'+'}else{''})$($p1Diff.ToString('F1', [System.Globalization.CultureInfo]::InvariantCulture))".PadLeft(6)
                $color = if ($p1Diff -gt 0) { "Green" } elseif ($p1Diff -lt 0) { "Red" } else { "Yellow" }
            }
        }

        $label = if ($entry.label) { "  $($entry.label)" } else { "" }
        Write-Host "  ║  $num  $date  $time   $avg   $p1  $avgDiffStr  $p1DiffStr  ║$label" -ForegroundColor $color
    }

    Write-Host "  ╚══════════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan

    # Overall comparison (first vs last)
    if ($history.Count -ge 2) {
        $first = $history[0]
        $last  = $history[-1]
        if ($null -eq $last.avgFps -or $null -eq $first.avgFps -or $null -eq $last.p1Fps -or $null -eq $first.p1Fps) {
            Write-Info "Cannot compute total change — some entries have missing FPS data."
            return
        }
        $totalAvgDiff = [math]::Round($last.avgFps - $first.avgFps, 1)
        $totalP1Diff  = [math]::Round($last.p1Fps - $first.p1Fps, 1)
        $totalColor = if ($totalP1Diff -gt 0) { "Green" } elseif ($totalP1Diff -lt 0) { "Red" } else { "Yellow" }

        Write-Blank
        Write-Host "  TOTAL CHANGE (first -> last):" -ForegroundColor $totalColor
        Write-Host "  Avg FPS: $($first.avgFps) -> $($last.avgFps)  ($(if($totalAvgDiff -ge 0){'+'})$totalAvgDiff)" -ForegroundColor $totalColor
        Write-Host "  1% Lows: $($first.p1Fps) -> $($last.p1Fps)  ($(if($totalP1Diff -ge 0){'+'})$totalP1Diff)" -ForegroundColor $totalColor

        if ($totalP1Diff -gt 5) {
            Write-OK "Significant improvement in 1% lows!"
        } elseif ($totalP1Diff -gt 0) {
            Write-OK "Marginal improvement in 1% lows."
        } elseif ($totalP1Diff -eq 0) {
            Write-Info "No change in 1% lows — within margin of error."
        } else {
            Write-Warn "1% lows degraded. Check recent changes."
        }
    }
}

function Invoke-BenchmarkCapture {
    <#
    .SYNOPSIS  Interactive benchmark capture with automatic parsing,
               comparison, and FPS cap calculation.
    #>
    param(
        [string]$Label = ""
    )

    # @() wrapper: PowerShell pipeline unwraps `return @()` to $null; with
    # Set-StrictMode -Version Latest, $null.Count is a terminating error.
    $history = @(Get-BenchmarkHistory)

    if ($history.Count -gt 0) {
        Write-Info "You have $($history.Count) previous benchmark result(s)."
        Show-BenchmarkComparison
        Write-Blank
    }

    Write-Host "  Run a FPSHeaven benchmark map in CS2, then paste the [VProf] output here." -ForegroundColor White
    Write-Host "  Format: [VProf] FPS: Avg=XXX.X, P1=XXX.X" -ForegroundColor DarkGray
    Write-Blank

    $userInput = Read-Host "  Paste [VProf] output (or [Enter] to skip)"
    if ([string]::IsNullOrWhiteSpace($userInput)) {
        Write-Info "Benchmark skipped."
        return $null
    }

    $result = Parse-BenchmarkOutput $userInput
    if (-not $result) {
        Write-Warn "Could not parse VProf output. Expected format: [VProf] FPS: Avg=XXX.X, P1=XXX.X"
        return $null
    }

    # Prompt for label if not provided
    if (-not $Label) {
        $Label = Read-Host "  Label for this result (e.g. 'baseline', 'after DDU', 'final') [Enter to skip]"
    }

    $null = Add-BenchmarkResult -AvgFps $result.Avg -P1Fps $result.P1 -Label $Label -Runs $result.Runs
    Write-OK "Recorded: Avg $($result.Avg) FPS, 1% low $($result.P1) FPS ($($result.Runs) run(s))"

    # Calculate FPS cap
    $cap = Calculate-FpsCap $result.Avg
    Write-OK "FPS Cap: $cap  (avg $($result.Avg) - 9%)"
    "$cap" | Set-ClipboardSafe
    Write-Info "FPS cap $cap copied to clipboard."

    # Show comparison with previous
    if ($history.Count -gt 0) {
        $prev = $history[-1]
        if ($null -eq $prev.avgFps -or $null -eq $prev.p1Fps) {
            Write-Info "Previous run has incomplete data — skipping comparison."
        } else {
        $avgDiff = [math]::Round($result.Avg - $prev.avgFps, 1)
        $p1Diff  = [math]::Round($result.P1 - $prev.p1Fps, 1)
        $pColor = if ($p1Diff -gt 0) { "Green" } elseif ($p1Diff -lt 0) { "Red" } else { "Yellow" }

        Write-Blank
        Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor $pColor
        Write-Host "  │  COMPARISON WITH PREVIOUS:                                   │" -ForegroundColor $pColor
        $avgLine = "Avg FPS: $($prev.avgFps.ToString('F1', [System.Globalization.CultureInfo]::InvariantCulture)) -> $($result.Avg.ToString('F1', [System.Globalization.CultureInfo]::InvariantCulture))  ($(if($avgDiff -ge 0){'+'})$($avgDiff.ToString('F1', [System.Globalization.CultureInfo]::InvariantCulture)))"
        $p1Line  = "1% Lows: $($prev.p1Fps.ToString('F1', [System.Globalization.CultureInfo]::InvariantCulture)) -> $($result.P1.ToString('F1', [System.Globalization.CultureInfo]::InvariantCulture))  ($(if($p1Diff -ge 0){'+'})$($p1Diff.ToString('F1', [System.Globalization.CultureInfo]::InvariantCulture)))"
        Write-Host "  │  $avgLine$((' ' * [math]::Max(0, 60 - $avgLine.Length)))│" -ForegroundColor White
        Write-Host "  │  $p1Line$((' ' * [math]::Max(0, 60 - $p1Line.Length)))│" -ForegroundColor White
        Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor $pColor

        if ($p1Diff -gt 5) {
            Write-OK "Notable improvement in 1% lows! The last change had measurable effect."
        } elseif ($p1Diff -gt 0) {
            Write-Info "Small improvement. Within typical variance — run 3x to confirm."
        } elseif ($p1Diff -lt -5) {
            Write-Warn "1% lows degraded significantly. Consider reverting the last change."
        } elseif ($p1Diff -lt 0) {
            Write-Info "Small degradation. May be variance — run 3x to confirm."
        } else {
            Write-Info "No change. Within margin of error."
        }
        } # end else (prev data valid)
    }

    return @{ Avg = $result.Avg; P1 = $result.P1; Cap = $cap }
}
