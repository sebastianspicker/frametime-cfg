#Requires -RunAsAdministrator
<#
.SYNOPSIS  Boot into Safe Mode for GPU driver clean removal (Phase 2).

  Quick-start shortcut that does exactly what Phase 1 Step 38 does:
    1. Copies scripts + helpers to C:\CS2_OPTIMIZE\
    2. Registers Phase 2 (SafeMode-DriverClean.ps1) via RunOnce
    3. Sets bcdedit safeboot minimal
    4. Prompts for restart

  Use this when Phase 1 has already been completed and you need to
  re-run the GPU driver clean process (Phase 2 + 3) without going
  through all 38 Phase 1 steps again.
#>

param([switch]$SmokeTest)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ScriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
. "$ScriptRoot\config.env.ps1"
. "$ScriptRoot\helpers.ps1"

if ($SmokeTest) {
    Write-Host "SMOKE TEST OK: Boot-SafeMode" -ForegroundColor Green
    exit 0
}

Ensure-Dir $CFG_WorkDir
Ensure-Dir $CFG_LogDir
Initialize-ScriptDefaults
Initialize-Log

Write-Host ""
Write-Host "  ======================================================" -ForegroundColor Cyan
Write-Host "   BOOT TO SAFE MODE  --  GPU Driver Clean (Phase 2)" -ForegroundColor Cyan
Write-Host "  ======================================================" -ForegroundColor Cyan
Write-Host ""

# -- Verify state.json exists and the Phase 1 handoff was actually prepared ----
$stateExists = Test-Path $CFG_StateFile
if (-not $stateExists) {
    Write-Warn "state.json not found at $CFG_StateFile"
    Write-Info "Phase 1 must be run at least once to create the configuration."
    Write-Info "Use START.bat -> [1] to run Phase 1 first."
    Write-Host ""
    Read-Host "  Press Enter to return"
    exit 0
}

try {
    $state = Load-State $CFG_StateFile
} catch {
    Write-Warn "state.json is corrupted: $_"
    Write-Info "Re-run Phase 1 from START.bat -> [1] to fix."
    Read-Host "  Press Enter to return"
    exit 1
}

if (-not (Test-Phase1SafeModeReady -State $state)) {
    Write-Warn "state.json exists, but Phase 1 Safe Mode prep is not marked complete."
    Write-Info "GUI settings alone do not prepare the reboot payload."
    Write-Info "Run START.bat -> [1] and complete Step 38 before using this shortcut."
    Read-Host "  Press Enter to return"
    exit 0
}

# Show current GPU config
$gpuName = switch ($state.gpuInput) {
    "1" {"NVIDIA RTX 5000"} "2" {"NVIDIA"} "3" {"AMD"} "4" {"Intel"} default {"NVIDIA"}
}
Write-Info "GPU vendor: $gpuName"
if ($state.PSObject.Properties['nvidiaDriverPath'] -and $state.nvidiaDriverPath) {
    Write-Info "Driver .exe: $($state.nvidiaDriverPath)"
}
if ($state.PSObject.Properties['rollbackDriver'] -and $state.rollbackDriver) {
    Write-Warn "Rollback target: $($state.rollbackDriver)"
}
Write-Host ""

Write-Info "This will:"
Write-Host "    1. Copy scripts to $CFG_WorkDir" -ForegroundColor White
Write-Host "    2. Register Phase 2 to run on next boot (Safe Mode)" -ForegroundColor White
Write-Host "    3. Set Safe Mode boot flag (bcdedit)" -ForegroundColor White
Write-Host "    4. Restart into Safe Mode" -ForegroundColor White
Write-Host ""
Write-Info "Phase 2 removes the GPU driver + all GPU software cleanly."
Write-Info "Phase 3 then installs a clean driver on next normal boot."
Write-Host ""

$confirm = Read-Host "  Proceed? [y/N]"
if ($confirm -notmatch "^[jJyY]$") {
    Write-Info "Cancelled."
    exit 0
}

# -- 1. Copy scripts to work directory ----------------------------------------
Write-Host ""
Write-Step "Copying scripts to $CFG_WorkDir..."
Ensure-Dir $CFG_WorkDir
Copy-PhaseRuntimePayload -SourceRoot $ScriptRoot -DestinationRoot $CFG_WorkDir

# -- 2. Register Phase 2 RunOnce ----------------------------------------------
Write-Step "Registering Phase 2 for Safe Mode boot..."
Set-RunOnce "CS2_Phase2" "$CFG_WorkDir\SafeMode-DriverClean.ps1" -SafeMode
Write-OK "Phase 2 registered via RunOnce."

# -- 3. Set Safe Mode boot flag -----------------------------------------------
Write-Step "Setting Safe Mode boot flag..."

$bcdOut = bcdedit /set "{current}" safeboot minimal 2>&1
$bcdExit = $LASTEXITCODE

if ($bcdExit -ne 0) {
    Write-Warn "bcdedit exited with code $bcdExit -- retrying without {current}..."
    $bcdOut = bcdedit /set safeboot minimal 2>&1
    $bcdExit = $LASTEXITCODE
}

$safebootOk = ($bcdExit -eq 0) -or (Test-BootConfigSet "safeboot")

if (-not $safebootOk) {
    Write-Host ""
    Write-Err "Safe Mode boot flag could NOT be set."
    Write-Host ""
    Write-Host "  Try manually from an elevated cmd.exe:" -ForegroundColor White
    Write-Host '    bcdedit /set {current} safeboot minimal' -ForegroundColor Cyan
    Write-Host "  Then restart to enter Safe Mode for Phase 2." -ForegroundColor White
    Write-Host ""
    Read-Host "  Press Enter to return"
    exit 1
}

Write-OK "Safe Mode boot flag set."

# -- 4. Restart prompt ---------------------------------------------------------
Write-Host ""
$r = Read-Host "  Restart into Safe Mode now? Save all work first! [y/N]"
if ($r -match "^[jJyY]$") {
    $countdownSec = 10
    Write-Host ""
    Write-Host "  RESTARTING INTO SAFE MODE" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "  If Safe Mode gets stuck or you need to return to Normal Mode:" -ForegroundColor Yellow
    Write-Host "    1. In Safe Mode, open an admin Command Prompt (cmd.exe)" -ForegroundColor White
    Write-Host '    2. Run:  bcdedit /deletevalue safeboot' -ForegroundColor Cyan
    Write-Host '    3. Run:  shutdown /r /t 0' -ForegroundColor Cyan
    Write-Host ""
    Write-Host "  Phase 2 runs automatically and removes the Safe Mode flag" -ForegroundColor White
    Write-Host "  as its first action -- next boot will be Normal Mode." -ForegroundColor White
    Write-Host ""
    Write-Host "  Press Ctrl+C to cancel." -ForegroundColor DarkGray
    Write-Host ""
    for ($i = $countdownSec; $i -ge 1; $i--) {
        Write-Host "`r  Restarting in $i... " -NoNewline -ForegroundColor Yellow
        Start-Sleep 1
    }
    Write-Host "`r  Restarting now...     " -ForegroundColor Red
    shutdown /r /t 0 /f
} else {
    Write-Host ""
    Write-Host "  Safe Mode is armed. Restart manually when ready." -ForegroundColor Yellow
    Write-Host "  The NEXT reboot will boot into Safe Mode." -ForegroundColor Yellow
    Write-Host ""
    Read-Host "  Press Enter to return"
}
