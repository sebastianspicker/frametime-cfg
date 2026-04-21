# ==============================================================================
#  helpers/system-analysis.ps1  —  Non-destructive system health checks
#  Returns structured [PSCustomObject] results for GUI display.
#  Safe to call from a RunSpace — does all its own registry reads.
# ==============================================================================

function script:Get-RegVal {
    param([string]$Path, [string]$Name)
    try { (Get-ItemProperty -Path $Path -Name $Name -ErrorAction Stop).$Name }
    catch { $null }
}

function script:New-CheckItem {
    param(
        [string]$Category,
        [string]$Group,
        [string]$Item,
        [string]$Current,
        [string]$Recommended,
        [string]$Status,   # OK | WARN | ERR | INFO | SKIP
        [string]$StepRef,
        [string]$Impact
    )
    $label = switch ($Status) {
        "OK"   { "✓  OK"   } "WARN" { "⚠  WARN" } "ERR"  { "✗  ERR" }
        "INFO" { "ℹ  INFO" } "SKIP" { "—  SKIP" } default { $Status }
    }
    $color = switch ($Status) {
        "OK"   { "#22c55e" } "WARN" { "#fbbf24" } "ERR"  { "#ef4444" }
        "INFO" { "#6b7280" } "SKIP" { "#374151" } default { "#e5e5e5" }
    }
    [PSCustomObject]@{
        Category    = $Category
        Group       = $Group
        Item        = $Item
        Current     = if ($null -eq $Current -or $Current -eq "") { "(not set)" } else { $Current }
        Recommended = $Recommended
        Status      = $Status
        StatusLabel = $label
        StatusColor = $color
        StepRef     = $StepRef
        Impact      = $Impact
    }
}

