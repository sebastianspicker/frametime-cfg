# ==============================================================================
#  Optimize-RegistryTweaks.ps1  —  Steps 23-33: Fast Startup, RAM, Nagle,
#                                   FSE, Scheduler, Timer, Mouse, GPU Pref,
#                                   Game DVR, Overlay, Audio
# ==============================================================================

# ══════════════════════════════════════════════════════════════════════════════
# STEP 23 — DISABLE FAST STARTUP  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 23) {
    Write-Section "Step 23 — Disable Fast Startup (Hybrid Boot)"
    Invoke-TieredStep -Tier 2 -Title "Disable Fast Startup (HiberbootEnabled=0)" `
        -Why "Fast Startup writes a hibernation snapshot on shutdown and resumes from it on boot, preserving driver state from the previous session. MSI interrupt registry changes and NIC affinity settings only fully take effect after a cold boot — Fast Startup bypasses this." `
        -Evidence "valleyofdoom/PC-Tuning: confirmed that MSI interrupt settings can fail to persist when Fast Startup is active. Multiple forum reports of MSI registry writes being present but not applied until cold reboot. HiberbootEnabled=0 disables only the hybrid shutdown — full hibernate mode is unaffected." `
        -Caveat "Boot time increases by 5-15 seconds (no hibernation resume shortcut). Hibernate mode (Sleep) is not affected by this change — only the 'Shut Down' behavior changes." `
        -Risk "SAFE" -Depth "REGISTRY" -EstimateKey "HiberbootEnabled=0" `
        -Improvement "Ensures MSI interrupts, NIC affinity, and all driver-level registry changes take effect on the next boot" `
        -SideEffects "Slightly longer shutdown and startup time (cold boot instead of hybrid boot)" `
        -Undo "Set HiberbootEnabled = 1 in HKLM:\SYSTEM\...\Power (or re-enable in Power Options -> Choose what the power buttons do)" `
        -Action {
            Set-RegistryValue "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power" `
                "HiberbootEnabled" 0 "DWord" "Disable Fast Startup (hybrid boot)"
            if (-not $SCRIPT:DryRun) {
                Write-OK "Fast Startup disabled. Changes take effect on next shutdown + cold boot."
                Write-Info "Note: Hibernate / Sleep mode is NOT affected — only 'Shut Down' behavior."
            }
            Complete-Step $PHASE 23 "HiberbootEnabled"
        } `
        -SkipAction { Skip-Step $PHASE 23 "HiberbootEnabled" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 24 — DUAL-CHANNEL RAM DETECTION  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 24) {
    Write-Section "Step 24 — Dual-Channel RAM Detection"
    Invoke-TieredStep -Tier 1 -Title "Check dual-channel RAM" `
        -Why "Single-channel halves memory bandwidth — devastating for CS2 CPU-bound scenarios." `
        -Evidence "T1: Memory bandwidth directly measurable. Dual-channel = ~2x throughput. Gamer Nexus / Hardware Unboxed confirmed." `
        -Risk "SAFE" -Depth "CHECK" `
        -Improvement "Identifies single-channel bottleneck (20-40% FPS loss if found)" `
        -SideEffects "None — read-only detection" `
        -Undo "N/A (check only)" `
        -SkipAction { Skip-Step $PHASE 24 "DualChannel" } `
        -Action {
            $dc = Test-DualChannel
            if ($null -eq $dc.DualChannel) {
                Write-Warn $dc.Reason
            } elseif ($dc.DualChannel) {
                Write-OK "$($dc.Reason)"
            } else {
                Write-Err "SINGLE-CHANNEL RAM DETECTED!"
                Write-Blank
                Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Red
                Write-Host "  │  $($dc.Reason)$((' ' * [math]::Max(0, 60 - $dc.Reason.Length)))│" -ForegroundColor Red
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  Single-channel halves memory bandwidth.                    │" -ForegroundColor Yellow
                Write-Host "  │  In CS2 (CPU-bound) this can mean 20-40% less FPS.         │" -ForegroundColor Yellow
                Write-Host "  │                                                              │" -ForegroundColor Red
                Write-Host "  │  SOLUTION:                                                  │" -ForegroundColor White
                Write-Host "  │  -> Buy a second identical RAM stick                        │" -ForegroundColor White
                Write-Host "  │  -> Insert in slot 2 or 4 (check motherboard manual)       │" -ForegroundColor White
                Write-Host "  │  -> Typical: Slot A2 + B2 for dual-channel                 │" -ForegroundColor White
                Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Red
                Write-Blank
                if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] to continue" }
            }
            Complete-Step $PHASE 24 "DualChannel"
        }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 25 — NAGLE'S ALGORITHM DISABLE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 25) {
    Write-Section "Step 25 — Disable Nagle's Algorithm"
    Invoke-TieredStep -Tier 2 -Title "Disable TCP Nagle delay (TcpNoDelay + TcpAckFrequency)" `
        -Why "Nagle's Algorithm bundles small TCP packets -> increases latency for TCP connections (Steam API, matchmaking — CS2 game ticks use UDP, unaffected by Nagle)." `
        -Evidence "Known since Quake/CS1.6. Reduces TCP packet bundling latency. Measurable in network captures." `
        -Caveat "May minimally increase bandwidth. No effect if CS2 uses pure UDP (most game traffic)." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Reduced TCP packet delay — measurable in network captures" `
        -SideEffects "Minimal bandwidth increase from smaller packets" `
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
                } catch {}
                if ($wifiOnly) {
                    Write-Warn "Wi-Fi connection detected — Nagle disable targets wired Ethernet adapters only."
                    Write-Info "For Wi-Fi: Settings -> Network -> Wi-Fi -> Hardware properties -> note the adapter GUID."
                } else {
                    Write-Warn "Active network adapter not found — set manually in regedit."
                }
                Write-Info "HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{NIC-GUID}"
                Write-Info "TcpNoDelay = 1 (DWord) | TcpAckFrequency = 1 (DWord)"
            }
            Complete-Step $PHASE 25 "Nagle"
        } `
        -SkipAction { Skip-Step $PHASE 25 "Nagle" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 26 — GAMECONFIGSTORE FSE REGISTRY  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 26) {
    Write-Section "Step 26 — GameConfigStore FSE Registry"
    Invoke-TieredStep -Tier 2 -Title "Set GameConfigStore fullscreen-exclusive keys" `
        -Why "Supplements Step 4 (FSO). Forces true fullscreen-exclusive via GameConfigStore." `
        -Evidence "Known Windows registry keys controlling FSE behavior. Supplement to AppCompatFlags." `
        -Caveat "Only relevant in fullscreen mode. No effect in windowed/borderless." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Ensures true fullscreen-exclusive — supplements Step 4" `
        -SideEffects "None — only affects fullscreen rendering behavior" `
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
# STEP 27 — SYSTEMRESPONSIVENESS + GAMING PRIORITY  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 27) {
    Write-Section "Step 27 — System Scheduling, Gaming Priority + Latency Tweaks"
    Invoke-TieredStep -Tier 2 -Title "Multimedia SystemProfile + scheduler + system latency tweaks (FTH, NTFS, Maintenance)" `
        -Why "Reserves less CPU for MMCSS multimedia scheduling. Win32PrioritySeparation=0x2A gives the foreground app (CS2) a 3x scheduler quantum boost with fixed quantum on thread wakeup — djdallmann confirmed via WinDbg kernel analysis. NoLazyMode=1 shifts MMCSS from idle-detection to realtime-only operation, eliminating periodic idle-switching overhead. DisablePagingExecutive keeps kernel/driver code in physical RAM. Intel 12th gen+: PowerThrottlingOff prevents E-core mismatch frametime spikes. NetworkThrottlingIndex is deliberately NOT set — see Caveat. Fault Tolerant Heap (FTH): Windows silently patches CS2 heap allocations after any crash event, slowing subsequent allocations 10-15% until process restart — disabled preemptively. Automatic Maintenance fires RunFullMemoryDiagnostic at scheduled intervals with 12-14% measured CPU consumption mid-game (djdallmann xperf). NTFS: DisableLastAccessUpdate stops per-read metadata writes; Disable8dot3NameCreation eliminates legacy 8.3 alias generation overhead. DisableCoInstallers: prevents third-party co-installer DLLs from executing during PnP device enumeration events." `
        -Evidence "Microsoft-documented: SystemResponsiveness 0-100. Win32PrioritySeparation 0x2A = short, fixed quantum, max foreground boost (2025 Blur Busters: fixed gives better 1% lows than variable 0x26; djdallmann WinDbg confirmed PspForegroundQuantum and PsPrioritySeparation). NoLazyMode: djdallmann GamingPCSetup MMCSS research. DisablePagingExecutive: standard Windows Server tuning. PowerThrottlingOff: confirmed by Hardware Unboxed for Intel 12th gen+. FTH: djdallmann xperf analysis — heap patching confirmed measurable overhead. Maintenance: djdallmann — RunFullMemoryDiagnostic measured 12-14% CPU mid-game. NTFS: valleyofdoom/PC-Tuning — DisableLastAccessUpdate reduces filesystem write I/O; Disable8dot3NameCreation standard Windows Server tuning. DisableCoInstallers: valleyofdoom/PC-Tuning." `
        -Caveat "NetworkThrottlingIndex 0xFFFFFFFF is a common guide recommendation that djdallmann xperf analysis found INCREASES DPC latency on Intel NICs — deliberately left at default (10). NoLazyMode=1 may slightly increase background CPU cycles. PowerThrottlingOff: Intel 12th gen+ only (auto-detected). FTH: heap allocation errors that FTH would have silently handled may surface as crashes in rare cases. Maintenance: automatic system maintenance tasks won't run automatically — trigger manually from Task Scheduler if needed." `
        -Risk "SAFE" -Depth "REGISTRY" -EstimateKey "Win32PrioritySeparation" `
        -Improvement "Foreground 3x scheduler quantum; MMCSS realtime scheduling; kernel in RAM; Intel E-core fix; FTH heap slowdown prevented; no 12-14% maintenance CPU spikes mid-game; NTFS metadata write elimination" `
        -SideEffects "Background media apps get slightly less priority. NoLazyMode: marginally higher CPU cycles. FTH disabled: rare heap errors may not be silently suppressed. Maintenance won't run automatically. NTFS: 8.3 filename aliases removed (breaks legacy 16-bit app compatibility)." `
        -Undo "Set SystemResponsiveness=20, Win32PrioritySeparation=2, delete Games/NoLazyMode keys; FTH\Enabled=1; MaintenanceDisabled=0; Delete NtfsDisableLastAccessUpdate or set to 2 (system-managed enabled); NtfsDisable8dot3NameCreation=0; DisableCoInstallers=0" `
        -Action {
            $mmPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile"
            Set-RegistryValue $mmPath "SystemResponsiveness" 10 "DWord" "Less CPU reserved for MMCSS"
            # NoLazyMode: shifts MMCSS from periodic idle-detection to realtime-only operation.
            # djdallmann GamingPCSetup: "shifts from idle-detection modes to realtime-only operation"
            Set-RegistryValue $mmPath "NoLazyMode" 1 "DWord" "MMCSS realtime-only (no idle detection)"
            # NOTE: NetworkThrottlingIndex deliberately NOT set — djdallmann xperf shows 0xFFFFFFFF increases DPC latency
            $gamesPath = "$mmPath\Tasks\Games"
            Set-RegistryValue $gamesPath "Priority"              6      "DWord"  "Gaming priority 6"
            Set-RegistryValue $gamesPath "Scheduling Category"   "High" "String" "High scheduling"
            Set-RegistryValue $gamesPath "GPU Priority"          8      "DWord"  "GPU priority 8"

            # Foreground scheduler quantum: short interval, FIXED, max priority separation (PsPrioritySeparation=2)
            # 0x2A = binary 00 10 10 10 = Interval:Short(2), Length:Fixed(2), PrioritySeparation:2(Max boost)
            # Previous: 0x26 (Variable quantum). 2025 Blur Busters + Overclock.net benchmarks showed
            # Fixed quantum (0x2A) gives measurably lower input latency and better 1% lows than Variable.
            # Fixed = foreground always gets the full boosted quantum length, no scheduler-decided variation.
            Set-RegistryValue "HKLM:\SYSTEM\CurrentControlSet\Control\PriorityControl" `
                "Win32PrioritySeparation" 0x2A "DWord" "Short quantum, fixed, max foreground boost (0x2A)"

            # Keep kernel + driver code in physical RAM — reduces latency on page fault paths
            $memMgmt = "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management"
            Set-RegistryValue $memMgmt "DisablePagingExecutive" 1 "DWord" "Keep kernel code in RAM"

            # Intel 12th gen+ hybrid: disable OS Power Throttling to prevent E-core thread migration
            $intelHybridName = Get-IntelHybridCpuName
            if ($intelHybridName) {
                $ptPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Power\PowerThrottling"
                # Set-RegistryValue creates the key path if missing — no need for standalone New-Item
                Set-RegistryValue $ptPath "PowerThrottlingOff" 1 "DWord" "Disable Intel Power Throttling (E-core mismatch)"
                Write-ActionOK "Intel hybrid CPU ($intelHybridName) — Power Throttling disabled."
            } else {
                Write-Sub "Power Throttling: not applicable (non-Intel-hybrid CPU)"
            }

            # ── Fault Tolerant Heap disable ───────────────────────────────────────
            # Windows FTH silently patches CS2's heap allocator after any crash/hang event.
            # This "fix" slows ALL memory allocations 10-15% until the process is restarted.
            # Disabling FTH globally prevents this silent performance regression.
            # Source: djdallmann/GamingPCSetup xperf analysis.
            Set-RegistryValue "HKLM:\SOFTWARE\Microsoft\FTH" "Enabled" 0 "DWord" `
                "Disable Fault Tolerant Heap (prevents post-crash heap slowdown)"

            # ── DisableCoInstallers ──────────────────────────────────────────────────
            # Prevents third-party co-installer DLLs from running during PnP device
            # enumeration events (e.g., driver re-installation, device plug-in).
            # These DLLs can spike CPU briefly and are unnecessary post-setup.
            # Source: valleyofdoom/PC-Tuning.
            Set-RegistryValue "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Device Installer" `
                "DisableCoInstallers" 1 "DWord" "Disable PnP co-installer DLLs"

            # ── Automatic Maintenance disable ────────────────────────────────────────
            # Windows schedules RunFullMemoryDiagnostic + disk defrag + scan during gaming.
            # djdallmann xperf captured 12-14% CPU consumption when maintenance fires mid-game.
            $maintPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\Maintenance"
            Set-RegistryValue $maintPath "MaintenanceDisabled" 1 "DWord" `
                "Disable Windows Automatic Maintenance scheduler"

            # ── NTFS metadata write elimination ──────────────────────────────────────
            # NtfsDisableLastAccessUpdate=1: stops NTFS from updating the "last access"
            # timestamp on every file read — eliminating a metadata write from each file I/O.
            # NtfsDisable8dot3NameCreation=1: stops NTFS from maintaining legacy 8.3 aliases
            # (e.g., "PROGRA~1") alongside every full-length filename. Removes per-create
            # overhead and slightly reduces directory entry sizes.
            # Source: valleyofdoom/PC-Tuning + standard Windows Server performance tuning.
            $fsPath = "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem"
            # 0x80000001 = user-managed + disabled. On Win10 1803+, value 1 alone means
            # "user-managed + ENABLED" (the opposite of intent). The high bit signals user-managed mode.
            # NOTE: Do NOT cast to [uint32] — in Windows PowerShell 5.1, 0x80000001 is parsed
            # as Int32 (-2147483647) and [uint32] rejects negative values. Passing the raw hex
            # literal works: Set-ItemProperty -Type DWord writes the correct bit pattern regardless
            # of signed/unsigned interpretation.
            Set-RegistryValue $fsPath "NtfsDisableLastAccessUpdate" 0x80000001 "DWord" `
                "NTFS: disable last-access timestamp writes on file reads"
            Set-RegistryValue $fsPath "NtfsDisable8dot3NameCreation" 1 "DWord" `
                "NTFS: disable 8.3 legacy filename alias generation"

            Write-ActionOK "SystemProfile gaming priority set."
            Write-ActionOK "System latency tweaks applied (FTH, Maintenance, NTFS, co-installers)."
            Complete-Step $PHASE 27 "SystemResponsiveness"
        } `
        -SkipAction { Skip-Step $PHASE 27 "SystemResponsiveness" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 28 — TIMER RESOLUTION  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 28) {
    Write-Section "Step 28 — Timer Resolution"
    Invoke-TieredStep -Tier 2 -Title "Enable global timer resolution (Win10 2004+)" `
        -Why "Reduces system timer from ~15.6ms to the highest requested resolution. CS2 benefits from more precise scheduling." `
        -Evidence "Microsoft-documented since Win10 2004. GlobalTimerResolutionRequests = 1 allows apps to lower timer resolution." `
        -Caveat "Minimally increases power consumption (CPU wakes more often). Desktop PCs only, not laptops on battery." `
        -Risk "SAFE" -Depth "REGISTRY" -EstimateKey "Timer Resolution" `
        -Improvement "More precise system timer — better thread scheduling for CS2" `
        -SideEffects "Minimal CPU power increase (CPU wakes more frequently)" `
        -Undo "Delete GlobalTimerResolutionRequests from kernel registry key" `
        -Action {
            $buildRaw = (Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion" -Name "CurrentBuildNumber" -ErrorAction SilentlyContinue).CurrentBuildNumber
            $build = 0
            if ($buildRaw) { try { $build = [int]$buildRaw } catch { $build = 0 } }
            if ($build -ge 19041) {
                Set-RegistryValue "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\kernel" `
                    "GlobalTimerResolutionRequests" 1 "DWord" "Timer resolution: allow highest request"
                Write-ActionOK "Timer resolution enabled (Build $build >= 19041)."
            } else {
                Write-Warn "Windows build $build < 19041 — feature not available."
                Write-Info "Requires Windows 10 version 2004 or newer."
            }
            Complete-Step $PHASE 28 "TimerResolution"
        } `
        -SkipAction { Skip-Step $PHASE 28 "TimerResolution" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 29 — MOUSE ACCELERATION DISABLE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 29) {
    Write-Section "Step 29 — Disable Mouse Acceleration"
    Invoke-TieredStep -Tier 2 -Title "Disable mouse acceleration + reduce mouclass kernel queue depth" -EstimateKey "Mouse Acceleration Off" `
        -Why "Mouse acceleration scales pointer speed with movement speed -> inconsistent aim. mouclass kernel queue: Windows allocates a nonpaged pool buffer for 100 mouse events by default. At 1kHz polling this means the kernel can buffer up to ~100ms of input events before being forced to flush. Reducing to 50 bounds the worst-case kernel-side input buffering to ~50ms. Source: djdallmann/GamingPCSetup mouclass.sys kernel analysis. NOTE: 2025 testing showed zero measurable impact — buffer drains faster than it fills at 1kHz." `
        -Evidence "100% of CS pros play without acceleration (prosettings.net). Consistent aim requires 1:1 input. mouclass queue: djdallmann — mouclass.sys analysis of kernel input event buffer depth vs. polling rate and frame latency. Queue of 50 is a safe conservative reduction (values below 30 can cause skipping on some hardware)." `
        -Caveat "Desktop navigation feels 'slower'. In CS2: acceleration only relevant if raw_input 0. mouclass queue: values below 30 can cause mouse skipping on some hardware — 50 is conservative." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "1:1 mouse input for consistent aim; mouclass queue bounds kernel-side input buffering latency" `
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
            # NOTE: 2025 Overclock.net + Blur Busters testing found zero measurable impact
            # from this tweak (buffer drains faster than it fills at 1kHz). Values below 30
            # can cause mouse skipping on some hardware. 50 is a safe conservative reduction.
            # Source: djdallmann/GamingPCSetup — mouclass.sys kernel input buffer analysis.
            $mouPath = "HKLM:\SYSTEM\CurrentControlSet\Services\mouclass\Parameters"
            Set-RegistryValue $mouPath "MouseDataQueueSize" 50 "DWord" `
                "mouclass kernel queue depth (50 events, down from default 100)"
            if (-not $SCRIPT:DryRun) {
                Write-OK "mouclass: kernel mouse event queue = 50 (default: 100). Reboot required."
                Write-Sub "Conservative reduction — values below 30 can cause skipping on some hardware."
            }

            Complete-Step $PHASE 29 "MouseAccel"
        } `
        -SkipAction { Skip-Step $PHASE 29 "MouseAccel" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 30 — CS2 GPU PREFERENCE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 30) {
    Write-Section "Step 30 — CS2 GPU Preference (Hybrid GPU)"
    Invoke-TieredStep -Tier 2 -Title "Fix CS2 to high-performance GPU" `
        -Why "Laptops/hybrid GPU systems sometimes use integrated GPU instead of dedicated." `
        -Evidence "Windows Graphics Settings API. GpuPreference=2 forces high-performance GPU." `
        -Caveat "Only relevant for laptops with iGPU + dGPU. Desktop with single GPU: no effect." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Ensures CS2 uses dedicated GPU — critical for laptop users" `
        -SideEffects "None — only sets GPU preference for cs2.exe" `
        -Undo "Delete cs2.exe entry from UserGpuPreferences registry key" `
        -Action {
            $cs2Path = Get-CS2InstallPath
            if ($cs2Path) {
                $cs2Exe = "$cs2Path\game\bin\win64\cs2.exe"
                $regPath = "HKCU:\SOFTWARE\Microsoft\DirectX\UserGpuPreferences"
                Set-RegistryValue $regPath $cs2Exe "GpuPreference=2;" "String" "CS2 GPU preference: High Performance"
                Write-ActionOK "GPU Preference = High Performance for: $cs2Exe"
            } else {
                Write-Warn "CS2 not found — manual: Windows Settings -> System -> Display -> Graphics settings"
                Write-Info "Add cs2.exe -> Options -> High performance"
            }
            Complete-Step $PHASE 30 "GpuPreference"
        } `
        -SkipAction { Skip-Step $PHASE 30 "GpuPreference" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 31 — XBOX GAME BAR / GAME DVR DISABLE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 31) {
    Write-Section "Step 31 — Disable Xbox Game Bar + Game DVR"
    Invoke-TieredStep -Tier 2 -Title "Disable Game Bar, Game DVR and App Capture" -EstimateKey "Game DVR / Game Bar Off" `
        -Why "Game Bar / DVR records in the background consuming GPU resources (encoder + VRAM)." `
        -Evidence "Multiple benchmark tests show 2-5% less avg FPS with active Game DVR." `
        -Caveat "Gaming Debloat (Step 13) partially already disables this. Explicit registry safety here." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "+2-5% avg FPS by removing background recording overhead" `
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
# STEP 32 — OVERLAY DISABLE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 32) {
    Write-Section "Step 32 — Disable Overlays"
    Invoke-TieredStep -Tier 2 -Title "Steam Overlay + overlay tips" -EstimateKey "Disable Overlays" `
        -Why "Overlays (Steam, Discord, GeForce) inject code into the render loop -> frame pacing disruptions." `
        -Evidence "Known effect since Source 1. Steam Overlay is measurably 1-3% overhead." `
        -Caveat "Steam Overlay needed for screenshots (F12) and Shift+Tab. Discord/GFE: disable manually." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "+1-3% by removing render loop injection from overlays" `
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
# STEP 33 — AUDIO OPTIMIZATION  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 33) {
    Write-Section "Step 33 — Audio Optimization"
    Invoke-TieredStep -Tier 2 -Title "Optimize audio (24-bit/48kHz, ducking off, Spatial Sound off)" `
        -Why "Wrong audio settings create DPC latency spikes. Bluetooth headsets: 40-200ms latency. Audio ducking reduces other app volumes when 'communication' is detected — prevents unexpected volume changes during gameplay." `
        -Evidence "DPC latency from audio stack measurable via LatencyMon. 24-bit/48kHz = CS2 native format. UserDuckingPreference=3 disables automatic volume reduction." `
        -Caveat "Some settings (format, spatial sound) must be set manually in Sound settings. Audio ducking registry key is applied automatically." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Reduced audio DPC latency + no unexpected volume changes during gameplay" `
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
                        Write-Host "  Bluetooth headsets have 40-200ms audio latency." -ForegroundColor Yellow
                        Write-Host "  For CS2 ALWAYS use a wired headset!" -ForegroundColor Yellow
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
