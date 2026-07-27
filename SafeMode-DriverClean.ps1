<# frametime.cfg - Safe Mode · GPU Driver Clean Removal

    CRASH RECOVERY SCENARIOS - this script is safety-critical.

    Steps execute in order: (1) bcdedit /deletevalue safeboot, (2) driver removal,
    (3) a per-user Phase 3 Run handoff.
    Step 1 is done FIRST so that a crash at any later point still boots into Normal Mode.

    Crash during Step 1 (bcdedit /deletevalue):
        bcdedit is atomic - either the BCD write completes or it doesn't.
        - If completed:  next boot = Normal Mode. Safe to re-run this script.
        - If not completed (power loss mid-write): next boot = Safe Mode. The script
          re-runs via RunOnce and retries Step 1. If BCD is corrupt, user sees manual
          fix instructions: "bcdedit /deletevalue safeboot" from elevated cmd.exe.

    Crash during Step 2 (driver removal):
        Step 1 already completed, so next boot = Normal Mode.
        - Partial driver removal: Windows auto-detects missing display driver and loads
          Microsoft Basic Display Adapter (MSBDA). Resolution limited to 1024x768 but
          the system is usable. User can install GPU driver normally.
        - The per-user Phase 3 handoff was NOT yet registered (Step 3), so Phase 3 won't
          auto-start. START.bat -> [P] launches the manifest-verified published runtime.

    Crash during Step 3 (per-user Run handoff registration):
        Steps 1+2 completed. Next boot = Normal Mode, GPU driver removed.
        - Phase 3 won't auto-start. START.bat -> [P] launches the verified runtime.
        - This is the lowest-risk crash point - system boots fine, just needs manual Phase 3.

    Power failure during Restart-Computer:
        All steps completed. System reboots normally. The same account's Run
        handoff starts Phase 3 after sign-in.
        - This is equivalent to a normal power cycle - no data loss risk.
#>
param([switch]$SmokeTest)

function Test-PublishedRuntimePayloadBootstrap {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$RuntimeRoot)

    try {
        $manifestPath = Join-Path $RuntimeRoot "runtime-manifest.json"
        if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw "runtime-manifest.json is missing" }
        $manifest = Get-Content -LiteralPath $manifestPath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
        if ($manifest.schemaVersion -ne 1) { throw "unsupported runtime manifest schema" }
        $expectedContract = "bba9d71061a9cd0b7897c97c5792aab42f29d6cd3f89f2bcd80883cd5f2c75c4"
        $entries = @($manifest.files)
        if ($entries.Count -eq 0) { throw "runtime manifest has no files" }
        $manifestPaths = @($entries | ForEach-Object { [string]$_.path })
        if (@($manifestPaths | Group-Object | Where-Object Count -gt 1).Count -gt 0) { throw "runtime manifest contains duplicate paths" }
        $contractText = (@($manifestPaths | Sort-Object) -join "`n")
        $sha256 = [Security.Cryptography.SHA256]::Create()
        try {
            $actualContract = (([BitConverter]::ToString($sha256.ComputeHash([Text.Encoding]::UTF8.GetBytes($contractText))) -replace '-', '').ToLowerInvariant())
        } finally {
            $sha256.Dispose()
        }
        if ($manifest.payloadContract -ne $expectedContract -or $actualContract -ne $expectedContract) { throw "runtime payload contract mismatch" }
        foreach ($relativePath in $manifestPaths) {
            if ($relativePath -notmatch '^[a-zA-Z0-9_.-]+(?:/[a-zA-Z0-9_.-]+)*$' -or $relativePath -match '(^|/)\.\.(/|$)') {
                throw "runtime manifest contains an unsafe path"
            }
        }
        $rootPath = [IO.Path]::GetFullPath($RuntimeRoot).TrimEnd([char[]]@('\', '/'))
        $actualPaths = @(Get-ChildItem -LiteralPath $RuntimeRoot -File -Recurse -Force -ErrorAction Stop |
            Where-Object { $_.FullName -ne $manifestPath } |
            ForEach-Object {
                (([IO.Path]::GetFullPath($_.FullName).Substring($rootPath.Length) -replace '^[\\/]+', '') -replace '\\', '/')
            })
        if (@(Compare-Object -ReferenceObject @($manifestPaths | Sort-Object) -DifferenceObject @($actualPaths | Sort-Object)).Count -gt 0) {
            throw "runtime contains missing or extra files"
        }
        foreach ($entry in $entries) {
            $relativePath = [string]$entry.path
            $expectedHash = [string]$entry.sha256
            if ($expectedHash -notmatch '^[A-Fa-f0-9]{64}$') { throw "invalid manifest hash for $relativePath" }
            $filePath = Join-Path $RuntimeRoot ($relativePath -replace '/', [IO.Path]::DirectorySeparatorChar)
            $actualHash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256 -ErrorAction Stop).Hash
            if ($actualHash -ne $expectedHash) { throw "runtime hash mismatch: $relativePath" }
        }
        return [PSCustomObject]@{ Valid = $true; Message = "Published runtime payload verified." }
    } catch {
        return [PSCustomObject]@{ Valid = $false; Message = "Published runtime validation failed: $_" }
    }
}


