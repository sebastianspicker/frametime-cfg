# ==============================================================================
#  Setup-Profile.ps1  —  Step 1: Profile, Configuration, Resume
# ==============================================================================

# ── STEP 1 — PROFILE + CONFIGURATION ─────────────────────────────────────────
Write-LogoBanner "CS2 Optimization Suite  ·  Phase 1 / 3"

Write-Host @"

  DISCLAIMER: This suite provides optimization suggestions based on
  community research and benchmarks. We take NO RESPONSIBILITY for any
  damage whatsoever. Use entirely at your own risk.
  All changes are backed up automatically and can be rolled back.

"@ -ForegroundColor DarkRed

Write-Host "  HOW SHOULD THE SUITE OPERATE?" -ForegroundColor White
Write-Host @"

  ┌──────────────────────────────────────────────────────────────────
  │
  │  [1]  SAFE — Auto-apply safe tweaks only
  │       Applies all proven, universally harmless optimizations
  │       automatically. No prompts, no risky changes.
  │       Skips: driver changes, boot config, service tweaks.
  │
  │       Includes:
  │       $([char]0x2714) Shader cache clear, fullscreen optimization, power plan
  │       $([char]0x2714) Mouse acceleration off, Game DVR off, overlays off
  │       $([char]0x2714) Autoexec.cfg, GPU preference, timer resolution
  │       $([char]0x2714) Dual-channel RAM check, XMP/EXPO check, benchmarks
  │       $([char]0x2718) Driver rollback, debloat, NIC tweaks, boot config
  │
  │       Best for: First-time users, work PCs, laptops
  │
  │  [2]  RECOMMENDED — Ask before moderate changes
  │       Safe tweaks auto-applied. Moderate tweaks explained and
  │       prompted — you decide each one with full context.
  │       Skips: community tips (T3), aggressive changes.
  │
  │       Includes everything in SAFE, plus (prompted):
  │       $([char]0x25B2) HAGS, pagefile, debloat, NIC tweaks, MSI interrupts
  │       $([char]0x2718) Boot config, driver rollback, service disabling
  │
  │       Best for: Most desktop gamers  (recommended)
  │
  │  [3]  COMPETITIVE — Maximum performance
  │       Every optimization offered including community tips.
  │       Each step prompted with risk assessment.
  │       Only skips changes with security implications.
  │
  │       Includes everything in RECOMMENDED, plus (prompted):
  │       $([char]0x25C6) Timer tweaks (bcdedit), Game Mode, visual effects
  │       $([char]0x25C6) SysMain/Search disable, NIC affinity, NVIDIA profile
  │       $([char]0x25C6) Driver rollback (if applicable)
  │       $([char]0x2718) Windows Update disable (security risk)
  │
  │       Best for: Experienced users, tournament prep
  │
  │  [4]  CUSTOM — Full control (expert)
  │       Every single step shown with full detail card:
  │       risk level, expected improvement, side effects, undo.
  │       Nothing is automatic — you decide everything.
  │
  │       Best for: Users who want complete informed control
  │
  │  [5]  YOLO — Everything, zero interaction
  │       Same scope as COMPETITIVE (all tiers, up to AGGRESSIVE)
  │       but every step auto-executes. No prompts whatsoever.
  │       GPU auto-detected, FPS defaults to 0, DNS to Cloudflare.
  │
  │       Best for: Experienced users who want maximum speed
  │
  │  [D]  DRY-RUN — Preview only
  │       Shows what any profile would change without modifying
  │       anything. Perfect for reviewing before committing.
  │       Select a profile afterwards to define scope.
  │
  └──────────────────────────────────────────────────────────────────
"@ -ForegroundColor White

# Check state.json for pre-selected YOLO profile (e.g. from GUI)
$_preYolo = $false
if (Test-Path $CFG_StateFile) {
    try {
        $_ps = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json
        if ($_ps.profile -eq "YOLO") { $_preYolo = $true }
    } catch {}
}

if ($_preYolo) {
    $pi = "5"
    Write-Host "  YOLO profile detected from settings — skipping menu." -ForegroundColor Red
} else {
    do { $pi = Read-Host "  Profile [1/2/3/4/5/D]" } while ($pi -notin @("1","2","3","4","5","d","D"))
}
$SCRIPT:DryRun = ($pi -in @("d","D"))

if ($SCRIPT:DryRun) {
    Write-Host "`n  DRY-RUN: Which profile scope to preview?" -ForegroundColor Magenta
    Write-Host "  [1] SAFE  [2] RECOMMENDED  [3] COMPETITIVE  [4] CUSTOM" -ForegroundColor DarkGray
    do { $pi = Read-Host "  [1/2/3/4]" } while ($pi -notin @("1","2","3","4"))
}

$SCRIPT:Profile = switch ($pi) { "1" {"SAFE"} "2" {"RECOMMENDED"} "3" {"COMPETITIVE"} "4" {"CUSTOM"} "5" {"YOLO"} }
# Mode is derived from profile (kept for backward-compat with Load-State / banner)
$SCRIPT:Mode = switch ($SCRIPT:Profile) {
    "SAFE"        { "AUTO" }
    "RECOMMENDED" { "AUTO" }
    "COMPETITIVE" { "CONTROL" }
    "CUSTOM"      { "INFORMED" }
    "YOLO"        { "YOLO" }
}
if ($SCRIPT:DryRun) { $SCRIPT:Mode = "DRY-RUN" }

