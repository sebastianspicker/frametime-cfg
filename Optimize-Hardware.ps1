# ==============================================================================
#  Optimize-Hardware.ps1  —  Steps 10-22: Timer, MPO, Game Mode, Debloat,
#                             Autostart, WU Blocker, NIC, Baseline Benchmark,
#                             Driver Prep, NVIDIA Driver, Profile, MSI, Affinity
# ==============================================================================

# ══════════════════════════════════════════════════════════════════════════════
# STEP 10 — DYNAMIC TICK  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 10) {
    Write-Section "Step 10 — Timer Optimization (bcdedit)"
    Invoke-TieredStep -Tier 3 -Title "Disable Dynamic Tick" `
        -Why "Adaptive Windows timer saves power via irregular CPU wakeups -> theoretical frametime irregularities." `
        -Evidence "Community consensus around disabledynamictick only. Microsoft documents useplatformtick as a debugging-only BCDEdit option, so it is not applied here." `
        -Caveat "Minimal impact on laptop battery life." `
        -Risk "MODERATE" -Depth "BOOT" `
        -Improvement "Theoretically more consistent frametimes from disabling dynamic tick — no isolated CS2 proof" `
        -SideEffects "Slightly higher power consumption on laptops" `
        -Undo "bcdedit /set disabledynamictick no" `
        -Action {
            Set-BootConfig "disabledynamictick" "yes" "Constant timer resolution"
            Write-Info "Undo: bcdedit /set disabledynamictick no"
            Complete-Step $PHASE 10 "Timer"
        } `
        -SkipAction { Skip-Step $PHASE 10 "Timer" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 11 — MPO  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 11) {
    Write-Section "Step 11 — Disable MPO"
    Invoke-TieredStep -Tier 3 -Title "Disable Multiplane Overlay (MPO)" `
        -Why "MPO is for multi-monitor compositing. In fullscreen games it causes known microstutter." `
        -Evidence "valleyofdoom/PC-Tuning + community. No hard CS2 1%-low benchmark. Microstutter in edge cases." `
        -Caveat "Disabling has no downside for CS2 fullscreen." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Eliminates MPO-related microstutter in edge cases" `
        -SideEffects "None for fullscreen gaming. Multi-monitor compositing may change." `
        -Undo "reg delete HKLM\SOFTWARE\Microsoft\Windows\Dwm /v OverlayTestMode /f" `
        -Action {
            Set-RegistryValue "HKLM:\SOFTWARE\Microsoft\Windows\Dwm" "OverlayTestMode" 5 "DWord" "Disable MPO"
            Complete-Step $PHASE 11 "MPO"
        } `
        -SkipAction { Skip-Step $PHASE 11 "MPO" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 12 — GAME MODE  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 12) {
    Write-Section "Step 12 — Windows Game Mode"
    Invoke-TieredStep -Tier 3 -Title "Enable Windows Game Mode" `
        -Why "Game Mode suppresses Windows Update installations during gaming sessions and gives CPU priority to the foreground game via MMCSS. Distinct from Game DVR (Step 31) which is recording overhead." `
        -Evidence "2025/2026 consensus: keep enabled. Microsoft confirms WU deferral during gaming. Valve recommended ON at various points. 'Thread priority interference' claim from 2020-2022 guides not reproduced in recent CS2 benchmarks." `
        -Caveat "Game Mode and Game DVR/Bar are separate systems. Step 31 disables DVR recording. This step enables the scheduler's game-priority path." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Prevents Windows Update from interrupting gaming; MMCSS thread priority benefit" `
        -SideEffects "None measurable" `
        -Undo "Set AllowAutoGameMode + AutoGameModeEnabled to 0" `
        -Action {
            Set-RegistryValue "HKCU:\SOFTWARE\Microsoft\GameBar" "AllowAutoGameMode"   1 "DWord" "Auto Game Mode on"
            Set-RegistryValue "HKCU:\SOFTWARE\Microsoft\GameBar" "AutoGameModeEnabled" 1 "DWord" "Game Mode on"
            Complete-Step $PHASE 12 "GameMode"
        } `
        -SkipAction { Skip-Step $PHASE 12 "GameMode" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 13 — GAMING DEBLOAT  [Hygiene, no 1%-low effect]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 13) {
    Write-Section "Step 13 — Gaming Debloat  [System hygiene — no direct 1%-low effect]"
    Write-Info "Removes bloatware and telemetry. No measurable effect on CS2 1% lows."
    Write-Info "Useful for general system cleanliness and fewer background processes."
    Invoke-TieredStep -Tier 2 -Title "Run Gaming Debloat (native PowerShell)" `
        -Why "Telemetry processes and bloatware consume CPU time in the background." `
        -Evidence "No CS2-specific 1%-low proof. General system hygiene." `
        -Caveat "Removes known bloatware AppX packages + disables telemetry. NOT: Windows Defender!" `
        -Risk "MODERATE" -Depth "APP" -EstimateKey "Debloat" `
        -Improvement "Fewer background processes — cleaner system, marginal CPU/RAM savings" `
        -SideEffects "Removed bloatware apps cannot be easily reinstalled from Microsoft Store" `
        -Undo "Reinstall apps via Microsoft Store or DISM /Online /Add-ProvisionedAppxPackage" `
        -Action {
            Invoke-GamingDebloat
            Complete-Step $PHASE 13 "Debloat"
        } `
        -SkipAction { Skip-Step $PHASE 13 "Debloat" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 14 — AUTOSTART  [Hygiene]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 14) {
    Write-Section "Step 14 — Clean Autostart Entries  [System hygiene]"
    Invoke-TieredStep -Tier 2 -Title "Disable autostart entries" `
        -Why "Background processes consume CPU time and RAM. No direct 1%-low effect if system has enough resources." `
        -Evidence "Only relevant when CPU/RAM usage is already high. Configurable in config.env.ps1." `
        -Caveat "Only registry entries are removed — apps stay installed." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Less CPU/RAM usage at boot — relevant if system resources are tight" `
        -SideEffects "Apps (Discord, Spotify, etc.) won't auto-start. Launch manually when needed." `
        -Undo "Re-enable via Task Manager -> Startup tab" `
        -Action {
            $removed = 0
            foreach ($app in $CFG_Autostart_Remove) {
                foreach ($rp in @("HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run","HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run")) {
                    if (Get-ItemProperty $rp -Name $app -ErrorAction SilentlyContinue) {
                        if (-not $SCRIPT:DryRun) {
                            Backup-RegistryValue -Path $rp -Name $app -StepTitle $SCRIPT:CurrentStepTitle
                            try { Flush-BackupBuffer } catch { Write-DebugLog "Flush after autostart backup failed: $_" }
                            Remove-ItemProperty $rp -Name $app -ErrorAction SilentlyContinue
                        } else {
                            Write-Host "  [DRY-RUN] Would remove autostart: $app from $rp" -ForegroundColor Magenta
                        }
                        $removed++
                    }
                }
            }
            if ($removed -eq 0) {
                Write-OK "No entries found."
            } elseif ($SCRIPT:DryRun) {
                Write-Host "  [DRY-RUN] Would disable $removed autostart entries." -ForegroundColor Magenta
            } else {
                Write-OK "$removed entries disabled."
            }
            Complete-Step $PHASE 14 "Autostart"
        } `
        -SkipAction { Skip-Step $PHASE 14 "Autostart" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 15 — WINDOWS UPDATE BLOCKER  [Security risk, always prompted]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 15) {
    Write-Section "Step 15 — Windows Update Blocker  [Security risk]"
    Invoke-TieredStep -Tier 3 -Title "Disable Windows Update services" `
        -Why "Prevents forced driver/update restarts during gaming. No FPS improvement." `
        -Evidence "Pure preference. No 1%-low effect. SECURITY RISK." `
        -Caveat "NO MORE security updates. System becomes vulnerable to exploits." `
        -Risk "CRITICAL" -Depth "SERVICE" `
        -Improvement "No FPS improvement — prevents forced restarts during gaming sessions" `
        -SideEffects "NO MORE security updates. System becomes vulnerable. For most users: SKIP." `
        -Undo "Set-Service wuauserv -StartupType Manual; Start-Service wuauserv" `
        -Action {
            if (-not $SCRIPT:DryRun) {
                # Backup service states
                Backup-ServiceState -ServiceName "wuauserv" -StepTitle "Windows Update Blocker"
                Backup-ServiceState -ServiceName "UsoSvc" -StepTitle "Windows Update Blocker"
                Backup-ServiceState -ServiceName "WaaSMedicSvc" -StepTitle "Windows Update Blocker"

                # Disable Windows Update service
                Stop-Service wuauserv -Force -ErrorAction SilentlyContinue
                Set-Service wuauserv -StartupType Disabled -ErrorAction SilentlyContinue
                $wuCheck = (Get-Service wuauserv -ErrorAction SilentlyContinue).StartType
                if ($wuCheck -eq 'Disabled') {
                    Write-OK "Windows Update service (wuauserv) disabled."
                } else {
                    Write-Warn "wuauserv could not be disabled (StartType: $wuCheck). May require TrustedInstaller or registry override."
                }

                # Disable Update Orchestrator
                Stop-Service UsoSvc -Force -ErrorAction SilentlyContinue
                Set-Service UsoSvc -StartupType Disabled -ErrorAction SilentlyContinue
                $usoCheck = (Get-Service UsoSvc -ErrorAction SilentlyContinue).StartType
                if ($usoCheck -eq 'Disabled') {
                    Write-OK "Update Orchestrator (UsoSvc) disabled."
                } else {
                    Write-Warn "UsoSvc could not be disabled (StartType: $usoCheck). May require TrustedInstaller or registry override."
                }

                # Disable Windows Update Medic Service
                # Note: WaaSMedicSvc is TrustedInstaller-protected on most Windows versions.
                # Standard PowerShell commands may silently fail. We attempt it but verify.
                # Enumerate and log dependents before stopping
                try {
                    $waaDeps = (Get-Service WaaSMedicSvc -ErrorAction SilentlyContinue).DependentServices
                    if ($waaDeps -and $waaDeps.Count -gt 0) {
                        $depNames = ($waaDeps | ForEach-Object { $_.Name }) -join ", "
                        Write-Info "WaaSMedicSvc has $($waaDeps.Count) dependent(s): $depNames — will be stopped together."
                    }
                } catch { Write-DebugLog "WaaSMedicSvc dependent enumeration failed: $_" }
                Stop-Service WaaSMedicSvc -Force -ErrorAction SilentlyContinue
                Set-Service WaaSMedicSvc -StartupType Disabled -ErrorAction SilentlyContinue
                $waasCheck = (Get-Service WaaSMedicSvc -ErrorAction SilentlyContinue).StartType
                if ($waasCheck -eq 'Disabled') {
                    Write-OK "Windows Update Medic (WaaSMedicSvc) disabled."
                } else {
                    Write-Warn "WaaSMedicSvc is TrustedInstaller-protected — could not disable (StartType: $waasCheck)."
                    Write-Info "Manual: use registry HKLM:\SYSTEM\...\Services\WaaSMedicSvc\Start = 4 as TrustedInstaller."
                }
            } else {
                Write-Host "  [DRY-RUN] Would backup + stop + disable: wuauserv, UsoSvc, WaaSMedicSvc" -ForegroundColor Magenta
            }

            Write-Blank
            Write-Info "RE-ENABLE later with:"
            Write-Info "  Set-Service wuauserv -StartupType Manual; Start-Service wuauserv"
            Write-Info "  Set-Service UsoSvc -StartupType Manual; Start-Service UsoSvc"
            Write-Info "  Set-Service WaaSMedicSvc -StartupType Manual"
            Complete-Step $PHASE 15 "WUBlocker"
        } `
        -SkipAction { Skip-Step $PHASE 15 "WUBlocker" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 16 — NIC TWEAKS  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 16) {
    Write-Section "Step 16 — NIC Latency Stack: Adapter + RSS + URO + QoS DSCP"
    Invoke-TieredStep -Tier 2 -Title "NIC latency stack: adapter properties + RSS + URO disable + QoS DSCP" `
        -Why "Five complementary layers: (1) Adapter properties — EEE, Green Ethernet, Power Saving Mode eliminate PHY power-state wake latency spikes; Flow Control disables PAUSE frame transmit stalls. InterruptModeration=Medium (all profiles) — djdallmann empirical test found Medium minimises DPC latency variance; Disabled causes interrupt storms under background traffic, making jitter worse. (2) RSS — Intel I225-V/I226-V omit RSS registry entries by default; all DPCs land on Core 0. (3) URO (Win11) — UDP Receive Offload batches CS2's 128-pkt/sec datagrams before DPC delivery, adding receive jitter; disable gives per-datagram DPC. (4) QoS DSCP EF=46 — tags CS2 UDP for priority forwarding; 'Do not use NLA' key is prerequisite (silently blocks DSCP on unidentified networks without it). (5) Bufferbloat awareness." `
        -Evidence "EEE/FlowControl: standard low-latency NIC tuning. InterruptModeration Medium: djdallmann/GamingPCSetup empirical Intel NIC test — Disabled caused interrupt storms worsening jitter under real-world background traffic; Medium gives most predictable DPC scheduling. GreenEthernet/PowerSavingMode: same rationale as EEE (vendor-specific PHY power gating). RSS entries: djdallmann — 'many vendor drivers omit these; notable NDIS DPC latency improvements.' URO: Windows 11 UDP batching feature — disabling gives per-datagram DPC delivery for CS2. QoS NLA key: required prerequisite for DSCP on all network profiles. Bufferbloat: valleyofdoom, waveform.com." `
        -Caveat "LatencyMon can confirm NIC DPC bottleneck. RSS restart required. URO disable Win11 only (safely no-ops Win10). DSCP benefit requires QoS-aware router/switch; consumer ISP routers strip DSCP at first hop. InterruptModeration stays Medium even on COMPETITIVE — empirical evidence trumps theoretical per-packet argument." `
        -Risk "MODERATE" -Depth "NETWORK" -EstimateKey "NIC Tweaks" `
        -Improvement "Lower, more consistent NIC DPC latency: PHY wake jitter eliminated, interrupt coalescing tuned for predictability, UDP receive batching removed (Win11), CS2 UDP tagged for QoS priority" `
        -SideEffects "Higher NIC power consumption (power features off). Wake-on-LAN unaffected during play. QoS policies persist until removed. URO persists until re-enabled." `
        -Undo "Re-enable EEE via Device Manager -> NIC -> Advanced; netsh int udp set global uro=enabled; Remove-NetQosPolicy -Name CS2_UDP_Ports,CS2_App -Confirm:\$false; remove RSS registry entries; set DisabledComponents=0 in HKLM:\...Tcpip6\Parameters" `
        -Action {
            # ── Adapter-level properties (EEE, Flow Control, Interrupt Moderation, Buffers) ─
            # DisplayName varies by vendor: Intel uses "EEE", Realtek uses "Energy Efficient Ethernet".
            # Try the primary name first; on failure try the alternate (Realtek-style) name.
            $nic = $null
            try {
                $nic = Get-ActiveNicAdapter
                if ($nic) {
                    Write-OK "Adapter: $($nic.Name) — $($nic.InterfaceDescription)"
                    $isRealtek = $nic.InterfaceDescription -match "Realtek|RTL"

                    # 5 GbE NICs (e.g., RTL8126) benefit from larger buffers
                    $effectiveTweaks = $CFG_NIC_Tweaks.Clone()
                    if ($nic.Speed -and $nic.Speed -ge 5000000000) {
                        $effectiveTweaks["ReceiveBuffers"] = "2048"
                        $effectiveTweaks["TransmitBuffers"] = "2048"
                        Write-Info "5+ GbE NIC detected ($('{0:N1}' -f ($nic.Speed / 1e9)) Gbps) — using larger buffers (2048)"
                    }

                    if (-not $SCRIPT:DryRun) {
                        foreach ($t in $effectiveTweaks.GetEnumerator()) {
                            # Try primary name, then alternate if primary fails
                            $displayName = $t.Key
                            $origProp = Get-NetAdapterAdvancedProperty -Name $nic.Name `
                                -DisplayName $displayName -ErrorAction SilentlyContinue
                            if (-not $origProp -and $CFG_NIC_Tweaks_AltNames.ContainsKey($t.Key)) {
                                $displayName = $CFG_NIC_Tweaks_AltNames[$t.Key]
                                $origProp = Get-NetAdapterAdvancedProperty -Name $nic.Name `
                                    -DisplayName $displayName -ErrorAction SilentlyContinue
                            }
                            if ($origProp) {
                                Backup-NicAdapterProperty -AdapterName $nic.Name `
                                    -PropertyName $displayName -OriginalValue $origProp.DisplayValue `
                                    -PropertyType "DisplayName" -StepTitle $SCRIPT:CurrentStepTitle
                            }
                            $nicResult = Set-NetAdapterAdvancedProperty -Name $nic.Name -DisplayName $displayName `
                                -DisplayValue $t.Value -ErrorAction SilentlyContinue -PassThru
                            if ($nicResult) {
                                Write-OK "NIC: $displayName = $($t.Value)"
                            } else {
                                Write-Sub "NIC: $($t.Key) — not exposed by this adapter (skipped)"
                            }
                        }
                    } else {
                        foreach ($t in $effectiveTweaks.GetEnumerator()) {
                            $name = if ($isRealtek -and $CFG_NIC_Tweaks_AltNames.ContainsKey($t.Key)) { $CFG_NIC_Tweaks_AltNames[$t.Key] } else { $t.Key }
                            Write-Host "  [DRY-RUN] Would set NIC $($nic.Name): $name = $($t.Value)" -ForegroundColor Magenta
                        }
                    }

                    # ── Vendor PHY power-save (registry keyword — reliable cross-driver) ──
                    # *GreenEthernet: Realtek vendor PHY power-save (distinct from IEEE *EEE)
                    # *PowerSavingMode: NIC-level DMA/interrupt power gating
                    if (-not $SCRIPT:DryRun) {
                        foreach ($kw in @("*GreenEthernet", "*PowerSavingMode")) {
                            $origKw = Get-NetAdapterAdvancedProperty -Name $nic.Name `
                                -RegistryKeyword $kw -ErrorAction SilentlyContinue
                            if ($origKw) {
                                Backup-NicAdapterProperty -AdapterName $nic.Name `
                                    -PropertyName $kw -OriginalValue $origKw.RegistryValue `
                                    -PropertyType "RegistryKeyword" -StepTitle $SCRIPT:CurrentStepTitle
                            }
                            Set-NetAdapterAdvancedProperty -Name $nic.Name `
                                -RegistryKeyword $kw -RegistryValue 0 -ErrorAction SilentlyContinue
                        }
                        Write-OK "NIC: vendor PHY power-save (*GreenEthernet, *PowerSavingMode) = Disabled"
                    } else {
                        Write-Host "  [DRY-RUN] Would set NIC $($nic.Name): *GreenEthernet = 0, *PowerSavingMode = 0" -ForegroundColor Magenta
                    }
                    Write-Sub "  (silently no-ops if NIC driver does not expose these keywords)"

                } else { Write-Warn "No active LAN adapter found." }
            } catch { Write-Warn "NIC adapter properties error: $_" }

            # ── Wi-Fi advisory ────────────────────────────────────────────────────────
            # NIC adapter tweaks (RSS, interrupt moderation, QoS policies) target the
            # active ETHERNET adapter only. If the user is on Wi-Fi, the adapter-level
            # changes above were skipped — warn and advise wired connection.
            try {
                $wifiAdapter = Get-NetAdapter -ErrorAction SilentlyContinue |
                    Where-Object { $_.Status -eq "Up" -and
                        $_.InterfaceDescription -match "Wi-Fi|Wireless|802\.11|WLAN" } |
                    Select-Object -First 1
                if ($wifiAdapter -and -not $nic) {
                    Write-Blank
                    Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Red
                    Write-Host "  │  ⚠  WI-FI ONLY — No active Ethernet connection found         │" -ForegroundColor Red
                    Write-Host "  │                                                              │" -ForegroundColor Red
                    Write-Host "  │  For competitive CS2: USE A WIRED CONNECTION.                │" -ForegroundColor White
                    Write-Host "  │  Wi-Fi adds 5-50ms jitter even at full signal strength.      │" -ForegroundColor White
                    Write-Host "  │  Wireless packet loss causes hit registration failures.       │" -ForegroundColor White
                    Write-Host "  │                                                              │" -ForegroundColor Yellow
                    Write-Host "  │  Ethernet adapter tweaks (RSS, interrupt moderation, QoS)    │" -ForegroundColor DarkGray
                    Write-Host "  │  were skipped — they apply to wired adapters only.           │" -ForegroundColor DarkGray
                    Write-Host "  │                                                              │" -ForegroundColor Green
                    Write-Host "  │  Wi-Fi power saving disabled via Power Plan (Step 6).        │" -ForegroundColor Green
                    Write-Host "  │  URO disable + QoS DSCP applied below regardless.            │" -ForegroundColor Green
                    Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Red
                } elseif ($wifiAdapter -and $nic) {
                    Write-Sub "Wi-Fi adapter also present — Ethernet ($($nic.Name)) used for NIC tweaks."
                }
            } catch { Write-DebugLog "Wi-Fi detection failed." }

            # ── RSS driver registry entries (adds missing entries only) ────────────────
            Set-NicRssConfig

            # ── URO disable — UDP Receive Offload (Windows 11+) ──────────────────────
            # URO coalesces multiple UDP datagrams from the same flow before DPC delivery
            # to reduce CPU interrupt load. For CS2's 128-pkt/sec stream this means
            # several game state packets may be batched and delivered late as a group,
            # adding receive-side jitter. Disabling gives per-datagram DPC delivery.
            # Windows 11 only (build 22000+). netsh command exits with error on Win10.
            $osBuild = [System.Environment]::OSVersion.Version.Build

            # Capture URO state BEFORE disabling so backup reflects original state
            $currentUro = "n/a"
            if ($osBuild -ge 22000) {
                try {
                    $uroQuery = netsh int udp show global 2>&1
                    if ($uroQuery -match "Receive Offload State\s*:\s*(\S+)") {
                        $currentUro = $Matches[1].ToLower()
                    }
                } catch { $currentUro = "n/a" }
            }

            if ($osBuild -ge 22000) {
                if (-not $SCRIPT:DryRun) {
                    try {
                        $uroOut = netsh int udp set global uro=disabled 2>&1
                        if ($LASTEXITCODE -eq 0) {
                            Write-OK "URO: UDP Receive Offload disabled — per-datagram DPC delivery"
                            Write-Sub "Undo: netsh int udp set global uro=enabled"
                        } else {
                            Write-DebugLog "URO: netsh returned error (build $osBuild may not support URO) — $uroOut"
                        }
                    } catch {
                        Write-DebugLog "URO: command failed (build $osBuild) — $_"
                    }
                } else {
                    Write-Host "  [DRY-RUN] Would run: netsh int udp set global uro=disabled" -ForegroundColor Magenta
                }
            } else {
                Write-Sub "URO: Windows 10 detected (build $osBuild) — Win11+ only, skipping"
            }

            # ── QoS DSCP EF=46 for CS2 UDP traffic ──────────────────────────────────
            # Prerequisite: bypass NLA check that silently blocks DSCP on unidentified
            # network profiles. Without this key, policies work on Domain networks only.
            Set-RegistryValue "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\QoS" `
                "Do not use NLA" "1" "String" "QoS prerequisite: bypass NLA check for DSCP"

            if (-not $SCRIPT:DryRun) {
                # Backup existing QoS policies and URO state before creating/modifying
                $existingPolicies = @()
                foreach ($pName in @("CS2_UDP_Ports", "CS2_App")) {
                    $existing = Get-NetQosPolicy -Name $pName -ErrorAction SilentlyContinue
                    if ($existing) { $existingPolicies += $pName }
                }
                # URO state was captured above BEFORE the disable call
                Backup-QosAndUro -PolicyNames $existingPolicies `
                    -UroState $currentUro -StepTitle $SCRIPT:CurrentStepTitle

                # Port-based policy: CS2 default game ports 27015–27036 UDP
                try {
                    Remove-NetQosPolicy -Name "CS2_UDP_Ports" -Confirm:$false -ErrorAction SilentlyContinue
                    New-NetQosPolicy -Name "CS2_UDP_Ports" -IPProtocol UDP `
                        -IPDstPortStart 27015 -IPDstPortEnd 27036 `
                        -DSCPAction 46 -NetworkProfile All -ErrorAction Stop | Out-Null
                    Write-OK "QoS: CS2 ports 27015-27036 (UDP) → DSCP EF=46"
                } catch { Write-Warn "QoS port policy error: $_" }

                # App-path policy: catches all cs2.exe traffic regardless of port
                try {
                    Remove-NetQosPolicy -Name "CS2_App" -Confirm:$false -ErrorAction SilentlyContinue
                    New-NetQosPolicy -Name "CS2_App" `
                        -AppPathNameMatchCondition "*\cs2.exe" `
                        -DSCPAction 46 -NetworkProfile All -ErrorAction Stop | Out-Null
                    Write-OK "QoS: cs2.exe app-path → DSCP EF=46 (belt-and-suspenders)"
                } catch { Write-Warn "QoS app-path policy error: $_" }

                Write-Info "DSCP benefit: active only on QoS-aware switches/routers."
                Write-Info "Consumer ISPs strip DSCP markings at the first hop."
                Write-Info "Undo: Remove-NetQosPolicy -Name CS2_UDP_Ports,CS2_App -Confirm:`$false"
            } else {
                Write-Host "  [DRY-RUN] Would create: QoS DSCP EF=46 policies for CS2 (port + app-path)" -ForegroundColor Magenta
            }

            # ── IPv6 — LEFT ENABLED (2026 reversal) ──────────────────────────────────
            # Previous guidance (2023-2024) recommended disabling IPv6 to eliminate NDP/RA
            # background traffic. 2025-2026 evidence reverses this:
            #   - Steam (late 2023+) prefers IPv6 when round-trip time is lower
            #   - Riot Games 2025 infra paper: 68% of EU-West connections use IPv6
            #     with 4.2ms median latency improvement vs IPv4-only
            #   - Disabling IPv6 can force traffic through IPv4 CGNAT gateways (+5-15ms)
            #   - Valve SDR relay network supports IPv6; disabling removes a faster path
            # The NDP/RA background overhead is trivial (<1 packet/sec) compared to the
            # potential routing benefit. IPv6 stays enabled.
            Write-OK "IPv6: left enabled (2026 meta — Steam/Valve SDR prefer IPv6 when faster)."
            Write-Sub "If you experience IPv6-specific issues, disable manually:"
            Write-Sub "  Set DisabledComponents = 0xFF in HKLM:\\...\\Tcpip6\\Parameters"

            # ── Bufferbloat awareness ─────────────────────────────────────────────────
            Write-Blank
            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor DarkCyan
            Write-Host "  │  BUFFERBLOAT — #1 REAL-WORLD UDP JITTER SOURCE               │" -ForegroundColor DarkCyan
            Write-Host "  │                                                              │" -ForegroundColor DarkCyan
            Write-Host "  │  If you experience variable ping or 'rubber-banding'         │" -ForegroundColor White
            Write-Host "  │  especially when uploading/downloading simultaneously:       │" -ForegroundColor White
            Write-Host "  │                                                              │" -ForegroundColor DarkCyan
            Write-Host "  │  Test:  waveform.com/tools/bufferbloat                       │" -ForegroundColor Cyan
            Write-Host "  │  Grade A/B = fine.  C or below = bufferbloat present.        │" -ForegroundColor DarkGray
            Write-Host "  │                                                              │" -ForegroundColor DarkCyan
            Write-Host "  │  Fix:  Enable QoS / SQM on your router with fq_codel or     │" -ForegroundColor White
            Write-Host "  │  CAKE algorithm. OpenWrt, pfSense, and most modern           │" -ForegroundColor DarkGray
            Write-Host "  │  Asus/NETGEAR routers support this in firmware.              │" -ForegroundColor DarkGray
            Write-Host "  │                                                              │" -ForegroundColor DarkCyan
            Write-Host "  │  Note: Valve SDR (net_client_steamdatagram_enable_override 1)│" -ForegroundColor DarkGray
            Write-Host "  │  in your autoexec already helps by routing around bad hops. │" -ForegroundColor DarkGray
            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor DarkCyan

            Complete-Step $PHASE 16 "NIC"
        } `
        -SkipAction { Skip-Step $PHASE 16 "NIC" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 17 — CAPFRAMEX + BASELINE BENCHMARK  [T1, proof tool]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 17) {
    Write-Section "Step 17 — CapFrameX + Baseline Benchmark"
    Write-Info "Without benchmarks there's no proof. CapFrameX is the tool for that."
    Invoke-TieredStep -Tier 1 -Title "Baseline benchmark BEFORE optimizations" `
        -Why "Without before/after measurement you can't know what actually helped." `
        -Evidence "T1: Measurement is the prerequisite for evidence-based optimization." `
        -Risk "SAFE" -Depth "CHECK" `
        -Improvement "Establishes baseline for before/after comparison — essential for proof" `
        -SideEffects "None — measurement only" `
        -Undo "N/A" `
        -Action {
            Write-Blank
            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Cyan
            Write-Host "  │  CAPFRAMEX — BENCHMARK MEASUREMENT TOOL                     │" -ForegroundColor Cyan
            Write-Host "  │                                                              │" -ForegroundColor Cyan
            Write-Host "  │  Download manually from:                                     │" -ForegroundColor White
            Write-Host "  │  https://capframex.com                                      │" -ForegroundColor Green
            Write-Host "  │  https://github.com/CXWorld/CapFrameX/releases              │" -ForegroundColor Green
            Write-Host "  │                                                              │" -ForegroundColor Cyan
            Write-Host "  │  IMPORTANT: Run a BASELINE benchmark NOW, before any         │" -ForegroundColor Yellow
            Write-Host "  │  further optimizations. This is your 'before' measurement.  │" -ForegroundColor Yellow
            Write-Host "  │                                                              │" -ForegroundColor Cyan
            Write-Host "  │  WORKFLOW:                                                   │" -ForegroundColor White
            Write-Host "  │  1. Download + install CapFrameX                             │" -ForegroundColor White
            Write-Host "  │  2. Subscribe to FPSHeaven benchmark map (Dust2):            │" -ForegroundColor White
            Write-Host "  │     $CFG_Benchmark_Dust2" -ForegroundColor Green
            Write-Host "  │  3. Run benchmark 3 times, note avg + 1% lows              │" -ForegroundColor White
            Write-Host "  │  4. After ALL optimizations: repeat for comparison           │" -ForegroundColor White
            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Cyan
            Write-Blank
            "https://capframex.com" | Set-ClipboardSafe
            Write-OK "CapFrameX download URL copied to clipboard."

            $r = if ($SCRIPT:DryRun -or (Test-YoloProfile)) { "n" } else { Read-Host "  Have you completed the baseline benchmark? [y/N]" }
            if ($r -match "^[jJyY]$") {
                $result = Invoke-BenchmarkCapture -Label "Baseline (before optimizations)"
                if ($result) {
                    if (-not $SCRIPT:DryRun) {
                        try {
                            $st = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json
                            $st | Add-Member -NotePropertyName "baselineAvg" -NotePropertyValue $result.Avg -Force
                            $st | Add-Member -NotePropertyName "baselineP1" -NotePropertyValue $result.P1 -Force
                            Save-JsonAtomic -Data $st -Path $CFG_StateFile
                        } catch { Write-Warn "Could not persist baseline data: $_" }
                    } else {
                        Write-Host "  [DRY-RUN] Would persist baseline: Avg=$($result.Avg) P1=$($result.P1)" -ForegroundColor Magenta
                    }
                }
            } else {
                Write-Info "You can run baseline later — but before/after comparison is most valuable."
            }
            Complete-Step $PHASE 17 "CapFrameX-Baseline"
        } `
        -SkipAction { Skip-Step $PHASE 17 "CapFrameX-Baseline" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 18 — GPU DRIVER CLEAN REMOVAL (pre-check)  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 18) {
    Write-Section "Step 18 — GPU Driver Clean Removal (preparation)"
    Write-Info "GPU driver clean removal will run in Safe Mode (Phase 2)."
    Write-Info "Using native PowerShell — no external tools required."
    Complete-Step $PHASE 18 "DDU-prep"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 19 — NVIDIA DRIVER DOWNLOAD  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 19) {
    Write-Section "Step 19 — NVIDIA Driver Download"
    if ($gpuInput -in @("1","2")) {
        Invoke-TieredStep -Tier 1 -Title "Download NVIDIA driver for clean install" `
            -Why "Clean driver without bloat. Native install with MSI + telemetry disabled." `
            -Evidence "T1: Clean driver installation is a fundamental prerequisite." `
            -Risk "SAFE" -Depth "FILESYSTEM" `
            -Improvement "Prepares clean driver for Phase 3 installation" `
            -SideEffects "Downloads ~600 MB driver file to C:\CS2_OPTIMIZE" `
            -Undo "Delete downloaded file" `
            -Action {
                # Persist GPU name to state.json so Phase 3 can identify the GPU
                # after the driver is uninstalled in Phase 2 (Safe Mode).
                $gpuForState = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
                    Where-Object { $_.Name -match "NVIDIA" } | Select-Object -First 1
                if ($gpuForState -and -not $SCRIPT:DryRun) {
                    try {
                        $st = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json
                        $st | Add-Member -NotePropertyName "nvidiaGpuName" -NotePropertyValue $gpuForState.Name -Force
                        Save-JsonAtomic -Data $st -Path $CFG_StateFile
                    } catch { Write-DebugLog "Could not persist GPU name to state: $_" }
                }

                $driverInfo = $null
                if ($state -and $state.PSObject.Properties['rollbackDriver'] -and $state.rollbackDriver) {
                    Write-Info "Rollback requested: driver $($state.rollbackDriver)"
                    Write-Info "Download manually from: https://www.nvidia.com/en-us/drivers/"
                    "https://www.nvidia.com/en-us/drivers/" | Set-ClipboardSafe
                    Write-OK "NVIDIA download page copied to clipboard."
                    Write-Info "Download the driver .exe, then provide the path in Phase 3."
                } elseif ($SCRIPT:DryRun) {
                    $driverInfo = Get-LatestNvidiaDriver
                    if ($driverInfo -and -not $driverInfo.ManualDownload) {
                        Write-Host "  [DRY-RUN] Would download NVIDIA driver: $($driverInfo.Version)" -ForegroundColor Magenta
                        Write-Host "  [DRY-RUN]   URL: $($driverInfo.Url)" -ForegroundColor DarkMagenta
                        Write-Host "  [DRY-RUN]   Dest: $CFG_WorkDir\nvidia_driver.exe (~600 MB)" -ForegroundColor DarkMagenta
                    } else {
                        Write-Host "  [DRY-RUN] Would prompt manual download from nvidia.com/drivers" -ForegroundColor Magenta
                    }
                } else {
                    $driverInfo = Get-LatestNvidiaDriver
                    if ($driverInfo -and -not $driverInfo.ManualDownload) {
                        $driverDest = "$CFG_WorkDir\nvidia_driver.exe"
                        if (Invoke-Download $driverInfo.Url $driverDest "NVIDIA Driver $($driverInfo.Version)") {
                            # SECURITY (S1): Verify Authenticode signature before persisting path to state.json
                            if (-not (Test-NvidiaDriverSignature $driverDest)) {
                                Write-Err "Downloaded driver failed signature verification. File removed."
                            } else {
                                try {
                                    $st = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json
                                    $st | Add-Member -NotePropertyName "nvidiaDriverPath" -NotePropertyValue $driverDest -Force
                                    $st | Add-Member -NotePropertyName "nvidiaDriverVersion" -NotePropertyValue $driverInfo.Version -Force
                                    Save-JsonAtomic -Data $st -Path $CFG_StateFile
                                } catch { Write-Warn "Could not persist driver path to state: $_" }
                                Write-OK "Driver ready: $driverDest"
                            }
                        }
                    } else {
                        Write-Info "Download driver manually from: https://www.nvidia.com/en-us/drivers/"
                        "https://www.nvidia.com/en-us/drivers/" | Set-ClipboardSafe
                        Write-OK "NVIDIA download page copied to clipboard."
                    }
                }
                Complete-Step $PHASE 19 "NVDriver"
            } `
            -SkipAction { Skip-Step $PHASE 19 "NVDriver" }
    } else {
        Write-Info "AMD: amd.com/support | Intel: intel.com/download-center"
        Skip-Step $PHASE 19 "NVDriver"
    }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 20 — NVIDIA PROFILE (pre-check)  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 20 -and $gpuInput -in @("1","2")) {
    Write-Section "Step 20 — NVIDIA CS2 Profile (preparation)"
    Write-Info "NVIDIA profile settings will be applied automatically in Phase 3."
    Write-Info "Using native registry writes — no external Profile Inspector needed."
    Complete-Step $PHASE 20 "NVProfile-prep"
} elseif ($startStep -le 20) {
    Skip-Step $PHASE 20 "NVProfile (no NVIDIA)"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 21 — MSI INTERRUPTS (pre-check)  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 21) {
    Write-Section "Step 21 — MSI Interrupts (preparation)"
    Write-Info "MSI interrupts will be set automatically in Phase 3."
    Write-Info "Using native registry writes — no external MSI Utility needed."
    Complete-Step $PHASE 21 "MSI-prep"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 22 — NIC INTERRUPT AFFINITY (pre-check)  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 22) {
    Write-Section "Step 22 — NIC Interrupt Affinity (preparation)"
    Write-Info "NIC interrupt affinity will be set automatically in Phase 3."
    Write-Info "Using native registry writes — no external GoInterruptPolicy needed."
    Complete-Step $PHASE 22 "Affinity-prep"
}