function Test-SafeModeDriverCleanAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]$identity
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Assert-SafeModeDriverCleanAdministrator {
    if (-not (Test-SafeModeDriverCleanAdministrator)) {
        throw "SafeMode-DriverClean.ps1 must be run as Administrator. Start PowerShell with 'Run as administrator' and try again."
    }
}

function Invoke-SafeModeDriverCleanEntryPoint {
    param([switch]$SmokeTest)

    if ($SmokeTest) {
        Write-Host "SMOKE TEST OK: SafeMode-DriverClean" -ForegroundColor Green
        return
    }

    $payloadValidation = Test-PublishedRuntimePayloadBootstrap -RuntimeRoot $PSScriptRoot
    if (-not $payloadValidation.Valid) {
        Write-Host "  CRITICAL: $($payloadValidation.Message)" -ForegroundColor Red
        Write-Host "  No boot-state or driver changes were attempted." -ForegroundColor Yellow
        Write-Host "  Re-run Phase 1 to publish a complete runtime payload." -ForegroundColor Cyan
        Write-Host "  If currently in Safe Mode, recover from elevated cmd.exe:" -ForegroundColor Cyan
        Write-Host "    bcdedit /deletevalue safeboot" -ForegroundColor White
        Write-Host "    bcdedit /enum {current} /v" -ForegroundColor White
        throw [IO.InvalidDataException]::new("Phase 2 stopped because the published runtime payload is invalid: $($payloadValidation.Message)")
    }
    Assert-SafeModeDriverCleanAdministrator
    Invoke-SafeModeDriverClean
}

