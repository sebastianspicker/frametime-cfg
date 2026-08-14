<#
.SYNOPSIS
Runs the three-phase frametime.cfg optimization workflow or its strict preview.

.DESCRIPTION
Live execution runs Phase 1 and prepares the resumable Safe Mode and Normal Mode
handoffs for Phases 2 and 3. With -FullDryRun, every tier and all three phases
are previewed for one selected GPU branch using in-memory boot simulations. The
full preview is non-interactive, requires no elevation, and makes no system or
persistent state changes.

  MINIMUM REQUIREMENTS:
    - x64 Windows 10 or Windows 11 desktop
    - PowerShell 5.1 (shipped with Windows 10/11; PS 7 has partial WMI gaps)
    - Administrator privileges for live execution; not required for -FullDryRun

  KNOWN LIMITATIONS:
    - ARM64 Windows: NVIDIA DRS writes are unavailable (nvapi64.dll is x64-only).
      Falls back to registry-only NVIDIA profile method automatically.
    - Windows Server / LTSC: AppX debloat is skipped (cmdlets unavailable).
      Xbox services may not exist. All handled gracefully.
    - Constrained Language Mode (AppLocker/WDAC): DRS writes and RAM trim
      are skipped (Add-Type is blocked). Registry-only paths are used instead.
    - PowerShell 7: Pagefile step uses Get-WmiObject (removed in PS 7).
      Use Windows PowerShell 5.1 for full functionality.
    - GPU driver package discovery and removal verification use
      locale-independent CIM data. Localized pnputil output is not parsed as a
      destructive fallback.

  TIER SYSTEM:
    T1  Baseline project operation
    T2  Setup-dependent
    T3  Experimental operation with weak or incomplete evidence

  Steps:
    1   Mode + log level + configuration
    2   XMP/EXPO check  [T1]
    3   Shader cache clear  [T1]
    4   Fullscreen optimizations disable  [T1]
    5   NVIDIA driver version inventory  [T1]
    6   frametime.cfg Power Plan (native powercfg)  [T1]
    7   HAGS configure  [T2]
    8   Pagefile fix  [T2]
    9   Resizable BAR check  [T2 GPU]
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
    27  SystemResponsiveness + priority + DisablePagingExecutive  [T2]
    28  Timer resolution  [T2]
    29  Mouse acceleration disable  [T2]
    30  CS2 GPU preference  [T2]
    31  Xbox Game Bar / Game DVR disable  [T2]
    32  Overlay disable  [T2]
    33  Audio optimization  [T2]
    34  optimization.cfg generator + autoexec.cfg bootstrap  [T2]
    35  Chipset driver check  [T2]
    36  Visual effects best performance  [T3]
    37  SysMain + Windows Search disable  [T3]
    38  Safe Mode -> restart

.PARAMETER SmokeTest
Loads only the public entrypoint smoke path and exits without initialization.

.PARAMETER FullDryRun
Runs the maximum-coverage, console-only lifecycle preview from Normal Mode. A
genuine Safe Mode environment fails closed.

.PARAMETER DryRunGpu
Selects the mutually exclusive path used by -FullDryRun: 1 = NVIDIA RTX 5000,
2 = other NVIDIA, 3 = AMD, and 4 = Intel Arc. The default is 2. This parameter
is rejected unless -FullDryRun is also supplied.

.EXAMPLE
PS> .\Run-Optimize.ps1 -FullDryRun

Previews the default NVIDIA branch without elevation.

.EXAMPLE
PS> .\Run-Optimize.ps1 -FullDryRun -DryRunGpu 3

Previews the AMD branch. No driver or AMD setting is changed.

.NOTES
Use Windows PowerShell 5.1 for supported live execution. See docs/dry-run.md
for the strict safety contract and the full four-branch command matrix.
#>
param(
    [switch]$SmokeTest,
    [switch]$FullDryRun,
    [ValidateSet("1", "2", "3", "4")]
    [string]$DryRunGpu = "2"
)

if ($SmokeTest) {
    Write-Host "SMOKE TEST OK: Run-Optimize" -ForegroundColor Green
    exit 0
}

if (-not $FullDryRun -and $PSBoundParameters.ContainsKey('DryRunGpu')) {
    throw "-DryRunGpu is only valid with -FullDryRun."
}

if (-not $FullDryRun) {
    throw "Portable live execution is unavailable until a trusted installer or signed payload establishes the source identity. Use -FullDryRun for the no-mutation preview."
}

# Full previews are intentionally host-independent. Supply deterministic empty
# inventory results when Windows-only discovery cmdlets are unavailable.
if ($FullDryRun) {
    if (-not (Get-Command Get-CimInstance -ErrorAction SilentlyContinue)) {
        function Get-CimInstance {
            [CmdletBinding()]
            param(
                [Parameter(Position = 0)][string]$ClassName,
                [string]$Filter,
                [Parameter(ValueFromRemainingArguments)]$RemainingArgs
            )
            if ($ClassName -eq "Win32_OperatingSystem") {
                return [PSCustomObject]@{ ProductType = 1; CurrentBuildNumber = "19045" }
            }
            return @()
        }
    }
    if (-not (Get-Command Get-Service -ErrorAction SilentlyContinue)) {
        function Get-Service {
            [CmdletBinding()]
            param([string]$Name, [Parameter(ValueFromRemainingArguments)]$RemainingArgs)
            return @()
        }
    }
    if (-not (Get-Command Get-ScheduledTask -ErrorAction SilentlyContinue)) {
        function Get-ScheduledTask {
            [CmdletBinding()]
            param(
                [string]$TaskName,
                [string]$TaskPath,
                [Parameter(ValueFromRemainingArguments)]$RemainingArgs
            )
            return @()
        }
    }
    if (-not (Get-Command Get-NetAdapter -ErrorAction SilentlyContinue)) {
        function Get-NetAdapter {
            [CmdletBinding()]
            param([string]$Name, [Parameter(ValueFromRemainingArguments)]$RemainingArgs)
            return @()
        }
    }
}

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]$identity
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Assert-Administrator {
    if (-not (Test-Administrator)) {
        throw "Run-Optimize.ps1 must be run as Administrator. Start PowerShell with 'Run as administrator' and try again."
    }
}

