# ==============================================================================
#  helpers/tier-system.ps1  —  Profile & Tier-Aware Step Execution
# ==============================================================================
#
#  PROFILES (user-facing):
#    SAFE          Only proven, safe tweaks. Auto-applied. No prompts.
#    RECOMMENDED   Safe + moderate tweaks. Moderate steps prompted.
#    COMPETITIVE   Everything incl. community tips. All prompted.
#    CUSTOM        Full detail card for every step. Nothing auto.
#    YOLO          Everything auto-executes (up to AGGRESSIVE). Zero prompts.
#
#  TIER SYSTEM (internal):
#    T1  Proven, measurable effect on 1%/0.1% lows.
#    T2  Real effect, but setup-dependent or situational.
#    T3  Community consensus, no hard benchmark proof.
#
#  RISK LEVELS:
#    SAFE        Read-only check, or universally harmless + auto-reversible
#    MODERATE    Registry/config change, easily reversible
#    AGGRESSIVE  Service/driver/boot change, needs restart or careful undo
#    CRITICAL    Security implications, driver removal, data-affecting
#
#  DEPTH CATEGORIES:
#    CHECK       Read-only, no modification
#    REGISTRY    Windows registry
#    SERVICE     Windows services
#    BOOT        Boot config (bcdedit)
#    DRIVER      GPU/device drivers
#    NETWORK     Network adapter/DNS
#    FILESYSTEM  File/cache deletion
#    APP         Application config (autoexec, etc.)
#
#  PROFILE → BEHAVIOR MATRIX:
#  ┌──────────────┬──────────┬──────────────────────────────┬──────────────────┐
#  │ Profile      │ T1       │ T2                           │ T3               │
#  ├──────────────┼──────────┼──────────────────────────────┼──────────────────┤
#  │ SAFE         │ auto     │ SAFE→auto, MODERATE+→skip    │ skip             │
#  │ RECOMMENDED  │ auto     │ ≤MODERATE→prompted, else skip│ skip             │
#  │ COMPETITIVE  │ auto     │ ≤AGGRESSIVE→prompted         │ ≤AGGRESSIVE→ask  │
#  │ CUSTOM       │ prompted │ prompted (full card)         │ prompted         │
#  │ YOLO         │ auto     │ ≤AGGRESSIVE→auto             │ ≤AGGRESSIVE→auto │
#  └──────────────┴──────────┴──────────────────────────────┴──────────────────┘
#  DRY-RUN is a modifier that can be combined with any profile.

if (-not (Get-Variable -Name RiskOrder -Scope Script -ErrorAction SilentlyContinue)) { $SCRIPT:RiskOrder = @{ "SAFE"=1; "MODERATE"=2; "AGGRESSIVE"=3; "CRITICAL"=4 } }
if (-not (Get-Variable -Name AppliedSteps -Scope Script -ErrorAction SilentlyContinue)) { $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new() }

function Save-AppliedSteps {
    <#  Persists $SCRIPT:AppliedSteps to state.json so Phase 3 can read Phase 1 estimates.  #>
    if (-not (Test-Path $CFG_StateFile)) { return $true }
    try {
        $st = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json
        $st | Add-Member -NotePropertyName "appliedSteps" -NotePropertyValue @($SCRIPT:AppliedSteps) -Force
        Save-JsonAtomic -Data $st -Path $CFG_StateFile
        Set-SecureAcl -Path $CFG_StateFile
        return $true
    } catch {
        Write-DebugLog "Could not persist AppliedSteps: $_"
        Write-Warn "Could not persist applied steps to state.json. Phase 3 estimates may be incomplete."
        return $false
    }
}

function Load-AppliedSteps {
    <#  Loads previously applied step keys from state.json into $SCRIPT:AppliedSteps.  #>
    if (-not (Test-Path $CFG_StateFile)) { return }
    try {
        $st = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json
        if ($st.appliedSteps) {
            foreach ($key in @($st.appliedSteps)) {
                if ($key -notin $SCRIPT:AppliedSteps) { $SCRIPT:AppliedSteps.Add($key) }
            }
        }
    } catch { Write-DebugLog "Could not load AppliedSteps: $_" }
}

