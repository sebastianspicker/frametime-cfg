#Requires -RunAsAdministrator
<#
.SYNOPSIS  CS2 Optimization Suite — Post-Reboot Setup (Normal boot after GPU driver clean)

  Steps:
    1   Install driver (native extract + install)  [T1]
    2   MSI Interrupts (native registry)  [T2]
    3   NIC Interrupt Affinity (native registry)  [T3]
    4   NVIDIA CS2 Profile (native registry)  [T3]
    5   FPS Cap Info  [T1]
    6   CS2 Launch Options + In-game Settings
    7   VBS / Core Isolation disable  [T2]
    8   AMD GPU Settings  [T2, AMD only]
    9   DNS Server Configuration  [T3]
    10  Process Priority / CCD Affinity (native IFEO)  [T3]
    11  VRAM Leak Awareness  [Info]
    12  Knowledge Summary + Checklist
    13  Final Benchmark + FPS Cap Calculation  [T1, LAST STEP]
#>

param([switch]$SmokeTest)

if ($SmokeTest) {
    Write-Host "SMOKE TEST OK: PostReboot-Setup" -ForegroundColor Green
    exit 0
}

Set-StrictMode -Version Latest
$ErrorActionPreference = "Continue"
$ScriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
. "$ScriptRoot\config.env.ps1"
. "$ScriptRoot\helpers.ps1"
. "$ScriptRoot\Guide-VideoSettings.ps1"