# ── Hardware ──────────────────────────────────────────────────────────────────
function Invoke-CheckHardware {
    $results = [System.Collections.Generic.List[object]]::new()

    # VBS / HVCI
    try {
        $dg = Get-CimInstance -ClassName Win32_DeviceGuard `
            -Namespace root/Microsoft/Windows/DeviceGuard -ErrorAction SilentlyContinue
        $vbsVal = $dg.VirtualizationBasedSecurityStatus
        $vbsStr = switch ($vbsVal) { 0 { "Off" } 1 { "Configured (not running)" } 2 { "Running" } default { "Unknown" } }
        $st = if ($vbsVal -eq 0) { "OK" } elseif ($vbsVal -eq 1) { "INFO" } else { "WARN" }
        $results.Add((New-CheckItem "Hardware" "Security" "VBS / HVCI" $vbsStr "Off" $st "P3-7" "VBS running = 5-15% CPU overhead in games on OEM Win11 builds"))
    } catch {
        $results.Add((New-CheckItem "Hardware" "Security" "VBS / HVCI" "Query failed" "Off" "INFO" "P3-7" "Could not query Win32_DeviceGuard"))
    }

    # WHEA errors (PBO/CO stability indicator)
    try {
        $whea = Test-WheaErrors
        if ($whea) {
            $wheaStr = if ($whea.HasErrors) { "$($whea.RecentCount) in last 24h" } else { "0 recent ($($whea.Count) total)" }
            $wheaSt = if ($whea.HasErrors) { "ERR" } elseif ($whea.Count -gt 0) { "WARN" } else { "OK" }
            $results.Add((New-CheckItem "Hardware" "CPU" "WHEA Errors" $wheaStr "0" $wheaSt "BIOS" "WHEA errors indicate PBO/CO instability — reduce Curve Optimizer by 5"))
        }
    } catch { Write-DebugLog "WHEA check failed: $_" }

    # AMD X3D CPU base clock (informational — MaxClockSpeed is base, not boost)
    $amdCpu = $null
    try {
        $amdCpu = Get-AmdCpuInfo
        if ($amdCpu -and $amdCpu.IsX3D -and $amdCpu.MaxClockSpeed -gt 0) {
            $baseStr = "$($amdCpu.MaxClockSpeed) MHz (base clock)"
            $results.Add((New-CheckItem "Hardware" "CPU" "X3D Base Clock" $baseStr "N/A (boost requires HWiNFO)" "INFO" "BIOS" "Win32_Processor.MaxClockSpeed reports base clock, not boost — use HWiNFO to verify boost"))
        }
    } catch { Write-DebugLog "X3D base clock check failed: $_" }

    # DDR5 FCLK/MCLK 1:1
    try {
        $ddr5 = Get-Ddr5TimingInfo
        if ($ddr5 -and $ddr5.IsDDR5) {
            $mtsStr = "DDR5-$($ddr5.ActiveMTs)"
            $ddr5St = if ($ddr5.IsOptimal1to1) { "OK" } elseif ($ddr5.ActiveMTs -gt 6400) { "WARN" } else { "INFO" }
            $ddr5Rec = if ($amdCpu -and $amdCpu.IsX3D) { "DDR5-6000 (1:1)" } else { "XMP rated speed" }
            $results.Add((New-CheckItem "Hardware" "Memory" "DDR5 Speed" $mtsStr $ddr5Rec $ddr5St "P1-2" "AM5 optimal: DDR5-6000 (FCLK 2000, MCLK 3000, 1:1 ratio)"))
        }
    } catch { Write-DebugLog "DDR5 timing check failed: $_" }

    # Dual-channel RAM
    try {
        $dc = Test-DualChannel
        if ($null -ne $dc) {
            $st = if ($dc.DualChannel -eq $true) { "OK" } elseif ($dc.DualChannel -eq $false) { "ERR" } else { "INFO" }
            $results.Add((New-CheckItem "Hardware" "Memory" "Dual-Channel RAM" $dc.Reason "Dual-channel" $st "P1-24" "Single-channel halves memory bandwidth — 20-40% FPS loss in CS2"))
        }
    } catch { Write-DebugLog "Dual-channel RAM check failed: $_" }

    # XMP / EXPO
    try {
        $ram = Get-RamInfo
        if ($ram) {
            $xmpStr = if ($ram.AtRatedSpeed) { "At rated speed ($($ram.ActiveMhz) MT/s)" } else { "Below rated ($($ram.ActiveMhz) MT/s, rated $($ram.SpeedMhz) MT/s)" }
            $st = if ($ram.AtRatedSpeed) { "OK" } else { "WARN" }
            $results.Add((New-CheckItem "Hardware" "Memory" "XMP / EXPO" $xmpStr "At rated speed" $st "P1-2" "RAM running below rated speed — enable XMP/EXPO in BIOS"))
        }
    } catch { Write-DebugLog "XMP/EXPO check failed: $_" }

    return $results
}

# ── Windows Gaming ────────────────────────────────────────────────────────────
function Invoke-CheckWindowsGaming {
    $r = [System.Collections.Generic.List[object]]::new()

    # HAGS
    try {
        $hags = Get-RegVal "HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers" "HwSchMode"
        $hagsStr = switch ($hags) { 2 { "Enabled" } 1 { "Disabled" } $null { "Not set" } default { "$hags" } }
        $r.Add((New-CheckItem "Windows" "Display" "HAGS" $hagsStr "Enabled (2)" "INFO" "P1-7" "Setup-dependent — benchmark both ON and OFF on your system"))
    } catch { Write-DebugLog "HAGS registry check failed: $_" }

    # Fast Startup
    $fs = Get-RegVal "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power" "HiberbootEnabled"
    $st = if ($null -ne $fs -and $fs -eq 0) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Boot" "Fast Startup" $(if ($null -ne $fs -and $fs -eq 0) {"Disabled"} else {"Enabled"}) "Disabled (0)" $st "P1-23" "Prevents MSI interrupt changes persisting across shutdown"))

    # Game Mode
    $gm = Get-RegVal "HKCU:\Software\Microsoft\GameBar" "AutoGameModeEnabled"
    $st = if ($gm -eq 1 -or $null -eq $gm) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Gaming" "Game Mode" $(if ($gm -eq 1 -or $null -eq $gm) {"Enabled"} else {"Disabled"}) "Enabled (1)" $st "P1-12" "WU deferral + MMCSS Games scheduling path"))

    # Game DVR
    $dvr = Get-RegVal "HKCU:\System\GameConfigStore" "GameDVR_Enabled"
    $st = if ($null -ne $dvr -and $dvr -eq 0) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Gaming" "Game DVR" $(if ($null -ne $dvr -and $dvr -eq 0) {"Disabled"} else {"Enabled"}) "Disabled (0)" $st "P1-31" "Background recording steals GPU time"))

    # MPO
    $mpo = Get-RegVal "HKLM:\SOFTWARE\Microsoft\Windows\Dwm" "OverlayTestMode"
    $st = if ($mpo -eq 5) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Display" "MPO" $(if ($mpo -eq 5) {"Disabled"} else {"Enabled"}) "Disabled (5)" $st "P1-11" "Multiplane Overlay can cause DWM compositing stutters"))

    # FSE Behavior
    $fse = Get-RegVal "HKCU:\System\GameConfigStore" "GameDVR_FSEBehavior"
    $st = if ($fse -eq 2) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Display" "FSE Behavior" $(if ($null -ne $fse) {"$fse"} else {"Not set"}) "2" $st "P1-26" "Fullscreen exclusivity mode for lower input latency"))

    # Auto HDR
    $hdr = Get-RegVal "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\VideoSettings" "AutoHDREnabled"
    if ($null -ne $hdr) {
        $st = if ($hdr -eq 0) { "OK" } else { "WARN" }
        $r.Add((New-CheckItem "Windows" "Display" "Auto HDR" $(if ($hdr -eq 0) {"Disabled"} else {"Enabled"}) "Disabled (0)" $st "P1-36" "Tone-mapping overhead + overbright window areas in CS2"))
    }

    # Visual Effects
    $vfx = Get-RegVal "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects" "VisualFXSetting"
    $st = if ($vfx -eq 2) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Display" "Visual Effects" $(if ($vfx -eq 2) {"Best Performance"} else {"Default/Custom"}) "Best Performance (2)" $st "P1-36" "Disables window animations and transparency"))

    # ClearType (preserved despite Best Performance)
    $ct = Get-RegVal "HKCU:\Control Panel\Desktop" "FontSmoothing"
    $st = if ($ct -eq "2") { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Display" "ClearType" $(if ($ct -eq "2") {"Enabled"} else {"Disabled/Not set"}) "Enabled (2)" $st "P1-36" "Font smoothing — preserved alongside Best Performance"))

    # Steam Overlay
    $steamOv = Get-RegVal "HKCU:\Software\Valve\Steam" "GameOverlayDisabled"
    $st = if ($steamOv -eq 1) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Overlay" "Steam Overlay" $(if ($steamOv -eq 1) {"Disabled"} else {"Enabled"}) "Disabled (1)" $st "P1-32" "Overlay hooks into GPU rendering pipeline"))

    return $r
}

# ── System Latency ────────────────────────────────────────────────────────────
function Invoke-CheckSystemLatency {
    $r = [System.Collections.Generic.List[object]]::new()

    # MMCSS SystemResponsiveness
    $sysResp = Get-RegVal "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile" "SystemResponsiveness"
    $sysRespDisplay = if ($null -eq $sysResp) { "Not set (default: 20)" } else { "$sysResp" }
    $st = if ($sysResp -eq 10) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "MMCSS" "SystemResponsiveness" $sysRespDisplay "10" $st "P1-27" "Controls CPU% reserved for multimedia — lower = more for CS2"))

    # NoLazyMode
    $lazy = Get-RegVal "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile" "NoLazyMode"
    $st = if ($lazy -eq 1) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "MMCSS" "MMCSS NoLazyMode" $(if ($lazy -eq 1) {"1"} else {"Not set"}) "1" $st "P1-27" "Shifts MMCSS from idle-detection to realtime-only"))

    # Win32PrioritySeparation
    $w32 = Get-RegVal "HKLM:\SYSTEM\CurrentControlSet\Control\PriorityControl" "Win32PrioritySeparation"
    $st = if ($w32 -eq 0x2A) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Scheduler" "Win32PrioritySeparation" $(if ($null -ne $w32) {"0x{0:X}" -f $w32} else {"Not set"}) "0x2A (42)" $st "P1-27" "Short fixed quantum, max foreground boost"))

    # DisablePagingExecutive
    $dpe = Get-RegVal "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management" "DisablePagingExecutive"
    $st = if ($dpe -eq 1) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Memory" "DisablePagingExecutive" $(if ($dpe -eq 1) {"1"} else {"0 / Not set"}) "1" $st "P1-27" "Keeps kernel code in RAM"))

    # GlobalTimerResolutionRequests
    $tmr = Get-RegVal "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\kernel" "GlobalTimerResolutionRequests"
    $st = if ($tmr -eq 1) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Timer" "GlobalTimerResolutionRequests" $(if ($tmr -eq 1) {"1"} else {"Not set"}) "1" $st "P1-28" "Enables 0.5ms timer resolution system-wide"))

    # FTH
    $fth = Get-RegVal "HKLM:\SOFTWARE\Microsoft\FTH" "Enabled"
    $st = if ($null -ne $fth -and $fth -eq 0) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "System" "Fault Tolerant Heap" $(if ($null -ne $fth -and $fth -eq 0) {"Disabled"} else {"Enabled"}) "Disabled (0)" $st "P1-27" "Prevents 10-15% heap slowdown after crashes"))

    # Automatic Maintenance
    $maint = Get-RegVal "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\Maintenance" "MaintenanceDisabled"
    $st = if ($maint -eq 1) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "System" "Auto Maintenance" $(if ($maint -eq 1) {"Disabled"} else {"Enabled"}) "Disabled (1)" $st "P1-27" "Stops 12-14% mid-game CPU spikes (djdallmann xperf)"))

    # NTFS Last Access
    $ntfsLa = Get-RegVal "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" "NtfsDisableLastAccessUpdate"
    # (-2147483647) = 0x80000001 as signed int32 — PS 5.1 reads DWORD as Int32; 1 = disabled (legacy)
    $st = if ($ntfsLa -eq (-2147483647) -or $ntfsLa -eq 1) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Filesystem" "NTFS Last Access" $(if ($ntfsLa -eq (-2147483647) -or $ntfsLa -eq 1) {"Disabled"} else {"Enabled"}) "Disabled (0x80000001)" $st "P1-27" "Removes metadata write on every file read"))

    # NTFS 8.3 Name Creation
    $ntfs83 = Get-RegVal "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" "NtfsDisable8dot3NameCreation"
    $st = if ($ntfs83 -eq 1) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "Filesystem" "NTFS 8.3 Names" $(if ($ntfs83 -eq 1) {"Disabled"} else {"Enabled"}) "Disabled (1)" $st "P1-27" "Eliminates legacy short name generation overhead"))

    # DisableCoInstallers
    $coIns = Get-RegVal "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Device Installer" "DisableCoInstallers"
    $st = if ($coIns -eq 1) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "System" "PnP Co-Installers" $(if ($coIns -eq 1) {"Disabled"} else {"Enabled"}) "Disabled (1)" $st "P1-27" "Prevents vendor bloatware injection during device install"))

    # MMCSS Games task
    $gamesPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games"
    $gPri = Get-RegVal $gamesPath "Priority"
    $gSch = Get-RegVal $gamesPath "Scheduling Category"
    $gGpu = Get-RegVal $gamesPath "GPU Priority"
    $gamesOk = ($gPri -eq 6) -and ($gSch -eq "High") -and ($gGpu -eq 8)
    $st = if ($gamesOk) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Windows" "MMCSS" "Games Task Priority" "Pri=$gPri Sched=$gSch GPU=$gGpu" "6 / High / 8" $st "P1-27" "MMCSS Games scheduling class for foreground game threads"))

    # Boot config (bcdedit) — read-only query
    # Use /v to get hex element IDs instead of localized key names.
    # Match on hex IDs (locale-independent) + any truthy value — same approach as Verify-Settings.ps1.
    # 0x26000060 = disabledynamictick, 0x26000092 = useplatformtick
    try {
        $bcdOutput = bcdedit /enum "{current}" /v 2>&1 | Out-String
        if ($LASTEXITCODE -ne 0) {
            Write-DebugLog "bcdedit exited with code $LASTEXITCODE — skipping boot config checks"
        } else {
            $dynTick = if ($bcdOutput -match "0x26000060\s+\S+") { "OK" } else { "WARN" }
            $r.Add((New-CheckItem "Windows" "Boot" "Dynamic Tick" $(if ($dynTick -eq "OK") {"Disabled"} else {"Active"}) "Disabled" $dynTick "P1-10" "Adaptive timer causes irregular CPU wakeups — frametime jitter"))

            $platTick = if ($bcdOutput -match "0x26000092\s+\S+") { "OK" } else { "WARN" }
            $r.Add((New-CheckItem "Windows" "Boot" "Platform Tick" $(if ($platTick -eq "OK") {"Active"} else {"Inactive"}) "Active" $platTick "P1-10" "Hardware timer instead of software timer"))
        }
    } catch { Write-DebugLog "bcdedit check failed: $_" }

    return $r
}

# ── Input ─────────────────────────────────────────────────────────────────────
function Invoke-CheckInput {
    $r = [System.Collections.Generic.List[object]]::new()

    $ms1 = Get-RegVal "HKCU:\Control Panel\Mouse" "MouseSpeed"
    $ms2 = Get-RegVal "HKCU:\Control Panel\Mouse" "MouseThreshold1"
    $ms3 = Get-RegVal "HKCU:\Control Panel\Mouse" "MouseThreshold2"
    $allOff = ("$ms1" -eq "0") -and ("$ms2" -eq "0") -and ("$ms3" -eq "0")
    $st = if ($allOff) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Input" "Mouse" "Mouse Acceleration (Windows)" "Speed=$ms1 Thr1=$ms2 Thr2=$ms3" "All 0" $st "P1-29" "EnhancePointerPrecision — adds non-linear speed scaling"))

    $queueSize = Get-RegVal "HKLM:\SYSTEM\CurrentControlSet\Services\mouclass\Parameters" "MouseDataQueueSize"
    $st = if ($queueSize -eq 50) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Input" "Mouse" "mouclass Queue Size" $(if ($null -ne $queueSize) {"$queueSize"} else {"Default (100)"}) "50" $st "P1-29" "Default 100; values below 30 cause input skipping; 50 = safe minimum"))

    return $r
}

# ── Network ───────────────────────────────────────────────────────────────────
function Invoke-CheckNetwork {
    $r = [System.Collections.Generic.List[object]]::new()

    # Nagle (check the active NIC interface specifically, not any random interface)
    try {
        $activeGuid = Get-ActiveNicGuid
        $nagleOk = $false
        if ($activeGuid) {
            $ifacePath = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\$activeGuid"
            $nd = Get-RegVal $ifacePath "TcpNoDelay"
            if ($nd -eq 1) { $nagleOk = $true }
        } else {
            # Fallback: check all interfaces if active NIC detection failed
            $interfaces = Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces" -ErrorAction SilentlyContinue
            foreach ($iface in $interfaces) {
                $nd = Get-RegVal $iface.PSPath "TcpNoDelay"
                if ($nd -eq 1) { $nagleOk = $true; break }
            }
        }
        $r.Add((New-CheckItem "Network" "TCP" "Nagle Disable (TcpNoDelay)" $(if ($nagleOk) {"Disabled on active NIC"} else {"Enabled / Not set"}) "1 (disabled)" $(if ($nagleOk) {"OK"} else {"WARN"}) "P1-25" "Nagle bundles small TCP packets → increases latency"))
    } catch { Write-DebugLog "Nagle/TcpNoDelay check failed: $_" }

    # IPv6 — intentionally left enabled (2026 reversal: Steam prefers IPv6 when faster)
    $ipv6 = Get-RegVal "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip6\Parameters" "DisabledComponents"
    $st = if ($null -eq $ipv6 -or $ipv6 -ne 0xFF) { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Network" "Stack" "IPv6" $(if ($ipv6 -eq 0xFF) {"Disabled (0xFF)"} else {"Enabled"}) "Enabled" $st "P1-16" "2026: Steam prefers IPv6 when faster; disabling forces CGNAT (+5-15ms)"))

    # QoS NLA bypass
    $nla = Get-RegVal "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\QoS" "Do not use NLA"
    $st = if ($nla -eq "1") { "OK" } else { "WARN" }
    $r.Add((New-CheckItem "Network" "QoS" "QoS NLA Bypass" $(if ($nla -eq "1") {"Enabled"} else {"Not set"}) "1" $st "P1-16" "Required for DSCP EF=46 QoS to function on unidentified networks"))

    # URO (Win11 only)
    # netsh output is locale-dependent — check for ABSENCE of enabled patterns
    # since there are fewer locale variants for "enabled" than "disabled"
    try {
        $build = [System.Environment]::OSVersion.Version.Build
        if ($build -ge 22000) {
            $uro = & netsh int udp show global 2>&1 | Select-String "uro"
            $uroStr = if ($uro) { "$uro".Trim() } else { "" }
            if (-not $uroStr) {
                $r.Add((New-CheckItem "Network" "Stack" "URO (UDP Receive Offload)" "not available" "disabled" "OK" "P1-16" "URO not present on this build — no action needed"))
            } else {
                $uroVal = if ($uroStr -notmatch "enabled|aktiviert|activé|abilitato|activado|ativado") { "disabled" } else { "enabled" }
                $st = if ($uroVal -eq "disabled") { "OK" } else { "WARN" }
                $r.Add((New-CheckItem "Network" "Stack" "URO (UDP Receive Offload)" $uroVal "disabled" $st "P1-16" "URO batches UDP datagrams causing receive jitter on Win11"))
            }
        }
    } catch { Write-DebugLog "URO check failed: $_" }

    return $r
}

# ── Services ─────────────────────────────────────────────────────────────────
function Invoke-CheckServices {
    $r = [System.Collections.Generic.List[object]]::new()

    # Xbox service labels for display; names sourced from $CFG_XboxServices
    $xboxLabels = @{
        "XblAuthManager" = "Xbox Auth Manager"
        "XblGameSave"    = "Xbox Game Save"
        "XboxNetApiSvc"  = "Xbox Network API"
        "XboxGipSvc"     = "Xbox Accessory Mgmt"
    }
    $xboxSvcs = if ($CFG_XboxServices) { $CFG_XboxServices } else { @("XblAuthManager","XblGameSave","XboxNetApiSvc","XboxGipSvc") }
    $checks = @(
        @{ Name="SysMain"; Label="SysMain (Superfetch)"; Target="Disabled" }
        @{ Name="WSearch"; Label="Windows Search";       Target="Disabled" }
    ) + $($xboxSvcs | ForEach-Object {
        @{ Name=$_; Label=$(if ($xboxLabels[$_]) { $xboxLabels[$_] } else { $_ }); Target="Disabled" }
    }) + @(
        @{ Name="qWave";  Label="qWave (QoS probe)";    Target="Disabled" }
    )

    foreach ($svc in $checks) {
        try {
            $s = Get-Service -Name $svc.Name -ErrorAction SilentlyContinue
            if ($null -eq $s) {
                $r.Add((New-CheckItem "Services" "Windows" $svc.Label "Not present" $svc.Target "OK" "P1-37" "Service not installed"))
            } else {
                $escapedSvcName = $svc.Name -replace "'", "''"
                $cimSvc = Get-CimInstance Win32_Service -Filter "Name='$escapedSvcName'" -ErrorAction SilentlyContinue
                $rawStart = if ($cimSvc) { $cimSvc.StartMode } else { $s.StartType.ToString() }
                $startType = switch ($rawStart) { "Auto" { "Automatic" } "Auto Delayed" { "AutomaticDelayedStart" } default { $rawStart } }
                $st = if ($startType -eq $svc.Target -or $rawStart -eq $svc.Target) { "OK" } else { "WARN" }
                $r.Add((New-CheckItem "Services" "Windows" $svc.Label $startType $svc.Target $st "P1-37" "Background service consuming CPU/memory during gaming"))
            }
        } catch {
            $r.Add((New-CheckItem "Services" "Windows" $svc.Label "Query failed" $svc.Target "INFO" "P1-37" ""))
        }
    }

    return $r
}

# ── CS2 Config ────────────────────────────────────────────────────────────────
function Invoke-CheckCS2 {
    $r = [System.Collections.Generic.List[object]]::new()

    # Steam base path — used for video.txt and launch options below
    $steamPath = Get-SteamPath

    # Find CS2 install
    $cs2Path = $null
    try { $cs2Path = Get-CS2InstallPath } catch { Write-DebugLog "CS2 install path detection failed: $_" }

    if (-not $cs2Path) {
        $r.Add((New-CheckItem "CS2" "Install" "CS2 Install" "Not found" "Found" "ERR" "—" "CS2 not detected — check Steam library"))
        return $r
    }

    $cfgDir  = "$cs2Path\game\csgo\cfg"
    $optPath = "$cfgDir\optimization.cfg"

    # optimization.cfg
    $optExists = Test-Path $optPath
    $r.Add((New-CheckItem "CS2" "Config" "optimization.cfg" $(if ($optExists) {"Present"} else {"Missing"}) "Present" $(if ($optExists) {"OK"} else {"ERR"}) "P1-34" "74 optimized CVars — network, audio, mouse, video"))

    if ($optExists) {
        # Check key CVars in optimization.cfg
        $optContent = Get-Content $optPath -Raw -ErrorAction SilentlyContinue
        $keyChecks = @(
            @{ CVar="snd_use_hrtf"; Expected="1"; Impact="Steam Audio HRTF enable" }
            @{ CVar="cl_autowepswitch"; Expected="0"; Impact="Prevents auto weapon switch on pickup" }
            @{ CVar="rate";         Expected="1000000"; Impact="CS2 max bandwidth rate" }
            @{ CVar="speaker_config"; Expected="1"; Impact="Headphones mode — required for HRTF" }
        )
        foreach ($ck in $keyChecks) {
            if ($optContent -match "(?m)^\s*$([regex]::Escape($ck.CVar))\s+(\S+)") {
                $val = $Matches[1].Trim()
                $st = if ($val -eq $ck.Expected) { "OK" } else { "WARN" }
                $r.Add((New-CheckItem "CS2" "Autoexec" $ck.CVar $val $ck.Expected $st "P1-34" $ck.Impact))
            } else {
                $r.Add((New-CheckItem "CS2" "Autoexec" $ck.CVar "Not in optimization.cfg" $ck.Expected "WARN" "P1-34" $ck.Impact))
            }
        }
    }

    # video.txt
    try {
        if ($steamPath) {
            $vtxt = Get-ChildItem "$steamPath\userdata\*\730\local\cfg\video.txt" -ErrorAction SilentlyContinue |
                Sort-Object LastWriteTime -Descending | Select-Object -First 1
            if ($vtxt) {
                $vtContent = Get-Content $vtxt.FullName -Raw -ErrorAction SilentlyContinue
                $vtChecks = @(
                    @{ Key="setting.msaa_samples";         Expected="4";  Label="MSAA";            Impact="Community benchmark default for stronger 1% lows" }
                    @{ Key="setting.r_low_latency";        Expected="1";  Label="NVIDIA Reflex";   Impact="Common low-latency default; benchmark if unsure" }
                    @{ Key="setting.mat_vsync";            Expected="0";  Label="VSync";           Impact="Must be OFF — adds 1-3 frames render queue latency" }
                    @{ Key="setting.sc_hdr_enabled_override"; Expected="3"; Label="HDR Shader"; Impact="Performance mode — Quality washes out window/sun areas" }
                    @{ Key="setting.fullscreen";           Expected="1";  Label="Fullscreen Mode"; Impact="Exclusive FS bypasses DWM compositor — lower input latency" }
                )
                foreach ($vc in $vtChecks) {
                    if ($vtContent -match "(?m)`"$([regex]::Escape($vc.Key))`"\s+`"([^`"]+)`"") {
                        $val = $Matches[1]
                        $st = if ($val -eq $vc.Expected) { "OK" } else { "WARN" }
                        $r.Add((New-CheckItem "CS2" "video.txt" $vc.Label $val $vc.Expected $st "P3-6" $vc.Impact))
                    } else {
                        $r.Add((New-CheckItem "CS2" "video.txt" $vc.Label "Not found" $vc.Expected "WARN" "P3-6" $vc.Impact))
                    }
                }
            } else {
                $r.Add((New-CheckItem "CS2" "video.txt" "video.txt" "Not found" "Present" "WARN" "P3-6" "Video settings file missing — launch CS2 once to generate it"))
            }
        }
    } catch { Write-DebugLog "video.txt check failed: $_" }

    # Launch options (localconfig.vdf)
    try {
        if ($steamPath) {
            $lcVdf = Get-ChildItem "$steamPath\userdata\*\config\localconfig.vdf" -ErrorAction SilentlyContinue |
                Sort-Object LastWriteTime -Descending | Select-Object -First 1
            if ($lcVdf) {
                $lc = Get-Content $lcVdf.FullName -Raw -ErrorAction SilentlyContinue
                if ($lc -match '"730"[\s\S]*?"LaunchOptions"\s+"([^"]*)"') {
                    $lo = $Matches[1]
                    $hasConsole = $lo -match "-console"
                    $hasExec    = $lo -match "\+exec"
                    $allGood = $hasConsole -and $hasExec
                    $r.Add((New-CheckItem "CS2" "Launch" "Launch Options" $lo "-console +exec autoexec" $(if ($allGood) {"OK"} else {"WARN"}) "P1-34" "Essential launch flags — exec must load autoexec.cfg"))
                } else {
                    $r.Add((New-CheckItem "CS2" "Launch" "Launch Options" "Not set" "-console +exec autoexec" "WARN" "P1-34" "Launch options not configured in Steam"))
                }
            }
        }
    } catch { Write-DebugLog "Launch options check failed: $_" }

    return $r
}

# ── Orchestrator ──────────────────────────────────────────────────────────────
function Invoke-SystemAnalysis {
    $all = [System.Collections.Generic.List[object]]::new()
    foreach ($fn in @(
        { Invoke-CheckHardware       }
        { Invoke-CheckWindowsGaming  }
        { Invoke-CheckSystemLatency  }
        { Invoke-CheckInput          }
        { Invoke-CheckNetwork        }
        { Invoke-CheckServices       }
        { Invoke-CheckCS2            }
    )) {
        try { $all.AddRange([object[]]@(& $fn)) } catch { Write-DebugLog "Analysis check failed: $_" }
    }
    return $all
}