function Test-YoloProfile { return $SCRIPT:Profile -eq "YOLO" }

function Get-ProfileMaxRisk {
    # Normalize profile to uppercase for case-insensitive matching
    $p = if ($SCRIPT:Profile) { $SCRIPT:Profile.ToUpper() } else { "" }
    switch ($p) {
        "SAFE"        { return "SAFE" }
        "RECOMMENDED" { return "MODERATE" }
        "COMPETITIVE" { return "AGGRESSIVE" }
        "YOLO"        { return "AGGRESSIVE" }
        "CUSTOM"      { return "CRITICAL" }
        default       { return "MODERATE" }
    }
}

function Test-RiskAllowed {
    <#  Returns $true if the step's risk is within the profile's threshold.  #>
    param([string]$StepRisk)
    if (-not $StepRisk) { return $true }
    if (-not $SCRIPT:RiskOrder.ContainsKey($StepRisk)) {
        Write-Warn "Unknown risk level '$StepRisk' — treating as blocked for safety."
        return $false
    }
    $max = Get-ProfileMaxRisk
    return $SCRIPT:RiskOrder[$StepRisk] -le $SCRIPT:RiskOrder[$max]
}

function Show-StepInfoCard {
    <#  Displays a detailed info card with risk, improvement, side effects.  #>
    [Diagnostics.CodeAnalysis.SuppressMessageAttribute('PSReviewUnusedParameter', 'Tier',
        Justification = 'Accepted for API consistency with Invoke-TieredStep; callers always pass it')]
    param(
        [int]    $Tier,
        [string] $Title,
        [string] $Why,
        [string] $Risk         = "",
        [string] $Depth        = "",
        [string] $Improvement  = "",
        [string] $SideEffects  = "",
        [string] $Undo         = "",
        [string] $Evidence     = "",
        [string] $Caveat       = ""
    )

    $riskColor = switch ($Risk) {
        "SAFE"       { "Green" }
        "MODERATE"   { "Yellow" }
        "AGGRESSIVE" { "DarkYellow" }
        "CRITICAL"   { "Red" }
        default      { "White" }
    }
    $riskLabel = switch ($Risk) {
        "SAFE"       { "SAFE — no risk, easily undone" }
        "MODERATE"   { "MODERATE — can be undone anytime" }
        "AGGRESSIVE" { "AGGRESSIVE — read carefully before applying" }
        "CRITICAL"   { "CRITICAL — may affect system security" }
        default      { $Risk }
    }
    $riskIcon = switch ($Risk) {
        "SAFE"       { [char]0x2714 }   # check
        "MODERATE"   { [char]0x25B2 }   # triangle
        "AGGRESSIVE" { [char]0x25C6 }   # diamond
        "CRITICAL"   { [char]0x2718 }   # cross
        default      { "?" }
    }

    Write-Host "  ┌──────────────────────────────────────────────────────────────────" -ForegroundColor DarkGray
    Write-Host "  │  $Title" -ForegroundColor White
    Write-Host "  │" -ForegroundColor DarkGray
    if ($Why)         { Write-Host "  │  Why:          $Why" -ForegroundColor DarkGray }
    if ($Risk)        { Write-Host "  │  Risk:         $riskIcon $riskLabel" -ForegroundColor $riskColor }
    if ($Depth)       { Write-Host "  │  Modifies:     $Depth" -ForegroundColor DarkGray }
    if ($Improvement) { Write-Host "  │  Expected:     $Improvement" -ForegroundColor Cyan }
    if ($SideEffects) { Write-Host "  │  Side effects: $SideEffects" -ForegroundColor DarkYellow }
    if ($Evidence)    { Write-Host "  │  Evidence:     $Evidence" -ForegroundColor DarkGray }
    if ($Caveat)      { Write-Host "  │  Caveat:       $Caveat" -ForegroundColor DarkYellow }
    if ($Undo)        { Write-Host "  │  Undo:         $Undo" -ForegroundColor DarkGray }
    Write-Host "  └──────────────────────────────────────────────────────────────────" -ForegroundColor DarkGray
}

