# ==============================================================================
#  helpers/tier-system.ps1  -  Profile & Tier-Aware Step Execution
# ==============================================================================
#
#  PROFILES (user-facing):
#    SAFE          Lower-impact project scope. Eligible steps auto-applied.
#    RECOMMENDED   Safe + moderate tweaks. Moderate steps prompted.
#    COMPETITIVE   Includes T3 experimental operations. All are prompted.
#    CUSTOM        Full detail card for every step. Nothing auto.
#    YOLO          Eligible steps through AGGRESSIVE auto-execute without prompts.
#
#  TIER SYSTEM (internal):
#    T1  Baseline project operation.
#    T2  Setup-dependent or situational operation.
#    T3  Experimental or weakly evidenced operation.
#
#  RISK LEVELS:
#    SAFE        Read-only or lower-impact project category
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
#  Scoped DRY-RUN exposes SAFE through CUSTOM; Full DRY-RUN forces CUSTOM.

if (-not (Get-Variable -Name RiskOrder -Scope Script -ErrorAction SilentlyContinue)) { $SCRIPT:RiskOrder = @{ "SAFE"=1; "MODERATE"=2; "AGGRESSIVE"=3; "CRITICAL"=4 } }
$SCRIPT:TieredStepRunModePolicy = @{
    "SAFE"        = @{ "1" = "Auto"; "2:SAFE" = "Auto"; default = "Skip" }
    "RECOMMENDED" = @{ "1" = "Auto"; "2" = "Prompt"; default = "Skip" }
    "COMPETITIVE" = @{ "1" = "Auto"; default = "Prompt" }
    "CUSTOM"      = @{ default = "Prompt" }
    "YOLO"        = @{ default = "Auto" }
    default        = @{ "1" = "Auto"; default = "Prompt" }
}
$SCRIPT:TieredStepPromptPolicy = @{
        "RECOMMENDED" = @{ default = @{
            Heading = "  [T2{0}] Do you want to run this step?"; Color = "Yellow"
            ShowImprovement = $true; Prompt = "  {0} - run? [y/N]"
            AcceptPattern = "^[jJyY]$"; AcceptOnMatch = $true
        } }
        "COMPETITIVE" = @{
            "2" = @{
                Heading = "  [T2{0}] Do you want to run this step?"; Color = "Yellow"
                ShowImprovement = $true; Prompt = "  {0} - run? [y/N]"
                AcceptPattern = "^[jJyY]$"; AcceptOnMatch = $true
            }
            default = @{
                Heading = "  $([char]0x25C6) [T3{0}] Experimental operation with weak or incomplete evidence."; Color = "DarkCyan"
                ShowImprovement = $true; Prompt = "  {0} - run anyway? [y/N]"
                AcceptPattern = "^[jJyY]$"; AcceptOnMatch = $true
            }
        }
        "CUSTOM" = @{
            "1" = @{
                Heading = "  [T1{0}] Baseline operation - apply this step?"; Color = "Green"
                ShowImprovement = $false; Prompt = "  {0} [Y/n]"
                AcceptPattern = "^[nN]$"; AcceptOnMatch = $false
            }
            "2" = @{
                Heading = "  [T2{0}] Setup-dependent - apply?"; Color = "Yellow"
                ShowImprovement = $false; Prompt = "  {0} [y/N]"
                AcceptPattern = "^[jJyY]$"; AcceptOnMatch = $true
            }
            "3" = @{
                Heading = "  [T3{0}] Experimental operation - apply?"; Color = "DarkCyan"
                ShowImprovement = $false; Prompt = "  {0} [y/N]"
                AcceptPattern = "^[jJyY]$"; AcceptOnMatch = $true
            }
            default = @{
                Heading = "  [T?{0}] Unknown tier - apply?"; Color = "White"
                ShowImprovement = $false; Prompt = "  {0} [y/N]"
                AcceptPattern = "^[jJyY]$"; AcceptOnMatch = $true
            }
        }
        default = @{ default = @{
            Heading = ""; Color = ""
            ShowImprovement = $false; Prompt = "  {0} - run? [y/N]"
            AcceptPattern = "^[jJyY]$"; AcceptOnMatch = $true
        } }
}
$SCRIPT:TieredStepStatusHandlers = @{
    "Auto:SAFE:1" = { param($StepTitle, $StepTier, $StepRisk) Write-DebugLog "SAFE/T1: Auto-Execute '$StepTitle'" }
    "Auto:SAFE:2" = { param($StepTitle, $StepTier, $StepRisk) Write-DebugLog "SAFE/T2(SAFE): Auto-Execute '$StepTitle'" }
    "Auto:RECOMMENDED:1" = { param($StepTitle, $StepTier, $StepRisk) Write-DebugLog "RECOMMENDED/T1: Auto-Execute '$StepTitle'" }
    "Auto:COMPETITIVE:1" = { param($StepTitle, $StepTier, $StepRisk) Write-DebugLog "COMPETITIVE/T1: Auto-Execute '$StepTitle'" }
    "Auto:YOLO:*" = { param($StepTitle, $StepTier, $StepRisk) Write-DebugLog "YOLO/T${StepTier}: Auto-Execute '$StepTitle'" }
    "Skip:SAFE:*" = { param($StepTitle, $StepTier, $StepRisk) Write-DebugLog "SAFE profile: Skipping '$StepTitle' (Tier=$StepTier, Risk=$StepRisk)" }
    "Skip:RECOMMENDED:*" = { param($StepTitle, $StepTier, $StepRisk) Write-ConsoleLine "  $([char]0x25C6) [T3] Skipped in RECOMMENDED profile (weak or incomplete evidence)." -ForegroundColor DarkCyan }
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
        Write-Warn "Unknown risk level '$StepRisk' - treating as blocked for safety."
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
        "SAFE"       { "SAFE - lower-impact project category; review required" }
        "MODERATE"   { "MODERATE - persistent change; check recovery coverage" }
        "AGGRESSIVE" { "AGGRESSIVE - restart or careful recovery may be required" }
        "CRITICAL"   { "CRITICAL - may affect system security or data" }
        default      { $Risk }
    }
    $riskIcon = switch ($Risk) {
        "SAFE"       { [char]0x2714 }   # check
        "MODERATE"   { [char]0x25B2 }   # triangle
        "AGGRESSIVE" { [char]0x25C6 }   # diamond
        "CRITICAL"   { [char]0x2718 }   # cross
        default      { "?" }
    }

    Write-ConsoleLine "  ┌──────────────────────────────────────────────────────────────────" -ForegroundColor DarkGray
    Write-ConsoleLine "  │  $Title" -ForegroundColor White
    Write-ConsoleLine "  │" -ForegroundColor DarkGray
    if ($Why)         { Write-ConsoleLine "  │  Why:          $Why" -ForegroundColor DarkGray }
    if ($Risk)        { Write-ConsoleLine "  │  Risk:         $riskIcon $riskLabel" -ForegroundColor $riskColor }
    if ($Depth)       { Write-ConsoleLine "  │  Modifies:     $Depth" -ForegroundColor DarkGray }
    if ($Improvement) { Write-ConsoleLine "  │  Expected:     $Improvement" -ForegroundColor Cyan }
    if ($SideEffects) { Write-ConsoleLine "  │  Side effects: $SideEffects" -ForegroundColor DarkYellow }
    if ($Evidence)    { Write-ConsoleLine "  │  Evidence:     $Evidence" -ForegroundColor DarkGray }
    if ($Caveat)      { Write-ConsoleLine "  │  Caveat:       $Caveat" -ForegroundColor DarkYellow }
    if ($Undo)        { Write-ConsoleLine "  │  Undo:         $Undo" -ForegroundColor DarkGray }
    Write-ConsoleLine "  └──────────────────────────────────────────────────────────────────" -ForegroundColor DarkGray
}

