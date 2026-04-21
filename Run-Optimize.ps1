#Requires -RunAsAdministrator
<#
.SYNOPSIS  CS2 Optimization Suite — Full Optimization Run (38 steps)

  MINIMUM REQUIREMENTS:
    - Windows 10 (1903+) or Windows 11 (any build) — x64 desktop edition
    - PowerShell 5.1 (shipped with Windows 10/11; PS 7 has partial WMI gaps)
    - Administrator privileges (enforced by #Requires above)

  KNOWN LIMITATIONS:
    - ARM64 Windows: NVIDIA DRS writes are unavailable (nvapi64.dll is x64-only).
      Falls back to registry-only NVIDIA profile method automatically.
    - Windows Server / LTSC: AppX debloat is skipped (cmdlets unavailable).
      Xbox services may not exist. All handled gracefully.
    - Constrained Language Mode (AppLocker/WDAC): DRS writes and RAM trim
      are skipped (Add-Type is blocked). Registry-only paths are used instead.
    - PowerShell 7: Pagefile step uses Get-WmiObject (removed in PS 7).
      Use Windows PowerShell 5.1 for full functionality.
    - Non-English Windows: GPU driver clean uses CIM (locale-independent).
      pnputil text-parsing fallback only works on English installations.

  TIER SYSTEM:
    T1  Proven measurable effect -> always runs automatically
    T2  Setup-dependent -> always prompted (even AUTO mode)
    T3  Community tip without hard benchmark -> CONTROL mode only

  Steps:
    1   Mode + log level + configuration
    2   XMP/EXPO check  [T1]
    3   Shader cache clear  [T1]
    4   Fullscreen optimizations disable  [T1]
    5   NVIDIA driver version check  [T2]
    6   CS2 Optimized Power Plan (native powercfg)  [T1]
    7   HAGS configure  [T2]
    8   Pagefile fix  [T2]
    9   Resizable BAR check  [T2 AMD]
    10  Dynamic tick + platform timer  [T3]
    11  MPO disable  [T3]
    12  Windows Game Mode (enable)  [T3]
    13  Gaming Debloat (native PowerShell)  [Hygiene]
    14  Autostart cleanup  [Hygiene]
    15  Windows Update Blocker (native services)  [Optional]
    16  NIC tweaks  [T2]
    17  CapFrameX + Baseline Benchmark  [T1]
    18  GPU driver clean prep  [T1]
    19  NVIDIA driver download  [T1]
    20  NVIDIA profile prep  [T3]
    21  MSI interrupts prep  [T2]
    22  NIC interrupt affinity prep  [T3]
    23  Disable Fast Startup (HiberbootEnabled=0)  [T2]
    24  Dual-channel RAM detection  [T1]
    25  Nagle's Algorithm disable  [T2]
    26  GameConfigStore FSE registry  [T2]
    27  SystemResponsiveness + priority + NetworkThrottlingIndex + DisablePagingExecutive  [T2]
    28  Timer resolution  [T2]
    29  Mouse acceleration disable  [T2]
    30  CS2 GPU preference  [T2]
    31  Xbox Game Bar / Game DVR disable  [T2]
    32  Overlay disable  [T2]
    33  Audio optimization  [T2]
    34  Autoexec.cfg generator  [T2]
    35  Chipset driver check  [T2]
    36  Visual effects best performance  [T3]
    37  SysMain + Windows Search disable  [T3]
    38  Safe Mode -> restart
#>
param([switch]$SmokeTest)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ScriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
. "$ScriptRoot\config.env.ps1"
. "$ScriptRoot\helpers.ps1"

Ensure-Dir $CFG_WorkDir
Ensure-Dir $CFG_LogDir

if ($SmokeTest) {
    Write-Host "SMOKE TEST OK: Run-Optimize" -ForegroundColor Green
    exit 0
}

$TOTAL_STEPS = 38
$SCRIPT:PhaseTotal = $TOTAL_STEPS
$SCRIPT:CurrentPhase = 1
$PHASE = 1
$SCRIPT:DryRun = $false         # safe default; Setup-Profile.ps1 will override
$SCRIPT:SafebootReady = $false  # set to $true by Step 38 if bcdedit safeboot confirmed
$appliedStepsPersisted = $true

try {
    Initialize-PhaseCounters
    . "$ScriptRoot\Setup-Profile.ps1"
    . "$ScriptRoot\Optimize-SystemBase.ps1"
    . "$ScriptRoot\Optimize-Hardware.ps1"
    . "$ScriptRoot\Optimize-RegistryTweaks.ps1"
    . "$ScriptRoot\Optimize-GameConfig.ps1"

    # ── Phase 1 complete ─────────────────────────────────────────────────────────
    # Persist applied step keys so Phase 3 improvement estimates are cumulative
    # Skip in DRY-RUN to avoid overwriting real applied steps with an empty array
    if (-not $SCRIPT:DryRun) { $appliedStepsPersisted = Save-AppliedSteps }
    if ($SCRIPT:DryRun) {
        Write-PhaseSummary -PhaseLabel "PHASE 1" -DryRun
    } else {
        if (-not $appliedStepsPersisted) {
            Write-Warn "Phase 1 completed, but applied-step history was not saved."
            Write-Warn "Phase 3 will still run, but cumulative improvement estimates may be incomplete."
        }
        $nextAction = "-> Restart -> Safe Mode -> GPU driver clean`n-> Normal boot -> Phase 3 starts automatically"
        Write-PhaseSummary -PhaseLabel "PHASE 1" -NextAction $nextAction

        # Only offer restart if safeboot is actually set in BCD.
        # $SCRIPT:SafebootReady is set by Step 38, but on resume (step 38 already
        # completed) it stays $false — fall back to live BCD check.
        $safebootConfirmed = $SCRIPT:SafebootReady -or (Test-BootConfigSet "safeboot")
        if (-not $safebootConfirmed) {
            Write-Blank
            Write-Warn "Safe Mode boot flag was NOT set — restarting would boot into Normal Mode."
            Write-Warn "Fix: open an admin cmd.exe and run:  bcdedit /set {current} safeboot minimal"
            Write-Warn "Then restart manually to enter Safe Mode for Phase 2."
        } else {
            Write-Blank
            $r = if (Test-YoloProfile) { "y" } else { Read-Host "  Restart into Safe Mode now? Save all work first! [y/N]" }
            if ($r -match "^[jJyY]$") {
                # ── Countdown with Safe Mode recovery instructions ────────────
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
                # Use shutdown.exe — more reliable than Restart-Computer on some builds
                shutdown /r /t 0 /f
            }
        }
    }
} finally {
    # Release backup lock — acquired by Initialize-Backup in Setup-Profile.ps1.
    # In try/finally to ensure release on crash, Ctrl+C, or normal exit.
    # On Restart-Computer, the lock file becomes stale (process dead) and is
    # auto-cleaned by Test-BackupLock on next boot.
    Remove-BackupLock
}