# Log level — simplified for profiles
if ($SCRIPT:Profile -eq "CUSTOM") {
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

Initialize-Log
Write-Banner 1 3 "Optimization · Downloads · Safe Mode"

$startStep = Show-ResumePrompt $PHASE $TOTAL_STEPS
if ($startStep -gt $TOTAL_STEPS) {
    Write-Info "Phase 1 already completed."
    Write-Blank
    $r = if (Test-YoloProfile) { "y" } else { Read-Host "  Continue with Phase 2 (Safe Mode GPU driver clean)? [y/N]" }
    if ($r -match "^[jJyY]$") {
        # Ensure safeboot flag is set before restarting
        $bcdOut = bcdedit /set "{current}" safeboot minimal 2>&1
        $bcdExit = $LASTEXITCODE
        # Retry without {current} if first attempt failed
        if ($bcdExit -ne 0) {
            $bcdOut = bcdedit /set safeboot minimal 2>&1
            $bcdExit = $LASTEXITCODE
        }
        # Trust exit code 0; fall back to BCD verification only if exit code is non-zero
        if ($bcdExit -ne 0 -and -not (Test-BootConfigSet "safeboot")) {
            Write-Host ""
            Write-Host "  ╔══════════════════════════════════════════════════════════╗" -ForegroundColor Red
            Write-Host "  ║  Could not set Safe Mode boot flag.                     ║" -ForegroundColor Red
            Write-Host "  ║                                                         ║" -ForegroundColor Red
            Write-Host "  ║  Fix manually in an admin Command Prompt (cmd.exe):     ║" -ForegroundColor Red
            Write-Host "  ║    bcdedit /set {current} safeboot minimal              ║" -ForegroundColor Cyan
            Write-Host "  ║    shutdown /r /t 0                                     ║" -ForegroundColor Cyan
            Write-Host "  ╚══════════════════════════════════════════════════════════╝" -ForegroundColor Red
            Write-Host "  bcdedit said: $($bcdOut | Out-String)".Trim() -ForegroundColor DarkGray
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
        Write-Host "  ║  Safe Mode flag as its very first action — next boot after       ║" -ForegroundColor White
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
Initialize-Backup

# Detect and warn about compatibility limitations (ARM64, CLM, Server, PS7)
Test-SystemCompatibility

# Restore Point
if ($startStep -eq 1) {
    Write-Blank
    if ($SCRIPT:Profile -eq "SAFE") {
        Write-Info "SAFE profile: only harmless, reversible changes will be applied."
        Write-Info "All changes are backed up automatically and can be rolled back."
    } else {
        Write-Warn "Create a System Restore Point NOW!"
        Write-Info "Windows Search -> 'restore point' -> C: -> Create"
        Write-Info "Additionally, this suite backs up every changed setting automatically."
        Write-Info "You can rollback individual steps via START.bat -> Restore."
        if (-not (Confirm-Risk "Restore point created. Continue?" "No rollback possible without a restore point!")) {
            exit 0
        }
    }
}

# Load or create state
$state = $null
if (Test-Path $CFG_StateFile) {
    try {
        $state = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    } catch {
        Write-Warn "Previous state file could not be read (corrupt or empty). Starting fresh."
        $state = $null
    }
}

if (-not $state -or $startStep -eq 1) {
    Write-Section "Step 1 — Configuration"

    Write-Host "`n  GPU:" -ForegroundColor White
    Write-Host "  [1] NVIDIA RTX 5000  [2] NVIDIA RTX 4000/older  [3] AMD  [4] Intel Arc"
    if (Test-YoloProfile) {
        # Auto-detect GPU via WMI
        $gpuInput = "2"  # Default: NVIDIA RTX 4000/older
        try {
            $gpu = Get-CimInstance Win32_VideoController -ErrorAction Stop | Select-Object -First 1
            if ($gpu.Name -match "AMD|Radeon")                      { $gpuInput = "3" }
            elseif ($gpu.Name -match "Intel.*Arc|Intel.*Graphics")   { $gpuInput = "4" }
            elseif ($gpu.Name -match "RTX\s*5\d{3}")                { $gpuInput = "1" }
        } catch { Write-DebugLog "YOLO GPU auto-detect failed, defaulting to NVIDIA: $_" }
        $gpuName = switch ($gpuInput) { "1" {"NVIDIA RTX 5000"} "2" {"NVIDIA RTX 4000/older"} "3" {"AMD"} "4" {"Intel Arc"} }
        Write-Info "YOLO: GPU auto-detected as [$gpuInput] $gpuName"
    } else {
        do { $gpuInput = Read-Host "  [1/2/3/4]" } while ($gpuInput -notin @("1","2","3","4"))
    }

    Write-Blank
    Write-Host "  AVG FPS IN CS2  (0 = calculate later with FpsCap-Calculator):" -ForegroundColor White
    if (Test-YoloProfile) {
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
    Save-JsonAtomic -Data $state -Path $CFG_StateFile
    Set-SecureAcl -Path $CFG_StateFile
    Complete-Step $PHASE 1 "Configuration"
} else {
    # Restore saved config but honor the fresh profile/DRY-RUN choice made above
    $SCRIPT:LogLevel = if ($state.logLevel) { $state.logLevel } else { "NORMAL" }
    if ($SCRIPT:Profile -ne "CUSTOM") { $SCRIPT:LogLevel = "NORMAL" }
    # Profile and Mode were already set from user input at lines 86-103 — keep them
    # Only fall back to state values if user chose the same profile
    $fpsCap  = $state.fpsCap
    $avgFps    = $state.avgFps;   $gpuInput = $state.gpuInput
    # Update state file with the fresh profile choice
    $state.mode     = $SCRIPT:Mode
    $state.profile  = $SCRIPT:Profile
    Save-JsonAtomic -Data $state -Path $CFG_StateFile
    Set-SecureAcl -Path $CFG_StateFile
    Write-Info "Configuration loaded from previous session (Profile: $($SCRIPT:Profile)$(if($SCRIPT:DryRun){' [DRY-RUN]'}))."
}