function Invoke-RemainingPhasesDryRun {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object]$PreviewState)

    if (-not $SCRIPT:DryRun) {
        throw "Remaining-phase preview can only run while DRY-RUN is active."
    }

    $previewStateObject = if ($PreviewState -is [hashtable]) {
        [PSCustomObject]$PreviewState
    } else {
        $PreviewState
    }

    Write-Blank
    Write-Section "DRY-RUN transition - Simulated reboot into Safe Mode"
    Write-ConsoleLine "  [DRY-RUN] No reboot, BCD change, payload copy, or RunOnce registration will occur." -ForegroundColor Magenta
    . "$ScriptRoot\SafeMode-DriverClean.ps1"
    Invoke-SafeModeDriverClean -PreviewState $previewStateObject -SimulateSafeMode

    Write-Blank
    Write-Section "DRY-RUN transition - Simulated reboot into Normal Mode"
    Write-ConsoleLine "  [DRY-RUN] No reboot or automatic handoff will occur." -ForegroundColor Magenta
    . "$ScriptRoot\PostReboot-Setup.ps1"
    Invoke-PostRebootSetup -PreviewState $previewStateObject -SimulateNormalBoot

    Write-PhaseSummary -PhaseLabel "ALL 3 PHASES" -DryRun
    $previewIssues = Get-DryRunPreviewIssueCount
    if ($previewIssues -gt 0) {
        throw "Full DRY-RUN completed with $previewIssues preview issue(s). Review the warnings above."
    }
}

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ScriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
. "$ScriptRoot\config.env.ps1"
. "$ScriptRoot\helpers.ps1"

$TOTAL_STEPS = 38
$SCRIPT:PhaseTotal = $TOTAL_STEPS
$SCRIPT:CurrentPhase = 1
$PHASE = 1
$SCRIPT:DryRun = [bool]$FullDryRun # strict preview behavior starts before profile setup
$SCRIPT:SafebootReady = $false  # set to $true by Step 38 if bcdedit safeboot confirmed
$SCRIPT:Profile = "RECOMMENDED" # safe defaults for early startup diagnostics
$SCRIPT:Mode = "CONTROL"
$SCRIPT:LogLevel = "NORMAL"
$SCRIPT:FullDryRunRequested = [bool]$FullDryRun
$SCRIPT:RequestedDryRunGpu = $DryRunGpu
$SCRIPT:FullDryRun = $false
$SCRIPT:DryRunPreviewIssues = 0

try {
    Initialize-PhaseCounters
    # The phase files execute while being dot-sourced; this order is the Phase 1
    # workflow, not a passive import list.
    . "$ScriptRoot\Setup-Profile.ps1"
    if ($SCRIPT:DryRun -and $env:SAFEBOOT_OPTION) {
        throw "DRY-RUN must be launched from Normal Mode. No Safe Mode boot setting was changed."
    }
    . "$ScriptRoot\Optimize-SystemBase.ps1"
    . "$ScriptRoot\Optimize-Hardware.ps1"
    . "$ScriptRoot\Optimize-RegistryTweaks.ps1"
    . "$ScriptRoot\Optimize-GameConfig.ps1"

    # ── Phase 1 complete ─────────────────────────────────────────────────────────
    if ($SCRIPT:DryRun) {
        Write-PhaseSummary -PhaseLabel "PHASE 1" -DryRun -ContinuePreview
        Invoke-RemainingPhasesDryRun -PreviewState $state
    } else {
        $nextAction = "-> Restart -> Safe Mode -> GPU driver clean`n-> Normal boot -> Phase 3 starts automatically"
        Write-PhaseSummary -PhaseLabel "PHASE 1" -NextAction $nextAction

        # Only offer restart after the current run completed the payload, RunOnce,
        # BCD verification, and readiness transaction.  A residual BCD flag alone
        # cannot prove that this reboot will launch the intended Phase 2 payload.
        $safebootConfirmed = $SCRIPT:SafebootReady
        if (-not $safebootConfirmed) {
            Write-Blank
            Write-Warn "Safe Mode boot flag was NOT set - restarting would boot into Normal Mode."
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
                # Use shutdown.exe - more reliable than Restart-Computer on some builds
                shutdown /r /t 0 /f
            }
        }
    }
} finally {
    # Release backup lock - acquired by Initialize-Backup in Setup-Profile.ps1.
    # In try/finally to ensure release on crash, Ctrl+C, or normal exit.
    # On Restart-Computer, the lock file becomes stale (process dead) and is
    # auto-cleaned by Test-BackupLock on next boot.
    if (-not $SCRIPT:DryRun) {
        Remove-BackupLock
    }
}
