# ==============================================================================
#  helpers/nvidia-profile.ps1  -  NVIDIA CS2 Profile Settings (DRS + Registry)
# ==============================================================================
#
#  Applies repository-defined NVIDIA driver settings for CS2 using two methods:
#
#  1. DRS Direct Write (preferred):
#     Calls nvapi64.dll via helpers/nvidia-drs.ps1 to write 42 DWORD
#     settings directly to the DRS binary database (nvdrs.dat).
#     This uses the public NVIDIA DRS API family also used by profile tools.
#
#  2. Registry Fallback (if DRS unavailable):
#     Writes 25 settings to HKLM registry keys. These keys are not equivalent
#     to per-application DRS values, and effective behavior is driver-dependent.
#     Directs users to re-run with DRS or inspect the profile manually.
#
#  42 DWORD settings applied via DRS (derived from public NVIDIA DRS documentation,
#  NVIDIA Profile Inspector metadata, and community testing).
#  3 settings intentionally excluded:
#    -  1 string setting (269308407) - unknown effect
#    -  1 hardware-specific (550564838) - GPU device ID
#    -  1 net-negative (2966161525) - frame interpolation = latency
#  Plus 1 registry-only (PerfLevelSrc) - applied via registry always
#

# ── Settings table: 42 DWORD settings for DRS ───────────────────────────────
# Each entry: Id (NvU32 settingId), Value (NvU32), Name (display label)
#
# Settings derived from public NVIDIA DRS IDs, community testing (djdallmann,
# valleyofdoom, Blur Busters), and NvApiDriverSettings.h enum definitions.

$NV_DRS_SETTINGS = @(
    # ── Power & Performance ──────────────────────────────────────────────────
    @{ Id=274197361;  Value=1;          Name="Power management mode: Prefer Max Performance" }
    @{ Id=8102046;    Value=1;          Name="Maximum pre-rendered frames: 1" }
    @{ Id=549528094;  Value=1;          Name="Threaded optimization: Force ON" }         # Default=0(Auto); Force ON for explicit multi-threading
    @{ Id=553505273;  Value=0;          Name="Triple buffering: OFF" }

    # ── Texture Filtering ────────────────────────────────────────────────────
    @{ Id=13510289;   Value=20;         Name="Texture filtering quality: High Performance" }
    @{ Id=1686376;    Value=1;          Name="Negative LOD bias: Clamp" }
    @{ Id=3066610;    Value=0;          Name="Trilinear optimization: ON (0=driver perf shortcut enabled)" }
    @{ Id=8703344;    Value=0;          Name="Anisotropic filter optimization: OFF" }
    @{ Id=15151633;   Value=0;          Name="Anisotropic sample optimization: OFF" }
    @{ Id=6524559;    Value=0;          Name="Driver controlled LOD bias: OFF" }

    # ── Anti-Aliasing ────────────────────────────────────────────────────────
    @{ Id=276652957;  Value=0;          Name="AA gamma correction: OFF" }
    @{ Id=276757595;  Value=0;          Name="AA mode: Application Controlled" }
    @{ Id=545898348;  Value=0;          Name="AA line gamma: OFF" }
    @{ Id=270426537;  Value=1;          Name="Anisotropic filtering: Off [Linear] (app decides AF level)" }
    @{ Id=282245910;  Value=0;          Name="Anisotropic mode: App Controlled" }

    # ── FXAA ─────────────────────────────────────────────────────────────────
    @{ Id=276089202;  Value=0;          Name="FXAA Default: OFF" }
    @{ Id=271895433;  Value=0;          Name="NVIDIA Predefined FXAA Usage: 0" }

    # ── VSync / Frame Rate ───────────────────────────────────────────────────
    @{ Id=11041231;   Value=138504007;  Name="VSync: Force OFF" }
    @{ Id=6600001;    Value=1;          Name="Preferred refresh rate: Highest" }
    @{ Id=277041152;  Value=0;          Name="FRL Low Latency: OFF" }
    @{ Id=277041154;  Value=0;          Name="Frame Rate Limiter (legacy): OFF" }
    @{ Id=277041162;  Value=500;        Name="FRL NVCPL: 500 FPS repository default" }

    # ── VRR / G-SYNC (suite default; benchmark on VRR/Pulsar displays) ─────
    @{ Id=278196567;  Value=0;          Name="VRR global feature: OFF (suite default)" }
    @{ Id=278196727;  Value=0;          Name="VRR requested state: OFF (suite default)" }
    @{ Id=279476652;  Value=1;          Name="G-SYNC: FORCE_OFF (suite default)" }
    # Removed: Id=279476686 (0x10A879CE) - not in NPI, likely inert; other 6 G-SYNC settings cover VRR
    @{ Id=279476687;  Value=1;          Name="G-SYNC (2): FORCE_OFF (suite default)" }
    @{ Id=294973784;  Value=0;          Name="G-SYNC globally: OFF (suite default)" }
    @{ Id=5912412;    Value=2525368439; Name="VSync tear control: disabled" }

    # ── Ansel ────────────────────────────────────────────────────────────────
    @{ Id=276158834;  Value=0;          Name="Ansel: OFF" }
    @{ Id=271965065;  Value=0;          Name="Predefined Ansel usage: 0" }

    # ── Optimus (laptop dGPU preference) ─────────────────────────────────────
    @{ Id=284810369;  Value=17;         Name="Optimus: force dGPU" }
    @{ Id=284810372;  Value=16777216;   Name="Optimus shim: force dGPU rendering" }

    # ── Resizable BAR ───────────────────────────────────────────────────────
    # NPI CustomSettingNames.xml: 0x000F00BA = "rBAR - Enable"
    # The profile disables per-application rBAR for CS2 as a repository default.
    # This repository includes no validated cross-system benchmark for the value.
    @{ Id=983226;     Value=0;          Name="rBAR: Disabled for CS2 by repository default" }
    @{ Id=983227;     Value=0;          Name="rBAR - Options: Disabled (companion setting)" }

    # ── Shader Cache ─────────────────────────────────────────────────────────
    @{ Id=11306135;   Value=10240;      Name="Shader disk cache max: 10240 MB (10 GB)" }

    # ── SLI / AFR ────────────────────────────────────────────────────────────
    @{ Id=270198627;  Value=0;          Name="Smooth AFR: OFF" }

    # ── CUDA performance-limit state ────────────────────────────────────────
    # Removed: Id=1074665807 (0x400E194F) - undocumented duplicate; NPI-recognized
    # Id=1343646814 (0x50166C5E) below handles the same CUDA P-state override.

    # ── Publicly identified flags (NVAPI/NVIDIA Profile Inspector metadata) ─
    @{ Id=390467;     Value=1;          Name="Ultra Low Latency - CPL State: On (separate from NVCP ULL-Enabled)" }
    @{ Id=14566042;   Value=0;          Name="DXR_ENABLE: OFF (repository profile value)" }
    @{ Id=274606621;  Value=4;          Name="ANSEL_FREESTYLE_MODE: APPROVED_ONLY (4)" }
    @{ Id=549198379;  Value=0;          Name="VK_NV_RAYTRACING: DISABLE (repository profile value)" }
    @{ Id=1343646814; Value=0;          Name="CUDA_STABLE_PERF_LIMIT: FORCE_OFF (0; driver behavior must be observed)" }
    @{ Id=2156231208; Value=1;          Name="GFE_MONITOR_USAGE: 1 (public NPI-named state)" }

)
# TOTAL: 42 DWORD settings via DRS. Ten leak-derived or unidentified entries
# were removed from the public alpha default.

