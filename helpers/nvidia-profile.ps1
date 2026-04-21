# ==============================================================================
#  helpers/nvidia-profile.ps1  —  NVIDIA CS2 Profile Settings (DRS + Registry)
# ==============================================================================
#
#  Applies optimized NVIDIA driver settings for CS2 using TWO methods:
#
#  1. DRS Direct Write (preferred):
#     Calls nvapi64.dll via helpers/nvidia-drs.ps1 to write all 52 DWORD
#     settings directly to the DRS binary database (nvdrs.dat).
#     This is the same mechanism NVIDIA Profile Inspector uses.
#
#  2. Registry Fallback (if DRS unavailable):
#     Writes 25 settings to HKLM registry keys. Only PerfLevelSrc=0x2222
#     in the GPU class key is confirmed effective on modern drivers.
#     Directs users to re-run with DRS or use NPI manually.
#
#  52 DWORD settings applied via DRS (derived from public NVIDIA DRS documentation,
#  NvApiDriverSettings.h, community testing, and reverse engineering).
#  3 settings intentionally excluded:
#    -  1 string setting (269308407) — unknown effect
#    -  1 hardware-specific (550564838) — GPU device ID
#    -  1 net-negative (2966161525) — frame interpolation = latency
#  Plus 1 registry-only (PerfLevelSrc) — applied via registry always
#

# ── Settings table: all 52 DWORD settings for DRS ───────────────────────────
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
    @{ Id=277041162;  Value=500;        Name="FRL NVCPL: 500 FPS cap (effectively unlimited for most monitors)" }

    # ── VRR / G-SYNC (suite default; benchmark on VRR/Pulsar displays) ─────
    @{ Id=278196567;  Value=0;          Name="VRR global feature: OFF (suite default)" }
    @{ Id=278196727;  Value=0;          Name="VRR requested state: OFF (suite default)" }
    @{ Id=279476652;  Value=1;          Name="G-SYNC: FORCE_OFF (suite default)" }
    # Removed: Id=279476686 (0x10A879CE) — not in NPI, likely inert; other 6 G-SYNC settings cover VRR
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
    # Community benchmark heuristic: some CS2 benches reported better 1% lows with
    # per-app rBAR disabled, but this is not a universal NVIDIA recommendation.
    # Keep system-wide BIOS rBAR enabled unless your own benchmarks suggest otherwise.
    @{ Id=983226;     Value=0;          Name="rBAR: Disabled for CS2 by suite default (community benchmark heuristic)" }
    @{ Id=983227;     Value=0;          Name="rBAR - Options: Disabled (companion setting)" }

    # ── Shader Cache ─────────────────────────────────────────────────────────
    @{ Id=11306135;   Value=10240;      Name="Shader disk cache max: 10240 MB (10 GB)" }

    # ── SLI / AFR ────────────────────────────────────────────────────────────
    @{ Id=270198627;  Value=0;          Name="Smooth AFR: OFF" }

    # ── CUDA P-State Lock ────────────────────────────────────────────────────
    # Removed: Id=1074665807 (0x400E194F) — undocumented duplicate; NPI-recognized
    # Id=1343646814 (0x50166C5E) below handles the same CUDA P-state override.

    # ── Decoded flags (source: CustomSettingNames.xml + NVIDIA 2022 leak DB) ─
    @{ Id=390467;     Value=1;          Name="Ultra Low Latency - CPL State: On (Reflex prereq — CPL mode active; NVCP ULL-Enabled is a separate setting)" }
    @{ Id=14566042;   Value=0;          Name="DXR_ENABLE: OFF (DirectX Raytracing disabled — CS2 doesn't use DXR)" }
    @{ Id=274606621;  Value=4;          Name="ANSEL_FREESTYLE_MODE: APPROVED_ONLY (4; no active overhead)" }
    @{ Id=549198379;  Value=0;          Name="VK_NV_RAYTRACING: DISABLE (Vulkan RT extension off — CS2 doesn't use VK RT)" }
    @{ Id=1343646814; Value=0;          Name="CUDA_STABLE_PERF_LIMIT: FORCE_OFF (0; prevents CUDA P-state drop to P2)" }
    @{ Id=2156231208; Value=1;          Name="GFE_MONITOR_USAGE: 1 (GFE telemetry state; no impact without GFE installed)" }

    # ── Partially decoded flags (inferred from position + NVIDIA leak DB) ────
    @{ Id=3224887;    Value=4;          Name="PS_ASYNC_SHADER_SCHEDULER variant (0x313537; value=4; likely thread count)" }
    @{ Id=11313945;   Value=1;          Name="PS_ pipeline/shader cache variant (0xACA319; value=1=enabled)" }
    @{ Id=12623113;   Value=2;          Name="FORCE_GPUKERNEL_COP_ARCH variant (0xC09D09; GPU kernel arch override)" }
    @{ Id=270883746;  Value=0;          Name="SHIM_RENDERING_OPTIONS companion flag (0x10255BA2; always 0)" }
    @{ Id=270883750;  Value=469762050;  Name="SHIM_RENDERING_OPTIONS extended (0x10255BA6; 0x1C000002 = EHSHELL_DETECT|DISABLE_TURING_POWER_POLICY)" }
    @{ Id=271076560;  Value=0;          Name="MCSXX/SLI flag (0x10284CD0; disabled; no-op on single-GPU)" }
    @{ Id=539250342;  Value=1;          Name="VK_SLI_WAR or similar Vulkan workaround flag (0x20244EA6; 1=enabled)" }
    @{ Id=544173595;  Value=60;         Name="VK_LOW_LATENCY family (0x206F6E1B; value=60; likely sleep/overlap target µs)" }

    # ── Unknown post-2022 flags (driver-ignored on current releases) ──────────
    # NPI verification: these IDs do not appear anywhere in the loaded profile —
    # not in named sections, not in Unknown. Driver silently discards on import.
    # DRS write is harmless; driver doesn't act on unrecognized IDs.
    @{ Id=276387096;  Value=60;         Name="Unknown post-2022 flag (0x10795518; driver-ignored on current releases)" }
    @{ Id=276387097;  Value=0;          Name="Unknown post-2022 flag (0x10795519; driver-ignored on current releases)" }
)
# TOTAL: 52 DWORD settings via DRS (was 51: added rBAR Options companion setting)