function Resolve-TieredStepRunMode {
    <# Returns the non-DRY-RUN policy outcome for a profile, tier, and risk. #>
    [OutputType([string])]
    param(
        [string] $SuiteProfile,
        [int]    $Tier,
        [string] $Risk = ""
    )

    $normalizedProfile = if ($SuiteProfile) { $SuiteProfile.ToUpperInvariant() } else { "" }
    $policy = $SCRIPT:TieredStepRunModePolicy[$normalizedProfile]
    if ($null -eq $policy) {
        if ($Tier -lt 1) { return "Skip" }
        $policy = $SCRIPT:TieredStepRunModePolicy.default
    }

    $riskKey = if ($Risk) { "${Tier}:$($Risk.ToUpperInvariant())" } else { $null }
    if ($riskKey -and $policy.ContainsKey($riskKey)) { return $policy[$riskKey] }
    if ($policy.ContainsKey("$Tier")) { return $policy["$Tier"] }
    return $policy.default
}

function Test-TieredStepPreviewSkipped {
    <# Dry-run filtering differs from live consent policy for SAFE T2 steps without a risk label. #>
    [OutputType([bool])]
    param(
        [string] $SuiteProfile,
        [int]    $Tier,
        [string] $Risk = ""
    )

    switch ($SuiteProfile) {
        "SAFE" {
            return ($Tier -ge 3 -or ($Tier -eq 2 -and $Risk -notin @("SAFE", "", $null)))
        }
        "RECOMMENDED" {
            return ($Tier -ge 3 -or ($Tier -eq 2 -and $Risk -and -not (Test-RiskAllowed $Risk)))
        }
        default { return $false }
    }
}