# ── Excluded settings ──────────────────────────────────────────────────────
# 2966161525 (0xB0CC0875) - Smooth Motion APIs = 1 → frame interpolation adds latency
# 550564838  (0x20D0F3E6) - OpenGL GPU Affinity → hardcoded GPU-specific PCI device ID
# 269308407  (0x100D51F7) - String setting "Buffers=(Depth)" → DRS string type, marginal
# ─────────────────────────────────────────────────────────────────────────────

function New-NvidiaProfileResult {
    param(
        [string]$Status,
        [bool]$CanCompleteStep,
        [string]$Method,
        [string]$Message,
        [int]$DrsApplied = 0,
        [int]$DrsFailed = 0,
        [int]$DrsTotal = 0,
        [int]$RegistryApplied = 0,
        [int]$RegistryFailed = 0,
        [bool]$BackupFailed = $false
    )

    [PSCustomObject]@{
        Status = $Status
        CanCompleteStep = $CanCompleteStep
        Method = $Method
        Message = $Message
        DrsApplied = $DrsApplied
        DrsFailed = $DrsFailed
        DrsTotal = $DrsTotal
        RegistryApplied = $RegistryApplied
        RegistryFailed = $RegistryFailed
        BackupFailed = $BackupFailed
    }
}


function Apply-NvidiaCS2Profile {
    <#
    .SYNOPSIS  Applies the repository-defined CS2 NVIDIA profile settings.
    .DESCRIPTION
        DRS-first: writes 42 DWORD settings directly to the NVIDIA DRS
        binary database via nvapi64.dll P/Invoke.  Falls back to registry
        writes if DRS is unavailable (AMD GPU, missing DLL, 32-bit PS).

        Also writes two GPU hardware-class values. Their effective behavior is
        driver-dependent and must be observed on the target system.
    #>

    Write-Step "Applying NVIDIA CS2 profile settings..."

    # ── Locate NVIDIA GPU registry key (needed for PerfLevelSrc) ────────────
    $classPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\$CFG_GUID_Display"

    $nvKeyPath = $null
    if ($SCRIPT:DryRun) {
        # The simulated driver install has not created a hardware class key.
        # Use a synthetic policy-valid value so both planners render their paths.
        $nvKeyPath = "$classPath\0000"
        Write-ConsoleLine "  [DRY-RUN] Would locate and validate the installed NVIDIA GPU class key." -ForegroundColor Magenta
    } elseif (Test-Path $classPath) {
        $subkeys = Get-ChildItem $classPath -ErrorAction SilentlyContinue |
            Where-Object { $_.PSChildName -match "^\d{4}$" }
        foreach ($key in $subkeys) {
            $props = Get-ItemProperty $key.PSPath -ErrorAction SilentlyContinue
            if ($props.ProviderName -match "NVIDIA" -or $props.DriverDesc -match "NVIDIA") {
                $nvKeyPath = $key.PSPath
                Write-DebugLog "NVIDIA GPU key: $($key.PSChildName) - $($props.DriverDesc)"
                break
            }
        }
    }

    if (-not $nvKeyPath) {
        Write-Warn "NVIDIA GPU registry key not found. Install the driver first."
        return New-NvidiaProfileResult `
            -Status "Failed" `
            -CanCompleteStep $false `
            -Method "None" `
            -Message "NVIDIA GPU registry key not found."
    }

    # ── FPS cap override for FRL setting ────────────────────────────────────
    $frlValue = 500
    $frlLabel = "500 (repository default)"
    if ((Get-Variable -Name fpsCap -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:fpsCap -gt 0) {
        $frlValue = $SCRIPT:fpsCap
        $frlLabel = "$($SCRIPT:fpsCap) (user FPS cap)"
    }

    # ── Try DRS direct write (preferred path) ──────────────────────────────
    if ($SCRIPT:DryRun) {
        $drsResult = Apply-NvidiaCS2ProfileDrs -FrlValue $frlValue -FrlLabel $frlLabel
    } elseif (Initialize-NvApiDrs) {
        $drsResult = Apply-NvidiaCS2ProfileDrs -FrlValue $frlValue -FrlLabel $frlLabel
    } else {
        $drsResult = $null
    }

    # ── Fallback: registry-only (if DRS unavailable or failed) ──────────────
    if (-not $drsResult -or $drsResult.Status -eq "SessionFailed") {
        Write-Warn "DRS direct write unavailable - falling back to registry method."
        return Apply-NvidiaCS2ProfileRegistry -NvKeyPath $nvKeyPath -FrlValue $frlValue -FrlLabel $frlLabel
    }

    if (-not $drsResult.CanCompleteStep -and $drsResult.Status -ne "DryRun") {
        Write-Blank
        Write-ConsoleLine "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Yellow
        Write-ConsoleLine "  │  NVIDIA CS2 PROFILE - DRS NOT FULLY APPLIED                 │" -ForegroundColor Yellow
        Write-ConsoleLine "  │                                                              │" -ForegroundColor Yellow
        Write-ConsoleLine "  │  Status: $($drsResult.Status)$((' ' * [math]::Max(0, 52 - $drsResult.Status.Length)))│" -ForegroundColor White
        Write-ConsoleLine "  │  DRS:    $($drsResult.DrsApplied)/$($drsResult.DrsTotal) applied, $($drsResult.DrsFailed) failed$((' ' * [math]::Max(0, 28 - "$($drsResult.DrsApplied)/$($drsResult.DrsTotal)".Length - "$($drsResult.DrsFailed)".Length)))│" -ForegroundColor White
        Write-ConsoleLine "  │                                                              │" -ForegroundColor Yellow
        Write-ConsoleLine "  │  Review warnings and retry before marking this step done.   │" -ForegroundColor White
        Write-ConsoleLine "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Yellow
        return $drsResult
    }

    # ── GPU class registry values ──────────────────────────────────────────
    # These hardware-class values are separate from DRS. Their names and stored
    # values do not prove an effective P-state; observe behavior on the target
    # driver and workload.
    $registryResults = @(
        Set-RegistryValue $nvKeyPath "PerfLevelSrc"       0x2222 "DWord" "Repository GPU class power-state value" -PassThru
        Set-RegistryValue $nvKeyPath "DisableDynamicPstate" 1    "DWord" "Repository dynamic P-state policy value" -PassThru
    )
    $registryApplied = @($registryResults | Where-Object { $_.Applied }).Count
    $registryFailed = @($registryResults | Where-Object { -not $_.Applied -and $_.Status -ne "DryRun" }).Count
    $profileStatus = if ($SCRIPT:DryRun) { "DryRun" } elseif ($registryFailed -eq 0) { "Success" } else { "Partial" }
    $profileCanComplete = ($profileStatus -eq "Success")
    $profileMessage = if ($SCRIPT:DryRun) {
        "NVIDIA DRS profile and supplemental registry writes previewed."
    } elseif ($profileCanComplete) {
        "NVIDIA DRS profile and supplemental registry writes applied."
    } else {
        "NVIDIA DRS profile applied, but $registryFailed supplemental registry write(s) failed."
    }

    # ── DRS Success Summary ─────────────────────────────────────────────────
    $settingCount = $NV_DRS_SETTINGS.Count
    $appliedCount = $drsResult.DrsApplied
    $errorCount   = $drsResult.DrsFailed
    Write-Blank
    $statusColor = if ($profileCanComplete) { "Green" } else { "Yellow" }
    $drsLabel = if ($errorCount -eq 0) { "$appliedCount DRS" } else { "$appliedCount/$settingCount DRS ($errorCount failed)" }
    $registryLabel = if ($registryFailed -eq 0) { "2 registry" } else { "$registryApplied/2 registry ($registryFailed failed)" }
    Write-ConsoleLine "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor $statusColor
    $contentStr = "  NVIDIA CS2 PROFILE - $drsLabel + $registryLabel"
    $padLen = [math]::Max(1, 64 - $contentStr.Length)
    Write-ConsoleLine "  │$contentStr$((' ' * $padLen))│" -ForegroundColor $statusColor
    Write-ConsoleLine "  │                                                              │" -ForegroundColor Green
    Write-ConsoleLine "  │  Method: DRS direct write (nvapi64.dll)                     │" -ForegroundColor White
    Write-ConsoleLine "  │  Profile: Counter-strike 2  (cs2.exe / csgos2.exe)          │" -ForegroundColor White
    Write-ConsoleLine "  │                                                              │" -ForegroundColor Green
    Write-ConsoleLine "  │  ✔  Power Management:    Prefer Maximum Performance         │" -ForegroundColor White
    Write-ConsoleLine "  │  ✔  Threaded Optimization: Force ON                         │" -ForegroundColor White
    Write-ConsoleLine "  │  ✔  Texture Filtering:   High Performance                   │" -ForegroundColor White
    Write-ConsoleLine "  │  ✔  Triple Buffering:    OFF                                │" -ForegroundColor White
    Write-ConsoleLine "  │  ✔  VSync:               Force OFF                          │" -ForegroundColor White
    Write-ConsoleLine "  │  ✔  G-SYNC / VRR:        Disabled by suite default          │" -ForegroundColor White
    Write-ConsoleLine "  │  ✔  FXAA / Ansel:        OFF                                │" -ForegroundColor White
    Write-ConsoleLine "  │  ✔  Max Pre-rendered:    1 frame                            │" -ForegroundColor White
    Write-ConsoleLine "  │  ✔  Frame Rate Limiter:  $frlLabel$((' ' * [math]::Max(0, 36 - $frlLabel.Length)))│" -ForegroundColor White
    $summaryDisplayed = 9  # Number of settings explicitly listed in the summary box above
    Write-ConsoleLine "  │  ✔  + $($settingCount - $summaryDisplayed) more DRS settings (AA, LOD, Optimus, cache...)     │" -ForegroundColor DarkGray
    Write-ConsoleLine "  │                                                              │" -ForegroundColor Green
    Write-ConsoleLine "  │  Verify: open NVIDIA Profile Inspector → Counter-strike 2   │" -ForegroundColor DarkGray
    Write-ConsoleLine "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor $statusColor

    Write-Info "All DRS settings backed up automatically for rollback."

    return New-NvidiaProfileResult `
        -Status $profileStatus `
        -CanCompleteStep $profileCanComplete `
        -Method "DRS" `
        -Message $profileMessage `
        -DrsApplied $drsResult.DrsApplied `
        -DrsFailed $drsResult.DrsFailed `
        -DrsTotal $drsResult.DrsTotal `
        -RegistryApplied $registryApplied `
        -RegistryFailed $registryFailed `
        -BackupFailed $drsResult.BackupFailed
}


function Apply-NvidiaCS2ProfileDrs {
    <#
    .SYNOPSIS  Writes 42 DWORD settings to DRS via nvapi64.dll.
    .DESCRIPTION
        Finds (or creates) the CS2 profile, backs up current values,
        writes all settings, and saves the DRS database.
        Returns $true on success, $false on failure.
    #>
    [Diagnostics.CodeAnalysis.SuppressMessageAttribute('PSReviewUnusedParameter', '',
        Justification = 'FrlValue and FrlLabel are captured by Invoke-DrsSession scriptblock closure')]
    param(
        [int]$FrlValue = 500,
        [string]$FrlLabel = "500"
    )

    # Reset counters from any previous invocation in the same session
    $SCRIPT:_drsApplied = 0
    $SCRIPT:_drsErrors  = 0
    $SCRIPT:_drsBackupFailed = $false

    # Validate FRL value - prevent nonsensical caps from corrupting the DRS profile
    if ($FrlValue -le 0 -or $FrlValue -gt 1000) { $FrlValue = 500 }

    # ── DRY-RUN: print what WOULD be applied, skip Invoke-DrsSession entirely ──
    # The entire Invoke-DrsSession call creates a real DRS session that executes
    # CreateProfile/AddApplication - mutating the DRS database even in DRY-RUN.
    # Only the registry fallback path (PerfLevelSrc, DisableDynamicPstate) goes
    # through Set-RegistryValue which handles DRY-RUN itself.
    if ($SCRIPT:DryRun) {
        $applied = 0
        foreach ($s in $NV_DRS_SETTINGS) {
            $writeValue = [uint32]$s.Value
            if ($s.Id -eq 277041162 -and $FrlValue -ne 500) {
                $writeValue = [uint32]$FrlValue
            }
            Write-ConsoleLine "  [DRY-RUN] Would set DRS: $($s.Name) = $writeValue" -ForegroundColor Magenta
            $applied++
        }
        Write-ConsoleLine "  [DRY-RUN] Would save DRS database" -ForegroundColor Magenta
        $SCRIPT:_drsApplied = $applied
        $SCRIPT:_drsErrors  = 0
        return New-NvidiaProfileResult `
            -Status "DryRun" `
            -CanCompleteStep $false `
            -Method "DRS" `
            -Message "DRS profile previewed." `
            -DrsApplied $applied `
            -DrsFailed 0 `
            -DrsTotal $NV_DRS_SETTINGS.Count
    }

    try {
        Invoke-DrsSession -Action {
            param($session)

            # ── Find the CS2 profile ────────────────────────────────────────
            $drsProfile = [IntPtr]::Zero
            $profileCreated = $false
            $profileName = $null

            # Strategy: first check if cs2.exe is already in a profile
            $drsProfile = [NvApiDrs]::FindApplicationProfile($session, "cs2.exe")

            if ($drsProfile -ne [IntPtr]::Zero) {
                # cs2.exe found in a profile - check if it's the Base Profile.
                # The Base Profile (aka "Global" / "_GLOBAL_DRIVER_PROFILE") is the
                # default catch-all profile for all applications. Writing CS2-specific
                # settings to it would affect EVERY application, not just CS2.
                # Detect the Base Profile by handle comparison, not by name search,
                # so we always write to the profile that actually owns cs2.exe.
                $baseProfile = [IntPtr]::Zero
                try { $baseProfile = [NvApiDrs]::FindProfileByName($session, "_GLOBAL_DRIVER_PROFILE") } catch {
                    Write-DebugLog "DRS: Base profile lookup failed: $($_.Exception.Message)"
                }
                if ($baseProfile -ne [IntPtr]::Zero -and $drsProfile -eq $baseProfile) {
                    # cs2.exe is in the Base Profile - create a dedicated profile and move it
                    Write-DebugLog "DRS: cs2.exe found in Base Profile - creating dedicated CS2 profile"
                    $profileName = "Counter-strike 2"
                    $drsProfile = [NvApiDrs]::CreateProfile($session, $profileName)
                    $profileCreated = $true
                    # Bind applications to the new dedicated profile (AddApplication
                    # with -179 on Base Profile is expected - the exe will be re-bound)
                    try { [NvApiDrs]::AddApplication($session, $drsProfile, "cs2.exe") } catch {
                        Write-Warn "DRS: AddApplication cs2.exe to dedicated profile - $_"
                    }
                    try { [NvApiDrs]::AddApplication($session, $drsProfile, "csgos2.exe") } catch {
                        Write-Warn "DRS: AddApplication csgos2.exe to dedicated profile - $_"
                    }
                } else {
                    # cs2.exe is in a non-Base profile - use that profile directly
                    # (regardless of its name - it's the profile the driver reads for cs2.exe)
                    $profileName = "(cs2.exe profile)"
                    Write-DebugLog "DRS: cs2.exe found in dedicated profile (handle $drsProfile)"
                }
            } else {
                # cs2.exe not in any profile - search by known names
                foreach ($name in @("Counter-strike 2", "Counter-Strike 2")) {
                    $drsProfile = [NvApiDrs]::FindProfileByName($session, $name)
                    if ($drsProfile -ne [IntPtr]::Zero) {
                        $profileName = $name
                        break
                    }
                }

                if ($drsProfile -eq [IntPtr]::Zero) {
                    # No existing profile - create one
                    $profileName = "Counter-strike 2"
                    $drsProfile = [NvApiDrs]::CreateProfile($session, $profileName)
                    $profileCreated = $true
                    Write-DebugLog "DRS: Created profile '$profileName'"
                }

                # Bind applications - only needed for newly created profiles.
                # Predefined/existing profiles already have cs2.exe pre-bound
                # in NVIDIA's shipped DRS database (nvdrs.dat).
                if ($profileCreated) {
                    try { [NvApiDrs]::AddApplication($session, $drsProfile, "cs2.exe") } catch {
                        Write-Warn "DRS: AddApplication cs2.exe - $_"
                    }
                    try { [NvApiDrs]::AddApplication($session, $drsProfile, "csgos2.exe") } catch {
                        Write-Warn "DRS: AddApplication csgos2.exe - $_"
                    }
                } else {
                    Write-DebugLog "DRS: Profile '$profileName' found - cs2.exe pre-bound by NVIDIA, skipping AddApplication"
                }
            }

            # ── Backup current DRS values ───────────────────────────────────
            $effectiveTitle = if ((Get-Variable -Name CurrentStepTitle -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:CurrentStepTitle) { $SCRIPT:CurrentStepTitle } else { "NVIDIA CS2 DRS Profile" }
            try {
                Backup-DrsSettings -Session $session -DrsProfile $drsProfile `
                    -SettingIds ($NV_DRS_SETTINGS | ForEach-Object { $_.Id }) `
                    -StepTitle $effectiveTitle `
                    -ProfileName $(if ($profileName) { $profileName } elseif ((Get-Variable -Name DRS_FOUND_VIA_APP -Scope Script -ErrorAction SilentlyContinue)) { $SCRIPT:DRS_FOUND_VIA_APP } else { "(found via cs2.exe)" }) `
                    -ProfileCreated $profileCreated
                Flush-BackupBuffer
                $durableBackup = Get-BackupDataRaw
                $durableCapture = @($durableBackup.entries | Where-Object {
                    $_.type -eq 'drs' -and $_.step -eq $effectiveTitle
                }).Count -gt 0
                if (-not $durableCapture) {
                    throw "backup.json does not contain the expected DRS restore record."
                }
            } catch {
                $SCRIPT:_drsBackupFailed = $true
                throw "DRS settings were not applied because their restore record could not be persisted: $_"
            }

            # ── Apply settings ──────────────────────────────────────────────
            $applied = 0
            $errors = 0
            $failedIds = @()
            foreach ($s in $NV_DRS_SETTINGS) {
                $writeValue = [uint32]$s.Value

                # FRL override: if user has a FPS cap, use it instead of 500
                # 277041162 = FRL NVCPL (frame rate limiter, not legacy FRL 277041154)
                if ($s.Id -eq 277041162 -and $FrlValue -ne 500) {
                    $writeValue = [uint32]$FrlValue
                }

                try {
                    [NvApiDrs]::SetDwordSetting($session, $drsProfile, [uint32]$s.Id, $writeValue)
                    $applied++
                } catch {
                    Write-DebugLog "DRS: Failed to set $($s.Name) (0x$($s.Id.ToString('X'))): $_"
                    $failedIds += "0x$($s.Id.ToString('X'))"
                    $errors++
                }
            }

            Write-DebugLog "DRS: Applied $applied settings, $errors errors"
            if ($failedIds.Count -gt 0) {
                Write-Warn "DRS: $($failedIds.Count) setting(s) rejected by driver (non-fatal): $($failedIds -join ', ')"
            }
            # Store counts for the caller's summary
            $SCRIPT:_drsApplied = $applied
            $SCRIPT:_drsErrors  = $errors
        }

        $settingCount = $NV_DRS_SETTINGS.Count
        $appliedCount = if ($null -ne $SCRIPT:_drsApplied) { $SCRIPT:_drsApplied } else { 0 }
        $errorCount = if ($null -ne $SCRIPT:_drsErrors) { $SCRIPT:_drsErrors } else { $settingCount }
        $backupFailed = if ($null -ne $SCRIPT:_drsBackupFailed) { [bool]$SCRIPT:_drsBackupFailed } else { $false }
        $drsStatus = if ($errorCount -eq 0 -and $appliedCount -eq $settingCount -and -not $backupFailed) {
            "Success"
        } elseif ($appliedCount -gt 0) {
            "Partial"
        } else {
            "Failed"
        }
        $drsMessage = switch ($drsStatus) {
            "Success" { "All $settingCount DRS settings applied." }
            "Partial" {
                if ($backupFailed -and $errorCount -eq 0) {
                    "DRS settings applied, but backup failed."
                } else {
                    "Only $appliedCount of $settingCount DRS settings applied."
                }
            }
            default { "No DRS settings were applied." }
        }

        return New-NvidiaProfileResult `
            -Status $drsStatus `
            -CanCompleteStep ($drsStatus -eq "Success") `
            -Method "DRS" `
            -Message $drsMessage `
            -DrsApplied $appliedCount `
            -DrsFailed $errorCount `
            -DrsTotal $settingCount `
            -BackupFailed $backupFailed
    } catch {
        Write-Warn "DRS write failed: $_"
        return New-NvidiaProfileResult `
            -Status "SessionFailed" `
            -CanCompleteStep $false `
            -Method "DRS" `
            -Message "DRS write failed: $_" `
            -DrsApplied 0 `
            -DrsFailed 0 `
            -DrsTotal $NV_DRS_SETTINGS.Count
    }
}


function Apply-NvidiaCS2ProfileRegistry {
    <#
    .SYNOPSIS  Registry-only fallback for NVIDIA settings.
    .DESCRIPTION
        Applies 25 settings via registry when DRS is unavailable. These values
        are not equivalent to a per-application DRS profile, and their effective
        behavior must be observed on the target NVIDIA driver.
    #>
    param(
        [string]$NvKeyPath,
        [int]$FrlValue = 500,
        [string]$FrlLabel = "500 (repository default)"
    )

    $d3dPath = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"
    $nvGlobalPath = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\NVTweak"

    # Table-driven registry settings. The d3d values are a limited fallback and
    # can be ignored by current drivers.
    $regSettings = @(
        # GPU class values. Stored state does not prove the effective P-state.
        @{ Path=$NvKeyPath;    Name="PerfLevelSrc";                  Value=0x2222; Why="Repository GPU class power-state value" }
        @{ Path=$NvKeyPath;    Name="DisableDynamicPstate";          Value=1;      Why="Repository dynamic P-state policy value" }
        # NVTweak
        @{ Path=$nvGlobalPath; Name="Gestalt";                       Value=1;      Why="Shader cache control enabled" }
        # d3d keys - may be ignored by modern drivers
        @{ Path=$d3dPath;      Name="OGL_THREAD_CONTROL_DEFAULT";    Value=1;      Why="Threaded Optimization: ON" }
        @{ Path=$d3dPath;      Name="OGL_QUALITY_ENHANCEMENTS_DEFAULT"; Value=0;   Why="Triple Buffering: OFF" }
        @{ Path=$d3dPath;      Name="OGL_QUALITY_ENHANCEMENTS";      Value=3;      Why="Texture Filtering: High Performance" }
        @{ Path=$d3dPath;      Name="OGL_FXAA_DEF";                  Value=0;      Why="FXAA: OFF" }
        @{ Path=$d3dPath;      Name="OGL_GAMMA_CORRECT_DEF";         Value=0;      Why="AA Gamma Correction: OFF" }
        @{ Path=$d3dPath;      Name="AA_MODE_SELECTOR";              Value=0;      Why="Antialiasing Mode: Application Controlled" }
        @{ Path=$d3dPath;      Name="AA_LINE_GAMMA";                 Value=0;      Why="AA Line Gamma: OFF" }
        @{ Path=$d3dPath;      Name="LOD_BIAS_ADJUST";               Value=1;      Why="Negative LOD Bias: Clamp" }
        @{ Path=$d3dPath;      Name="PS_TEXFILTER_BILINEAR_QUAL";    Value=0;      Why="Trilinear Optimization: OFF" }
        @{ Path=$d3dPath;      Name="PS_TEXFILTER_ANISO_OPTS2";      Value=0;      Why="Anisotropic Filter Optimization: OFF" }
        @{ Path=$d3dPath;      Name="PS_TEXFILTER_ANISO_OPTS";       Value=0;      Why="Anisotropic Sample Optimization: OFF" }
        @{ Path=$d3dPath;      Name="PS_TEXFILTER_LOD_BIAS";         Value=0;      Why="Driver Controlled LOD Bias: OFF" }
        @{ Path=$d3dPath;      Name="ANISO_SETTING";                 Value=1;      Why="Anisotropic Filtering: Application Controlled" }
        @{ Path=$d3dPath;      Name="ANISO_MODE_SELECTOR";           Value=0;      Why="Anisotropic Mode: Application Controlled" }
        @{ Path=$d3dPath;      Name="MAX_PRERENDERED_FRAMES";        Value=1;      Why="Max Pre-rendered Frames: 1 (requested fallback value)" }
        @{ Path=$d3dPath;      Name="VSYNC_MODE";                    Value=0;      Why="VSync: Force OFF" }
        @{ Path=$d3dPath;      Name="PRERENDERLIMIT_OPTION";         Value=1;      Why="Preferred Refresh Rate: Highest" }
        @{ Path=$d3dPath;      Name="ANSEL_ENABLE";                  Value=0;      Why="Ansel: OFF (requested fallback value)" }
        @{ Path=$d3dPath;      Name="FRL_VALUE";                     Value=$FrlValue; Why="Frame Rate Limiter: $FrlLabel" }
        @{ Path=$d3dPath;      Name="FRL_LOW_LATENCY";               Value=0;      Why="FRL Low Latency: OFF" }
        @{ Path=$d3dPath;      Name="PS_FRAMERATE_LIMITER";          Value=0;      Why="Frame Rate Limiter (legacy): OFF" }
        @{ Path=$d3dPath;      Name="AFR_CONTROL";                   Value=0;      Why="Smooth AFR: OFF" }
    )

    $appliedCount = 0
    $failedCount = 0
    $dryRunCount = 0
    foreach ($s in $regSettings) {
        $writeResult = Set-RegistryValue $s.Path $s.Name $s.Value "DWord" $s.Why -PassThru
        if ($writeResult.Applied) {
            $appliedCount++
        } elseif ($writeResult.Status -eq "DryRun") {
            $dryRunCount++
        } else {
            $failedCount++
        }
    }

    # ── Fallback Summary ────────────────────────────────────────────────────
    Write-Blank
    Write-ConsoleLine "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Yellow
    Write-ConsoleLine "  │  NVIDIA CS2 PROFILE - $appliedCount settings via REGISTRY (fallback)$((' ' * (6 - "$appliedCount".Length)))│" -ForegroundColor Yellow
    Write-ConsoleLine "  │                                                              │" -ForegroundColor Yellow
    Write-ConsoleLine "  │  ⚠  DRS direct write was unavailable.                       │" -ForegroundColor Yellow
    Write-ConsoleLine "  │  These values are not equivalent to an application DRS     │" -ForegroundColor Yellow
    Write-ConsoleLine "  │  profile. Current drivers may ignore registry d3d keys.     │" -ForegroundColor Yellow
    Write-ConsoleLine "  │                                                              │" -ForegroundColor Yellow
    Write-ConsoleLine "  │  FOR THE PER-APPLICATION DRS PATH:                          │" -ForegroundColor White
    Write-ConsoleLine "  │  Re-run after installing NVIDIA driver with nvapi64.dll    │" -ForegroundColor White
    Write-ConsoleLine "  │  or use NVIDIA Profile Inspector to set manually.          │" -ForegroundColor DarkGray
    Write-ConsoleLine "  │  NPI: github.com/Orbmu2k/nvidiaProfileInspector            │" -ForegroundColor DarkGray
    Write-ConsoleLine "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Yellow

    if ($failedCount -eq 0 -and $dryRunCount -eq 0) {
        Write-Info "All $appliedCount registry settings backed up automatically for rollback."
    } elseif ($dryRunCount -gt 0 -and $failedCount -eq 0) {
        Write-Info "Registry fallback previewed in dry-run mode."
    } else {
        Write-Warn "$failedCount registry fallback setting(s) failed. Review warnings before continuing."
    }

    $status = if ($dryRunCount -gt 0 -and $failedCount -eq 0) { "DryRun" } elseif ($failedCount -eq 0) { "Fallback" } else { "Failed" }
    $message = switch ($status) {
        "DryRun" { "Registry fallback previewed." }
        "Fallback" { "Registry fallback applied." }
        default { "Registry fallback had failed writes." }
    }
    return New-NvidiaProfileResult `
        -Status $status `
        -CanCompleteStep ($status -eq "Fallback") `
        -Method "RegistryFallback" `
        -Message $message `
        -RegistryApplied $appliedCount `
        -RegistryFailed $failedCount
}
