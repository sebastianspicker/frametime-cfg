# ==============================================================================
#  Optimize-RegistryTweaks.ps1  -  Steps 23-33: Fast Startup, RAM, Nagle,
#                                   FSE, Scheduler, Timer, Mouse, GPU Pref,
#                                   Game DVR, Overlay, Audio
# ==============================================================================

# ══════════════════════════════════════════════════════════════════════════════
# STEP 23 - DISABLE FAST STARTUP  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 23) {
    Write-Section "Step 23 - Disable Fast Startup (Hybrid Boot)"
    $null = Invoke-TieredStep -Tier 2 -Title "Disable Fast Startup (HiberbootEnabled=0)" `
        -Why "Disables Windows hybrid shutdown so a shutdown is followed by a cold boot instead of resuming the hiberboot image." `
        -Evidence "HiberbootEnabled controls Fast Startup. This repository includes no hardware-specific startup-time or driver-behavior benchmark." `
        -Caveat "Startup can take longer. Sleep and full hibernation are separate Windows features." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Uses a cold-boot path after shutdown" `
        -SideEffects "Slightly longer shutdown and startup time (cold boot instead of hybrid boot)" `
        -Undo "Set HiberbootEnabled = 1 in HKLM:\SYSTEM\...\Power (or re-enable in Power Options -> Choose what the power buttons do)" `
        -Action {
            Set-RegistryValue "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power" `
                "HiberbootEnabled" 0 "DWord" "Disable Fast Startup (hybrid boot)"
            if (-not $SCRIPT:DryRun) {
                Write-OK "Fast Startup disabled. Changes take effect on next shutdown + cold boot."
                Write-Info "Note: Hibernate / Sleep mode is NOT affected - only 'Shut Down' behavior."
            }
            Complete-Step $PHASE 23 "HiberbootEnabled"
        } `
        -SkipAction { Skip-Step $PHASE 23 "HiberbootEnabled" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 24 - DUAL-CHANNEL RAM DETECTION  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 24) {
    Write-Section "Step 24 - Dual-Channel RAM Detection"
    $null = Invoke-TieredStep -Tier 1 -Title "Check dual-channel RAM" `
        -Why "Memory channel population can affect available bandwidth. The effect is hardware- and workload-specific." `
        -Evidence "The check reads installed memory topology. This repository contains no local CS2 benchmark for channel configuration." `
        -Risk "SAFE" -Depth "CHECK" `
        -Improvement "Identifies a single-channel memory configuration" `
        -SideEffects "None; this is a read-only detection step" `
        -Undo "N/A (check only)" `
        -SkipAction { Skip-Step $PHASE 24 "DualChannel" } `
        -Action {
            $dc = Test-DualChannel
            if ($null -eq $dc.DualChannel) {
                Write-Warn $dc.Reason
            } elseif ($dc.DualChannel) {
                Write-OK "$($dc.Reason)"
            } else {
                Write-Warn "MEMORY CHANNEL POPULATION REQUIRES MANUAL REVIEW"
                Write-Blank
                Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Red
                Write-Host "  │  $($dc.Reason)$((' ' * [math]::Max(0, 60 - $dc.Reason.Length)))│" -ForegroundColor Red
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  A single-module layout can reduce available memory        │" -ForegroundColor Yellow
                Write-Host "  │  bandwidth. The effect varies by platform and workload.    │" -ForegroundColor Yellow
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  REVIEW:                                                    │" -ForegroundColor White
                Write-Host "  │  Consult the motherboard population guide and memory QVL. │" -ForegroundColor White
                Write-Host "  │  Use a validated matched configuration if changing RAM.   │" -ForegroundColor White
                Write-Host "  │  Verify channel mode and run stability tests afterwards.  │" -ForegroundColor White
                Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Red
                Write-Blank
                if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] to continue" }
            }
            Complete-Step $PHASE 24 "DualChannel"
        }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 25 - NAGLE'S ALGORITHM DISABLE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 25) {
    Write-Section "Step 25 - Disable Nagle's Algorithm"
    $null = Invoke-TieredStep -Tier 2 -Title "Disable TCP Nagle delay (TcpNoDelay + TcpAckFrequency)" `
        -Why "Changes TCP acknowledgment frequency and Nagle behavior for the active interface. CS2 gameplay traffic is not assumed to use these TCP controls." `
        -Evidence "The registry values are deterministic. This repository includes no application-level latency benchmark for the change." `
        -Caveat "These values affect TCP applications on the selected interface. They do not control UDP game datagrams." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Requests immediate TCP acknowledgments and disables Nagle coalescing on the selected interface" `
        -SideEffects "Can change TCP packet and acknowledgment frequency for other applications on the selected interface" `
        -Undo "Delete TcpNoDelay + TcpAckFrequency values from NIC interface key" `
        -Action {
            $nicGuid = Get-ActiveNicGuid
            if ($nicGuid) {
                $regBase = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\$nicGuid"
                Set-RegistryValue $regBase "TcpNoDelay" 1 "DWord" "Disable Nagle's Algorithm"
                Set-RegistryValue $regBase "TcpAckFrequency" 1 "DWord" "Send TCP ACK immediately"
                Write-ActionOK "Nagle disabled for NIC: $nicGuid"
            } else {
                # Check if the user is on Wi-Fi only (wired adapter filter excludes Wi-Fi)
                $wifiOnly = $false
                try {
                    $wifiUp = Get-NetAdapter -ErrorAction SilentlyContinue |
                        Where-Object { $_.Status -eq "Up" -and $_.InterfaceDescription -match "Wi-Fi|Wireless" }
                    if ($wifiUp) { $wifiOnly = $true }
                } catch {
                    Write-DebugLog "Wi-Fi adapter detection failed during Nagle guidance: $($_.Exception.Message)"
                }
                if ($wifiOnly) {
                    Write-Warn "Wi-Fi connection detected - Nagle disable targets wired Ethernet adapters only."
                    Write-Info "For Wi-Fi: Settings -> Network -> Wi-Fi -> Hardware properties -> note the adapter GUID."
                } else {
                    Write-Warn "Active network adapter not found - set manually in regedit."
                }
                Write-Info "HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{NIC-GUID}"
                Write-Info "TcpNoDelay = 1 (DWord) | TcpAckFrequency = 1 (DWord)"
            }
            Complete-Step $PHASE 25 "Nagle"
        } `
        -SkipAction { Skip-Step $PHASE 25 "Nagle" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 26 - GAMECONFIGSTORE FSE REGISTRY  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 26) {
    Write-Section "Step 26 - GameConfigStore FSE Registry"
    $null = Invoke-TieredStep -Tier 2 -Title "Set GameConfigStore fullscreen-exclusive keys" `
        -Why "Applies the repository GameConfigStore values associated with fullscreen behavior." `
        -Evidence "The registry state is verifiable. Presentation behavior still depends on Windows, the driver, and the game." `
        -Caveat "The values may be ignored outside applicable fullscreen presentation paths." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Applies the documented GameConfigStore policy values" `
        -SideEffects "Can change fullscreen presentation behavior" `
        -Undo "Delete GameDVR_DXGIHonorFSEWindowsCompatible, GameDVR_FSEBehavior, GameDVR_FSEBehaviorMode, GameDVR_HonorUserFSEBehaviorMode from HKCU:\System\GameConfigStore" `
        -Action {
            $gcsPath = "HKCU:\System\GameConfigStore"
            Set-RegistryValue $gcsPath "GameDVR_DXGIHonorFSEWindowsCompatible" 1 "DWord" "FSE compatible"
            Set-RegistryValue $gcsPath "GameDVR_FSEBehavior"                   2 "DWord" "FSE behavior"
            Set-RegistryValue $gcsPath "GameDVR_FSEBehaviorMode"               2 "DWord" "FSE mode"
            Set-RegistryValue $gcsPath "GameDVR_HonorUserFSEBehaviorMode"      1 "DWord" "Respect user FSE mode"
            Write-ActionOK "GameConfigStore FSE keys set."
            Complete-Step $PHASE 26 "GameConfigStore"
        } `
        -SkipAction { Skip-Step $PHASE 26 "GameConfigStore" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 27 - SYSTEMRESPONSIVENESS + GAMING PRIORITY  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 27) {
    Write-Section "Step 27 - System Scheduling, Gaming Priority + Latency Tweaks"
    $null = Invoke-TieredStep -Tier 2 -Title "Multimedia SystemProfile + scheduler + system latency tweaks (FTH, NTFS, Maintenance)" `
        -Why "Applies the repository's MMCSS, foreground scheduling, memory, maintenance, NTFS, and device-installer policy values. NetworkThrottlingIndex is deliberately not changed." `
        -Evidence "Windows defines the registry values and their control surfaces. This repository does not include isolated benchmark artifacts for the combined policy set." `
        -Caveat "NoLazyMode may increase background CPU activity. PowerThrottlingOff is limited to detected Intel hybrid CPUs. Disabling FTH can expose heap errors. Automatic maintenance must be run manually. Disabling 8.3 names can break legacy applications." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Applies the documented repository policy values and records them for restoration" `
        -SideEffects "Background media apps get slightly less priority. NoLazyMode: marginally higher CPU cycles. FTH disabled: rare heap errors may not be silently suppressed. Maintenance won't run automatically. NTFS: 8.3 filename aliases removed (breaks legacy 16-bit app compatibility)." `
        -Undo "Set SystemResponsiveness=20, Win32PrioritySeparation=2, delete Games/NoLazyMode keys; FTH\Enabled=1; MaintenanceDisabled=0; Delete NtfsDisableLastAccessUpdate or set to 2 (system-managed enabled); NtfsDisable8dot3NameCreation=0; DisableCoInstallers=0" `
        -Action {
            function Set-RequiredRegistryValue {
                param([string]$Path, [string]$Name, $Value, [string]$Type, [string]$Why)

                $result = Set-RegistryValue $Path $Name $Value $Type $Why -PassThru
                if (-not $result.Applied -and $result.Status -ne "DryRun") {
                    throw "Required registry write failed for $Path | ${Name}: $($result.Message)"
                }
            }

            $mmPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile"
            Set-RequiredRegistryValue $mmPath "SystemResponsiveness" 10 "DWord" "Less CPU reserved for MMCSS"
            # NoLazyMode: shifts MMCSS from periodic idle-detection to realtime-only operation.
            # djdallmann GamingPCSetup: "shifts from idle-detection modes to realtime-only operation"
            Set-RequiredRegistryValue $mmPath "NoLazyMode" 1 "DWord" "MMCSS realtime-only (no idle detection)"
            # NOTE: NetworkThrottlingIndex deliberately NOT set - djdallmann xperf shows 0xFFFFFFFF increases DPC latency
            $gamesPath = "$mmPath\Tasks\Games"
            Set-RequiredRegistryValue $gamesPath "Priority"              6      "DWord"  "Gaming priority 6"
            Set-RequiredRegistryValue $gamesPath "Scheduling Category"   "High" "String" "High scheduling"
            Set-RequiredRegistryValue $gamesPath "GPU Priority"          8      "DWord"  "GPU priority 8"

            # Foreground scheduler quantum: short interval, FIXED, max priority separation (PsPrioritySeparation=2)
            # 0x2A = binary 00 10 10 10 = Interval:Short(2), Length:Fixed(2), PrioritySeparation:2(Max boost)
            # Previous repository value: 0x26 (variable quantum).
            # The current policy selects a fixed foreground quantum. The repository does not
            # include an isolated benchmark comparing these two values.
            Set-RequiredRegistryValue "HKLM:\SYSTEM\CurrentControlSet\Control\PriorityControl" `
                "Win32PrioritySeparation" 0x2A "DWord" "Short quantum, fixed, max foreground boost (0x2A)"

            # Request that pageable kernel and driver code remain resident. The repository
            # does not contain a trace establishing a frame-time result from this policy.
            $memMgmt = "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management"
            Set-RequiredRegistryValue $memMgmt "DisablePagingExecutive" 1 "DWord" "Keep kernel code in RAM"

            # Repository heuristic for detected Intel hybrid CPUs: disable the
            # operating-system Power Throttling policy.
            $intelHybridName = Get-IntelHybridCpuName
            if ($intelHybridName) {
                $ptPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Power\PowerThrottling"
                # Set-RegistryValue creates the key path if missing - no need for standalone New-Item
                Set-RequiredRegistryValue $ptPath "PowerThrottlingOff" 1 "DWord" "Disable Intel Power Throttling policy"
                Write-ActionOK "Intel hybrid CPU ($intelHybridName) - Power Throttling disabled."
            } else {
                Write-Sub "Power Throttling: not applicable (non-Intel-hybrid CPU)"
            }

            # ── Fault Tolerant Heap disable ───────────────────────────────────────
            # Fault Tolerant Heap is a Windows compatibility mitigation. This
            # global policy can affect applications other than CS2.
            Set-RequiredRegistryValue "HKLM:\SOFTWARE\Microsoft\FTH" "Enabled" 0 "DWord" `
                "Disable the Fault Tolerant Heap compatibility mitigation"

            # ── DisableCoInstallers ──────────────────────────────────────────────────
            # Request disabled third-party co-installer execution during PnP
            # device installation. This can affect later device setup.
            Set-RequiredRegistryValue "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Device Installer" `
                "DisableCoInstallers" 1 "DWord" "Disable PnP co-installer DLLs"

            # ── Automatic Maintenance disable ────────────────────────────────────────
            # This policy disables automatic maintenance scheduling. Maintenance tasks then
            # need to be initiated manually.
            $maintPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\Maintenance"
            Set-RequiredRegistryValue $maintPath "MaintenanceDisabled" 1 "DWord" `
                "Disable Windows Automatic Maintenance scheduler"

            # ── NTFS metadata write elimination ──────────────────────────────────────
            # NtfsDisableLastAccessUpdate=1: stops NTFS from updating the "last access"
            # timestamp on every file read - eliminating a metadata write from each file I/O.
            # NtfsDisable8dot3NameCreation=1: stops NTFS from maintaining legacy 8.3 aliases
            # (e.g., "PROGRA~1") alongside every full-length filename. Removes per-create
            # overhead and slightly reduces directory entry sizes.
            # Source: valleyofdoom/PC-Tuning + standard Windows Server performance tuning.
            $fsPath = "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem"
            # 0x80000001 = user-managed + disabled. On Win10 1803+, value 1 alone means
            # "user-managed + ENABLED" (the opposite of intent). The high bit signals user-managed mode.
            # NOTE: Do NOT cast to [uint32] - in Windows PowerShell 5.1, 0x80000001 is parsed
            # as Int32 (-2147483647) and [uint32] rejects negative values. Passing the raw hex
            # literal works: Set-ItemProperty -Type DWord writes the correct bit pattern regardless
            # of signed/unsigned interpretation.
            Set-RequiredRegistryValue $fsPath "NtfsDisableLastAccessUpdate" 0x80000001 "DWord" `
                "NTFS: disable last-access timestamp writes on file reads"
            Set-RequiredRegistryValue $fsPath "NtfsDisable8dot3NameCreation" 1 "DWord" `
                "NTFS: disable 8.3 legacy filename alias generation"

            Write-ActionOK "SystemProfile gaming priority set."
            Write-ActionOK "System latency tweaks applied (FTH, Maintenance, NTFS, co-installers)."
            Complete-Step $PHASE 27 "SystemResponsiveness"
        } `
        -SkipAction { Skip-Step $PHASE 27 "SystemResponsiveness" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 28 - TIMER RESOLUTION  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 28) {
    Write-Section "Step 28 - Timer Resolution"
    $null = Invoke-TieredStep -Tier 2 -Title "Enable global timer resolution (Win10 2004+)" `
        -Why "Enables the Windows policy that allows application timer-resolution requests to be handled globally." `
        -Evidence "The registry state is verifiable. This repository includes no isolated CS2 scheduling benchmark." `
        -Caveat "Higher timer resolution can increase wake frequency and power use. Avoid on battery unless measured and required." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Enables global handling of application timer-resolution requests" `
        -SideEffects "Can increase CPU wake frequency and power use" `
        -Undo "Delete GlobalTimerResolutionRequests from kernel registry key" `
        -Action {
            $buildProps = Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion" `
                -Name "CurrentBuildNumber" -ErrorAction SilentlyContinue
            $buildRaw = if ($buildProps -and $buildProps.PSObject.Properties["CurrentBuildNumber"]) {
                $buildProps.CurrentBuildNumber
            }
            $build = 0
            if ($buildRaw) { try { $build = [int]$buildRaw } catch { $build = 0 } }
            if ($build -ge 19041) {
                Set-RegistryValue "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\kernel" `
                    "GlobalTimerResolutionRequests" 1 "DWord" "Timer resolution: allow highest request"
                Write-ActionOK "Timer resolution enabled (Build $build >= 19041)."
            } else {
                Write-Warn "Windows build $build < 19041 - feature not available."
                Write-Info "Requires Windows 10 version 2004 or newer."
            }
            Complete-Step $PHASE 28 "TimerResolution"
        } `
        -SkipAction { Skip-Step $PHASE 28 "TimerResolution" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 29 - MOUSE ACCELERATION DISABLE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 29) {
    Write-Section "Step 29 - Disable Mouse Acceleration"
    $null = Invoke-TieredStep -Tier 2 -Title "Disable mouse acceleration + reduce mouclass kernel queue depth" `
        -Why "Disables Windows pointer acceleration and changes MouseDataQueueSize from the Windows default to the repository value of 50." `
        -Evidence "The pointer and queue registry values are deterministic. This repository includes no isolated input-latency benchmark for the queue change." `
        -Caveat "Raw-input applications may bypass pointer acceleration. Queue behavior is driver-dependent, and unsuitable values can cause input loss." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Applies linear Windows pointer settings and the repository queue-size value" `
        -SideEffects "Desktop mouse movement feels 'slower' (linear instead of accelerated). mouclass change requires reboot." `
        -Undo "Control Panel -> Mouse -> Pointer Options -> Enable 'Enhance pointer precision'; delete MouseDataQueueSize from HKLM:\SYSTEM\...\mouclass\Parameters" `
        -Action {
            $mousePath = "HKCU:\Control Panel\Mouse"
            Set-RegistryValue $mousePath "MouseSpeed"      "0" "String" "Acceleration multiplier off"
            Set-RegistryValue $mousePath "MouseThreshold1"  "0" "String" "Acceleration threshold 1 off"
            Set-RegistryValue $mousePath "MouseThreshold2"  "0" "String" "Acceleration threshold 2 off"
            # SmoothMouse curves are 5-point INT64 arrays (40 bytes each, little-endian).
            # For a flat 1:1 curve (no acceleration), Y = X at each point.
            # Values: 0, 0xA000, 0x14000, 0x28000, 0x50000 (fixed-point speed thresholds)
            $flatX = [byte[]](
                0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,  # 0
                0x00,0xA0,0x00,0x00,0x00,0x00,0x00,0x00,  # 0xA000
                0x00,0x40,0x01,0x00,0x00,0x00,0x00,0x00,  # 0x14000
                0x00,0x80,0x02,0x00,0x00,0x00,0x00,0x00,  # 0x28000
                0x00,0x00,0x05,0x00,0x00,0x00,0x00,0x00   # 0x50000
            )
            $flatY = [byte[]](
                0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,  # 0
                0x00,0xA0,0x00,0x00,0x00,0x00,0x00,0x00,  # 0xA000
                0x00,0x40,0x01,0x00,0x00,0x00,0x00,0x00,  # 0x14000
                0x00,0x80,0x02,0x00,0x00,0x00,0x00,0x00,  # 0x28000
                0x00,0x00,0x05,0x00,0x00,0x00,0x00,0x00   # 0x50000
            )
            if (-not $SCRIPT:DryRun) {
                try {
                    Backup-RegistryValue -Path $mousePath -Name "SmoothMouseXCurve" -StepTitle $SCRIPT:CurrentStepTitle
                    Backup-RegistryValue -Path $mousePath -Name "SmoothMouseYCurve" -StepTitle $SCRIPT:CurrentStepTitle
                    Set-ItemProperty -Path $mousePath -Name "SmoothMouseXCurve" -Value $flatX -Type Binary
                    Set-ItemProperty -Path $mousePath -Name "SmoothMouseYCurve" -Value $flatY -Type Binary
                    Write-OK "Flat mouse curve set (1:1 movement)."
                } catch { Write-Warn "SmoothMouse curve could not be set: $_" }
            } else {
                Write-Host "  [DRY-RUN] Would set flat mouse curves (SmoothMouseX/YCurve)" -ForegroundColor Magenta
            }
            if (-not $SCRIPT:DryRun) {
                Write-ActionOK "Mouse acceleration disabled. Takes effect after re-login."
            }

            # ── mouclass kernel input queue depth ────────────────────────────────────
            # Default queue of 100 events allows Windows to buffer up to ~100ms of mouse
            # input at 1kHz polling before flushing to the user-mode message queue.
            # Reducing to 50 bounds worst-case kernel-side buffering to ~50ms.
            # The repository does not include an isolated benchmark for this value. Values
            # that are too low can cause input loss on some hardware, so 50 is retained as
            # the repository default.
            # Source: djdallmann/GamingPCSetup - mouclass.sys kernel input buffer analysis.
            $mouPath = "HKLM:\SYSTEM\CurrentControlSet\Services\mouclass\Parameters"
            Set-RegistryValue $mouPath "MouseDataQueueSize" 50 "DWord" `
                "mouclass kernel queue depth (50 events, down from default 100)"
            if (-not $SCRIPT:DryRun) {
                Write-OK "mouclass: kernel mouse event queue = 50 (default: 100). Reboot required."
                Write-Sub "Conservative reduction - values below 30 can cause skipping on some hardware."
            }

            Complete-Step $PHASE 29 "MouseAccel"
        } `
        -SkipAction { Skip-Step $PHASE 29 "MouseAccel" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 30 - CS2 GPU PREFERENCE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 30) {
    Write-Section "Step 30 - CS2 GPU Preference (Hybrid GPU)"
    $null = Invoke-TieredStep -Tier 2 -Title "Fix CS2 to high-performance GPU" `
        -Why "Sets the Windows per-application high-performance GPU preference for the resolved cs2.exe path." `
        -Evidence "The UserGpuPreferences value is verifiable. Windows and the driver retain final device-selection control." `
        -Caveat "The preference can be ignored on unsupported or single-GPU systems." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Applies the per-application high-performance GPU preference" `
        -SideEffects "Can change GPU selection and power use for cs2.exe" `
        -Undo "Delete cs2.exe entry from UserGpuPreferences registry key" `
        -Action {
            $cs2Path = Get-CS2InstallPath
            if ($cs2Path) {
                $cs2Exe = "$cs2Path\game\bin\win64\cs2.exe"
                $regPath = "HKCU:\SOFTWARE\Microsoft\DirectX\UserGpuPreferences"
                $gpuPreferenceWriteResult = Set-RegistryValue $regPath $cs2Exe `
                    "GpuPreference=2;" "String" "CS2 GPU preference: High Performance" -PassThru
                if (-not $gpuPreferenceWriteResult) {
                    throw "CS2 GPU preference registry write returned no result."
                }
                if ($SCRIPT:DryRun -and $gpuPreferenceWriteResult.Status -eq "DryRun") {
                    Write-Info "CS2 GPU preference registry write previewed for: $cs2Exe"
                } elseif (-not $SCRIPT:DryRun -and
                    $gpuPreferenceWriteResult.Status -eq "Success" -and
                    $gpuPreferenceWriteResult.Applied) {
                    Write-ActionOK "GPU Preference = High Performance for: $cs2Exe"
                } else {
                    throw "CS2 GPU preference registry write did not complete: $($gpuPreferenceWriteResult.Message)"
                }
            } else {
                Write-Warn "CS2 not found - manual: Windows Settings -> System -> Display -> Graphics settings"
                Write-Info "Add cs2.exe -> Options -> High performance"
                Skip-Step $PHASE 30 "GpuPreference"
                return
            }
            Complete-Step $PHASE 30 "GpuPreference"
        } `
        -SkipAction { Skip-Step $PHASE 30 "GpuPreference" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 31 - XBOX GAME BAR / GAME DVR DISABLE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 31) {
    Write-Section "Step 31 - Disable Xbox Game Bar + Game DVR"
    $null = Invoke-TieredStep -Tier 2 -Title "Disable Game Bar, Game DVR and App Capture" `
        -Why "Game Bar / DVR can perform background capture and use encoder or VRAM resources when enabled." `
        -Evidence "The registry values control capture and Game Bar behavior. This repository includes no isolated performance benchmark." `
        -Caveat "Gaming Debloat (Step 13) partially already disables this. Explicit registry safety here." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Disables Windows background game capture and the Game Bar entry points" `
        -SideEffects "No more Game Bar screenshots/recording (Win+G). Use Steam/external tools instead." `
        -Undo "Windows Settings -> Gaming -> Game Bar -> ON" `
        -Action {
            Set-RegistryValue "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\GameDVR" "AppCaptureEnabled" 0 "DWord" "App Capture off"
            Set-RegistryValue "HKCU:\SOFTWARE\Microsoft\GameBar" "UseNexusForGameBarEnabled" 0 "DWord" "Game Bar Nexus off"
            Set-RegistryValue "HKLM:\SOFTWARE\Policies\Microsoft\Windows\GameDVR" "AllowGameDVR" 0 "DWord" "Game DVR Policy off"
            Set-RegistryValue "HKCU:\System\GameConfigStore" "GameDVR_Enabled" 0 "DWord" "Game DVR master switch off"
            Write-ActionOK "Game Bar + Game DVR disabled (master switch + policy + app capture)."
            Complete-Step $PHASE 31 "GameDVR"
        } `
        -SkipAction { Skip-Step $PHASE 31 "GameDVR" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 32 - OVERLAY DISABLE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 32) {
    Write-Section "Step 32 - Disable Overlays"
    $null = Invoke-TieredStep -Tier 2 -Title "Steam Overlay + overlay tips" `
        -Why "Overlays add in-process or companion UI while a game is running." `
        -Evidence "The Steam registry value controls its overlay. This repository includes no isolated performance benchmark for overlays." `
        -Caveat "Steam Overlay needed for screenshots (F12) and Shift+Tab. Discord/GFE: disable manually." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Disables the Steam overlay and documents manual controls for other overlays" `
        -SideEffects "No Steam overlay (Shift+Tab, F12 screenshots). Discord/GFE overlays need manual disable." `
        -Undo "Steam -> Settings -> In-Game -> Enable Steam Overlay" `
        -Action {
            Set-RegistryValue "HKCU:\Software\Valve\Steam" "GameOverlayDisabled" 1 "DWord" "Steam Overlay globally off"
            Write-ActionOK "Steam Overlay globally disabled."
            Write-Blank
            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Yellow
            Write-Host "  │  DISABLE MANUALLY (no registry access possible):             │" -ForegroundColor Yellow
            Write-Host "  │                                                              │" -ForegroundColor Yellow
            Write-Host "  │  Discord:  Settings -> Overlay -> In-Game Overlay OFF       │" -ForegroundColor White
            Write-Host "  │  GeForce:  GFE -> Settings -> In-Game Overlay OFF           │" -ForegroundColor White
            Write-Host "  │  AMD:      Adrenalin -> Performance -> Metrics Overlay OFF  │" -ForegroundColor White
            Write-Host "  │                                                              │" -ForegroundColor Yellow
            Write-Host "  │  NOTE: Re-enable Steam Overlay for screenshots:             │" -ForegroundColor DarkGray
            Write-Host "  │  Steam -> Settings -> In-Game -> Enable Overlay             │" -ForegroundColor DarkGray
            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Yellow
            if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] when overlays disabled" }
            Complete-Step $PHASE 32 "Overlay"
        } `
        -SkipAction { Skip-Step $PHASE 32 "Overlay" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 33 - AUDIO OPTIMIZATION  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 33) {
    Write-Section "Step 33 - Audio Optimization"
    $null = Invoke-TieredStep -Tier 2 -Title "Optimize audio (24-bit/48kHz, ducking off, Spatial Sound off)" `
        -Why "Disables Windows communications ducking and provides manual format and spatial-sound guidance." `
        -Evidence "UserDuckingPreference=3 controls automatic volume reduction. The repository includes no cross-device audio-latency benchmark." `
        -Caveat "Some settings (format, spatial sound) must be set manually in Sound settings. Audio ducking registry key is applied automatically." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Prevents Windows communications ducking and documents the repository audio configuration" `
        -SideEffects "Windows will no longer auto-reduce game/music volume when VoIP is detected" `
        -Undo "Set UserDuckingPreference=0 in HKCU:\Software\Microsoft\Multimedia\Audio; or Sound -> Communications -> change to 'Reduce by 80%'" `
        -Action {
            # Disable audio ducking (Windows auto-reduces other app volumes when VoIP is active)
            Set-RegistryValue "HKCU:\Software\Microsoft\Multimedia\Audio" `
                "UserDuckingPreference" 3 "DWord" "Audio ducking: Do Nothing (0=Default, 3=Never reduce)"
            Write-ActionOK "Audio ducking disabled (will not auto-reduce game volume during VoIP)."

            try {
                $audioDevs = Get-CimInstance Win32_SoundDevice | Where-Object { $_.Status -eq "OK" }
                if ($audioDevs) {
                    Write-Info "Detected audio devices:"
                    foreach ($dev in $audioDevs) {
                        Write-Sub "$($dev.Name)"
                    }

                    $btAudio = $audioDevs | Where-Object { $_.Name -match "Bluetooth|BT" }
                    if ($btAudio) {
                        Write-Blank
                        Write-Host "  ⚠  BLUETOOTH AUDIO DETECTED!" -ForegroundColor Red
                        Write-Host "  Wireless audio latency varies by device, codec, and radio conditions." -ForegroundColor Yellow
                        Write-Host "  Compare with a wired path if latency is a concern." -ForegroundColor Yellow
                        Write-Blank
                    }
                }
            } catch { Write-DebugLog "Audio device detection failed." }

            Write-Blank
            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Cyan
            Write-Host "  │  AUDIO SETTINGS (manual):                                    │" -ForegroundColor Cyan
            Write-Host "  │                                                              │" -ForegroundColor Cyan
            Write-Host "  │  1.  Right-click speaker icon -> Sound settings              │" -ForegroundColor White
            Write-Host "  │  2.  Output device -> Properties                             │" -ForegroundColor White
            Write-Host "  │  3.  Format: 24-bit, 48000 Hz (Studio Quality)               │" -ForegroundColor White
            Write-Host "  │  4.  Spatial Sound: OFF                                      │" -ForegroundColor White
            Write-Host "  │  5.  Audio enhancements: OFF                                 │" -ForegroundColor White
            Write-Host "  │  6.  Exclusive mode: check BOTH boxes                        │" -ForegroundColor White
            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Cyan
            if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] when done" }
            Complete-Step $PHASE 33 "Audio"
        } `
        -SkipAction { Skip-Step $PHASE 33 "Audio" }
}