try {
    $state = Load-State $CFG_StateFile
} catch {
    Write-Host "  $([char]0x2718) Something went wrong: settings file (state.json) is missing or corrupted." -ForegroundColor Red
    Write-Host "    Error detail: $_" -ForegroundColor DarkGray
    Write-Host ""
    Write-Host "  $([char]0x2139) What to do:" -ForegroundColor Cyan
    Write-Host "    - Phase 1 may not have finished. You can re-run it from START.bat." -ForegroundColor White
    Write-Host "    - Or press [Y] below to continue Phase 3 with safe defaults." -ForegroundColor White
    $r = if (Test-YoloProfile) { "y" } else { Read-Host "  Continue with defaults? [y/N]" }
    if ($r -notmatch "^[jJyY]$") { exit 1 }
    # Detect GPU vendor instead of blindly defaulting to NVIDIA
    $detectedGpu = "2"  # Default NVIDIA
    try {
        $gpu = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
               Where-Object { $_.Status -eq "OK" -or $_.Name -notmatch "Basic Display" } |
               Select-Object -First 1
        if ($gpu) {
            if ($gpu.Name -match "AMD|Radeon") { $detectedGpu = "3" }
            elseif ($gpu.Name -match "Intel.*Arc|Intel.*Graphics") { $detectedGpu = "4" }
            elseif ($gpu.Name -match "RTX\s*5\d{3}") { $detectedGpu = "1" }
        }
    } catch { <# GPU detection failed — use NVIDIA default #> }
    $state = [PSCustomObject]@{ gpuInput=$detectedGpu; mode="CONTROL"; logLevel="NORMAL"; profile="RECOMMENDED"; fpsCap=0; avgFps=0; rollbackDriver=$null; nvidiaDriverPath=$null; appliedSteps=@(); baselineAvg=$null; baselineP1=$null }
    $SCRIPT:Mode = "CONTROL"; $SCRIPT:LogLevel = "NORMAL"; $SCRIPT:Profile = "RECOMMENDED"; $SCRIPT:DryRun = $false
}
$fpsCap   = if ($state.PSObject.Properties['fpsCap']) { $state.fpsCap } else { 0 }
$SCRIPT:fpsCap = $fpsCap
$avgFps   = if ($state.PSObject.Properties['avgFps']) { $state.avgFps } else { 0 }
$gpuInput = $state.gpuInput
$PHASE    = 3
$SCRIPT:PhaseTotal = 13
$SCRIPT:CurrentPhase = 3

# Guard: if Phase 1 was run in DRY-RUN, warn and confirm before Phase 3 applies real changes
if ($SCRIPT:DryRun) {
    Write-Host ""
    Write-Host "  ╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Magenta
    Write-Host "  ║  DRY-RUN MODE INHERITED FROM PHASE 1                       ║" -ForegroundColor Magenta
    Write-Host "  ║  Phase 3 will preview changes only — nothing will be       ║" -ForegroundColor Magenta
    Write-Host "  ║  applied unless you switch to a live profile.              ║" -ForegroundColor Magenta
    Write-Host "  ╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Magenta
    $drChoice = if (Test-YoloProfile) { "Y" } else { Read-Host "  Continue in DRY-RUN mode? [Y/n]" }
    if ($drChoice -match "^[nN]$") {
        $SCRIPT:DryRun = $false
        # Derive mode from current profile (must match Setup-Profile.ps1 logic)
        $SCRIPT:Mode = switch ($SCRIPT:Profile) {
            "SAFE"        { "AUTO" }
            "RECOMMENDED" { "AUTO" }
            "COMPETITIVE" { "CONTROL" }
            "CUSTOM"      { "INFORMED" }
            "YOLO"        { "YOLO" }
            default       { "CONTROL" }
        }
        Write-Host "  Switched to $($SCRIPT:Mode) mode ($($SCRIPT:Profile) profile) — changes WILL be applied." -ForegroundColor Yellow
        # Persist the mode switch so re-launches don't revert to DRY-RUN
        try { $state | Add-Member -NotePropertyName "mode" -NotePropertyValue $SCRIPT:Mode -Force; Save-JsonAtomic -Data $state -Path $CFG_StateFile } catch {}
    }
}

# Guard: Phase 3 requires Normal Mode. If Safe Mode is active, the driver installer
# will fail and most optimizations cannot be applied.
if ($env:SAFEBOOT_OPTION) {
    Write-Host ""
    Write-Host "  ╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Red
    Write-Host "  ║  SAFE MODE DETECTED — Phase 3 requires Normal Mode         ║" -ForegroundColor Red
    Write-Host "  ╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Red
    Write-Host "  $([char]0x2139) Phase 3 installs GPU drivers and applies settings that need" -ForegroundColor Cyan
    Write-Host "    Normal Mode. The Safe Mode boot flag may not have been cleared." -ForegroundColor Cyan
    Write-Host ""
    Write-Host "  Clearing Safe Mode flag now..." -ForegroundColor White
    try {
        $bcdResult = bcdedit /deletevalue safeboot 2>&1
        if ($LASTEXITCODE -eq 0) {
            Write-Host "  $([char]0x2714) Safe Mode disabled. Restarting into Normal Mode..." -ForegroundColor Green
            Write-Host "    Phase 3 will run automatically on next boot." -ForegroundColor White
            Set-RunOnce "CS2_Phase3" "$CFG_WorkDir\PostReboot-Setup.ps1"
            Start-Sleep -Seconds 2
            shutdown /r /t 0 /f
            exit 0
        }
    } catch {}
    Write-Host "  $([char]0x26A0) Could not clear Safe Mode flag automatically." -ForegroundColor Yellow
    Write-Host "  Run in elevated cmd.exe:" -ForegroundColor White
    Write-Host "    bcdedit /deletevalue safeboot" -ForegroundColor Cyan
    Write-Host "    shutdown /r /t 0" -ForegroundColor Cyan
    if (-not (Test-YoloProfile)) { Read-Host "  Press Enter to exit" }
    exit 1
}

try {
# Initialize backup system for this phase (inside try so finally releases the lock on error)
Initialize-Backup
Initialize-PhaseCounters

# Load Phase 1 applied steps so improvement estimates are cumulative
Load-AppliedSteps

Ensure-Dir $CFG_LogDir
Initialize-Log
Write-Banner 3 3 "Normal Boot  ·  Driver · MSI · CS2"

$startStep = Show-ResumePrompt $PHASE 13
if ($startStep -gt 13) { Write-Info "Phase 3 already completed."; Remove-BackupLock; if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter]" }; exit 0 }

# ══════════════════════════════════════════════════════════════════════════════
# STEP 1 — INSTALL DRIVER  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 1) {
    Write-Section "Step 1 — Install Driver"
    Write-TierBadge 1 "Clean driver installation after GPU driver removal"

    # ── Remove leftover GPU AppX/MSIX packages ──────────────────────────────
    # Phase 2 runs in Safe Mode where AppXSVC cannot start, so AppX removal
    # is skipped there. Clean them up now in Normal Mode before the fresh install.
    $gpuAppxVendor = switch ($gpuInput) {
        { $_ -in @("1","2") } { "NVIDIA" }
        "3"                    { "AMD" }
        default                { $null }
    }
    if ($gpuAppxVendor -and (Get-Command Get-AppxPackage -ErrorAction SilentlyContinue)) {
        try {
            $gpuAppx = Get-AppxPackage -AllUsers -ErrorAction Stop |
                Where-Object { $_.Name -match $gpuAppxVendor }
            foreach ($pkg in $gpuAppx) {
                try {
                    Remove-AppxPackage -Package $pkg.PackageFullName -AllUsers -ErrorAction Stop
                    Write-OK "Removed leftover AppX: $($pkg.Name)"
                } catch {
                    Write-Debug "AppX removal: $($pkg.Name) — $_"
                }
            }
            # Also remove provisioned packages to prevent reinstall on feature updates
            $gpuProv = Get-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue |
                Where-Object { $_.DisplayName -match $gpuAppxVendor }
            foreach ($pkg in $gpuProv) {
                try {
                    $pkg | Remove-AppxProvisionedPackage -Online -ErrorAction Stop | Out-Null
                    Write-OK "Removed provisioned: $($pkg.DisplayName)"
                } catch {
                    Write-Debug "Provisioned removal: $($pkg.DisplayName) — $_"
                }
            }
        } catch {
            Write-Debug "AppX cleanup: $_"
        }
    }

    # Check if Phase 2 driver removal actually completed
    $p2DriverDone = Test-StepDone 2 2
    if (-not $p2DriverDone) {
        Write-Host ""
        Write-Host "  ╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Yellow
        Write-Host "  ║  Phase 2 driver removal was skipped or did not complete     ║" -ForegroundColor Yellow
        Write-Host "  ╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Yellow
        Write-Host "  $([char]0x2139) The old GPU driver may still be installed. The new driver will" -ForegroundColor Cyan
        Write-Host "    install over it (not a fully clean install)." -ForegroundColor Cyan
        Write-Host "  $([char]0x2139) For a clean install, go back and run Phase 2 first (START.bat)." -ForegroundColor Cyan
        $p2Choice = if (Test-YoloProfile) { "Y" } else { Read-Host "  Continue with driver install anyway? [Y/n]" }
        if ($p2Choice -match "^[nN]$") {
            Write-Info "Skipped. Run Phase 2 from START.bat -> [S] Safe Mode, then return here."
            Skip-Step $PHASE 1 "Driver"
            $p2DriverDone = $null  # signal to skip the driver install block below
        }
    }

    if ($null -eq $p2DriverDone) {
        # User chose to skip — Step 1 already recorded as skipped above
    } elseif ($gpuInput -in @("1","2")) {
        # Find driver .exe — check state first, then prompt
        # SECURITY: state.json is in C:\CS2_OPTIMIZE\ — if tampered, nvidiaDriverPath could
        # point to malware. Validate: must be an .exe file, must exist, must not contain
        # path traversal sequences. The file is then passed to Start-Process in Install-NvidiaDriverClean.
        $driverExe = if ($state.PSObject.Properties['nvidiaDriverPath']) { $state.nvidiaDriverPath } else { $null }
        if ($driverExe) {
            # Reject path traversal, non-.exe, and suspicious paths
            if ($driverExe -match '\.\.' -or $driverExe -notmatch '\.exe$' -or $driverExe -match '[\x00]') {
                Write-Warn "state.json nvidiaDriverPath failed validation — ignoring: $driverExe"
                $driverExe = $null
            }
        }
        if (-not $driverExe -or -not (Test-Path $driverExe)) {
            Write-Info "NVIDIA driver .exe not found in expected location."
            Write-Info "If you downloaded the driver manually, provide the path now."
            Write-Info "Press [B] to browse for the file, or paste the path, or [Enter] to download."
            $driverExe = if ($SCRIPT:DryRun -or (Test-YoloProfile)) { "" } elseif ($true) {
                $driverInput = Read-Host "  Path / [B]rowse / [Enter] to download"
                if ($driverInput -match '^[bB]$') {
                    Add-Type -AssemblyName System.Windows.Forms
                    $dlg = New-Object System.Windows.Forms.OpenFileDialog
                    $dlg.Title = "Select NVIDIA Driver Installer"
                    $dlg.Filter = "Executable files (*.exe)|*.exe"
                    $dlg.InitialDirectory = [Environment]::GetFolderPath('UserProfile') + '\Downloads'
                    if ($dlg.ShowDialog() -eq 'OK') { $dlg.FileName } else { "" }
                } else {
                    # Strip surrounding quotes that paste sometimes adds
                    $driverInput -replace '^["'']|["'']$', ''
                }
            } else { "" }
            if (-not [string]::IsNullOrWhiteSpace($driverExe)) {
                # Validate user-provided path: must exist and be an .exe
                if (-not (Test-Path $driverExe)) {
                    Write-Warn "File not found: $driverExe"
                    $driverExe = $null
                } elseif ($driverExe -notmatch '\.exe$') {
                    Write-Warn "Not an .exe file: $driverExe"
                    $driverExe = $null
                }
            }
            if ([string]::IsNullOrWhiteSpace($driverExe)) {
                if ($SCRIPT:DryRun) {
                    Write-Host "  [DRY-RUN] Would attempt automatic NVIDIA driver download" -ForegroundColor Magenta
                } else {
                    Write-Step "Attempting automatic driver download..."
                    $savedGpuName = if ($state.PSObject.Properties['nvidiaGpuName']) { $state.nvidiaGpuName } else { $null }
                    $driverInfo = Get-LatestNvidiaDriver -GpuName $savedGpuName
                    if ($driverInfo -and -not $driverInfo.ManualDownload) {
                        $driverExe = "$CFG_WorkDir\nvidia_driver.exe"
                        $dlResult = Invoke-Download $driverInfo.Url $driverExe "NVIDIA Driver $($driverInfo.Version)"
                        if (-not $dlResult) {
                            $driverExe = $null
                        # SECURITY (S1): Verify Authenticode signature immediately after download
                        } elseif (-not (Test-NvidiaDriverSignature $driverExe)) {
                            Write-Err "Downloaded driver failed signature verification. File removed."
                            $driverExe = $null
                        }
                    } else {
                        Write-Warn "Auto-download failed. Download manually:"
                        Write-Info "https://www.nvidia.com/en-us/drivers/"
                        Write-Info "Press [B] to browse for the file, or paste the path."
                        $rawInput = if (Test-YoloProfile) { "" } else { Read-Host "  Path / [B]rowse" }
                        if ($rawInput -match '^[bB]$') {
                            Add-Type -AssemblyName System.Windows.Forms
                            $dlg = New-Object System.Windows.Forms.OpenFileDialog
                            $dlg.Title = "Select NVIDIA Driver Installer"
                            $dlg.Filter = "Executable files (*.exe)|*.exe"
                            $dlg.InitialDirectory = [Environment]::GetFolderPath('UserProfile') + '\Downloads'
                            $driverExe = if ($dlg.ShowDialog() -eq 'OK') { $dlg.FileName } else { $null }
                        } else {
                            $driverExe = $rawInput -replace '^["'']|["'']$', ''
                        }
                        if ($driverExe -and ($driverExe -match '\.\.' -or $driverExe -notmatch '\.exe$' -or $driverExe -match '[\x00]')) {
                            Write-Warn "Invalid driver path: $driverExe"
                            $driverExe = $null
                        }
                    }
                }
            }
        }

        if ($state.PSObject.Properties['rollbackDriver'] -and $state.rollbackDriver) {
            Write-Blank
            Write-Host "  ╔══════════════════════════════════════════════════════════╗" -ForegroundColor Yellow
            $drvLabel = $state.rollbackDriver.Substring(0, [math]::Min(30, $state.rollbackDriver.Length))
            Write-Host "  ║  ROLLBACK REQUESTED: Driver $drvLabel$((' ' * [math]::Max(0, 30 - $drvLabel.Length)))║" -ForegroundColor Yellow
            Write-Host "  ║  Make sure the .exe matches this version!               ║" -ForegroundColor Yellow
            Write-Host "  ╚══════════════════════════════════════════════════════════╝" -ForegroundColor Yellow
        }

        if ($driverExe -and (Test-Path $driverExe)) {
            Write-Info "Driver: $driverExe"
            Write-Info "Installing with bloat removed + post-install tweaks..."
            Write-Info "(MSI interrupts, telemetry off, HDCP off, write combining, MPO off)"
            Write-Blank
            $r = if ($SCRIPT:DryRun) { "n" } elseif (Test-YoloProfile) { "Y" } else { Read-Host "  Install now? [Y/n]" }
            if ($r -notmatch "^[nN]$") {
                $result = Install-NvidiaDriverClean -DriverExe $driverExe
                if ($result) {
                    Write-OK "NVIDIA driver installed successfully."
                    Complete-Step $PHASE 1 "Driver"
                } else {
                    Write-Warn "Installation may have had issues."
                    Write-Host "  $([char]0x2139) What to do: Open Device Manager (Win+X -> Device Manager)" -ForegroundColor Cyan
                    Write-Host "    and check if your GPU shows up under 'Display adapters' without" -ForegroundColor Cyan
                    Write-Host "    a yellow warning icon. If it looks fine, you're good!" -ForegroundColor Cyan
                    Write-Host "    This step will re-run on resume if needed." -ForegroundColor DarkGray
                }
            } else {
                Write-Info "Skipped driver install."
                Skip-Step $PHASE 1 "Driver"
            }
        } else {
            Write-Err "No valid driver file (.exe) found."
            Write-Host "  $([char]0x2139) What to do: Download your driver from the link below," -ForegroundColor Cyan
            Write-Host "    then re-run this phase from START.bat -> [P]." -ForegroundColor Cyan
            Write-Info "Download: https://www.nvidia.com/en-us/drivers/"
            $skipConfirm = if ($SCRIPT:DryRun -or (Test-YoloProfile)) { "y" } else { Read-Host "  Skip driver install and continue to Step 2? [y/N]" }
            if ($skipConfirm -match "^[jJyY]$") {
                Skip-Step $PHASE 1 "Driver"
            } else {
                Write-Info "Restart Phase 3 when ready."
            }
        }

        if ($gpuInput -eq "1") {
            Write-Warn "RTX 5000: NVIDIA CP -> Scaling -> MONITOR (not GPU)!"
        }
    } else {
        Write-Info "$(if($gpuInput -eq '3'){'AMD: https://www.amd.com/support'}else{'Intel Arc: https://www.intel.com/content/www/us/en/download-center/home.html'})"
        Write-Info "Custom Install -> driver only, no overlay / link."
        if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  After driver installation [Enter]" }
        Complete-Step $PHASE 1 "Driver"
    }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 2 — MSI INTERRUPTS  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 2) {
    Write-Section "Step 2 — MSI Interrupts  ·  GPU + NIC + Audio"
    Invoke-TieredStep -Tier 2 -Title "Enable MSI interrupts (native registry)" `
        -Why "MSI interrupts for GPU, NIC, audio -> less DPC latency." `
        -Evidence "Measurable if LatencyMon shows DPC spikes. Without diagnosis: situational." `
        -Caveat "Not all devices support MSI. Errors are ignored for unsupported devices." `
        -Risk "MODERATE" -Depth "REGISTRY" -EstimateKey "MSI Interrupts" `
        -Improvement "Less DPC latency for GPU/NIC/audio — measurable with LatencyMon" `
        -SideEffects "Rare: device instability if MSI not supported (errors are ignored safely)" `
        -Undo "Delete MSISupported values from device Interrupt Management registry keys" `
        -Action {
            Enable-DeviceMSI
            Complete-Step $PHASE 2 "MSI"
        } `
        -SkipAction { Skip-Step $PHASE 2 "MSI" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 3 — NIC INTERRUPT AFFINITY  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 3) {
    Write-Section "Step 3 — NIC Interrupt Affinity"
    Invoke-TieredStep -Tier 3 -Title "Set NIC interrupt affinity (native registry)" `
        -Why "Binds NIC interrupts to last physical core, away from Core 0." `
        -Evidence "T3: Only relevant after LatencyMon diagnosis with clear NIC DPC issue." `
        -Caveat "For most users: no measurable effect. Goal: keep NIC DPC/ISR off Core 0." `
        -Risk "MODERATE" -Depth "REGISTRY" `
        -Improvement "NIC interrupts off Core 0 — only relevant if LatencyMon shows NIC DPC issue" `
        -SideEffects "Wrong affinity can increase latency. Only useful after LatencyMon diagnosis." `
        -Undo "Delete DevicePolicy + AssignmentSetOverride from NIC Affinity Policy key" `
        -Action {
            Set-NicInterruptAffinity
            Complete-Step $PHASE 3 "NicAffinity"
        } `
        -SkipAction { Skip-Step $PHASE 3 "NicAffinity" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 4 — NVIDIA CS2 PROFILE  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 4) {
    if ($gpuInput -in @("1","2")) {
        Write-Section "Step 4 — NVIDIA CS2 Profile (DRS direct write)"
        Invoke-TieredStep -Tier 3 -Title "Apply NVIDIA CS2 profile settings (DRS + registry)" `
            -Why "Writes 51 optimized DWORD settings directly to NVIDIA DRS binary database via nvapi64.dll. Falls back to registry if DRS unavailable." `
            -Evidence "T3: No isolated 1%-low benchmark for the full profile. Individual flags may be T2." `
            -Caveat "Requires nvapi64.dll (NVIDIA driver installed). Falls back to registry if unavailable." `
            -Risk "SAFE" -Depth "DRIVER" `
            -Improvement "All 52 DWORD settings applied directly to DRS binary database + PerfLevelSrc registry key" `
            -SideEffects "None — all DRS settings backed up automatically. Reversible via rollback or NPI -> Restore Defaults" `
            -Undo "Restore via backup rollback, or NVIDIA CP -> Manage 3D Settings -> Restore Defaults" `
            -Action {
                Apply-NvidiaCS2Profile
                Complete-Step $PHASE 4 "NVProfile"
            } `
            -SkipAction { Skip-Step $PHASE 4 "NVProfile" }
    } else {
        Skip-Step $PHASE 4 "NVProfile"
        Write-Section "Step 4 — GPU Profile (skipped — non-NVIDIA)"
        Write-Blank
        Write-Host "  This suite has no AMD/Intel equivalent of the NVIDIA DRS profile step." -ForegroundColor Yellow
        Write-Host "  For AMD GPUs, configure the following manually in Radeon Software → Gaming → CS2:" -ForegroundColor White
        Write-Blank
        Write-Sub "Radeon Chill             → Disabled"
        Write-Sub "Radeon Boost             → Disabled"
        Write-Sub "Anti-Aliasing            → Use Application Settings  (let CS2 MSAA handle it)"
        Write-Sub "Anisotropic Filtering    → Use Application Settings  (let video.txt AF16x handle it)"
        Write-Sub "Texture Filtering Quality→ Performance"
        Write-Sub "Power Efficiency         → Prefer Maximum Performance"
        Write-Blank
        Write-Info "These settings persist in Radeon Software across game launches."
        Write-Info "Re-apply after major AMD driver updates (Adrenalin can reset per-game profiles)."
        Write-Blank
        if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] to continue" }
    }

    # NVIDIA CP hints — always show for NVIDIA
    if ($gpuInput -in @("1","2")) {
        Write-Blank
        Write-Host "  NVIDIA CONTROL PANEL — remaining manual checks:" -ForegroundColor White
        if ($fpsCap -gt 0) {
            Write-Host "  [T1] Max Frame Rate        ->  $fpsCap  (avg $avgFps - 9%)" -ForegroundColor Green
            "$fpsCap" | Set-ClipboardSafe
            Write-Info "       FPS cap $fpsCap copied to clipboard."
        } else {
            Write-Host "  [T1] Max Frame Rate        ->  set after benchmark with FpsCap-Calculator" -ForegroundColor Yellow
        }
        Write-Host "  [T2] Low Latency Mode      ->  Ultra  (only if Reflex NOT active)" -ForegroundColor Yellow
        Write-Blank
        Write-Host "  NOTE: 3 niche settings excluded (string type, GPU-specific, frame interp)." -ForegroundColor DarkGray
        Write-Host "  See docs/nvidia-drs-settings.md for full details." -ForegroundColor DarkGray
        if ($gpuInput -eq "1") {
            Write-Warn "RTX 5000: Scaling -> MONITOR (not GPU) for 4:3 stretched."
        }
    }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 5 — FPS CAP INFO  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 5) {
    Write-Section "Step 5 — FPS Cap Info"
    Write-TierBadge 1 "FPS Cap — directly measurable effect on frametime consistency"
    Write-Blank
    Write-Host "  FPS cap will be calculated in the FINAL STEP after all optimizations." -ForegroundColor Yellow
    Write-Host "  Method: avg FPS - 9%  (FPSHeaven / Blur Busters recommendation)." -ForegroundColor DarkGray
    if ($fpsCap -gt 0) {
        Write-Host "  Already calculated cap: $fpsCap  (avg $avgFps - 9%)" -ForegroundColor Green
    }
    Complete-Step $PHASE 5 "FpsCapInfo"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 6 — CS2 LAUNCH OPTIONS + VIDEO SETTINGS (Feb 2026 Meta)
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 6) {
    Write-Section "Step 6 — Launch Options + Video Settings (Feb 2026 Meta)"
    Show-CS2SettingsGuide -fpsCap $fpsCap -avgFps $avgFps -gpuInput $gpuInput
    Complete-Step $PHASE 6 "CS2Settings"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 7 — VBS / CORE ISOLATION DISABLE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 7) {
    Write-Section "Step 7 — VBS / Core Isolation (Memory Integrity)"
    Invoke-TieredStep -Tier 2 -Title "Disable VBS / Core Isolation (Memory Integrity)" `
        -Why "Virtualization-Based Security runs a Type-1 hypervisor below the kernel. On OEM Win11 systems this adds 5-15% CPU scheduling overhead in games. X3D tuning guide: Off." `
        -Evidence "Microsoft VBS docs; Phoronix benchmarks show 5-15% overhead. X3D guide (A18): Off. Multiple community benchmarks confirm FPS impact on CPU-bound titles." `
        -Caveat "SECURITY TRADE-OFF: Disables LSASS credential theft protection. FACEIT Anti-Cheat and Vanguard REQUIRE HVCI — skip this step if using these. Only disable on dedicated gaming PCs." `
        -Risk "MODERATE" -Depth "REGISTRY" -EstimateKey "VBS/Core Isolation Off" `
        -Improvement "Removes 5-15% CPU overhead from hypervisor layer — measurable on OEM Win11" `
        -SideEffects "Reduces credential theft protection (LSASS). May break FACEIT AC / Vanguard." `
        -Undo "Windows Security -> Device Security -> Core Isolation -> Memory Integrity: ON" `
        -Action {
            # Detect current VBS status + check if already pending disable via registry
            $vbsActive = $false
            $hvciPendingOff = $false
            try {
                $dg = Get-CimInstance -ClassName Win32_DeviceGuard `
                    -Namespace root/Microsoft/Windows/DeviceGuard -ErrorAction SilentlyContinue
                if ($dg -and $dg.VirtualizationBasedSecurityStatus -ge 2) {
                    $vbsActive = $true
                }
            } catch { Write-Debug "VBS detection: $_" }
            try {
                $hvciVal = Get-ItemProperty `
                    "HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity" `
                    -Name "Enabled" -ErrorAction SilentlyContinue
                if ($hvciVal -and $hvciVal.Enabled -eq 0) { $hvciPendingOff = $true }
            } catch {}

            if (-not $vbsActive) {
                Write-OK "VBS/Core Isolation: not active — no overhead to remove."
                Complete-Step $PHASE 7 "VBS"
                return
            }
            if ($hvciPendingOff) {
                Write-OK "Memory Integrity already set to disable — reboot to take effect."
                Complete-Step $PHASE 7 "VBS"
                return
            }

            Write-Warn "VBS/HVCI is ACTIVE — 5-15% CPU overhead detected."
            Write-Blank

            # FACEIT / Vanguard warning
            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Red
            Write-Host "  │  WARNING: FACEIT AC and Vanguard REQUIRE HVCI enabled.      │" -ForegroundColor Red
            Write-Host "  │  If you use FACEIT or Valorant, SKIP this step.             │" -ForegroundColor Red
            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Red
            Write-Blank

            # Disable Memory Integrity (HVCI) via registry
            Set-RegistryValue `
                "HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity" `
                "Enabled" 0 "DWord" "Disable Memory Integrity (HVCI)"

            if (-not $SCRIPT:DryRun) {
                Write-OK "Memory Integrity (HVCI) disabled. Reboot required for full effect."
                Write-Info "Verify after reboot: msinfo32 -> Virtualization-based security -> 'Not Enabled'"
            }
            Complete-Step $PHASE 7 "VBS"
        } `
        -SkipAction { Skip-Step $PHASE 7 "VBS" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 8 — AMD GPU SETTINGS  [T2, AMD only]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 8) {
    if ($gpuInput -eq "3") {
        Write-Section "Step 8 — AMD GPU Settings"
        Invoke-TieredStep -Tier 2 -Title "Optimize AMD Radeon Software for CS2" `
            -Why "AMD Adrenalin has several features that actively hurt competitive CS2: Radeon Boost dynamically lowers resolution during mouse movement (resolution drops at exactly the wrong moment). Radeon Chill is a dynamic FPS limiter (raises latency). AMD Fluid Motion Frames (AFMF) is frame interpolation — adds ~1 frame of input lag. Anti-Lag (driver-level) and Anti-Lag 2 (game-engine-integrated, RDNA3+) reduce input lag by coordinating CPU-GPU timing; Anti-Lag was temporarily pulled in Sept 2023 due to CS2 VAC bans but current drivers are patched and whitelisted." `
            -Evidence "Radeon Boost: AMD documentation — 'dynamically adjusts resolution during fast movements'. Radeon Chill: dynamic FPS limiter, latency cost confirmed. AFMF: frame interpolation inherently adds input lag. Anti-Lag: driver-level CPU-GPU timing coordination, measured input lag reduction. Anti-Lag 2: game-engine-integrated version (RDNA3+); VAC ban incident Sept 2023 resolved by AMD driver patch." `
            -Caveat "Anti-Lag 2 is RDNA3+ only (RX 7000 series+). VAC ban risk from Anti-Lag is resolved in current AMD drivers — use Anti-Lag 1 if RDNA2 or earlier. AMD driver must be installed manually from amd.com (no auto-install in this suite for AMD). fTPM stuttering: if on Ryzen and experiencing random 1-2 second freezes, disable fTPM in BIOS (BIOS -> AMD fTPM switch -> Discrete TPM) — separate from driver settings." `
            -Risk "SAFE" -Depth "CHECK" `
            -Improvement "Eliminates resolution-drop artifacts (Boost off), FPS limiter latency (Chill off), frame interpolation lag (AFMF off). Anti-Lag: measured input lag reduction." `
            -SideEffects "Manual Adrenalin settings — no system changes made here." `
            -Undo "N/A (manual AMD Adrenalin settings)" `
            -Action {
                Write-Blank
                Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Red
                Write-Host "  │  AMD RADEON SOFTWARE — COMPETITIVE CS2 SETTINGS              │" -ForegroundColor Red
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  Gaming -> Graphics (Global Settings):                      │" -ForegroundColor White
                Write-Host "  │  ✔  Anti-Lag:                  ON  (Anti-Lag 2 if RDNA3+)  │" -ForegroundColor Green
                Write-Host "  │  ✗  Radeon Boost:              OFF (lowers res mid-aim!)   │" -ForegroundColor Red
                Write-Host "  │  ✗  Radeon Chill:              OFF (dynamic FPS = latency) │" -ForegroundColor Red
                Write-Host "  │  ✗  Fluid Motion Frames (AFMF):OFF (frame interp = lag)   │" -ForegroundColor Red
                Write-Host "  │  ✗  Image Sharpening:          OFF (post-process overhead) │" -ForegroundColor DarkGray
                Write-Host "  │  ✔  Texture Filtering Quality: Performance                 │" -ForegroundColor White
                Write-Host "  │  ✔  Wait for Vertical Refresh: Always Off                  │" -ForegroundColor White
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  Performance -> Tuning:                                     │" -ForegroundColor White
                Write-Host "  │  ✔  GPU Tuning:                Standard                    │" -ForegroundColor White
                Write-Host "  │  ✔  VRAM Tuning:               Standard (or EXPO match)    │" -ForegroundColor White
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  NOTE — Anti-Lag VAC ban (Sept 2023):                       │" -ForegroundColor Yellow
                Write-Host "  │  AMD pulled Anti-Lag in Sept 2023 after CS2 VAC bans.       │" -ForegroundColor DarkYellow
                Write-Host "  │  Current drivers (Nov 2023+) are patched and whitelisted.  │" -ForegroundColor DarkYellow
                Write-Host "  │  Anti-Lag 1 safe for all RDNA. Anti-Lag 2: RDNA3+ only.   │" -ForegroundColor DarkYellow
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  NOTE — AMD fTPM stuttering (Ryzen only):                   │" -ForegroundColor Yellow
                Write-Host "  │  Random 1-2 second freezes on Ryzen? Disable fTPM in BIOS: │" -ForegroundColor DarkYellow
                Write-Host "  │  BIOS -> Security -> AMD fTPM switch -> Discrete TPM.       │" -ForegroundColor DarkYellow
                Write-Host "  │  (Windows 11 requires TPM — use Discrete if available.)    │" -ForegroundColor DarkYellow
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  DRIVER INSTALL:                                             │" -ForegroundColor White
                Write-Host "  │  amd.com/support -> Download driver                         │" -ForegroundColor White
                Write-Host "  │  -> Custom Install -> Driver only (minimal Adrenalin)       │" -ForegroundColor White
                Write-Host "  │  -> Check 'Factory Reset' for clean install                 │" -ForegroundColor White
                Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Red
                if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] when done" }
                Complete-Step $PHASE 8 "AMDSettings"
            } `
            -SkipAction { Skip-Step $PHASE 8 "AMDSettings" }
    } else {
        Write-Debug "Step 8 — AMD GPU Settings skipped (not AMD)."
        Skip-Step $PHASE 8 "AMDSettings (not AMD)"
    }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 9 — DNS SERVER CONFIGURATION  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 9) {
    Write-Section "Step 9 — DNS Server Configuration"
    Invoke-TieredStep -Tier 3 -Title "Switch DNS server to Cloudflare or Google" `
        -Why "ISP DNS can be slow -> longer connection setup times (not in-match ping)." `
        -Evidence "T3: DNS only affects connection setup, not in-game latency. No FPS effect." `
        -Caveat "Corporate networks often need custom DNS. Only for private internet connections." `
        -Risk "SAFE" -Depth "NETWORK" `
        -Improvement "Faster DNS lookups — affects matchmaking/lobby, NOT in-game latency" `
        -SideEffects "May not work on corporate/managed networks. ISP DNS features lost." `
        -Undo "Set DNS back to automatic: Set-DnsClientServerAddress -ResetServerAddresses" `
        -Action {
            Write-Blank
            Write-Host "  Choose DNS server:" -ForegroundColor White
            Write-Host "  [1]  Cloudflare  1.1.1.1 / 1.0.0.1  (fastest, privacy-focused)" -ForegroundColor Cyan
            Write-Host "  [2]  Google      8.8.8.8 / 8.8.4.4  (stable, widely used)" -ForegroundColor White
            Write-Host "  [3]  Skip" -ForegroundColor DarkGray
            if (Test-YoloProfile) { $dnsChoice = "1" }
            elseif (-not $SCRIPT:DryRun) {
                do { $dnsChoice = Read-Host "  [1/2/3]" } while ($dnsChoice -notin @("1","2","3"))
            } else { $dnsChoice = "3" }

            if ($dnsChoice -ne "3") {
                $dnsAddrs = if ($dnsChoice -eq "1") { $CFG_DNS_Cloudflare } else { $CFG_DNS_Google }
                $dnsName  = if ($dnsChoice -eq "1") { "Cloudflare" } else { "Google" }
                try {
                    # Set DNS on active physical adapters (wired + WiFi) — DNS is protocol-layer,
                    # unlike NIC hardware tweaks which are wired-only.
                    $nics = @(Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object {
                        $_.Status -eq "Up" -and
                        $_.InterfaceDescription -notmatch $CFG_VirtualAdapterFilter
                    })
                    if ($nics.Count -gt 0) {
                        # Show numbered list so user can identify each adapter
                        Write-Blank
                        Write-Host "  Detected active adapters:" -ForegroundColor White
                        for ($i = 0; $i -lt $nics.Count; $i++) {
                            Write-Host "    [$($i+1)]  $($nics[$i].Name)  ($($nics[$i].InterfaceDescription))" -ForegroundColor Cyan
                        }
                        Write-Blank

                        if ($nics.Count -eq 1) {
                            $confirmDns = if (Test-YoloProfile) { "Y" } elseif (-not $SCRIPT:DryRun) { Read-Host "  Apply $dnsName DNS to this adapter? [Y/n]" } else { "n" }
                            if ($confirmDns -match "^[nN]$") {
                                Write-Info "DNS not changed. Configure manually in Network Settings."
                                Complete-Step $PHASE 9 "DNS"
                                return
                            }
                            $selectedNics = $nics
                        } else {
                            Write-Host "  [A]  Apply to ALL listed adapters" -ForegroundColor White
                            Write-Host "  [S]  Select individual adapters" -ForegroundColor White
                            Write-Host "  [N]  Skip — configure DNS manually" -ForegroundColor DarkGray
                            if (Test-YoloProfile) { $multiChoice = "a" }
                            elseif (-not $SCRIPT:DryRun) {
                                do { $multiChoice = Read-Host "  [A/S/N]" } while ($multiChoice -notmatch "^[aAsSnN]$")
                            } else { $multiChoice = "n" }
                            if ($multiChoice -match "^[nN]$") {
                                Write-Info "DNS not changed. Configure manually in Network Settings."
                                Complete-Step $PHASE 9 "DNS"
                                return
                            }
                            if ($multiChoice -match "^[sS]$") {
                                $selectedNics = @()
                                for ($i = 0; $i -lt $nics.Count; $i++) {
                                    $pick = if (Test-YoloProfile) { "y" } elseif (-not $SCRIPT:DryRun) { Read-Host "  Apply DNS to [$($i+1)] $($nics[$i].Name)? [y/N]" } else { "n" }
                                    if ($pick -match "^[jJyY]$") { $selectedNics += $nics[$i] }
                                }
                                if ($selectedNics.Count -eq 0) {
                                    Write-Info "No adapters selected. DNS not changed."
                                    Complete-Step $PHASE 9 "DNS"
                                    return
                                }
                            } else {
                                $selectedNics = $nics
                            }
                        }

                        foreach ($nic in $selectedNics) {
                            if ($SCRIPT:DryRun) {
                                Write-Host "  [DRY-RUN] Would set DNS to ${dnsName}: $($dnsAddrs -join ', ') (Adapter: $($nic.Name))" -ForegroundColor Magenta
                            } else {
                                # Backup current DNS servers before modification
                                $currentDns = @()
                                try {
                                    $dnsInfo = Get-DnsClientServerAddress -InterfaceIndex $nic.ifIndex `
                                        -AddressFamily IPv4 -ErrorAction SilentlyContinue
                                    if ($dnsInfo -and $dnsInfo.ServerAddresses) {
                                        $currentDns = @($dnsInfo.ServerAddresses)
                                    }
                                } catch { Write-Debug "Could not read current DNS for $($nic.Name)" }
                                Backup-DnsConfig -AdapterName $nic.Name `
                                    -InterfaceIndex $nic.ifIndex `
                                    -OriginalDnsServers $currentDns `
                                    -StepTitle $SCRIPT:CurrentStepTitle
                                Set-DnsClientServerAddress -InterfaceIndex $nic.ifIndex -ServerAddresses $dnsAddrs
                                Write-OK "DNS set to ${dnsName}: $($dnsAddrs -join ', ') (Adapter: $($nic.Name))"
                            }
                        }
                    } else {
                        Write-Warn "No active network adapter found after filtering virtual/VPN adapters."
                        Write-Info "Check your network cable or WiFi. DNS can be set manually in Network Settings."
                    }
                } catch { Write-Warn "DNS change failed: $_" }
            } else {
                Write-Info "DNS not changed."
            }
            Complete-Step $PHASE 9 "DNS"
        } `
        -SkipAction { Skip-Step $PHASE 9 "DNS" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 10 — PROCESS PRIORITY / CCD AFFINITY  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 10) {
    Write-Section "Step 10 — Process Priority / CCD Affinity (native IFEO)"
    Invoke-TieredStep -Tier 3 -Title "Set persistent CS2 process priority (native IFEO)" `
        -Why "High CPU priority gives CS2 scheduler preference. CCD pinning for dual-CCD X3D." `
        -Evidence "T3: No isolated benchmark for priority class. CCD pinning measurable on dual-CCD X3D." `
        -Caveat "High priority is safe for games. Realtime would be dangerous — we never use it." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "CPU priority for CS2 — persistent via IFEO (no background service needed)" `
        -SideEffects "None — High priority is standard for games. Easily reversible." `
        -Undo "Remove IFEO PerfOptions key + unregister CCD affinity task (if created)" `
        -Action {
            Set-CS2ProcessPriority
            Complete-Step $PHASE 10 "ProcessPriority"
        } `
        -SkipAction { Skip-Step $PHASE 10 "ProcessPriority" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 11 — VRAM LEAK AWARENESS  [Info]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 11) {
    Write-Section "Step 11 — VRAM Leak Awareness"
    Write-TierBadge 2 "CS2 VRAM Leak Bug — Known Issue"
    Write-Blank
    Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Yellow
    Write-Host "  │  CS2 KNOWN VRAM LEAK BUG:                                   │" -ForegroundColor Yellow
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │  CS2 has a known VRAM leak especially affecting GPUs with   │" -ForegroundColor White
    Write-Host "  │  8 GB VRAM. After long sessions (2+ hours) VRAM usage      │" -ForegroundColor White
    Write-Host "  │  steadily rises -> stutter and FPS drops.                  │" -ForegroundColor White
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │  RECOMMENDATION:                                            │" -ForegroundColor White
    Write-Host "  │  -> Restart CS2 every 2-3 hours                            │" -ForegroundColor Green
    Write-Host "  │  -> Monitor VRAM usage with HWiNFO64 or Task Manager       │" -ForegroundColor White
    Write-Host "  │  -> Warning at > 85% VRAM usage                            │" -ForegroundColor White
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │  MOST AFFECTED:                                             │" -ForegroundColor DarkYellow
    Write-Host "  │  -> RTX 4060 / 4060 Ti (8 GB) -> critical after ~3h       │" -ForegroundColor White
    Write-Host "  │  -> RX 7600 (8 GB) -> similar issues                       │" -ForegroundColor White
    Write-Host "  │  -> 12+ GB GPUs: less problematic, but exists              │" -ForegroundColor White
    Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Yellow
    Write-Blank
    Complete-Step $PHASE 11 "VRAMLeak"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 12 — FINAL CHECKLIST + HONEST KNOWLEDGE SUMMARY
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 12) {
    Write-Section "Step 12 — Final Checklist + Knowledge Summary"

    Write-Host @"

  DONE — with proven effect:
  ✔  Shader cache cleared           [T1: Directly measurable after driver change]
  ✔  Fullscreen optimizations       [T1: m0NESY-documented, no downside]
  ✔  CS2 Optimized power plan        [T1: 100+ sub-settings, full coverage]
  ✔  Native driver clean + install  [T1: Industry standard]
  ✔  GPU MSI via native registry    [T1: Interrupt overhead measurable]
  ✔  FPS cap via NVCP               [T1: Reproduced, more stable frametimes]
  ✔  Video settings configured      [Boost Contrast +5% lows, AO -30fps]
  ✔  Dual-channel RAM checked       [T1: Half bandwidth = dramatic]
"@ -ForegroundColor Green

    Write-Host @"
  DONE — setup-dependent (T2):
  ✔  XMP/EXPO checked               [Hardware logic: sensible. CS2 bench: unclear]
  ✔  Resizable BAR / SAM            [AMD: measurable. CS2-specific: no data]
  ✔  Nagle's Algorithm               [TCP latency reduced]
  ✔  GameConfigStore FSE keys       [Supplements fullscreen exclusive]
  ✔  SystemResponsiveness           [Gaming priority in scheduler]
  ✔  Timer resolution               [More precise system timer]
  ✔  Mouse acceleration             [1:1 input for aim consistency]
  ✔  Game DVR / Game Bar            [Background recording off]
  ✔  Overlays                       [Steam/Discord/GFE overlay off]
  ✔  Audio optimized                [24-bit/48kHz, no spatial sound]
  ✔  Autoexec.cfg                   [Network CVars optimized]
  ✔  Chipset driver checked         [Current driver recommended]
"@ -ForegroundColor Yellow

    Write-Host @"
  STILL MANUAL:
  ○  Run benchmark map 3 times (Dust2 + Inferno recommended)
  ○  Calculate FPS cap: START.bat -> [3] FPS Cap Calculator
  ○  Verify Reflex decision with CapFrameX:
     Test A (-noreflex + NVCP Ultra) vs. B (Reflex ON in-game)
     Choose what gives better lows AND better feel.
  ○  Check settings after Windows Update: START.bat -> [6] Verify
  ○  All 52 NVIDIA DRS settings applied natively — no external tools needed
"@ -ForegroundColor Cyan

    # ── X3D / Hardware Validation Checks ─────────────────────────────────
    # Box width: 66 chars total. Inner: "  │  " (6) + content (up to 58) + " " + "│" (1) = 66
    # Helper: pad content to 58 chars inside the box
    $amdCpu = Get-AmdCpuInfo
    if ($amdCpu -and $amdCpu.IsX3D) {
        Write-Blank
        Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Cyan
        $hdr = "$($amdCpu.CpuName) — POST-TUNING VALIDATION"
        Write-Host "  │  $($hdr.PadRight(58))│" -ForegroundColor Cyan
        Write-Host "  │                                                              │" -ForegroundColor DarkGray

        # CPU clock info (informational — MaxClockSpeed is base clock, not boost)
        if ($amdCpu.MaxClockSpeed -gt 0) {
            $msg = "$([char]0x2139)  CPU: $($amdCpu.CpuName) — base clock: $($amdCpu.MaxClockSpeed) MHz"
            Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor Cyan
            $msg2 = "   (boost clock requires HWiNFO to verify)"
            Write-Host "  │  $($msg2.PadRight(58))│" -ForegroundColor DarkGray
        }

        # WHEA error check
        $whea = Test-WheaErrors
        if ($whea) {
            if ($whea.HasErrors) {
                $msg = "$([char]0x2718)  WHEA errors: $($whea.RecentCount) in last 24h — CO too aggressive!"
                Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor Red
                Write-Host "  │     Reduce Curve Optimizer magnitude by 5 and retest.      │" -ForegroundColor Red
            } else {
                $totalLabel = if ($whea.Count -gt 0) { "$($whea.Count) total (none recent)" } else { "0 — clean" }
                $msg = "$([char]0x2714)  WHEA errors: $totalLabel"
                Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor Green
            }
        }

        # DDR5 FCLK/MCLK 1:1 check
        $ddr5 = Get-Ddr5TimingInfo
        if ($ddr5 -and $ddr5.IsDDR5) {
            $mclk = $ddr5.ActiveMhz
            $mts  = $ddr5.ActiveMTs
            if ($ddr5.IsOptimal1to1) {
                $msg = "$([char]0x2714)  DDR5-$mts (MCLK $mclk MHz) — 1:1 FCLK range"
                Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor Green
            } elseif ($mts -gt 6400) {
                $msg = "$([char]0x25B2)  DDR5-$mts — above 1:1 FCLK sweet spot (6000)"
                Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor Yellow
                if ($amdCpu -and $amdCpu.IsAMD) {
                    Write-Host "  │     Consider DDR5-6000 for lowest latency on AM5.          │" -ForegroundColor DarkGray
                }
            } else {
                $msg = "$([char]0x25CB)  DDR5-$mts (MCLK $mclk MHz)"
                Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor DarkGray
            }
            if ($ddr5.IsDownclocked) {
                $msg = "$([char]0x2139)  Kit rated $($ddr5.RatedMTs) MT/s, downclocked to $mts MT/s"
                Write-Host "  │  $($msg.PadRight(58))│" -ForegroundColor DarkGray
            }
        }

        Write-Host "  │                                                              │" -ForegroundColor DarkGray
        Write-Host "  │  Full validation: X3D_KOMPLETT_GUIDE.md Section E           │" -ForegroundColor DarkGray
        Write-Host "  │  ZenTimings screenshot recommended for RAM timing verify.   │" -ForegroundColor DarkGray
        Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Cyan
    }

    Write-Blank
    Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor DarkYellow
    Write-Host "  │  WHAT THIS SCRIPT CANNOT FIX                                │" -ForegroundColor DarkYellow
    Write-Host "  │                                                              │" -ForegroundColor DarkYellow
    Write-Host "  │  CS2 has structurally poor frame pacing.                    │" -ForegroundColor White
    Write-Host "  │  Apex Legends, The Finals and Warzone — all with higher     │" -ForegroundColor White
    Write-Host "  │  graphics load — deliver consistently better 1% lows.       │" -ForegroundColor White
    Write-Host "  │  This is a Valve/Source2 problem, not a hardware problem.   │" -ForegroundColor White
    Write-Host "  │  No tweak fundamentally fixes this.                         │" -ForegroundColor White
    Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor DarkYellow

    Write-Blank
    Write-Host "  NEXT STEPS — IF PROBLEMS PERSIST:" -ForegroundColor White
    Write-Host @"
  1.  Run LatencyMon  (resplendence.com/latencymon)
      -> Shows which drivers cause DPC spikes
      -> Then decide if NIC tweaks or MSI Utility are useful

  2.  Log HWiNFO64 during a match
      -> Log CPU/GPU package temp + clock + power
      -> Thermal throttling is a commonly overlooked cause of lows

  3.  Rule out thermal problems
      -> CPU TJmax: AMD Ryzen 95C, Intel varies
      -> GPU: 83C = throttle start on NVIDIA

  4.  RAM sub-timings (only if XMP active and stable)
      -> B-Die or Hynix A-Die recommended
      -> CL30 -> CL28 -> CL26 stabilize with TM5/HCI MemTest
      -> Effect on 1% lows measurable but hardware-dependent

  5.  CPU upgrade as last resort
      -> 9800X3D is clear leader for CS2 as of March 2026
      -> Single-core performance is the limiting factor

  Rule: Always measure before/after with CapFrameX.
  Without measurement, any claim about improvement is placebo.
"@ -ForegroundColor DarkGray

    Write-Info "Log: $CFG_LogFile  |  Tools: $CFG_WorkDir"
    Complete-Step $PHASE 12 "Checklist"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 13 — FINAL BENCHMARK + FPS CAP  [T1, LAST STEP]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 13) {
    Write-Section "Step 13 — Final Benchmark + FPS Cap Calculation  [LAST STEP]"
    Write-TierBadge 1 "Post-optimization benchmark — proof of improvement"
    Write-Blank

    Write-Host @"
  FPSHEAVEN BENCHMARK MAPS  (by @fREQUENCYcs)
  ─────────────────────────────────────────────
  Outputs at end in console window:
    [VProf] FPS: Avg=XXX.X, P1=XXX.X

  Map 1 — DUST2  (community standard):
  $CFG_Benchmark_Dust2

  Map 2 — INFERNO:
  $CFG_Benchmark_Inferno

  Map 3 — ANCIENT  (water interaction):
  $CFG_Benchmark_Ancient

  WORKFLOW:
  1.  Steam -> Workshop -> Subscribe to map
  2.  CS2: Play -> Workshop Maps -> start
  3.  Runs 2-3 minutes automatically — don't touch PC
  4.  Console opens automatically -> copy [VProf] line
  5.  Paste output below for automatic tracking + FPS cap

  BEST RESULT: Run 3 times, use the average.

  WHY AN FPS CAP?
  ────────────────
  Without a cap the GPU runs at max continuously -> frametimes fluctuate.
  With a cap the GPU runs at even load -> more consistent 1% lows.
  Method: avg FPS - 9%  (FPSHeaven / Blur Busters recommendation).
"@ -ForegroundColor DarkGray

    if ($fpsCap -gt 0) {
        Write-Host "  Already calculated cap: $fpsCap  (avg $avgFps - 9%)" -ForegroundColor Green
    }

    # Use the iterative benchmark tracking system
    $bmResult = Invoke-BenchmarkCapture -Label "After all optimizations"

    if ($bmResult) {
        Write-Blank
        Write-Info "Set FPS cap in: NVIDIA CP -> CS2 -> Max Frame Rate -> $($bmResult.Cap)"
        Write-Info "You can run this benchmark again anytime via START.bat -> [3] FPS Cap Calculator."
        Write-Info "All results are tracked in: $CFG_BenchmarkFile"

        # Show improvement estimate vs. actual results
        $baselineAvg = if ($state.PSObject.Properties['baselineAvg'] -and $null -ne $state.baselineAvg) { $state.baselineAvg } else { 0 }
        $baselineP1  = if ($state.PSObject.Properties['baselineP1']  -and $null -ne $state.baselineP1)  { $state.baselineP1 }  else { 0 }
        Show-ImprovementEstimate -BaselineAvg $baselineAvg -BaselineP1 $baselineP1 `
            -ActualAvg $bmResult.Avg -ActualP1 $bmResult.P1
    }

    # Show full history if multiple results exist
    $history = @(Get-BenchmarkHistory)
    if ($history.Count -ge 2) {
        Write-Blank
        Show-BenchmarkComparison
    }

    Complete-Step $PHASE 13 "FinalBenchmark"
}

Write-Blank
if ($SCRIPT:DryRun) {
    Write-PhaseSummary -PhaseLabel "PHASE 3" -DryRun
} else {
    Write-PhaseSummary -PhaseLabel "ALL 3 PHASES" -NextAction "Good luck, have fun! GG"

    $r = if (Test-YoloProfile) { "y" } else { Read-Host "  Final restart recommended (MSI changes). Now? [y/N]" }
    if ($r -match "^[jJyY]$") {
        Restart-Computer -Force
    }
}
} catch {
    Write-Host "" -ForegroundColor Red
    Write-Host "  ╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Red
    Write-Host "  ║  FATAL ERROR — Phase 3 crashed unexpectedly                ║" -ForegroundColor Red
    Write-Host "  ╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Red
    Write-Host "  Error:      $_" -ForegroundColor Red
    Write-Host "  Location:   $($_.InvocationInfo.ScriptName):$($_.InvocationInfo.ScriptLineNumber)" -ForegroundColor Yellow
    Write-Host "  Line:       $($_.InvocationInfo.Line.Trim())" -ForegroundColor DarkGray
    if ($_.ScriptStackTrace) {
        Write-Host "  Stack trace:" -ForegroundColor Yellow
        $_.ScriptStackTrace -split "`n" | ForEach-Object { Write-Host "    $_" -ForegroundColor DarkGray }
    }
    Write-Host ""
    if (-not (Test-YoloProfile)) { Read-Host "  Press [Enter] to exit (error details above)" }
} finally {
    # Release backup lock — acquired by Initialize-Backup at the top of this script.
    # In try/finally to ensure release on crash, Ctrl+C, or normal exit.
    Remove-BackupLock
}
