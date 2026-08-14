# ==============================================================================
#  Setup-Profile.ps1  -  Step 1: Profile, Configuration, Resume
# ==============================================================================

# ── STEP 1 - PROFILE + CONFIGURATION ─────────────────────────────────────────
$fullDryRunRequested = (Get-Variable FullDryRunRequested -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:FullDryRunRequested
$SCRIPT:FullDryRun = [bool]$fullDryRunRequested
if ($fullDryRunRequested) { $SCRIPT:DryRun = $true }

Write-LogoBanner "frametime.cfg  ·  Phase 1 / 3"

if (-not $fullDryRunRequested) {
Write-Host @"

  DISCLAIMER: This suite applies Windows and CS2 configuration operations
  with mixed evidence and recovery coverage. Review docs\evidence.md and
  docs\backup-restore.md before approving a live run.
  Supported reversible settings are backed up before live changes.
  Some operations require separate or manual recovery.

"@ -ForegroundColor DarkRed

Write-Host "  HOW SHOULD THE SUITE OPERATE?" -ForegroundColor White
Write-Host @"

  ┌──────────────────────────────────────────────────────────────────
  │
  │  [1]  SAFE - Lower-impact project scope
  │       Automatically runs T1 and eligible T2 operations within the
  │       internal SAFE threshold. The label is not a universal guarantee.
  │       Tier filtering does not cancel the Phase 2/3 reboot handoff.
  │
  │       Includes:
  │       $([char]0x2714) Shader cache clear, fullscreen optimization, power plan
  │       $([char]0x2714) Mouse acceleration off, Game DVR off, overlays off
  │       $([char]0x2714) optimization.cfg bootstrap, GPU preference, timer resolution
  │       $([char]0x2714) Dual-channel RAM check, XMP/EXPO check, benchmarks
  │       $([char]0x2718) T2 MODERATE and T3 operations
  │
  │       Completing Phase 1 still offers Safe Mode driver cleanup and
  │       registers the Phase 3 handoff. Review both reboot prompts.
  │
  │  [2]  RECOMMENDED - Prompt for moderate changes
  │       T1 operations run automatically. Eligible T2 operations are
  │       explained and prompted.
  │       Skips: experimental operations (T3), aggressive changes.
  │
  │       Includes everything in SAFE, plus (prompted):
  │       $([char]0x25B2) HAGS, pagefile, debloat, NIC tweaks, MSI interrupts
  │       $([char]0x2718) T3 timer-policy and service-disabling operations
  │
  │       T3 and higher-risk operations remain excluded.
  │
  │  [3]  COMPETITIVE - Broader operation scope
  │       Offers eligible operations including experimental T3 items.
  │       Eligible T2 and T3 steps are prompted; T1 steps run automatically.
  │       Only skips changes with security implications.
  │
  │       Includes everything in RECOMMENDED, plus (prompted):
  │       $([char]0x25C6) Timer tweaks (bcdedit), Game Mode, visual effects
  │       $([char]0x25C6) SysMain/Search disable, NIC affinity, NVIDIA profile
  │       $([char]0x25C6) NVIDIA profile changes (if applicable)
  │       $([char]0x2718) Windows Update disable (security risk)
  │
  │       Review evidence and recovery limits for each prompt.
  │
  │  [4]  CUSTOM - Prompt for each tiered operation
  │       Each tiered operation is shown with a detail card:
  │       risk level, expected effect, side effects, undo.
  │       Phase setup and reboot handoffs remain workflow-controlled.
  │
  │       Use when reviewing each operation individually.
  │
  │  [5]  YOLO - Automatic broader scope
  │       Same scope as COMPETITIVE (all tiers, up to AGGRESSIVE)
  │       but every step auto-executes. No prompts whatsoever.
  │       GPU auto-detected, FPS defaults to 0, DNS to Cloudflare.
  │
  │       This mode removes per-step confirmation and increases risk.
  │
  │  [D]  DRY-RUN - Preview only
  │       Shows what a selected scope would change without
  │       modifying anything. Review before committing.
  │       SAFE, RECOMMENDED, COMPETITIVE, CUSTOM, or full coverage.
  │
  └──────────────────────────────────────────────────────────────────
"@ -ForegroundColor White
} else {
    $previewGpuName = switch ([string]$SCRIPT:RequestedDryRunGpu) {
        "1" { "NVIDIA RTX 5000" }
        "2" { "Other NVIDIA" }
        "3" { "AMD Radeon" }
        "4" { "Intel Arc" }
    }
    Write-Host @"

  FULL DRY-RUN
  ────────────────────────────────────────────────────────────────
  Scope:   every tier and all three phases
  GPU:     [$($SCRIPT:RequestedDryRunGpu)] $previewGpuName
  Boot:    Safe Mode and Normal Mode transitions are simulated
  Output:  console only; no persistent preview log is created
  Safety:  no system changes, downloads, handoffs, or reboots

"@ -ForegroundColor Magenta
}

# Check state.json for pre-selected YOLO profile (e.g. from GUI). A saved
# preview must never be promoted to a live YOLO run.
$_preYolo = $false
if (-not $fullDryRunRequested) {
    try {
        if (Test-Path -LiteralPath $CFG_StateFile -ErrorAction Stop) {
            $_ps = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json
            if ($_ps.profile -eq "YOLO" -and $_ps.mode -ne "DRY-RUN" -and (Test-Administrator)) {
                $_preYolo = $true
            }
        }
    } catch {
        Write-DebugLog "Could not inspect saved profile selection: $($_.Exception.Message)"
    }
}

if ($fullDryRunRequested) {
    $pi = "4"
    $SCRIPT:DryRun = $true
    Write-Host "  Preview starting for the selected GPU branch. No input is required." -ForegroundColor Magenta
} elseif ($_preYolo) {
    $pi = "5"
    $SCRIPT:DryRun = $false
    Write-Host "  YOLO profile detected from settings - skipping menu." -ForegroundColor Red
} else {
    do { $pi = Read-Host "  Profile [1/2/3/4/5/D]" } while ($pi -notin @("1","2","3","4","5","d","D"))
    $SCRIPT:DryRun = ($pi -in @("d","D"))
}

if ($SCRIPT:DryRun -and -not $fullDryRunRequested) {
    Write-Host "`n  DRY-RUN: Which profile scope should all three phases preview?" -ForegroundColor Magenta
    Write-Host "  [1] SAFE  [2] RECOMMENDED  [3] COMPETITIVE  [4] CUSTOM" -ForegroundColor DarkGray
    Write-Host "  [5] FULL COVERAGE - every tier for the selected GPU branch" -ForegroundColor Magenta
    do { $pi = Read-Host "  [1/2/3/4/5]" } while ($pi -notin @("1","2","3","4","5"))
    if ($pi -eq "5") {
        $SCRIPT:FullDryRun = $true
        $pi = "4"
    }
}

$SCRIPT:Profile = switch ($pi) { "1" {"SAFE"} "2" {"RECOMMENDED"} "3" {"COMPETITIVE"} "4" {"CUSTOM"} "5" {"YOLO"} }
# Mode is derived from profile (kept for backward-compat with Load-State / banner)
$SCRIPT:Mode = Get-ModeForProfile -Profile $SCRIPT:Profile -DryRun:$SCRIPT:DryRun

# Preview mode must be selectable before the administrator and persistent
# work-directory gates. A DRY-RUN only reads host state and writes to the
# console; real runs retain the existing elevation and ACL requirements.
if (-not $SCRIPT:DryRun) {
    Assert-Administrator
    Assert-NoLegacyPhaseHandoff
    Ensure-SecureWorkDir -Path $CFG_WorkDir
    Ensure-Dir $CFG_LogDir
}

# Log level - simplified for profiles
if ($SCRIPT:FullDryRun) {
    $SCRIPT:LogLevel = "VERBOSE"
} elseif ($SCRIPT:Profile -eq "CUSTOM") {
    Write-Host @"

  LOG LEVEL:
  [1]  MINIMAL   Errors, warnings and successes only
  [2]  NORMAL    Standard  (recommended)
  [3]  VERBOSE   Everything incl. registry values and download details
"@ -ForegroundColor White
    do { $li = Read-Host "  [1/2/3]" } while ($li -notin @("1","2","3"))
    $SCRIPT:LogLevel = switch ($li) { "1" {"MINIMAL"} "2" {"NORMAL"} "3" {"VERBOSE"} }
} else {
    $SCRIPT:LogLevel = "NORMAL"
}

if (-not $SCRIPT:DryRun) { Initialize-Log }
Write-Banner 1 3 "Optimization · Downloads · Safe Mode"

$startStep = if ($SCRIPT:DryRun) { 1 } else { Show-ResumePrompt $PHASE $TOTAL_STEPS }
if ($startStep -gt $TOTAL_STEPS) {
    Write-Info "Phase 1 already completed."
    Write-Blank
    $r = if (Test-YoloProfile) { "y" } else { Read-Host "  Continue with Phase 2 (Safe Mode GPU driver clean)? [y/N]" }
    if ($r -match "^[jJyY]$") {
        $phase2Transaction = Enable-Phase2SafeModeTransaction -SourceRoot $ScriptRoot -DestinationRoot $CFG_WorkDir -StatePath $CFG_StateFile -Why "Completed Phase 1 resume"
        if (-not $phase2Transaction.Applied) {
            Write-Host ""
            Write-Host "  ╔══════════════════════════════════════════════════════════╗" -ForegroundColor Red
            Write-Host "  ║  Could not prepare the Safe Mode handoff.               ║" -ForegroundColor Red
            Write-Host "  ║                                                         ║" -ForegroundColor Red
            Write-Host "  ║  Do NOT reboot until the transaction succeeds.          ║" -ForegroundColor Red
            Write-Host "  ╚══════════════════════════════════════════════════════════╝" -ForegroundColor Red
            Write-Host "  $($phase2Transaction.Message)" -ForegroundColor DarkGray
            if (-not (Test-YoloProfile)) { Read-Host "`n  Press Enter to return to menu" }
            exit 0
        }
        # Show warning countdown, then restart
        $countdownSec = 10
        Write-Host ""
        Write-Host "  ╔══════════════════════════════════════════════════════════════════╗" -ForegroundColor Yellow
        Write-Host "  ║              RESTARTING INTO SAFE MODE                           ║" -ForegroundColor Yellow
        Write-Host "  ╠══════════════════════════════════════════════════════════════════╣" -ForegroundColor Yellow
        Write-Host "  ║                                                                  ║" -ForegroundColor Yellow
        Write-Host "  ║  If Safe Mode gets stuck or you need to return to Normal Mode:   ║" -ForegroundColor Yellow
        Write-Host "  ║                                                                  ║" -ForegroundColor Yellow
        Write-Host "  ║    1. In Safe Mode, open an admin Command Prompt (cmd.exe)       ║" -ForegroundColor White
        Write-Host "  ║    2. Run:  bcdedit /deletevalue safeboot                        ║" -ForegroundColor Cyan
        Write-Host "  ║    3. Run:  shutdown /r /t 0                                     ║" -ForegroundColor Cyan
        Write-Host "  ║                                                                  ║" -ForegroundColor Yellow
        Write-Host "  ║  Or: Hold SHIFT + click Restart in the Start menu to access      ║" -ForegroundColor White
        Write-Host "  ║  Windows Recovery, then choose Normal Startup.                   ║" -ForegroundColor White
        Write-Host "  ║                                                                  ║" -ForegroundColor Yellow
        Write-Host "  ║  Phase 2 runs automatically in Safe Mode and removes the         ║" -ForegroundColor White
        Write-Host "  ║  Safe Mode flag as its very first action - next boot after       ║" -ForegroundColor White
        Write-Host "  ║  Phase 2 will be Normal Mode again.                              ║" -ForegroundColor White
        Write-Host "  ║                                                                  ║" -ForegroundColor Yellow
        Write-Host "  ╚══════════════════════════════════════════════════════════════════╝" -ForegroundColor Yellow
        Write-Host ""
        Write-Host "  Press Ctrl+C to cancel." -ForegroundColor DarkGray
        Write-Host ""
        for ($i = $countdownSec; $i -ge 1; $i--) {
            Write-Host "`r  Restarting in $i... " -NoNewline -ForegroundColor Yellow
            Start-Sleep 1
        }
        Write-Host "`r  Restarting now...     " -ForegroundColor Red
        shutdown /r /t 0 /f
    }
    exit 0
}

# Initialize backup system
if (-not $SCRIPT:DryRun) { Initialize-Backup }

# Detect and warn about compatibility limitations (ARM64, CLM, Server, PS7)
Test-SystemCompatibility

# Restore Point
if ($startStep -eq 1 -and -not $SCRIPT:DryRun) {
    Write-Blank
    if ($SCRIPT:Profile -eq "SAFE") {
        Write-Info "SAFE profile: only operations within the project's lower-impact threshold will run."
        Write-Info "Supported reversible settings are backed up before live changes."
    } else {
        Write-Warn "Create a System Restore Point NOW!"
        Write-Info "Windows Search -> 'restore point' -> C: -> Create"
        Write-Info "The suite backs up supported reversible settings before live changes."
        Write-Info "See docs\backup-restore.md, then use Restore from an authenticated release."
        if (-not (Confirm-Risk "Restore point created. Continue?" "No rollback possible without a restore point!")) {
            exit 0
        }
    }
}

# Load or create state
$state = $null
try {
    if (Test-Path -LiteralPath $CFG_StateFile -ErrorAction Stop) {
        $state = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    }
} catch {
    Write-Warn "Previous state file could not be read (corrupt, empty, or inaccessible). Starting fresh."
    $state = $null
}

if (-not $state -or $startStep -eq 1) {
    Write-Section "Step 1 - Configuration"

    Write-Host "`n  GPU:" -ForegroundColor White
    Write-Host "  [1] NVIDIA RTX 5000  [2] Other NVIDIA  [3] AMD  [4] Intel Arc"
    if ($fullDryRunRequested) {
        $gpuInput = [string]$SCRIPT:RequestedDryRunGpu
        Write-Info "FULL DRY-RUN: GPU branch [$gpuInput] selected from -DryRunGpu."
    } elseif (Test-YoloProfile) {
        # Auto-detect GPU via WMI
        $gpuInput = "2"  # Default: other NVIDIA
        try {
            $gpu = Get-CimInstance Win32_VideoController -ErrorAction Stop | Select-Object -First 1
            if ($gpu.Name -match "AMD|Radeon")                      { $gpuInput = "3" }
            elseif ($gpu.Name -match "Intel.*Arc|Intel.*Graphics")   { $gpuInput = "4" }
            elseif ($gpu.Name -match "RTX\s*5\d{3}")                { $gpuInput = "1" }
        } catch { Write-DebugLog "YOLO GPU auto-detect failed, defaulting to NVIDIA: $_" }
        $gpuName = switch ($gpuInput) { "1" {"NVIDIA RTX 5000"} "2" {"Other NVIDIA"} "3" {"AMD"} "4" {"Intel Arc"} }
        Write-Info "YOLO: GPU auto-detected as [$gpuInput] $gpuName"
    } else {
        do { $gpuInput = Read-Host "  [1/2/3/4]" } while ($gpuInput -notin @("1","2","3","4"))
    }

    Write-Blank
    Write-Host "  AVG FPS IN CS2  (0 = calculate later with FpsCap-Calculator):" -ForegroundColor White
    if ($fullDryRunRequested) {
        $avgFps = 0
        Write-Info "FULL DRY-RUN: FPS defaults to 0 (benchmark capture will be simulated)."
    } elseif (Test-YoloProfile) {
        $avgFps = 0
        Write-Info "YOLO: FPS defaults to 0 (calculate later)"
    } else {
        do {
            $f = Read-Host "  Avg FPS"; $avgFps = 0
            $ok2 = [int]::TryParse($f,[ref]$avgFps) -and $avgFps -ge 0
            if (-not $ok2) { Write-Warn "Enter >= 0." }
        } while (-not $ok2)
    }
    $fpsCap = if ($avgFps -gt 0) { Calculate-FpsCap $avgFps } else { 0 }

    $state = @{
        mode = $SCRIPT:Mode; logLevel = $SCRIPT:LogLevel
        profile = $SCRIPT:Profile
        fpsCap = $fpsCap; avgFps = $avgFps
        gpuInput = $gpuInput; pagefileMB = 0
        workDir = $CFG_WorkDir; scriptRoot = $ScriptRoot
    }
    if (-not $SCRIPT:DryRun) { Save-SuiteState -State $state }
    Complete-Step $PHASE 1 "Configuration"
} else {
    # Restore saved config but honor the fresh profile/DRY-RUN choice made above
    $SCRIPT:LogLevel = if ($state.logLevel) { $state.logLevel } else { "NORMAL" }
    if ($SCRIPT:Profile -ne "CUSTOM") { $SCRIPT:LogLevel = "NORMAL" }
    # Profile and Mode were already set from user input at lines 86-103 - keep them
    # Only fall back to state values if user chose the same profile
    $fpsCap  = $state.fpsCap
    $avgFps    = $state.avgFps;   $gpuInput = $state.gpuInput
    # Update state file with the fresh profile choice
    $state.mode     = $SCRIPT:Mode
    $state.profile  = $SCRIPT:Profile
    if (-not $SCRIPT:DryRun) { Save-SuiteState -State $state }
    Write-Info "Configuration loaded from previous session (Profile: $($SCRIPT:Profile)$(if($SCRIPT:DryRun){' [DRY-RUN]'}))."
}
