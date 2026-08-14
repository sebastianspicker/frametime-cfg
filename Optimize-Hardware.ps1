# ==============================================================================
#  Optimize-Hardware.ps1  -  Steps 10-22: Timer, MPO, Game Mode, Debloat,
#                             Autostart, WU Blocker, NIC, Baseline Benchmark,
#                             Driver Prep, NVIDIA Driver, Profile, MSI, Affinity
# ==============================================================================

# ══════════════════════════════════════════════════════════════════════════════
# STEP 10 - DYNAMIC TICK  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 10) {
    Write-Section "Step 10 - Timer Optimization (bcdedit)"
    $null = Invoke-TieredStep -Tier 3 -Title "Disable Dynamic Tick" `
        -Why "Requests the Windows disabledynamictick BCD option for a controlled comparison." `
        -Evidence "Microsoft documents useplatformtick as a debugging option, so it is not applied. This repository includes no isolated benchmark for disabledynamictick." `
        -Caveat "This is a system-wide boot option. Windows timer and power behavior can vary by build and hardware." `
        -Risk "MODERATE" -Depth "BOOT" `
        -Improvement "Applies disabledynamictick=yes for local before-and-after comparison" `
        -SideEffects "Can change system timer behavior and idle power use" `
        -Undo "bcdedit /set disabledynamictick no" `
        -Action {
            $bootResult = Set-BootConfig "disabledynamictick" "yes" "Constant timer resolution" -PassThru
            if (-not $bootResult.Applied -and $bootResult.Status -ne "DryRun") {
                throw "Required boot config write failed for disabledynamictick: $($bootResult.Message)"
            }
            Write-Info "Undo: bcdedit /set disabledynamictick no"
            Complete-Step $PHASE 10 "Timer"
        } `
        -SkipAction { Skip-Step $PHASE 10 "Timer" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 11 - MPO  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 11) {
    Write-Section "Step 11 - Disable MPO"
    $null = Invoke-TieredStep -Tier 3 -Title "Disable Multiplane Overlay (MPO)" `
        -Why "Sets the Windows DWM OverlayTestMode policy to disable Multiplane Overlay." `
        -Evidence "The policy state is verifiable. This repository includes no isolated CS2 benchmark for MPO." `
        -Caveat "Disabling MPO can change desktop and multi-monitor composition behavior." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Provides a controlled MPO-disabled state for local comparison" `
        -SideEffects "Desktop, video, or multi-monitor compositing may change" `
        -Undo "reg delete HKLM\SOFTWARE\Microsoft\Windows\Dwm /v OverlayTestMode /f" `
        -Action {
            Set-RegistryValue "HKLM:\SOFTWARE\Microsoft\Windows\Dwm" "OverlayTestMode" 5 "DWord" "Disable MPO"
            Complete-Step $PHASE 11 "MPO"
        } `
        -SkipAction { Skip-Step $PHASE 11 "MPO" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 12 - GAME MODE  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 12) {
    Write-Section "Step 12 - Windows Game Mode"
    $null = Invoke-TieredStep -Tier 3 -Title "Enable Windows Game Mode" `
        -Why "Enables the Windows Game Mode preference. Game Mode and Game DVR are separate controls." `
        -Evidence "The registry values are deterministic, but Windows controls their scheduling behavior. This repository includes no isolated benchmark." `
        -Caveat "Behavior can change across Windows builds. Step 31 separately controls Game DVR recording." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Enables the Windows Game Mode preference" `
        -SideEffects "May change Windows scheduling and background-activity policy while a game runs" `
        -Undo "Set AllowAutoGameMode + AutoGameModeEnabled to 0" `
        -Action {
            Set-RegistryValue "HKCU:\SOFTWARE\Microsoft\GameBar" "AllowAutoGameMode"   1 "DWord" "Auto Game Mode on"
            Set-RegistryValue "HKCU:\SOFTWARE\Microsoft\GameBar" "AutoGameModeEnabled" 1 "DWord" "Game Mode on"
            Complete-Step $PHASE 12 "GameMode"
        } `
        -SkipAction { Skip-Step $PHASE 12 "GameMode" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 13 - OPTIONAL APPX AND TELEMETRY CLEANUP
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 13) {
    Write-Section "Step 13 - Optional AppX and Telemetry Cleanup"
    Write-Info "Removes the configured optional AppX package set and changes telemetry-related controls."
    Write-Info "This repository contains no CS2 performance result for the cleanup."
    $null = Invoke-TieredStep -Tier 2 -Title "Run optional AppX and telemetry cleanup (native PowerShell)" `
        -Why "Removes the repository AppX package list and changes selected services, tasks, and policies." `
        -Evidence "The resulting package and policy state can be verified. No isolated CS2 benchmark is included." `
        -Caveat "Removes known bloatware AppX packages + disables telemetry. NOT: Windows Defender!" `
        -Risk "MODERATE" -Depth "APP" `
        -Improvement "Removes the selected packages and records supported state for restoration" `
        -SideEffects "Removed bloatware apps cannot be easily reinstalled from Microsoft Store" `
        -Undo "Reinstall apps via Microsoft Store or DISM /Online /Add-ProvisionedAppxPackage" `
        -Action {
            Invoke-GamingDebloat
            Complete-Step $PHASE 13 "Debloat"
        } `
        -SkipAction { Skip-Step $PHASE 13 "Debloat" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 14 - AUTOSTART  [Hygiene]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 14) {
    Write-Section "Step 14 - Clean Autostart Entries  [System hygiene]"
    $null = Invoke-TieredStep -Tier 2 -Title "Disable autostart entries" `
        -Why "Removes configured registry startup entries without uninstalling the applications." `
        -Evidence "The startup-entry change is verifiable. This repository includes no isolated CS2 benchmark." `
        -Caveat "Only registry entries are removed - apps stay installed." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Removes the configured applications from registry startup" `
        -SideEffects "Apps (Discord, Spotify, etc.) won't auto-start. Launch manually when needed." `
        -Undo "Use Recovery to restore each captured registry startup command" `
        -Action {
            $removed = 0
            foreach ($app in $CFG_Autostart_Remove) {
                foreach ($rp in @("HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run","HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run")) {
                    if (Get-ItemProperty $rp -Name $app -ErrorAction SilentlyContinue) {
                        if (-not $SCRIPT:DryRun) {
                            $capture = Backup-RegistryValue -Path $rp -Name $app `
                                -StepTitle $SCRIPT:CurrentStepTitle -PassThru
                            if (-not $capture -or -not $capture.Captured) {
                                $detail = if ($capture -and $capture.Message) { $capture.Message } else { 'No capture result was returned.' }
                                throw "Autostart removal blocked for '$app': $detail"
                            }
                            Flush-BackupBuffer
                            $durableBackup = Get-BackupDataRaw
                            $capturePersisted = @($durableBackup.entries | Where-Object {
                                $_.type -eq 'registry' -and $_.step -eq $SCRIPT:CurrentStepTitle -and
                                $_.path -eq $rp -and $_.name -eq $app
                            }).Count -gt 0
                            if (-not $capturePersisted) {
                                throw "Autostart removal blocked for '$app': backup.json does not contain its restore record."
                            }
                            Remove-ItemProperty $rp -Name $app -ErrorAction Stop
                            $remainingValue = Get-ItemProperty -Path $rp -Name $app -ErrorAction SilentlyContinue
                            if ($remainingValue -and $remainingValue.PSObject.Properties[$app]) {
                                throw "Autostart entry '$app' is still present after removal."
                            }
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
            Write-Info "Undo: use Recovery to restore the captured startup commands."
            Complete-Step $PHASE 14 "Autostart"
        } `
        -SkipAction { Skip-Step $PHASE 14 "Autostart" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 15 - WINDOWS UPDATE BLOCKER  [Security risk, always prompted]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 15) {
    Write-Section "Step 15 - Windows Update Blocker  [Security risk]"
    $null = Invoke-TieredStep -Tier 3 -Title "Disable Windows Update services" `
        -Why "Disables selected Windows Update services to prevent their normal operation." `
        -Evidence "Service state is verifiable. This step is not a performance optimization." `
        -Caveat "Windows security and quality updates will not operate normally while these services remain disabled." `
        -Risk "CRITICAL" -Depth "SERVICE" `
        -Improvement "Stops the selected Windows Update services until restored" `
        -SideEffects "Security and quality updates can be delayed. Skip this step on systems that depend on normal Windows servicing." `
        -Undo "Use Recovery to restore each captured service startup type and running state" `
        -Action {
            if (-not $SCRIPT:DryRun) {
                $serviceStepTitle = "Windows Update Blocker"
                $serviceNames = @("wuauserv", "UsoSvc", "WaaSMedicSvc")
                $presentServices = [System.Collections.Generic.List[string]]::new()

                foreach ($serviceName in $serviceNames) {
                    $service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
                    if (-not $service) { continue }
                    $presentServices.Add($serviceName) | Out-Null
                    $capture = Backup-ServiceState -ServiceName $serviceName `
                        -StepTitle $serviceStepTitle -PassThru
                    if (-not $capture -or -not $capture.Captured) {
                        $detail = if ($capture -and $capture.Message) { $capture.Message } else { 'No capture result was returned.' }
                        throw "Windows Update changes blocked because '$serviceName' was not captured: $detail"
                    }
                }
                Flush-BackupBuffer
                $durableBackup = Get-BackupDataRaw
                foreach ($serviceName in $presentServices) {
                    $capturePersisted = @($durableBackup.entries | Where-Object {
                        $_.type -eq 'service' -and $_.step -eq $serviceStepTitle -and $_.name -eq $serviceName
                    }).Count -gt 0
                    if (-not $capturePersisted) {
                        throw "Windows Update changes blocked because backup.json has no restore record for '$serviceName'."
                    }
                }

                $serviceFailures = [System.Collections.Generic.List[string]]::new()
                foreach ($serviceName in $presentServices) {
                    try {
                        Set-Service -Name $serviceName -StartupType Disabled -ErrorAction Stop
                        Stop-Service -Name $serviceName -Force -ErrorAction Stop
                        $updatedService = Get-Service -Name $serviceName -ErrorAction Stop
                        if ($updatedService.StartType -ne 'Disabled' -or $updatedService.Status -eq 'Running') {
                            throw "verification returned StartType=$($updatedService.StartType), Status=$($updatedService.Status)"
                        }
                        Write-OK "$serviceName disabled and stopped."
                    } catch {
                        $serviceFailures.Add($serviceName) | Out-Null
                        Write-Warn "Could not disable and stop ${serviceName}: $_"
                    }
                }
                if ($serviceFailures.Count -gt 0) {
                    throw "Windows Update service changes failed: $($serviceFailures -join ', '). Step 15 was not completed."
                }
            } else {
                Write-Host "  [DRY-RUN] Would backup + stop + disable: wuauserv, UsoSvc, WaaSMedicSvc" -ForegroundColor Magenta
            }

            Write-Blank
            Write-Info "Undo: use Recovery to restore the captured service states."
            Complete-Step $PHASE 15 "WUBlocker"
        } `
        -SkipAction { Skip-Step $PHASE 15 "WUBlocker" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 16 - NIC TWEAKS  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 16) {
    Write-Section "Step 16 - NIC Latency Stack: Adapter + RSS + URO + QoS DSCP"
    $null = Invoke-TieredStep -Tier 2 -Title "NIC latency stack: adapter properties + RSS + URO disable + QoS DSCP" `
        -Why "Changes selected adapter power, flow-control, interrupt-moderation, RSS, URO, and QoS policy values for the active wired adapter." `
        -Evidence "The adapter, registry, netsh, and QoS states are verifiable. This repository includes no cross-adapter or network-path latency benchmark." `
        -Caveat "Property names and support vary by driver. RSS changes can require a restart. DSCP handling depends on the local network, and an unsuitable setting can increase latency or reduce throughput." `
        -Risk "MODERATE" -Depth "NETWORK" `
        -Improvement "Applies and records the repository NIC and QoS policy values" `
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
                    Write-OK "Adapter: $($nic.Name) - $($nic.InterfaceDescription)"
                    $isRealtek = $nic.InterfaceDescription -match "Realtek|RTL"

                    # 5 GbE NICs (e.g., RTL8126) benefit from larger buffers
                    $effectiveTweaks = $CFG_NIC_Tweaks.Clone()
                    if ($nic.Speed -and $nic.Speed -ge 5000000000) {
                        $effectiveTweaks["ReceiveBuffers"] = "2048"
                        $effectiveTweaks["TransmitBuffers"] = "2048"
                        Write-Info "5+ GbE NIC detected ($('{0:N1}' -f ($nic.Speed / 1e9)) Gbps) - using larger buffers (2048)"
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
                                Write-Sub "NIC: $($t.Key) - not exposed by this adapter (skipped)"
                            }
                        }
                    } else {
                        foreach ($t in $effectiveTweaks.GetEnumerator()) {
                            $name = if ($isRealtek -and $CFG_NIC_Tweaks_AltNames.ContainsKey($t.Key)) { $CFG_NIC_Tweaks_AltNames[$t.Key] } else { $t.Key }
                            Write-Host "  [DRY-RUN] Would set NIC $($nic.Name): $name = $($t.Value)" -ForegroundColor Magenta
                        }
                    }

                    # ── Vendor PHY power-save (registry keyword - reliable cross-driver) ──
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
            # changes above were skipped - warn and advise wired connection.
            try {
                $wifiAdapter = Get-NetAdapter -ErrorAction SilentlyContinue |
                    Where-Object { $_.Status -eq "Up" -and
                        $_.InterfaceDescription -match "Wi-Fi|Wireless|802\.11|WLAN" } |
                    Select-Object -First 1
                if ($wifiAdapter -and -not $nic) {
                    Write-Blank
                    Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Red
                    Write-Host "  │  ⚠  WI-FI ONLY - No active Ethernet connection found         │" -ForegroundColor Red
                    Write-Host "  │                                                              │" -ForegroundColor Red
                    Write-Host "  │  A wired connection avoids radio and roaming variability.    │" -ForegroundColor White
                    Write-Host "  │  Measure packet loss and jitter on the local network.         │" -ForegroundColor White
                    Write-Host "  │                                                              │" -ForegroundColor Yellow
                    Write-Host "  │  Ethernet adapter tweaks (RSS, interrupt moderation, QoS)    │" -ForegroundColor DarkGray
                    Write-Host "  │  were skipped - they apply to wired adapters only.           │" -ForegroundColor DarkGray
                    Write-Host "  │                                                              │" -ForegroundColor Green
                    Write-Host "  │  Wi-Fi power saving disabled via Power Plan (Step 6).        │" -ForegroundColor Green
                    Write-Host "  │  URO disable + QoS DSCP applied below regardless.            │" -ForegroundColor Green
                    Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Red
                } elseif ($wifiAdapter -and $nic) {
                    Write-Sub "Wi-Fi adapter also present - Ethernet ($($nic.Name)) used for NIC tweaks."
                }
            } catch { Write-DebugLog "Wi-Fi detection failed." }

            # ── RSS driver registry entries (adds missing entries only) ────────────────
            Set-NicRssConfig

            # ── URO disable - UDP Receive Offload (Windows 11+) ──────────────────────
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
                            Write-OK "URO: UDP Receive Offload disabled - per-datagram DPC delivery"
                            Write-Sub "Undo: netsh int udp set global uro=enabled"
                        } else {
                            Write-DebugLog "URO: netsh returned error (build $osBuild may not support URO) - $uroOut"
                        }
                    } catch {
                        Write-DebugLog "URO: command failed (build $osBuild) - $_"
                    }
                } else {
                    Write-Host "  [DRY-RUN] Would run: netsh int udp set global uro=disabled" -ForegroundColor Magenta
                }
            } else {
                Write-Sub "URO: Windows 10 detected (build $osBuild) - Win11+ only, skipping"
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
                    if ($existing) { $existingPolicies += $existing }
                }
                # URO state was captured above BEFORE the disable call
                Backup-QosAndUro -Policies $existingPolicies `
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

            # IPv6 remains enabled
            # IPv6 remains enabled so Windows can select an available IPv4 or IPv6 route.
            # The repository includes no route-performance guarantee for either protocol.
            Write-OK "IPv6 is left enabled so Windows can select an available route."
            Write-Sub "If you experience IPv6-specific issues, disable manually:"
            Write-Sub "  Set DisabledComponents = 0xFF in HKLM:\\...\\Tcpip6\\Parameters"

            # ── Bufferbloat awareness ─────────────────────────────────────────────────
            Write-Blank
            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor DarkCyan
            Write-Host "  │  BUFFERBLOAT REVIEW                                           │" -ForegroundColor DarkCyan
            Write-Host "  │                                                              │" -ForegroundColor DarkCyan
            Write-Host "  │  If you experience variable ping or 'rubber-banding'         │" -ForegroundColor White
            Write-Host "  │  especially when uploading/downloading simultaneously:       │" -ForegroundColor White
            Write-Host "  │                                                              │" -ForegroundColor DarkCyan
            Write-Host "  │  Compare idle and loaded latency with a repeatable test.     │" -ForegroundColor White
            Write-Host "  │  One third-party option: waveform.com/tools/bufferbloat      │" -ForegroundColor Cyan
            Write-Host "  │  A grade is diagnostic input, not a root-cause finding.      │" -ForegroundColor DarkGray
            Write-Host "  │                                                              │" -ForegroundColor DarkCyan
            Write-Host "  │  If loaded latency is repeatably higher, review current      │" -ForegroundColor White
            Write-Host "  │  router documentation for queue-management controls.         │" -ForegroundColor DarkGray
            Write-Host "  │  This suite does not change router configuration.             │" -ForegroundColor DarkGray
            Write-Host "  │                                                              │" -ForegroundColor DarkCyan
            Write-Host "  │  Valve SDR can select a relay path, but this suite cannot    │" -ForegroundColor DarkGray
            Write-Host "  │  guarantee that path or its latency.                          │" -ForegroundColor DarkGray
            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor DarkCyan

            Complete-Step $PHASE 16 "NIC"
        } `
        -SkipAction { Skip-Step $PHASE 16 "NIC" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 17 - OPTIONAL BASELINE MEASUREMENT
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 17) {
    Write-Section "Step 17 - CapFrameX + Baseline Benchmark"
    Write-Info "Use a repeatable capture before and after a candidate change."
    $null = Invoke-TieredStep -Tier 1 -Title "Baseline benchmark BEFORE optimizations" `
        -Why "A baseline provides comparison data for later captures." `
        -Evidence "CapFrameX is optional and installed separately. The repository stores summary values, not per-frame telemetry." `
        -Risk "SAFE" -Depth "CHECK" `
        -Improvement "Records a baseline for a later comparison" `
        -SideEffects "None - measurement only" `
        -Undo "N/A" `
        -Action {
            Write-Blank
            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Cyan
            Write-Host "  │  CAPFRAMEX - BENCHMARK MEASUREMENT TOOL                     │" -ForegroundColor Cyan
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
            $baselinePersisted = $false
            if ($r -match "^[jJyY]$") {
                $result = Invoke-BenchmarkCapture -Label "Baseline (before optimizations)"
                if ($result) {
                    if (-not $SCRIPT:DryRun) {
                        try {
                            $st = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json
                            $st | Add-Member -NotePropertyName "baselineAvg" -NotePropertyValue $result.Avg -Force
                            $st | Add-Member -NotePropertyName "baselineP1" -NotePropertyValue $result.P1 -Force
                            Save-SuiteState -State $st
                            $baselinePersisted = $true
                        } catch { Write-Warn "Could not persist baseline data: $_" }
                    } else {
                        Write-Host "  [DRY-RUN] Would persist baseline: Avg=$($result.Avg) P1=$($result.P1)" -ForegroundColor Magenta
                    }
                }
            } else {
                Write-Info "A later baseline will not represent the pre-change system state."
            }
            if ($baselinePersisted) {
                Complete-Step $PHASE 17 "CapFrameX-Baseline"
            } elseif (-not $SCRIPT:DryRun) {
                Write-Warn "Baseline step remains incomplete until a captured result is saved to suite state."
            }
        } `
        -SkipAction { Skip-Step $PHASE 17 "CapFrameX-Baseline" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 18 - GPU DRIVER CLEAN REMOVAL (pre-check)  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 18) {
    Write-Section "Step 18 - GPU Driver Clean Removal (preparation)"
    Write-Info "GPU driver clean removal will run in Safe Mode (Phase 2)."
    Write-Info "Using native PowerShell - no external tools required."
    Complete-Step $PHASE 18 "DDU-prep"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 19 - NVIDIA DRIVER DOWNLOAD  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 19) {
    Write-Section "Step 19 - NVIDIA Driver Download"
    if ($gpuInput -in @("1","2")) {
        $null = Invoke-TieredStep -Tier 1 -Title "Download NVIDIA driver for clean install" `
            -Why "Prepare a validated NVIDIA installer for the post-reboot phase." `
            -Evidence "T1: The workflow verifies download metadata and Authenticode identity. It does not establish driver performance." `
            -Risk "SAFE" -Depth "FILESYSTEM" `
            -Improvement "Prepares clean driver for Phase 3 installation" `
            -SideEffects "Downloads a vendor installer file to C:\FRAMETIME_CFG" `
            -Undo "Delete downloaded file" `
            -Action {
                $gpuForState = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
                    Where-Object { $_.Name -match "NVIDIA" } | Select-Object -First 1

                if ($state -and $state.PSObject.Properties['rollbackDriver'] -and $state.rollbackDriver) {
                    Write-Warn "Ignoring legacy rollbackDriver metadata; fixed-version rollback selection is not supported by this alpha."
                }

                $driverInfo = Get-LatestNvidiaDriver
                if (-not $driverInfo -or $driverInfo.ManualDownload) {
                    Write-Info "Download driver manually from: https://www.nvidia.com/en-us/drivers/"
                    "https://www.nvidia.com/en-us/drivers/" | Set-ClipboardSafe
                    Write-OK "NVIDIA download page copied to clipboard."
                    Write-Warn "Automatic NVIDIA driver download deferred to manual selection in Phase 3."
                    Skip-Step $PHASE 19 "NVDriver-manual"
                    return
                }
                if ([string]::IsNullOrWhiteSpace([string]$driverInfo.Url) -or
                    [string]::IsNullOrWhiteSpace([string]$driverInfo.Version)) {
                    throw "NVIDIA driver lookup returned incomplete automatic download metadata."
                }

                $driverDest = "$CFG_WorkDir\nvidia_driver.exe"
                if ($SCRIPT:DryRun) {
                    Write-Host "  [DRY-RUN] Would download NVIDIA driver: $($driverInfo.Version)" -ForegroundColor Magenta
                    Write-Host "  [DRY-RUN]   URL: $($driverInfo.Url)" -ForegroundColor DarkMagenta
                    Write-Host "  [DRY-RUN]   Dest: $driverDest" -ForegroundColor DarkMagenta
                    Complete-Step $PHASE 19 "NVDriver"
                    return
                }

                if (-not (Invoke-Download $driverInfo.Url $driverDest "NVIDIA Driver $($driverInfo.Version)")) {
                    throw "NVIDIA driver download failed for version $($driverInfo.Version)."
                }
                if (-not (Test-NvidiaDriverSignature $driverDest)) {
                    throw "NVIDIA driver signature verification failed for '$driverDest'."
                }

                try {
                    $st = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
                    if ($null -eq $st) { throw "State file contained no data." }
                    if ($gpuForState) {
                        $st | Add-Member -NotePropertyName "nvidiaGpuName" -NotePropertyValue $gpuForState.Name -Force
                    }
                    $st | Add-Member -NotePropertyName "nvidiaDriverPath" -NotePropertyValue $driverDest -Force
                    $st | Add-Member -NotePropertyName "nvidiaDriverVersion" -NotePropertyValue $driverInfo.Version -Force
                    Save-SuiteState -State $st
                } catch {
                    throw "NVIDIA driver state persistence failed: $($_.Exception.Message)"
                }

                Write-OK "Driver ready: $driverDest"
                Complete-Step $PHASE 19 "NVDriver"
            } `
            -SkipAction { Skip-Step $PHASE 19 "NVDriver" }
    } else {
        Write-Info "AMD: amd.com/support | Intel: intel.com/download-center"
        Skip-Step $PHASE 19 "NVDriver"
    }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 20 - NVIDIA PROFILE (pre-check)  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 20 -and $gpuInput -in @("1","2")) {
    Write-Section "Step 20 - NVIDIA CS2 Profile (preparation)"
    Write-Info "NVIDIA profile settings will be applied automatically in Phase 3."
    Write-Info "Using native registry writes - no external Profile Inspector needed."
    Complete-Step $PHASE 20 "NVProfile-prep"
} elseif ($startStep -le 20) {
    Skip-Step $PHASE 20 "NVProfile (no NVIDIA)"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 21 - MSI INTERRUPTS (pre-check)  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 21) {
    Write-Section "Step 21 - MSI Interrupts (preparation)"
    Write-Info "MSI interrupts will be set automatically in Phase 3."
    Write-Info "Using native registry writes - no external MSI Utility needed."
    Complete-Step $PHASE 21 "MSI-prep"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 22 - NIC INTERRUPT AFFINITY (pre-check)  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 22) {
    Write-Section "Step 22 - NIC Interrupt Affinity (preparation)"
    Write-Info "NIC interrupt affinity will be set automatically in Phase 3."
    Write-Info "Using native registry writes - no external GoInterruptPolicy needed."
    Complete-Step $PHASE 22 "Affinity-prep"
}