function Request-TieredStepRun {
    <# Displays the existing profile-specific consent prompt and returns its answer. #>
    param(
        [string] $SuiteProfile,
        [int]    $Tier,
        [string] $Title,
        [string] $Risk = "",
        [string] $Improvement = ""
    )

    $normalizedProfile = if ($SuiteProfile) { $SuiteProfile.ToUpperInvariant() } else { "" }
    $profilePolicy = $SCRIPT:TieredStepPromptPolicy[$normalizedProfile]
    if ($null -eq $profilePolicy) { $profilePolicy = $SCRIPT:TieredStepPromptPolicy.default }
    $config = $profilePolicy["$Tier"]
    if ($null -eq $config) { $config = $profilePolicy.default }

    $riskTag = if ($Risk) { " [$Risk]" } else { "" }
    if ($config.Heading) {
        Write-Blank
        Write-ConsoleLine ($config.Heading -f $riskTag) -ForegroundColor $config.Color
        if ($config.ShowImprovement -and $Improvement) {
            Write-ConsoleLine "       Expected: $Improvement" -ForegroundColor Cyan
        }
        Write-Blank
    }

    $response = Read-Host ($config.Prompt -f $Title)
    return (($response -match $config.AcceptPattern) -eq $config.AcceptOnMatch)
}