function Invoke-SafeModeDriverClean {
[CmdletBinding()]
param(
    [object]$PreviewState,
    [switch]$SimulateSafeMode
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$ScriptRoot = $PSScriptRoot
. "$ScriptRoot\config.env.ps1"
. "$ScriptRoot\helpers.ps1"

if ($null -ne $PreviewState) {
    $state = if ($PreviewState -is [hashtable]) { [PSCustomObject]$PreviewState } else { $PreviewState }
    if (-not $SimulateSafeMode -or [string]$state.mode -ne "DRY-RUN") {
        throw "Injected Phase 2 state is allowed only for an explicit simulated DRY-RUN."
    }
    $SCRIPT:Mode = "DRY-RUN"
    $SCRIPT:Profile = if ($state.PSObject.Properties['profile']) { [string]$state.profile } else { "CUSTOM" }
    $SCRIPT:LogLevel = if ($state.PSObject.Properties['logLevel']) { [string]$state.logLevel } else { "VERBOSE" }
    $SCRIPT:DryRun = $true
} else {
    try {
        # Read first without creating directories or changing ACLs. If the saved
        # state is live, reload it through the hardened live path before acting.
        $state = Load-State -Path $CFG_StateFile -ReadOnly
        if (-not $SCRIPT:DryRun) {
            $state = Load-State -Path $CFG_StateFile
        }
    } catch {
    Write-Host "  $([char]0x2718) Something went wrong: settings file (state.json) is missing or corrupted." -ForegroundColor Red
    Write-Host "    Error detail: $_" -ForegroundColor DarkGray
    Write-Host "" -ForegroundColor Yellow
    Write-Host "  $([char]0x2139) What to do:" -ForegroundColor Cyan
    Write-Host "    Option 1: Press [Y] below to use hardware detection and safe defaults." -ForegroundColor White
    Write-Host "    Option 2: Exit Safe Mode manually by opening an admin Command Prompt" -ForegroundColor White
    Write-Host "              and running these two commands:" -ForegroundColor White
    Write-Host "                bcdedit /deletevalue safeboot" -ForegroundColor White
    Write-Host "                shutdown /r /t 0" -ForegroundColor White
    Write-Host ""
    $r = if (Test-YoloProfile) { "y" } else { Read-Host "  Detect the GPU and continue with defaults? [y/N]" }
    if ($r -notmatch "^[jJyY]$") { exit 1 }
    $detectedGpu = $null
    try {
        $gpu = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
               Where-Object { $_.Status -eq "OK" } | Select-Object -First 1
        if ($gpu) {
            if ($gpu.Name -match "AMD|Radeon") { $detectedGpu = "3" }
            elseif ($gpu.Name -match "Intel") { $detectedGpu = "4" }
            elseif ($gpu.Name -match "NVIDIA|GeForce|RTX") {
                $detectedGpu = if ($gpu.Name -match "RTX\s*5\d{3}") { "1" } else { "2" }
            }
        }
    } catch { Write-DebugLog "GPU auto-detection via WMI failed in Safe Mode: $_" }
    $state = [PSCustomObject]@{ gpuInput=$detectedGpu; mode="CONTROL"; logLevel="NORMAL"; profile="RECOMMENDED"; fpsCap=0; avgFps=0; rollbackDriver=$null; nvidiaDriverPath=$null; baselineAvg=$null; baselineP1=$null }
    if ($detectedGpu) { Save-SuiteState -State $state }
    $SCRIPT:Mode = "CONTROL"; $SCRIPT:LogLevel = "NORMAL"; $SCRIPT:Profile = "RECOMMENDED"; $SCRIPT:DryRun = $false
    }
}
$gpuInput = Get-PhaseGpuInput -State $state
$phaseStateInvalid = [string]::IsNullOrWhiteSpace([string]$gpuInput)

$PHASE = 2
$SCRIPT:PhaseTotal = 3
$SCRIPT:CurrentPhase = 2

function Register-Phase3UserHandoff {
    [CmdletBinding()]
    param()

    $phase3Script = if ($SCRIPT:DryRun) { Join-Path $CFG_WorkDir "PostReboot-Setup.ps1" } else { "$ScriptRoot\PostReboot-Setup.ps1" }
    $runOnceResult = Set-RunOnce "FRAMETIME_Phase3" $phase3Script -PassThru
    if ($SCRIPT:DryRun -and $runOnceResult.Status -eq "DryRun") {
        Write-Info "DRY-RUN: Phase 3 handoff command and target path were previewed; nothing was registered."
        return $true
    }
    if (-not $runOnceResult.Applied) {
        Write-Err "Phase 3 automatic handoff registration failed."
        Write-Host "  $([char]0x2139) What to do: after rebooting into Normal Mode, launch Phase 3 manually: START.bat -> [P]" -ForegroundColor Cyan
        return $false
    }
    return $true
}

$safeBootVerified = $false
$driverCleanupAttempted = $false
$driverRemoved = $false
$phase3Registered = $false
$backupInitialized = $false

try {
    # Validate we're actually in Safe Mode
    # $env:SAFEBOOT_OPTION is set by winload.exe on Safe Mode boot ("MINIMAL" or "NETWORK").
    # Reliable on all Windows 10/11 editions. If absent, we're in normal boot.
    $simulatedSafeMode = $SCRIPT:DryRun -and $SimulateSafeMode
    if (-not $env:SAFEBOOT_OPTION -and -not $simulatedSafeMode) {
        Write-Warn "This script needs Safe Mode to work properly, but you booted normally."
        Write-Host "  $([char]0x2139) Why it matters: Your GPU driver files are in use right now and cannot" -ForegroundColor Cyan
        Write-Host "    be cleanly removed. This could cause a black screen after restart." -ForegroundColor Cyan
        Write-Host "  $([char]0x2139) Recommended: Go back to START.bat and let it boot into Safe Mode." -ForegroundColor Cyan
        Write-Info "Aborted. No boot-state changes or GPU driver removal were performed. Boot into Safe Mode first (START.bat -> [1])."
        if ($phaseStateInvalid) {
            Write-Err "Phase 2 also stopped because state.json has no valid gpuInput value."
            throw [IO.InvalidDataException]::new("Phase 2 state validation failed: gpuInput must be a scalar value from 1 through 4.")
        }
        return
    }

    # An invalid vendor choice must not block Safe Mode recovery. Clear and
    # verify SafeBoot before backup initialization, then stop before any driver
    # or handoff operation. No rollback record is needed for removing the
    # temporary SafeBoot value created by Phase 1.
    if ($phaseStateInvalid) {
        Write-Section "Step 1 - Disable Safe Mode"
        if ($SCRIPT:DryRun) {
            Write-Host "  [DRY-RUN] Would remove and verify the Safe Mode boot flag." -ForegroundColor Magenta
            $safeBootVerified = $true
        } else {
            $safeBootResult = Clear-SafeBootVerified
            $safeBootVerified = $safeBootResult.Verified
            if (-not $safeBootVerified) {
                Write-Err "CRITICAL: $($safeBootResult.Message)"
                Write-Err "MANUAL RECOVERY (run in elevated cmd.exe):"
                Write-Err "  bcdedit /deletevalue safeboot"
                Write-Err "  bcdedit /enum {current} /v"
                Write-Err "  shutdown /r /t 0"
                throw [IO.InvalidDataException]::new("Phase 2 state validation failed and the Safe Mode flag could not be verified absent.")
            }
            Write-OK $safeBootResult.Message
        }

        Write-Err "Phase 2 cannot select a GPU vendor because state.json has no valid gpuInput value."
        Write-Err "The Safe Mode flag was cleared and verified. No driver removal or Phase 3 registration was attempted."
        Write-Host "  Restart into Normal Mode, rerun Phase 1, and select GPU branch 1, 2, 3, or 4." -ForegroundColor Cyan
        throw [IO.InvalidDataException]::new("Phase 2 state validation failed: gpuInput must be a scalar value from 1 through 4.")
    }

    if (-not $SCRIPT:DryRun) { Initialize-Log }
    Write-Banner 2 3 "Safe Mode  ·  GPU Driver Clean Removal"
    if ($SCRIPT:DryRun -and $SimulateSafeMode) {
        Write-Info "DRY-RUN: Safe Mode is simulated; no boot environment was changed."
    } else {
        Write-Info "Safe Mode active. GPU driver files are unlocked."
    }

    # Initialize backup inside try so finally releases the lock on error.
    if (-not $SCRIPT:DryRun) {
        Initialize-Backup
        $backupInitialized = $true
    }

    Write-Section "Step 1 - Disable Safe Mode"
    if ($SCRIPT:DryRun) {
        Write-Host "  [DRY-RUN] Would remove and verify the Safe Mode boot flag." -ForegroundColor Magenta
        if (-not $simulatedSafeMode) {
            Write-Info "Standalone Phase 2 preview stops here; use Run-Optimize.ps1 -FullDryRun for the complete lifecycle simulation."
            return
        }
        $safeBootVerified = $true
        Complete-Step $PHASE 1 "SafeMode off (DRY-RUN preview)"
    } else {
        $safeBootResult = Clear-SafeBootVerified
        $safeBootVerified = $safeBootResult.Verified
        if (-not $safeBootVerified) {
            Write-Err "CRITICAL: $($safeBootResult.Message)"
            Write-Err "No driver removal, Phase 3 registration, or restart will be attempted."
            Write-Err "MANUAL RECOVERY (run in elevated cmd.exe):"
            Write-Err "  bcdedit /deletevalue safeboot"
            Write-Err "  bcdedit /enum {current} /v"
            Write-Err "  shutdown /r /t 0"
            if (-not (Test-YoloProfile)) { Read-Host "  Press Enter after noting the recovery commands" }
            return
        }
        Write-OK $safeBootResult.Message
        Complete-Step $PHASE 1 "SafeMode off"
    }

    Write-Section "Step 2 - GPU Driver Clean Removal"
    $gpuName = switch ($gpuInput) {
        "1" {"NVIDIA"} "2" {"NVIDIA"} "3" {"AMD"} "4" {"Intel"}
    }

    if ($SCRIPT:DryRun) {
        Write-Info "Selected preview GPU vendor: $gpuName"
        Write-Info "A live run would remove matching display-driver packages and selected rebuildable residue using Windows CIM and pnputil."
        Write-Info "This preview renders the package-removal and vendor-cleanup plan without touching the Driver Store."
    } else {
        Write-Info "Detected GPU vendor: $gpuName"
        Write-Info "This removes matching display-driver packages and selected rebuildable residue using native PowerShell."
        Write-Info "Uses Windows CIM + pnputil and proceeds to vendor cleanup only after verified package removal."
    }

    # Older local state can contain a fixed-version rollback request. The alpha
    # does not use that unverified metadata to select an installer.
    if ($state.PSObject.Properties['rollbackDriver'] -and $state.rollbackDriver) {
        Write-Warn "Ignoring legacy rollbackDriver metadata; fixed-version rollback selection is not supported by this alpha."
    }

    Write-Blank
    $r = if ($SCRIPT:DryRun -or (Test-YoloProfile)) { "Y" } else { Read-Host "  Proceed with GPU driver removal? [Y/n]" }
    if ($r -match "^[nN]$") {
        Write-Warn "Skipped GPU driver removal."
        Skip-Step $PHASE 2 "DriverClean"

        # Ask whether to still proceed with Phase 3
        Write-Blank
        $rPhase3 = if (Test-YoloProfile) { "y" } else { Read-Host "  Still register Phase 3 for next boot? [y/N]" }
        if ($rPhase3 -match "^[jJyY]$") {
            Write-Section "Step 3 - Register Phase 3 for next boot"
            if (Register-Phase3UserHandoff) {
                $phase3Registered = $true
                Complete-Step $PHASE 3 "Phase 3 user handoff"
            }
        } else {
            Write-Info "Phase 3 not registered. Re-run from START.bat when ready."
            Skip-Step $PHASE 3 "Phase 3 user handoff"
        }
    } else {
        $driverCleanupAttempted = $true
        $driverCleanResult = Remove-GpuDriverClean -GpuVendor $gpuName -PassThru
        if ($driverCleanResult.CanCompleteStep -or ($SCRIPT:DryRun -and $driverCleanResult.Status -eq "DryRun")) {
            $driverRemoved = $true
            Complete-Step $PHASE 2 "DriverClean"

            # Register the per-user Phase 3 Run handoff after driver removal.
            Write-Section "Step 3 - Register Phase 3 for next boot"
            if (Register-Phase3UserHandoff) {
                $phase3Registered = $true
                Complete-Step $PHASE 3 "Phase 3 user handoff"
            }
        } else {
            Write-Err "GPU driver clean removal did not complete: $($driverCleanResult.Message)"
            Write-Host "  $([char]0x2139) What to do: review the warnings above, install or remove the driver manually if needed," -ForegroundColor Cyan
            Write-Host "    then use START.bat -> [P] to launch the manifest-verified published Phase 3 runtime." -ForegroundColor Cyan
        }
    }

    if ($phase3Registered -and (-not $driverCleanupAttempted -or $driverRemoved)) {
        Write-Blank
        if ($SCRIPT:DryRun) {
            Write-Host "  [DRY-RUN] Would restart into Normal Mode to continue with Phase 3." -ForegroundColor Magenta
            Write-PhaseSummary -PhaseLabel "PHASE 2" -DryRun -ContinuePreview
        } else {
            Write-Info "Restart to continue."
            $r2 = if (Test-YoloProfile) { "Y" } else { Read-Host "  Restart now? [Y/n]" }
            if ($r2 -notmatch "^[nN]$") { shutdown /r /t 0 /f }
        }
    } else {
        Write-Warn "Automatic restart is blocked because the Phase 3 handoff was not applied."
        Write-Host "  Resolve the error above, or use the documented manual recovery commands." -ForegroundColor Cyan
        if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  Press Enter to remain in this session" }
    }
} catch {
    if ($null -ne $PreviewState) {
        throw
    }
    # Unhandled exception - display recovery instructions so user isn't stuck.
    Write-Host "" -ForegroundColor Red
    Write-Host "  ╔══════════════════════════════════════════════════════════╗" -ForegroundColor Red
    Write-Host "  ║  UNEXPECTED ERROR DURING SAFE MODE SCRIPT               ║" -ForegroundColor Red
    Write-Host "  ╚══════════════════════════════════════════════════════════╝" -ForegroundColor Red
    Write-Host "  Error: $_" -ForegroundColor Red
    if ($_.ScriptStackTrace) { Write-Host "  Stack: $($_.ScriptStackTrace)" -ForegroundColor DarkGray }

    # Recovery registration is safe only after verified normal-boot configuration
    # and once cleanup has actually begun. Initialize-Backup failures therefore
    # cannot create a Phase 3 handoff.
    try {
        if (-not $SCRIPT:DryRun -and $safeBootVerified -and $driverCleanupAttempted -and -not $phase3Registered -and -not (Test-StepCompleted $PHASE 3)) {
            if (Register-Phase3UserHandoff) {
                $phase3Registered = $true
                Write-Host "" -ForegroundColor Green
                Write-Host "  $([char]0x2714) Phase 3 registered - it will start automatically on next boot." -ForegroundColor Green
            }
        }
    } catch {
        Write-Host "" -ForegroundColor Yellow
        Write-Host "  $([char]0x26A0) Could not register Phase 3 for next boot." -ForegroundColor Yellow
    }

    Write-Host "" -ForegroundColor Yellow
    Write-Host "  RECOVERY:" -ForegroundColor Yellow
    Write-Host "  Step 1 (bcdedit) runs first. If it completed, next boot = Normal Mode." -ForegroundColor White
    Write-Host "  If you're stuck in Safe Mode, run in elevated cmd.exe:" -ForegroundColor White
    Write-Host "    bcdedit /deletevalue safeboot" -ForegroundColor Cyan
    Write-Host "    shutdown /r /t 0" -ForegroundColor Cyan
    Write-Host "" -ForegroundColor White
    Write-Host "  If GPU driver was partially removed, Windows will load Basic Display" -ForegroundColor White
    Write-Host "  Adapter on next boot. Phase 3 will handle clean driver installation." -ForegroundColor White
    Write-Host "  If Phase 3 does not start automatically, run it from" -ForegroundColor White
    Write-Host "  START.bat -> [P] (manifest-verified published Phase 3 runtime)." -ForegroundColor White
    Write-Host "" -ForegroundColor White
    if (-not $phaseStateInvalid -and -not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  Press Enter to exit" }
    if ($phaseStateInvalid) { throw }
} finally {
    # Release only the lock acquired by this invocation. Initialize-Backup can
    # reject an active lock owned by another process before acquiring one.
    if ($backupInitialized) { Remove-BackupLock }
}
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-SafeModeDriverCleanEntryPoint -SmokeTest:$SmokeTest
}
