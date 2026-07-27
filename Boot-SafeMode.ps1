<#
.SYNOPSIS
Boots into Safe Mode for GPU driver clean removal (Phase 2).

.DESCRIPTION
This shortcut publishes the verified Phase 2/3 runtime, registers Phase 2,
configures Safe Mode, and offers a restart after Phase 1 has already completed.

  Quick-start shortcut that does exactly what Phase 1 Step 38 does:
    1. Publishes a manifest-verified immutable runtime generation
    2. Registers Phase 2 (SafeMode-DriverClean.ps1) via RunOnce
    3. Sets bcdedit safeboot minimal
    4. Prompts for restart

  Use this when Phase 1 has already been completed and you need to
  re-run the GPU driver clean process (Phase 2 + 3) without going
  through all 38 Phase 1 steps again.

.PARAMETER SmokeTest
Checks that the public entrypoint loads, then exits without initialization.

.PARAMETER DryRun
Previews only this Safe Mode shortcut transaction without elevation, state,
payload, BCD, RunOnce, or reboot changes. It does not preview all three phases;
use Run-Optimize.ps1 -FullDryRun for the complete lifecycle.

.EXAMPLE
PS> .\Boot-SafeMode.ps1 -DryRun
#>

param(
    [switch]$SmokeTest,
    [switch]$DryRun
)

if ($SmokeTest) {
    Write-Host "SMOKE TEST OK: Boot-SafeMode" -ForegroundColor Green
    exit 0
}

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]$identity
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "Boot-SafeMode.ps1 must be run as Administrator. Start PowerShell with 'Run as administrator' and try again."
    }
}

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ScriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
. "$ScriptRoot\config.env.ps1"
. "$ScriptRoot\helpers.ps1"

if ($DryRun) {
    $SCRIPT:DryRun = $true
    $SCRIPT:Mode = "DRY-RUN"
    $SCRIPT:Profile = "CUSTOM"
    $SCRIPT:LogLevel = "VERBOSE"
    $SCRIPT:PhaseTotal = 4
    $SCRIPT:CurrentPhase = 2
    Initialize-PhaseCounters
    Write-Banner 2 3 "Safe Mode shortcut · Full transaction preview"
    $phase2Transaction = Enable-Phase2SafeModeTransaction -SourceRoot $ScriptRoot -DestinationRoot $CFG_WorkDir -StatePath $CFG_StateFile -Why "Boot-SafeMode shortcut"
    Write-Info $phase2Transaction.Message
    Write-Host "  [DRY-RUN] Would restart into Safe Mode only after payload, handoff, BCD, and readiness verification." -ForegroundColor Magenta
    Write-PhaseSummary -PhaseLabel "SAFE MODE SHORTCUT" -DryRun
    exit 0
}

Assert-Administrator

Assert-NoLegacyPhaseHandoff
Ensure-SecureWorkDir -Path $CFG_WorkDir
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
    Write-Warn "Legacy rollbackDriver metadata is present and will be ignored."
}
Write-Host ""

Write-Info "This will:"
Write-Host "    1. Copy scripts to $CFG_WorkDir" -ForegroundColor White
Write-Host "    2. Register Phase 2 to run on next boot (Safe Mode)" -ForegroundColor White
Write-Host "    3. Set Safe Mode boot flag (bcdedit)" -ForegroundColor White
Write-Host "    4. Restart into Safe Mode" -ForegroundColor White
Write-Host ""
Write-Info "Phase 2 removes verified display-driver packages and clears selected vendor state."
Write-Info "Phase 3 can install a validated NVIDIA package; AMD and Intel remain partly manual."
Write-Host ""

$confirm = Read-Host "  Proceed? [y/N]"
if ($confirm -notmatch "^[jJyY]$") {
    Write-Info "Cancelled."
    exit 0
}

# -- 1-3. Prepare and verify the Phase 2 Safe Mode transaction ----------------
Write-Host ""
$phase2Transaction = Enable-Phase2SafeModeTransaction -SourceRoot $ScriptRoot -DestinationRoot $CFG_WorkDir -StatePath $CFG_StateFile -Why "Boot-SafeMode shortcut"
if (-not $phase2Transaction.Applied) {
    Write-Host ""
    Write-Err "$($phase2Transaction.Message)"
    Write-Host ""
    Write-Host ""
    Read-Host "  Press Enter to return"
    exit 1
}
Write-OK "Phase 2 handoff and Safe Mode boot flag are verified."

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