function Write-TieredStepRunModeStatus {
    <# Preserves profile-specific messages for automatic and skipped steps. #>
    param(
        [string] $SuiteProfile,
        [int]    $Tier,
        [string] $Risk,
        [string] $Title,
        [ValidateSet("Auto", "Prompt", "Skip")]
        [string] $RunMode
    )

    $handler = $SCRIPT:TieredStepStatusHandlers["$($RunMode):$($SuiteProfile):$Tier"]
    if ($null -eq $handler) { $handler = $SCRIPT:TieredStepStatusHandlers["$($RunMode):$($SuiteProfile):*"] }
    if ($handler) { & $handler $Title $Tier $Risk }
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
        [scriptblock] $Action,
        [scriptblock] $SkipAction = $null
    )

    # Track current step for automatic backup integration
    $SCRIPT:CurrentStepTitle = $Title

    Write-Blank
    Write-TierBadge $Tier $Title

    # ── Profile risk filter (T2/T3 only - T1 always runs) ───────────
    if ($Tier -gt 1 -and $Risk -and -not (Test-RiskAllowed $Risk)) {
        $max = Get-ProfileMaxRisk
        Write-ConsoleLine "  $([char]0x25CB) [SKIP] Exceeds $($SCRIPT:Profile) profile ($Risk > $max threshold)" -ForegroundColor DarkGray
        if ($Improvement) { Write-ConsoleLine "         Would have: $Improvement" -ForegroundColor DarkGray }
        if ($Undo)        { Write-ConsoleLine "         Available in COMPETITIVE or CUSTOM profile" -ForegroundColor DarkGray }
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

    # Risk filtering above intentionally precedes this pure profile/tier policy.
    $runMode = Resolve-TieredStepRunMode -SuiteProfile $SCRIPT:Profile -Tier $Tier -Risk $Risk

    # ── DRY-RUN modifier ────────────────────────────────────────────
    if ($SCRIPT:DryRun) {
        $wouldSkip = Test-TieredStepPreviewSkipped -SuiteProfile $SCRIPT:Profile -Tier $Tier -Risk $Risk
        if ($wouldSkip) {
            Write-ConsoleLine "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Would SKIP: $Title (filtered by $($SCRIPT:Profile) profile)" -ForegroundColor DarkGray
            $SCRIPT:CurrentStepTitle = $null
            return $false
        }
        Write-ConsoleLine "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Would execute: $Title" -ForegroundColor Magenta
        if ($Depth)       { Write-ConsoleLine "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Modifies: $Depth" -ForegroundColor Magenta }
        if ($Improvement) { Write-ConsoleLine "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Expected: $Improvement" -ForegroundColor Magenta }
        Write-DebugLog "DRY-RUN: '$Title' - preview only, no changes applied"
        # Run the action but Set-RegistryValue/Set-BootConfig intercept writes
        try {
            & $Action
        } catch {
            Add-DryRunPreviewIssue
            Write-Warn "Step '$Title' preview issue (DRY-RUN): $_"
        }
        # Defensive flush - should be a no-op because Backup-* functions self-guard in DRY-RUN.
        try { Flush-BackupBuffer } catch { Write-DebugLog "Flush-BackupBuffer failed after DRY-RUN '$Title': $_" }
        $SCRIPT:CurrentStepTitle = $null
        return $false
    }

    # ── Decide whether to run based on profile + tier ───────────────
    Write-TieredStepRunModeStatus -SuiteProfile $SCRIPT:Profile -Tier $Tier -Risk $Risk -Title $Title -RunMode $runMode
    $run = switch ($runMode) {
        "Auto" { $true }
        "Prompt" { Request-TieredStepRun -SuiteProfile $SCRIPT:Profile -Tier $Tier -Title $Title -Risk $Risk -Improvement $Improvement }
        default { $false }
    }

    # ── Execute or skip ─────────────────────────────────────────────
    $actionOk = $true
    $actionOutcome = $null
    if ($run) {
        Write-DebugLog "Executing: '$Title'"
        $SCRIPT:CurrentTierStepOutcome = $null
        try { & $Action } catch {
            Write-Err "Step '$Title' failed: $_"
            Write-ConsoleLine "  $([char]0x2139) What to do: This step did not complete; some earlier changes in it may already be applied." -ForegroundColor Cyan
            Write-ConsoleLine "  $([char]0x2139) Backups were retained. Retry through an authenticated Restore/Rollback release before continuing." -ForegroundColor Cyan
            $actionOk = $false
        }
        $actionOutcome = $SCRIPT:CurrentTierStepOutcome
        $SCRIPT:CurrentTierStepOutcome = $null
        # Update phase counters
        if (-not $actionOk) {
            Add-PhaseFailed
        } elseif ($actionOutcome -eq 'Skipped') {
            Add-PhaseSkipped
        } else {
            Add-PhaseApplied
        }
    } else {
        Write-DebugLog "Skipped: '$Title'"
        Add-PhaseSkipped
        if ($SkipAction) { & $SkipAction }
    }

    # Flush any pending backup entries to disk in one I/O pass.
    # This is the primary flush point - backup functions buffer entries in memory
    # during the step's action, and we persist them here once the step finishes.
    try { Flush-BackupBuffer } catch { Write-Warn "Backup entries could not be saved to disk after '$Title': $_  (entries retained in memory for next flush)" }

    $SCRIPT:CurrentStepTitle = $null
    return ($run -and $actionOk -and $actionOutcome -ne 'Skipped')
}

# Backward-compatible wrapper
function Confirm-Risk($msg, $warning) {
    if (Test-YoloProfile) { return $true }
    Write-Blank
    Write-Warn $warning
    $r = Read-Host "  $msg [y/N]"
    return ($r -match "^[jJyY]$")
}
