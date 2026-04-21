#Requires -RunAsAdministrator
<#
.SYNOPSIS  FPS Cap Calculator — CS2 benchmark output -> FPS cap  [T1]

  Reads [VProf] FPS: Avg=XXX.X, P1=XXX.X from clipboard or input.
  Calculates cap (avg - 9%), shows P1/Avg ratio, copies cap to clipboard.

  Parameter:
    -ManualAvg  Provide avg FPS directly (skips input dialog)
#>
param(
    [int]$ManualAvg = 0,
    [switch]$SmokeTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ScriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
. "$ScriptRoot\config.env.ps1"
. "$ScriptRoot\helpers.ps1"

Initialize-ScriptDefaults
Ensure-Dir $CFG_LogDir

if ($SmokeTest) {
    Write-Host "SMOKE TEST OK: FpsCap-Calculator" -ForegroundColor Green
    exit 0
}

Write-LogoBanner "FPS Cap Calculator  [T1]  ·  CS2 Optimization Suite"
Write-Host "  $("─" * 58)" -ForegroundColor DarkGray

Write-Host @"
  BENCHMARK MAPS  (by @fREQUENCYcs / FPSHeaven)
  ═══════════════════════════════════════════════
  Outputs at end of map in CS2 console window:
    [VProf] FPS: Avg=XXX.X, P1=XXX.X

  Dust2    $CFG_Benchmark_Dust2
  Inferno  $CFG_Benchmark_Inferno
  Ancient  $CFG_Benchmark_Ancient

  Workflow:
  1.  Workshop -> Subscribe to map -> Start CS2 -> Play -> Workshop Maps
  2.  Map runs 2-3 min automatically — don't touch PC
  3.  Console window opens at end -> copy [VProf] line
  4.  Come back here -> Option [1] -> [Enter]
  Tip: Run 3 times, copy all [VProf] lines at once.
  ──────────────────────────────────────────────────────────────────
"@ -ForegroundColor DarkGray

# ── Input ────────────────────────────────────────────────────────────────────
$result = $null

if ($ManualAvg -gt 0) {
    $result = @{ Avg=$ManualAvg; P1=0; Runs=1; RawAvg=@($ManualAvg); RawP1=@(0) }
} else {
    Write-Host "  INPUT:" -ForegroundColor White
    Write-Host "  [1]  Read from clipboard" -ForegroundColor White
    Write-Host "  [2]  Enter [VProf] line(s) manually" -ForegroundColor White
    Write-Host "  [3]  Enter avg FPS directly" -ForegroundColor White
    do { $im = Read-Host "  [1/2/3]" } while ($im -notin @("1","2","3"))

    switch ($im) {
        "1" {
            Write-Step "Reading clipboard..."
            try {
                $clip = Get-Clipboard
                # Get-Clipboard may return a string[] (one element per line) —
                # join into a single string for regex matching in Parse-BenchmarkOutput
                if ($clip -is [array]) { $clip = $clip -join "`n" }
                if ($clip) {
                    $result = Parse-BenchmarkOutput $clip
                    if ($result) { Write-OK "Detected: $($result.Runs) run(s)." }
                    else         { Write-Warn "No [VProf] FPS pattern found." }
                } else { Write-Warn "Clipboard empty." }
            } catch { Write-Warn "Error: $_" }
        }
        "2" {
            Write-Info "Format: [VProf] FPS: Avg=XXX.X, P1=XXX.X  |  Empty line to finish"
            $lines = @()
            do { $line = Read-Host "  Input"; if ($line.Trim()) { $lines += $line } } while ($line.Trim())
            if ($lines) { $result = Parse-BenchmarkOutput ($lines -join "`n") }
            if (-not $result) { Write-Warn "No valid pattern detected." }
        }
        "3" {
            do {
                $v = Read-Host "  Avg FPS"; $fv = 0
                $ok = [float]::TryParse($v, [System.Globalization.NumberStyles]::Float, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$fv) -and $fv -gt 0
                if (-not $ok) { Write-Warn "Enter positive number." }
            } while (-not $ok)
            $result = @{ Avg=$fv; P1=0; Runs=1; RawAvg=@($fv); RawP1=@(0) }
        }
    }
}

# Fallback
if (-not $result) {
    Write-Warn "No result detected — please enter manually."
    do {
        $v = Read-Host "  Avg FPS"; $fv = 0
        $ok = [float]::TryParse($v, [System.Globalization.NumberStyles]::Float, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$fv) -and $fv -gt 0
        if (-not $ok) { Write-Warn "Positive number." }
    } while (-not $ok)
    $result = @{ Avg=$fv; P1=0; Runs=1; RawAvg=@($fv); RawP1=@(0) }
}

# ── Calculation ──────────────────────────────────────────────────────────────
$avg  = $result.Avg
$p1   = $result.P1
$cap  = Calculate-FpsCap $avg
$cut  = [math]::Round($avg - $cap)
$pct  = [math]::Round($CFG_FpsCap_Percent * 100)

Write-Blank
Write-Host "  ┌──────────────────────────────────────────────────────────┐" -ForegroundColor Green
Write-Host "  │  RESULT                                                  │" -ForegroundColor Green
if ($result.Runs -gt 1) {
    Write-Host "  │  $("Runs: $($result.Runs)".PadRight(56))│" -ForegroundColor DarkGreen
    for ($i = 0; $i -lt $result.Runs; $i++) {
        $l = "  Run $($i+1):  Avg=$($result.RawAvg[$i])  P1=$($result.RawP1[$i])"
        Write-Host "  │$($l.PadRight(58))│" -ForegroundColor DarkGreen
    }
}
Write-Host "  │  $("Avg FPS:  $avg".PadRight(56))│" -ForegroundColor Green
if ($p1 -gt 0) {
    Write-Host "  │  $("1% Lows:  $p1".PadRight(56))│" -ForegroundColor Green
}
Write-Host "  │  $("- ${pct}%:     - $cut FPS".PadRight(56))│" -ForegroundColor Green
Write-Host "  │  ──────────────────────────────────────────────────────  │" -ForegroundColor Green
Write-Host "  │  $("FPS CAP:  ► $cap ◄".PadRight(56))│" -ForegroundColor Cyan
Write-Host "  └──────────────────────────────────────────────────────────┘" -ForegroundColor Green

# P1/Avg ratio
if ($p1 -gt 0) {
    $ratio = [math]::Round($p1 / $avg, 2)
    $rText  = if ($ratio -ge 0.40) { "✔  Good ($ratio) — consistent frametimes" }
              elseif ($ratio -ge 0.30) { "⚠  OK ($ratio) — room for improvement" }
              else { "✘  Bad ($ratio) — check bottleneck! XMP active? Temps OK?" }
    $rColor = if ($ratio -ge 0.40) { "Green" } elseif ($ratio -ge 0.30) { "Yellow" } else { "Red" }
    Write-Host "  P1/Avg ratio:  $rText" -ForegroundColor $rColor
    Write-Info "  Healthy: > 0.40  |  Critical: < 0.30"
}

"$cap" | Set-ClipboardSafe
Write-OK "FPS cap $cap copied to clipboard."
Write-DebugLog "FPS cap calculated: avg=$avg p1=$p1 cap=$cap runs=$($result.Runs)"

# Update state and record benchmark (skip in DRY-RUN)
if (-not $SCRIPT:DryRun) {
    if (Test-Path $CFG_StateFile) {
        try {
            $st = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
            $st | Add-Member -NotePropertyName "fpsCap"  -NotePropertyValue $cap  -Force
            $st | Add-Member -NotePropertyName "avgFps"  -NotePropertyValue $avg  -Force
            if ($p1 -gt 0) { $st | Add-Member -NotePropertyName "p1Fps" -NotePropertyValue $p1 -Force }
            $st | Add-Member -NotePropertyName "capDate" -NotePropertyValue (Get-Date -Format "yyyy-MM-dd HH:mm") -Force
            Save-JsonAtomic -Data $st -Path $CFG_StateFile
        } catch { Write-DebugLog "Could not persist FPS cap data: $_" }
    }
    if ($p1 -gt 0) {
        Add-BenchmarkResult -AvgFps $avg -P1Fps $p1 -Label "FpsCap-Calculator" -Runs $result.Runs | Out-Null
        $bmHistory = @(Get-BenchmarkHistory)
        if ($bmHistory.Count -ge 2) {
            Write-Blank
            Show-BenchmarkComparison
        }
    }
    try {
        Add-TextFileUtf8Line -Path $CFG_LogFile -Value "[$(Get-Date -Format HH:mm:ss)][OK] FPS-Cap: avg=$avg p1=$p1 cap=$cap runs=$($result.Runs)"
    } catch {
        Write-DebugLog "Could not append FPS cap log entry: $_"
    }
}

# ── Guide to set cap ─────────────────────────────────────────────────────────
Write-Blank
Write-Host "  SET THE CAP:" -ForegroundColor White
Write-Host @"
  NVIDIA Control Panel:
    Right-click Desktop -> NVIDIA Control Panel
    -> Manage 3D Settings -> Program Settings
    -> CS2 / cs2.exe -> Max Frame Rate: ON -> $cap  (in clipboard)

  AMD Adrenalin:
    Gaming -> CS2 -> Frame Rate Target Control -> $cap

  In-Game (fallback):  fps_max $cap  (in autoexec.cfg)
"@ -ForegroundColor DarkGray