# ── Excluded settings ──────────────────────────────────────────────────────
# 2966161525 (0xB0CC0875) — Smooth Motion APIs = 1 → frame interpolation adds latency
# 550564838  (0x20D0F3E6) — OpenGL GPU Affinity → hardcoded GPU-specific PCI device ID
# 269308407  (0x100D51F7) — String setting "Buffers=(Depth)" → DRS string type, marginal
# ─────────────────────────────────────────────────────────────────────────────


function Apply-NvidiaCS2Profile {
    <#
    .SYNOPSIS  Applies optimized CS2 NVIDIA driver profile settings.
    .DESCRIPTION
        DRS-first: writes all 52 DWORD settings directly to the NVIDIA DRS
        binary database via nvapi64.dll P/Invoke.  Falls back to registry
        writes if DRS is unavailable (AMD GPU, missing DLL, 32-bit PS).

        Always applies PerfLevelSrc=0x2222 via registry (the one confirmed-
        effective registry key on all driver versions).
    #>

    Write-Step "Applying NVIDIA CS2 profile settings..."

    # ── Locate NVIDIA GPU registry key (needed for PerfLevelSrc) ────────────
    $classPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\$CFG_GUID_Display"

    $nvKeyPath = $null
    if (Test-Path $classPath) {
        $subkeys = Get-ChildItem $classPath -ErrorAction SilentlyContinue |
            Where-Object { $_.PSChildName -match "^\d{4}$" }
        foreach ($key in $subkeys) {
            $props = Get-ItemProperty $key.PSPath -ErrorAction SilentlyContinue
            if ($props.ProviderName -match "NVIDIA" -or $props.DriverDesc -match "NVIDIA") {
                $nvKeyPath = $key.PSPath
                Write-DebugLog "NVIDIA GPU key: $($key.PSChildName) — $($props.DriverDesc)"
                break
            }
        }
    }

    if (-not $nvKeyPath) {
        Write-Warn "NVIDIA GPU registry key not found. Install the driver first."
        return
    }

    # ── FPS cap override for FRL setting ────────────────────────────────────
    $frlValue = 500
    $frlLabel = "500 (effectively unlimited)"
    if ((Get-Variable -Name fpsCap -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:fpsCap -gt 0) {
        $frlValue = $SCRIPT:fpsCap
        $frlLabel = "$($SCRIPT:fpsCap) (user FPS cap)"
    }

    # ── Try DRS direct write (preferred path) ──────────────────────────────
    $drsSuccess = $false
    if (Initialize-NvApiDrs) {
        $drsSuccess = Apply-NvidiaCS2ProfileDrs -FrlValue $frlValue -FrlLabel $frlLabel
    }

    # ── Fallback: registry-only (if DRS unavailable or failed) ──────────────
    if (-not $drsSuccess) {
        Write-Warn "DRS direct write unavailable — falling back to registry method."
        Apply-NvidiaCS2ProfileRegistry -NvKeyPath $nvKeyPath -FrlValue $frlValue -FrlLabel $frlLabel
        return
    }

    # ── GPU class registry keys: P-state locks ──────────────────────────────
    # PerfLevelSrc: the ONE confirmed-effective registry key for P-state override.
    # DisableDynamicPstate: complementary — djdallmann confirmed via nvidia-smi
    # monitoring that this locks P0 at the driver level independently from NVCP.
    # Both go in the GPU hardware class key, NOT d3d. Effective on all drivers.
    Set-RegistryValue $nvKeyPath "PerfLevelSrc"       0x2222 "DWord" "P-state: Max Performance (GPU class key)"
    Set-RegistryValue $nvKeyPath "DisableDynamicPstate" 1    "DWord" "Lock P0 at driver level (complements PerfLevelSrc)"

    # ── DRS Success Summary ─────────────────────────────────────────────────
    $settingCount = $NV_DRS_SETTINGS.Count
    $appliedCount = if ($null -ne $SCRIPT:_drsApplied) { $SCRIPT:_drsApplied } else { $settingCount }
    $errorCount   = if ($null -ne $SCRIPT:_drsErrors)  { $SCRIPT:_drsErrors }  else { 0 }
    Write-Blank
    $statusColor = if ($errorCount -eq 0) { "Green" } else { "Yellow" }
    $drsLabel = if ($errorCount -eq 0) { "$appliedCount DRS" } else { "$appliedCount/$settingCount DRS ($errorCount failed)" }
    Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor $statusColor
    $contentStr = "  NVIDIA CS2 PROFILE — $drsLabel + 2 registry"
    $padLen = [math]::Max(1, 64 - $contentStr.Length)
    Write-Host "  │$contentStr$((' ' * $padLen))│" -ForegroundColor $statusColor
    Write-Host "  │                                                              │" -ForegroundColor Green
    Write-Host "  │  Method: DRS direct write (nvapi64.dll)                     │" -ForegroundColor White
    Write-Host "  │  Profile: Counter-strike 2  (cs2.exe / csgos2.exe)          │" -ForegroundColor White
    Write-Host "  │                                                              │" -ForegroundColor Green
    Write-Host "  │  ✔  Power Management:    Prefer Maximum Performance         │" -ForegroundColor White
    Write-Host "  │  ✔  Threaded Optimization: Force ON                         │" -ForegroundColor White
    Write-Host "  │  ✔  Texture Filtering:   High Performance                   │" -ForegroundColor White
    Write-Host "  │  ✔  Triple Buffering:    OFF                                │" -ForegroundColor White
    Write-Host "  │  ✔  VSync:               Force OFF                          │" -ForegroundColor White
    Write-Host "  │  ✔  G-SYNC / VRR:        Disabled by suite default          │" -ForegroundColor White
    Write-Host "  │  ✔  FXAA / Ansel:        OFF                                │" -ForegroundColor White
    Write-Host "  │  ✔  Max Pre-rendered:    1 frame                            │" -ForegroundColor White
    Write-Host "  │  ✔  Frame Rate Limiter:  $frlLabel$((' ' * [math]::Max(0, 36 - $frlLabel.Length)))│" -ForegroundColor White
    $summaryDisplayed = 9  # Number of settings explicitly listed in the summary box above
    Write-Host "  │  ✔  + $($settingCount - $summaryDisplayed) more DRS settings (AA, LOD, Optimus, cache...)     │" -ForegroundColor DarkGray
    Write-Host "  │                                                              │" -ForegroundColor Green
    Write-Host "  │  Verify: open NVIDIA Profile Inspector → Counter-strike 2   │" -ForegroundColor DarkGray
    Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Green

    Write-Info "All DRS settings backed up automatically for rollback."
}


function Apply-NvidiaCS2ProfileDrs {
    <#
    .SYNOPSIS  Writes all 52 DWORD settings to DRS via nvapi64.dll.
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

    # Validate FRL value — prevent nonsensical caps from corrupting the DRS profile
    if ($FrlValue -le 0 -or $FrlValue -gt 1000) { $FrlValue = 500 }

    # ── DRY-RUN: print what WOULD be applied, skip Invoke-DrsSession entirely ──
    # The entire Invoke-DrsSession call creates a real DRS session that executes
    # CreateProfile/AddApplication — mutating the DRS database even in DRY-RUN.
    # Only the registry fallback path (PerfLevelSrc, DisableDynamicPstate) goes
    # through Set-RegistryValue which handles DRY-RUN itself.
    if ($SCRIPT:DryRun) {
        $applied = 0
        foreach ($s in $NV_DRS_SETTINGS) {
            $writeValue = [uint32]$s.Value
            if ($s.Id -eq 277041162 -and $FrlValue -ne 500) {
                $writeValue = [uint32]$FrlValue
            }
            Write-Host "  [DRY-RUN] Would set DRS: $($s.Name) = $writeValue" -ForegroundColor Magenta
            $applied++
        }
        Write-Host "  [DRY-RUN] Would save DRS database" -ForegroundColor Magenta
        $SCRIPT:_drsApplied = $applied
        $SCRIPT:_drsErrors  = 0
        return $true
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
                # cs2.exe found in a profile — check if it's the Base Profile.
                # The Base Profile (aka "Global" / "_GLOBAL_DRIVER_PROFILE") is the
                # default catch-all profile for all applications. Writing CS2-specific
                # settings to it would affect EVERY application, not just CS2.
                # Detect the Base Profile by handle comparison, not by name search,
                # so we always write to the profile that actually owns cs2.exe.
                $baseProfile = [IntPtr]::Zero
                try { $baseProfile = [NvApiDrs]::FindProfileByName($session, "_GLOBAL_DRIVER_PROFILE") } catch { }
                if ($baseProfile -ne [IntPtr]::Zero -and $drsProfile -eq $baseProfile) {
                    # cs2.exe is in the Base Profile — create a dedicated profile and move it
                    Write-DebugLog "DRS: cs2.exe found in Base Profile — creating dedicated CS2 profile"
                    $profileName = "Counter-strike 2"
                    $drsProfile = [NvApiDrs]::CreateProfile($session, $profileName)
                    $profileCreated = $true
                    # Bind applications to the new dedicated profile (AddApplication
                    # with -179 on Base Profile is expected — the exe will be re-bound)
                    try { [NvApiDrs]::AddApplication($session, $drsProfile, "cs2.exe") } catch {
                        Write-Warn "DRS: AddApplication cs2.exe to dedicated profile — $_"
                    }
                    try { [NvApiDrs]::AddApplication($session, $drsProfile, "csgos2.exe") } catch {
                        Write-Warn "DRS: AddApplication csgos2.exe to dedicated profile — $_"
                    }
                } else {
                    # cs2.exe is in a non-Base profile — use that profile directly
                    # (regardless of its name — it's the profile the driver reads for cs2.exe)
                    $profileName = "(cs2.exe profile)"
                    Write-DebugLog "DRS: cs2.exe found in dedicated profile (handle $drsProfile)"
                }
            } else {
                # cs2.exe not in any profile — search by known names
                foreach ($name in @("Counter-strike 2", "Counter-Strike 2")) {
                    $drsProfile = [NvApiDrs]::FindProfileByName($session, $name)
                    if ($drsProfile -ne [IntPtr]::Zero) {
                        $profileName = $name
                        break
                    }
                }

                if ($drsProfile -eq [IntPtr]::Zero) {
                    # No existing profile — create one
                    $profileName = "Counter-strike 2"
                    $drsProfile = [NvApiDrs]::CreateProfile($session, $profileName)
                    $profileCreated = $true
                    Write-DebugLog "DRS: Created profile '$profileName'"
                }

                # Bind applications — only needed for newly created profiles.
                # Predefined/existing profiles already have cs2.exe pre-bound
                # in NVIDIA's shipped DRS database (nvdrs.dat).
                if ($profileCreated) {
                    try { [NvApiDrs]::AddApplication($session, $drsProfile, "cs2.exe") } catch {
                        Write-Warn "DRS: AddApplication cs2.exe — $_"
                    }
                    try { [NvApiDrs]::AddApplication($session, $drsProfile, "csgos2.exe") } catch {
                        Write-Warn "DRS: AddApplication csgos2.exe — $_"
                    }
                } else {
                    Write-DebugLog "DRS: Profile '$profileName' found — cs2.exe pre-bound by NVIDIA, skipping AddApplication"
                }
            }

            # ── Backup current DRS values ───────────────────────────────────
            # Backup failure must not abort the settings write — wrap separately
            $effectiveTitle = if ((Get-Variable -Name CurrentStepTitle -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:CurrentStepTitle) { $SCRIPT:CurrentStepTitle } else { "NVIDIA CS2 DRS Profile" }
            try {
                Backup-DrsSettings -Session $session -DrsProfile $drsProfile `
                    -SettingIds ($NV_DRS_SETTINGS | ForEach-Object { $_.Id }) `
                    -StepTitle $effectiveTitle `
                    -ProfileName $(if ($profileName) { $profileName } elseif ((Get-Variable -Name DRS_FOUND_VIA_APP -Scope Script -ErrorAction SilentlyContinue)) { $SCRIPT:DRS_FOUND_VIA_APP } else { "(found via cs2.exe)" }) `
                    -ProfileCreated $profileCreated
            } catch {
                Write-Warn "DRS backup failed (settings will still be applied): $_"
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

        return $true
    } catch {
        Write-Warn "DRS write failed: $_"
        return $false
    }
}


function Apply-NvidiaCS2ProfileRegistry {
    <#
    .SYNOPSIS  Registry-only fallback for NVIDIA settings.
    .DESCRIPTION
        Applies 25 settings via registry. Only PerfLevelSrc=0x2222 is
        confirmed effective on modern drivers. Included for systems where
        nvapi64.dll is unavailable (AMD GPU, 32-bit PS, old driver).
    #>
    param(
        [string]$NvKeyPath,
        [int]$FrlValue = 500,
        [string]$FrlLabel = "500 (effectively unlimited)"
    )

    $d3dPath = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"
    $nvGlobalPath = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\NVTweak"

    # Table-driven registry settings. Only PerfLevelSrc (GPU class key) is confirmed
    # effective on modern drivers. d3d keys are best-effort fallback.
    $regSettings = @(
        # GPU class key — confirmed effective
        @{ Path=$NvKeyPath;    Name="PerfLevelSrc";                  Value=0x2222; Why="Power Management: Max Performance" }
        @{ Path=$NvKeyPath;    Name="DisableDynamicPstate";          Value=1;      Why="Lock P0 at driver level (complements PerfLevelSrc)" }
        # NVTweak
        @{ Path=$nvGlobalPath; Name="Gestalt";                       Value=1;      Why="Shader cache control enabled" }
        # d3d keys — may be ignored by modern drivers
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
        @{ Path=$d3dPath;      Name="MAX_PRERENDERED_FRAMES";        Value=1;      Why="Max Pre-rendered Frames: 1 (less input lag)" }
        @{ Path=$d3dPath;      Name="VSYNC_MODE";                    Value=0;      Why="VSync: Force OFF" }
        @{ Path=$d3dPath;      Name="PRERENDERLIMIT_OPTION";         Value=1;      Why="Preferred Refresh Rate: Highest" }
        @{ Path=$d3dPath;      Name="ANSEL_ENABLE";                  Value=0;      Why="Ansel: OFF (saves overhead)" }
        @{ Path=$d3dPath;      Name="FRL_VALUE";                     Value=$FrlValue; Why="Frame Rate Limiter: $FrlLabel" }
        @{ Path=$d3dPath;      Name="FRL_LOW_LATENCY";               Value=0;      Why="FRL Low Latency: OFF" }
        @{ Path=$d3dPath;      Name="PS_FRAMERATE_LIMITER";          Value=0;      Why="Frame Rate Limiter (legacy): OFF" }
        @{ Path=$d3dPath;      Name="AFR_CONTROL";                   Value=0;      Why="Smooth AFR: OFF" }
    )

    $appliedCount = 0
    foreach ($s in $regSettings) {
        Set-RegistryValue $s.Path $s.Name $s.Value "DWord" $s.Why
        $appliedCount++
    }

    # ── Fallback Summary ────────────────────────────────────────────────────
    Write-Blank
    Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Yellow
    Write-Host "  │  NVIDIA CS2 PROFILE — $appliedCount settings via REGISTRY (fallback)$((' ' * (6 - "$appliedCount".Length)))│" -ForegroundColor Yellow
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │  ⚠  DRS direct write was unavailable.                       │" -ForegroundColor Yellow
    Write-Host "  │  Only PerfLevelSrc (GPU class key) is confirmed effective   │" -ForegroundColor Yellow
    Write-Host "  │  on modern drivers. Registry d3d keys may be ignored.       │" -ForegroundColor Yellow
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │  FOR FULL DRS COVERAGE:                                      │" -ForegroundColor White
    Write-Host "  │  Re-run after installing NVIDIA driver with nvapi64.dll    │" -ForegroundColor White
    Write-Host "  │  or use NVIDIA Profile Inspector to set manually.          │" -ForegroundColor DarkGray
    Write-Host "  │  NPI: github.com/Orbmu2k/nvidiaProfileInspector            │" -ForegroundColor DarkGray
    Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Yellow

    Write-Info "All $appliedCount registry settings backed up automatically for rollback."
}