function Invoke-TieredStep {
    <#
    .SYNOPSIS  Executes a step based on profile, tier, and risk.

    Profile determines which steps are included and how:
      SAFE:         T1 auto. T2(SAFE) auto. T2(MODERATE+) skip. T3 skip.
      RECOMMENDED:  T1 auto. T2(<=MODERATE) prompted. T2(AGGRESSIVE+) skip. T3 skip.
      COMPETITIVE:  T1 auto. T2(<=AGGRESSIVE) prompted. T3(<=AGGRESSIVE) prompted.
      CUSTOM:       Everything prompted with full detail card.
    DRY-RUN is a modifier: shows what would change, nothing is applied.
    #>
    param(
        [int]    $Tier,
        [string] $Title,
        [string] $Why,
        [string] $Evidence     = "",
        [string] $Caveat       = "",
        [string] $Risk         = "",
        [string] $Depth        = "",
        [string] $Improvement  = "",
        [string] $SideEffects  = "",
        [string] $Undo         = "",
        [string] $EstimateKey   = "",
        [scriptblock] $Action,
        [scriptblock] $SkipAction = $null
    )

    # Track current step for automatic backup integration
    $SCRIPT:CurrentStepTitle = $Title

    Write-Blank
    Write-TierBadge $Tier $Title

    # ── Profile risk filter (T2/T3 only — T1 always runs) ───────────
    if ($Tier -gt 1 -and $Risk -and -not (Test-RiskAllowed $Risk)) {
        $max = Get-ProfileMaxRisk
        Write-Host "  $([char]0x25CB) [SKIP] Exceeds $($SCRIPT:Profile) profile ($Risk > $max threshold)" -ForegroundColor DarkGray
        if ($Improvement) { Write-Host "         Would have: $Improvement" -ForegroundColor DarkGray }
        if ($Undo)        { Write-Host "         Available in COMPETITIVE or CUSTOM profile" -ForegroundColor DarkGray }
        Write-DebugLog "Profile filter: '$Title' skipped ($Risk > $max)"
        Add-PhaseSkipped
        if ($SkipAction) { & $SkipAction }
        $SCRIPT:CurrentStepTitle = $null
        return $false
    }

    # ── Determine whether to show full info card ────────────────────
    $showCard = ($SCRIPT:Profile -eq "CUSTOM") -or
                ($SCRIPT:DryRun) -or
                ($SCRIPT:LogLevel -eq "VERBOSE") -or
                ($Tier -gt 1 -and $SCRIPT:Profile -notin @("SAFE","YOLO")) -or
                ($Risk -in @("AGGRESSIVE","CRITICAL") -and $SCRIPT:Profile -ne "YOLO")

    if ($showCard -and ($Risk -or $Improvement -or $SideEffects)) {
        Show-StepInfoCard -Tier $Tier -Title $Title -Why $Why `
            -Risk $Risk -Depth $Depth -Improvement $Improvement `
            -SideEffects $SideEffects -Undo $Undo `
            -Evidence $Evidence -Caveat $Caveat
    } elseif ($Why -or $Evidence -or $Caveat) {
        if ($SCRIPT:LogLevel -eq "VERBOSE" -or ($Tier -gt 1 -and $SCRIPT:Profile -ne "SAFE")) {
            if ($Why)      { Write-Info "Reason:    $Why" }
            if ($Evidence) { Write-Info "Evidence:  $Evidence" }
            if ($Caveat)   { Write-Info "Caveat:    $Caveat" }
        }
    }

    # ── DRY-RUN modifier ────────────────────────────────────────────
    if ($SCRIPT:DryRun) {
        # Still respect profile tier filtering — SAFE DRY-RUN should not preview T3 steps
        $wouldSkip = $false
        switch ($SCRIPT:Profile) {
            "SAFE"        { if ($Tier -ge 3 -or ($Tier -eq 2 -and $Risk -notin @("SAFE","",$null))) { $wouldSkip = $true } }
            "RECOMMENDED" { if ($Tier -ge 3 -or ($Tier -eq 2 -and $Risk -and -not (Test-RiskAllowed $Risk))) { $wouldSkip = $true } }
            # COMPETITIVE/CUSTOM/YOLO: intentionally preview all steps in DRY-RUN
        }
        if ($wouldSkip) {
            Write-Host "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Would SKIP: $Title (filtered by $($SCRIPT:Profile) profile)" -ForegroundColor DarkGray
            $SCRIPT:CurrentStepTitle = $null
            return $false
        }
        Write-Host "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Would execute: $Title" -ForegroundColor Magenta
        if ($Depth)       { Write-Host "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Modifies: $Depth" -ForegroundColor Magenta }
        if ($Improvement) { Write-Host "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Expected: $Improvement" -ForegroundColor Magenta }
        Write-DebugLog "DRY-RUN: '$Title' — preview only, no changes applied"
        # Run the action but Set-RegistryValue/Set-BootConfig intercept writes
        try { & $Action } catch { Write-Warn "Step '$Title' preview issue (DRY-RUN): $_" }
        # Defensive flush — should be a no-op because Backup-* functions self-guard in DRY-RUN.
        try { Flush-BackupBuffer } catch { Write-DebugLog "Flush-BackupBuffer failed after DRY-RUN '$Title': $_" }
        $SCRIPT:CurrentStepTitle = $null
        return $false
    }

    # ── Decide whether to run based on profile + tier ───────────────
    $run = $false

    switch ($SCRIPT:Profile) {

        "SAFE" {
            # T1: auto-run. T2(SAFE): auto-run. T3: already filtered above.
            if ($Tier -eq 1) {
                Write-DebugLog "SAFE/T1: Auto-Execute '$Title'"
                $run = $true
            } elseif ($Tier -eq 2 -and $Risk -eq "SAFE") {
                Write-DebugLog "SAFE/T2(SAFE): Auto-Execute '$Title'"
                $run = $true
            } else {
                # T2 without Risk="SAFE" or unexpected tier — skip with debug message
                Write-DebugLog "SAFE profile: Skipping '$Title' (Tier=$Tier, Risk=$Risk)"
                $run = $false
            }
        }

        "RECOMMENDED" {
            if ($Tier -eq 1) {
                Write-DebugLog "RECOMMENDED/T1: Auto-Execute '$Title'"
                $run = $true
            } elseif ($Tier -eq 2) {
                # Prompt for T2 steps
                Write-Blank
                $rTag = if ($Risk) { " [$Risk]" } else { "" }
                Write-Host "  [T2$rTag] Do you want to run this step?" -ForegroundColor Yellow
                if ($Improvement) {
                    Write-Host "       Expected: $Improvement" -ForegroundColor Cyan
                }
                Write-Blank
                $r = Read-Host "  $Title — run? [y/N]"
                $run = ($r -match "^[jJyY]$")
            } else {
                # T3: skip in RECOMMENDED
                Write-Host "  $([char]0x25C6) [T3] Skipped in RECOMMENDED profile (community tip, no hard proof)." -ForegroundColor DarkCyan
                $run = $false
            }
        }

        "COMPETITIVE" {
            if ($Tier -eq 1) {
                Write-DebugLog "COMPETITIVE/T1: Auto-Execute '$Title'"
                $run = $true
            } elseif ($Tier -eq 2) {
                Write-Blank
                $rTag = if ($Risk) { " [$Risk]" } else { "" }
                Write-Host "  [T2$rTag] Do you want to run this step?" -ForegroundColor Yellow
                if ($Improvement) {
                    Write-Host "       Expected: $Improvement" -ForegroundColor Cyan
                }
                Write-Blank
                $r = Read-Host "  $Title — run? [y/N]"
                $run = ($r -match "^[jJyY]$")
            } else {
                # T3: prompt in COMPETITIVE
                Write-Blank
                $rTag = if ($Risk) { " [$Risk]" } else { "" }
                Write-Host "  $([char]0x25C6) [T3$rTag] Community tip $([char]0x2014) no hard benchmark proof." -ForegroundColor DarkCyan
                if ($Improvement) {
                    Write-Host "       Expected: $Improvement" -ForegroundColor Cyan
                }
                Write-Blank
                $r = Read-Host "  $Title — run anyway? [y/N]"
                $run = ($r -match "^[jJyY]$")
            }
        }

        "CUSTOM" {
            # Everything prompted with full detail
            Write-Blank
            $rTag = if ($Risk) { " [$Risk]" } else { "" }
            $tLabel = switch ($Tier) {
                1 { "[T1$rTag] Proven — apply this step?" }
                2 { "[T2$rTag] Setup-dependent — apply?" }
                3 { "[T3$rTag] Community tip — apply?" }
                default { "[T?$rTag] Unknown tier — apply?" }
            }
            $tColor = switch ($Tier) { 1 {"Green"} 2 {"Yellow"} 3 {"DarkCyan"} default {"White"} }
            Write-Host "  $tLabel" -ForegroundColor $tColor
            $defaultYes = ($Tier -eq 1)
            if ($defaultYes) {
                $r = Read-Host "  $Title [Y/n]"
                $run = ($r -notmatch "^[nN]$")
            } else {
                $r = Read-Host "  $Title [y/N]"
                $run = ($r -match "^[jJyY]$")
            }
        }

        "YOLO" {
            # Everything auto-executes. No prompts. Risk ceiling enforced above.
            Write-DebugLog "YOLO/T${Tier}: Auto-Execute '$Title'"
            $run = $true
        }

        default {
            # Fallback: treat as RECOMMENDED
            $run = ($Tier -eq 1)
            if ($Tier -gt 1) {
                $r = Read-Host "  $Title — run? [y/N]"
                $run = ($r -match "^[jJyY]$")
            }
        }
    }

    # ── Execute or skip ─────────────────────────────────────────────
    $actionOk = $true
    if ($run) {
        Write-DebugLog "Executing: '$Title'"
        try { & $Action } catch {
            Write-Err "Step '$Title' failed: $_"
            Write-Host "  $([char]0x2139) What to do: This step was skipped safely. Your system is not affected." -ForegroundColor Cyan
            Write-Host "  $([char]0x2139) You can retry later via START.bat, or continue $([char]0x2014) remaining steps still work." -ForegroundColor Cyan
            $actionOk = $false
        }
        # Track applied steps for improvement estimation (only on success)
        if ($actionOk -and $EstimateKey -and -not $SCRIPT:DryRun) {
            $SCRIPT:AppliedSteps.Add($EstimateKey)
        }
        # Update phase counters
        if ($actionOk) { Add-PhaseApplied } else { Add-PhaseFailed }
    } else {
        Write-DebugLog "Skipped: '$Title'"
        Add-PhaseSkipped
        if ($SkipAction) { & $SkipAction }
    }

    # Flush any pending backup entries to disk in one I/O pass.
    # This is the primary flush point — backup functions buffer entries in memory
    # during the step's action, and we persist them here once the step finishes.
    try { Flush-BackupBuffer } catch { Write-Warn "Backup entries could not be saved to disk after '$Title': $_  (entries retained in memory for next flush)" }

    $SCRIPT:CurrentStepTitle = $null
    return ($run -and $actionOk)
}

function Get-ImprovementEstimate {
    <#
    .SYNOPSIS  Calculates cumulative estimated improvement from all applied steps.
               Returns a summary with min/max P1 low and avg FPS ranges.
    #>
    $totalP1Min = 0; $totalP1Max = 0
    $totalAvgMin = 0; $totalAvgMax = 0
    $steps = [System.Collections.Generic.List[object]]::new()

    if (-not $CFG_ImprovementEstimates) {
        return @{ P1LowMin=0; P1LowMax=0; AvgMin=0; AvgMax=0; Steps=@(); Count=0 }
    }
    foreach ($key in $SCRIPT:AppliedSteps) {
        if ($CFG_ImprovementEstimates.ContainsKey($key)) {
            $est = $CFG_ImprovementEstimates[$key]
            $totalP1Min  += $est.P1LowMin
            $totalP1Max  += $est.P1LowMax
            $totalAvgMin += $est.AvgMin
            $totalAvgMax += $est.AvgMax
            $steps.Add(@{ Key=$key; Estimate=$est })
        }
    }

    return @{
        P1LowMin = $totalP1Min;  P1LowMax = $totalP1Max
        AvgMin   = $totalAvgMin; AvgMax   = $totalAvgMax
        Steps    = $steps;       Count    = $steps.Count
    }
}

function Show-ImprovementEstimate {
    <#
    .SYNOPSIS  Shows the cumulative estimated improvement and compares against
               actual benchmark results if a baseline exists.
    #>
    param(
        [double]$BaselineAvg = 0,
        [double]$BaselineP1  = 0,
        [double]$ActualAvg   = 0,
        [double]$ActualP1    = 0
    )

    $est = Get-ImprovementEstimate
    if ($est.Count -eq 0) {
        Write-Info "No tracked improvement estimates available."
        return
    }

    Write-Blank
    Write-Host "  ┌──────────────────────────────────────────────────────────────────" -ForegroundColor Cyan
    Write-Host "  │  IMPROVEMENT ESTIMATE vs. ACTUAL RESULTS" -ForegroundColor Cyan
    Write-Host "  │" -ForegroundColor DarkGray
    Write-Host "  │  Applied steps with tracked estimates: $($est.Count)" -ForegroundColor DarkGray
    Write-Host "  │" -ForegroundColor DarkGray

    # Show per-step estimates
    foreach ($s in $est.Steps) {
        $e = $s.Estimate
        $conf = switch ($e.Confidence) { "HIGH" {"Green"} "MEDIUM" {"Yellow"} "LOW" {"DarkGray"} default {"White"} }
        $confIcon = switch ($e.Confidence) { "HIGH" {"$([char]0x2714)"} "MEDIUM" {"$([char]0x25B2)"} "LOW" {"?"} default {"?"} }
        $p1Range = if ($e.P1LowMin -eq $e.P1LowMax) { "$($e.P1LowMin)%" } else { "$($e.P1LowMin)-$($e.P1LowMax)%" }
        Write-Host "  │  $confIcon $($s.Key.PadRight(28)) 1% lows: +$p1Range" -ForegroundColor $conf
    }

    Write-Host "  │" -ForegroundColor DarkGray
    Write-Host "  │  CUMULATIVE ESTIMATE:" -ForegroundColor White
    Write-Host "  │  1% Lows:  +$($est.P1LowMin)% to +$($est.P1LowMax)%  improvement" -ForegroundColor Cyan
    if ($est.AvgMin -ne 0 -or $est.AvgMax -ne 0) {
        $avgMinSign = if ($est.AvgMin -ge 0) { "+" } else { "" }
        $avgMaxSign = if ($est.AvgMax -ge 0) { "+" } else { "" }
        Write-Host "  │  Avg FPS:  ${avgMinSign}$($est.AvgMin)% to ${avgMaxSign}$($est.AvgMax)%" -ForegroundColor DarkCyan
    }

    # Compare against actual benchmark if we have data
    if ($BaselineP1 -gt 0 -and $ActualP1 -gt 0) {
        $actualP1Pct = [math]::Round(($ActualP1 - $BaselineP1) / $BaselineP1 * 100, 1)
        $actualAvgPct = if ($BaselineAvg -gt 0) { [math]::Round(($ActualAvg - $BaselineAvg) / $BaselineAvg * 100, 1) } else { 0 }

        Write-Host "  │" -ForegroundColor DarkGray
        Write-Host "  │  ACTUAL BENCHMARK RESULT:" -ForegroundColor White
        Write-Host "  │  1% Lows:  $($BaselineP1.ToString('F1')) -> $($ActualP1.ToString('F1'))  ($(if($actualP1Pct -ge 0){'+'})$actualP1Pct%)" -ForegroundColor White
        if ($BaselineAvg -gt 0) {
            Write-Host "  │  Avg FPS:  $($BaselineAvg.ToString('F1')) -> $($ActualAvg.ToString('F1'))  ($(if($actualAvgPct -ge 0){'+'})$actualAvgPct%)" -ForegroundColor White
        }

        Write-Host "  │" -ForegroundColor DarkGray

        # Verdict
        if ($actualP1Pct -ge $est.P1LowMin -and $actualP1Pct -le $est.P1LowMax) {
            Write-Host "  │  $([char]0x2714) WITHIN EXPECTED RANGE" -ForegroundColor Green
            Write-Host "  │  Estimated +$($est.P1LowMin)-$($est.P1LowMax)%, got $(if($actualP1Pct -ge 0){'+'})$actualP1Pct%" -ForegroundColor Green
        } elseif ($actualP1Pct -gt $est.P1LowMax) {
            Write-Host "  │  $([char]0x2714) BETTER THAN EXPECTED" -ForegroundColor Green
            Write-Host "  │  Estimated +$($est.P1LowMin)-$($est.P1LowMax)%, got +$actualP1Pct% $([char]0x2014) great result!" -ForegroundColor Green
        } elseif ($actualP1Pct -ge 0 -and $actualP1Pct -lt $est.P1LowMin) {
            Write-Host "  │  $([char]0x25B2) BELOW ESTIMATE" -ForegroundColor Yellow
            Write-Host "  │  Estimated +$($est.P1LowMin)-$($est.P1LowMax)%, got +$actualP1Pct%" -ForegroundColor Yellow
            Write-Host "  │  Some steps may not apply to your setup. Run 3x to confirm." -ForegroundColor DarkGray
        } else {
            Write-Host "  │  $([char]0x2718) REGRESSION" -ForegroundColor Red
            Write-Host "  │  Estimated improvement, got $actualP1Pct% $([char]0x2014) investigate!" -ForegroundColor Red
            Write-Host "  │  Consider reverting recent changes: START.bat -> [7] Restore" -ForegroundColor DarkGray
        }
    } else {
        Write-Host "  │" -ForegroundColor DarkGray
        Write-Host "  │  Run a benchmark to compare estimate vs. actual result." -ForegroundColor DarkGray
        if ($BaselineP1 -gt 0) {
            $estMinFps = [math]::Round($BaselineP1 * (1 + $est.P1LowMin / 100), 1)
            $estMaxFps = [math]::Round($BaselineP1 * (1 + $est.P1LowMax / 100), 1)
            Write-Host "  │  Based on baseline ($($BaselineP1.ToString('F1')) 1% lows):" -ForegroundColor DarkGray
            Write-Host "  │  Expected after optimization: $estMinFps - $estMaxFps FPS (1% lows)" -ForegroundColor Cyan
        }
    }

    Write-Host "  │" -ForegroundColor DarkGray
    Write-Host "  │  NOTE: Estimates are ranges based on community benchmarks." -ForegroundColor DarkGray
    Write-Host "  │  Individual results vary by hardware. Always measure yourself." -ForegroundColor DarkGray
    Write-Host "  └──────────────────────────────────────────────────────────────────" -ForegroundColor DarkGray
}

# Backward-compatible wrapper
function Confirm-Risk($msg, $warning) {
    if (Test-YoloProfile) { return $true }
    Write-Blank
    Write-Warn $warning
    $r = Read-Host "  $msg [y/N]"
    return ($r -match "^[jJyY]$")
}
