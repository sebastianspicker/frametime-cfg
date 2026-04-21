#Requires -RunAsAdministrator
<#
.SYNOPSIS  CS2 Optimization Suite — Settings Verifier (Read-Only)

  Checks all registry keys, boot config and service states set by the suite.
  Useful after Windows Updates that reset settings.

  Color-coded:
    ✓  OK       (Green)  — Value matches
    ✗  CHANGED  (Yellow) — Value was modified
    ?  MISSING  (Red)    — Key doesn't exist
#>
param([switch]$SmokeTest)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ScriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not (Get-Variable -Name CFG_WorkDir -ErrorAction SilentlyContinue)) {
    . "$ScriptRoot\config.env.ps1"
}
if (-not (Get-Command Initialize-VerifyCounters -ErrorAction SilentlyContinue)) {
    . "$ScriptRoot\helpers.ps1"
}
if (-not (Get-Command Get-X3DCcdInfo -ErrorAction SilentlyContinue)) {
    . "$ScriptRoot\helpers\process-priority.ps1"
}
if (-not (Get-Command Initialize-NvApiDrs -ErrorAction SilentlyContinue)) {
    . "$ScriptRoot\helpers\nvidia-drs.ps1"
}
if (-not (Get-Variable -Name NV_DRS_SETTINGS -Scope Script -ErrorAction SilentlyContinue)) {
    . "$ScriptRoot\helpers\nvidia-profile.ps1"
}
if (-not (Get-Variable -Name PP_SUB_PROCESSOR -ErrorAction SilentlyContinue)) {
    . "$ScriptRoot\helpers\power-plan.ps1"
}

if ($SmokeTest) {
    Write-Host "SMOKE TEST OK: Verify-Settings" -ForegroundColor Green
    exit 0
}

function New-VerifyCheckResult {
    param(
        [ValidateSet("OK", "CHANGED", "MISSING", "INFO")]
        [string]$Status,
        [string]$Label,
        [string]$Detail = "",
        [string]$Path = ""
    )
    return [PSCustomObject]@{
        Status = $Status
        Label  = $Label
        Detail = $Detail
        Path   = $Path
    }
}

function Write-VerifyCheckResult {
    param([Parameter(Mandatory)]$Result)

    switch ($Result.Status) {
        "OK" {
            Write-Host "  ✓  OK        $($Result.Label)$(if ($Result.Detail) { "  $($Result.Detail)" })" -ForegroundColor Green
            $Script:_verifyOkCount++
        }
        "CHANGED" {
            Write-Host "  ✗  CHANGED   $($Result.Label)$(if ($Result.Detail) { "  $($Result.Detail)" })" -ForegroundColor Yellow
            $Script:_verifyChangedCount++
        }
        "MISSING" {
            Write-Host "  ?  MISSING   $($Result.Label)$(if ($Result.Detail) { "  $($Result.Detail)" })" -ForegroundColor Red
            $Script:_verifyMissingCount++
        }
        "INFO" {
            Write-Host "  ✓  INFO      $($Result.Label)$(if ($Result.Detail) { "  $($Result.Detail)" })" -ForegroundColor Cyan
            $Script:_verifyInfoCount++
        }
    }

    if ($Result.Path) {
        Write-Host "               $($Result.Path)" -ForegroundColor DarkGray
    }
}

function Test-VerifyNicAdvancedProperties {
    $results = [System.Collections.Generic.List[object]]::new()
    $nic = $null
    try { $nic = Get-ActiveNicAdapter } catch {}
    if (-not $nic) {
        $results.Add((New-VerifyCheckResult -Status "MISSING" -Label "Active LAN adapter for NIC tweaks" -Detail "not found")) | Out-Null
        return @($results)
    }

    foreach ($tweak in $CFG_NIC_Tweaks.GetEnumerator()) {
        $displayName = $tweak.Key
        $prop = Get-NetAdapterAdvancedProperty -Name $nic.Name -DisplayName $displayName -ErrorAction SilentlyContinue
        if (-not $prop -and $CFG_NIC_Tweaks_AltNames.ContainsKey($tweak.Key)) {
            $displayName = $CFG_NIC_Tweaks_AltNames[$tweak.Key]
            $prop = Get-NetAdapterAdvancedProperty -Name $nic.Name -DisplayName $displayName -ErrorAction SilentlyContinue
        }
        if (-not $prop) {
            $results.Add((New-VerifyCheckResult -Status "INFO" -Label "NIC tweak: $displayName" -Detail "property not exposed on $($nic.Name)")) | Out-Null
            continue
        }

        $currentValue = "$($prop.DisplayValue)"
        $expectedValue = "$($tweak.Value)"
        $status = if ($currentValue -eq $expectedValue) { "OK" } else { "CHANGED" }
        $detail = if ($status -eq "OK") { "($currentValue)" } else { "(is: $currentValue, expected: $expectedValue)" }
        $results.Add((New-VerifyCheckResult -Status $status -Label "NIC tweak: $displayName" -Detail $detail)) | Out-Null
    }

    foreach ($keyword in @("*GreenEthernet", "*PowerSavingMode")) {
        $kwProp = Get-NetAdapterAdvancedProperty -Name $nic.Name -RegistryKeyword $keyword -ErrorAction SilentlyContinue
        if (-not $kwProp) {
            $results.Add((New-VerifyCheckResult -Status "INFO" -Label "NIC keyword $keyword" -Detail "not exposed on $($nic.Name)")) | Out-Null
            continue
        }

        $currentValue = @($kwProp.RegistryValue) -join ','
        $status = if ($currentValue -eq "0") { "OK" } else { "CHANGED" }
        $detail = if ($status -eq "OK") { "(0)" } else { "(is: $currentValue, expected: 0)" }
        $results.Add((New-VerifyCheckResult -Status $status -Label "NIC keyword $keyword" -Detail $detail)) | Out-Null
    }

    return @($results)
}

function Test-VerifyPowerPlan {
    try {
        $plansOutput = powercfg /list 2>&1 | Out-String
        $activeOutput = powercfg /getactivescheme 2>&1 | Out-String
        $targetGuid = $null
        foreach ($line in ($plansOutput -split "`r?`n")) {
            if ($line -match "([a-fA-F0-9\-]{36}).*CS2 Optimized") {
                $targetGuid = $Matches[1].ToLower()
                break
            }
        }
        if (-not $targetGuid) {
            return New-VerifyCheckResult -Status "MISSING" -Label "Power plan: CS2 Optimized" -Detail "plan not found"
        }
        if ($activeOutput -notmatch "([a-fA-F0-9\-]{36})") {
            return New-VerifyCheckResult -Status "MISSING" -Label "Active power plan" -Detail "could not read active scheme"
        }
        $activeGuid = $Matches[1].ToLower()
        if ($activeGuid -ne $targetGuid) {
            return New-VerifyCheckResult -Status "CHANGED" -Label "Active power plan = CS2 Optimized" -Detail "(active: $activeGuid, expected: $targetGuid)"
        }

        $settingsToVerify = @(
            @{ Label = "CPU max perf state"; Subgroup = $PP_SUB_PROCESSOR; Setting = $PP_PROCTHROTTLEMAX; Expected = 100 },
            @{ Label = "Core parking max"; Subgroup = $PP_SUB_PROCESSOR; Setting = $PP_CPMAXCORES; Expected = 100 },
            @{ Label = "USB selective suspend"; Subgroup = $PP_SUB_USB; Setting = $PP_USBSS; Expected = 1 },
            @{ Label = "Disk idle timeout"; Subgroup = $PP_SUB_DISK; Setting = $PP_DISKIDLE; Expected = 0 },
            @{ Label = "Standby timeout"; Subgroup = $PP_SUB_SLEEP; Setting = $PP_STANDBYIDLE; Expected = 0 },
            @{ Label = "Hibernate timeout"; Subgroup = $PP_SUB_SLEEP; Setting = $PP_HIBERNATEIDLE; Expected = 0 },
            @{ Label = "System cooling"; Subgroup = $PP_SUB_COOLING; Setting = $PP_SYSCOOLPOL; Expected = 1 },
            @{ Label = "PCIe ASPM"; Subgroup = $PP_SUB_PCIE; Setting = $PP_ASPM; Expected = 0 }
        )

        $driftDetails = [System.Collections.Generic.List[string]]::new()
        foreach ($setting in $settingsToVerify) {
            $queryOutput = powercfg /query $targetGuid $setting.Subgroup $setting.Setting 2>&1 | Out-String
            if ($queryOutput -notmatch 'Current AC Power Setting Index:\s*0x([0-9a-fA-F]+)') {
                return New-VerifyCheckResult -Status "MISSING" -Label "Power plan: CS2 Optimized" -Detail "could not read setting '$($setting.Label)'"
            }
            $currentValue = [Convert]::ToInt32($Matches[1], 16)
            if ($currentValue -ne $setting.Expected) {
                $driftDetails.Add("$($setting.Label)=$currentValue (expected $($setting.Expected))") | Out-Null
            }
        }

        if ($driftDetails.Count -gt 0) {
            return New-VerifyCheckResult -Status "CHANGED" -Label "Active power plan = CS2 Optimized" -Detail "($(@($driftDetails) -join '; '))"
        }

        return New-VerifyCheckResult -Status "OK" -Label "Active power plan = CS2 Optimized" -Detail "($activeGuid)"
    } catch {
        return New-VerifyCheckResult -Status "MISSING" -Label "Power plan verification" -Detail "powercfg not readable"
    }
}

function Test-VerifyQosPolicies {
    $results = [System.Collections.Generic.List[object]]::new()
    foreach ($policyName in @("CS2_UDP_Ports", "CS2_App")) {
        try {
            $policy = Get-NetQosPolicy -Name $policyName -ErrorAction SilentlyContinue
            if (-not $policy) {
                $results.Add((New-VerifyCheckResult -Status "MISSING" -Label "QoS policy: $policyName" -Detail "not found")) | Out-Null
                continue
            }
            $dscp = "$($policy.DSCPAction)"
            if ($dscp -eq "46") {
                $results.Add((New-VerifyCheckResult -Status "OK" -Label "QoS policy: $policyName" -Detail "(DSCP 46)")) | Out-Null
            } else {
                $results.Add((New-VerifyCheckResult -Status "CHANGED" -Label "QoS policy: $policyName" -Detail "(DSCP $dscp, expected 46)")) | Out-Null
            }
        } catch {
            $results.Add((New-VerifyCheckResult -Status "MISSING" -Label "QoS policy: $policyName" -Detail "not readable")) | Out-Null
        }
    }
    return @($results)
}

function Test-VerifyDnsConfiguration {
    $results = [System.Collections.Generic.List[object]]::new()
    $targetDnsSets = @(
        [string[]]$CFG_DNS_Cloudflare,
        [string[]]$CFG_DNS_Google
    )

    try {
        $adapters = @(
            Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object {
                $_.Status -eq "Up" -and $_.InterfaceDescription -notmatch $CFG_VirtualAdapterFilter
            }
        )
    } catch {
        $adapters = @()
    }

    if ($adapters.Count -eq 0) {
        $results.Add((New-VerifyCheckResult -Status "MISSING" -Label "DNS verification adapters" -Detail "no active physical adapter found")) | Out-Null
        return @($results)
    }

    foreach ($adapter in $adapters) {
        try {
            $ifIndex = if ($adapter.PSObject.Properties['ifIndex']) { $adapter.ifIndex } else { $adapter.InterfaceIndex }
            $dnsInfo = Get-DnsClientServerAddress -InterfaceIndex $ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue
            $current = if ($dnsInfo -and $dnsInfo.ServerAddresses) { [string[]]@($dnsInfo.ServerAddresses) } else { @() }
            if ($current.Count -eq 0) {
                $results.Add((New-VerifyCheckResult -Status "INFO" -Label "DNS: $($adapter.Name)" -Detail "set to automatic/DHCP")) | Out-Null
                continue
            }

            $matchedSet = $targetDnsSets | Where-Object {
                (@($_).Count -eq $current.Count) -and ((@($_) -join ',') -eq ($current -join ','))
            } | Select-Object -First 1
            if ($matchedSet) {
                $provider = if ((@($matchedSet) -join ',') -eq ($CFG_DNS_Cloudflare -join ',')) { "Cloudflare" } else { "Google" }
                $results.Add((New-VerifyCheckResult -Status "OK" -Label "DNS: $($adapter.Name)" -Detail "($provider = $($current -join ', '))")) | Out-Null
            } else {
                $results.Add((New-VerifyCheckResult -Status "CHANGED" -Label "DNS: $($adapter.Name)" -Detail "(is: $($current -join ', '), expected Cloudflare or Google)")) | Out-Null
            }
        } catch {
            $results.Add((New-VerifyCheckResult -Status "MISSING" -Label "DNS: $($adapter.Name)" -Detail "settings not readable")) | Out-Null
        }
    }

    return @($results)
}

function Test-VerifyTrimConfiguration {
    try {
        $trim = Get-TrimHealthStatus
        if (-not $trim -or @($trim.States).Count -eq 0) {
            return New-VerifyCheckResult -Status "MISSING" -Label "Storage maintenance: TRIM" -Detail "state not readable"
        }

        if ($trim.AnyTrimDisabled) {
            $disabled = @($trim.States | Where-Object { -not $_.TrimEnabled } | ForEach-Object { $_.FileSystem }) -join ', '
            return New-VerifyCheckResult -Status "CHANGED" -Label "Storage maintenance: TRIM" -Detail "(disabled on: $disabled)"
        }

        $detail = $trim.Summary
        if ($trim.RetrimAvailable) {
            $detail += "; retrim available on: $(@($trim.RetrimmableVolumes) -join ', ')"
        }
        return New-VerifyCheckResult -Status "OK" -Label "Storage maintenance: TRIM" -Detail "($detail)"
    } catch {
        return New-VerifyCheckResult -Status "MISSING" -Label "Storage maintenance: TRIM" -Detail "verification failed"
    }
}

function Test-VerifyScheduledTasks {
    $x3d = $null
    try { $x3d = Get-X3DCcdInfo } catch {}
    if (-not $x3d -or -not $x3d.IsX3D -or -not $x3d.DualCCD) {
        return New-VerifyCheckResult -Status "INFO" -Label "Scheduled task: CS2 CCD affinity" -Detail "N/A (not a dual-CCD X3D system)"
    }

    try {
        $task = Get-ScheduledTask -TaskName $CS2_AffinityTaskName -ErrorAction SilentlyContinue
        if (-not $task) {
            return New-VerifyCheckResult -Status "MISSING" -Label "Scheduled task: $CS2_AffinityTaskName" -Detail "not found"
        }
        $healthyStates = @("Ready", "Running")
        $taskState = [string]$task.State
        if ($taskState -notin $healthyStates) {
            return New-VerifyCheckResult -Status "CHANGED" -Label "Scheduled task: $CS2_AffinityTaskName" -Detail "(state: $taskState)"
        }

        $expectedCommand = '%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe'
        $expectedArguments = "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$CS2_AffinityScriptPath`""
        $actions = @($task.Actions)
        $actualCommand = if ($actions.Count -gt 0) { [string]$actions[0].Execute } else { "" }
        $actualArguments = if ($actions.Count -gt 0) { [string]$actions[0].Arguments } else { "" }
        if ($actualCommand -ne $expectedCommand -or $actualArguments -ne $expectedArguments) {
            return New-VerifyCheckResult -Status "CHANGED" -Label "Scheduled task: $CS2_AffinityTaskName" -Detail "(action mismatch: exec='$actualCommand' args='$actualArguments')"
        }
        return New-VerifyCheckResult -Status "OK" -Label "Scheduled task: $CS2_AffinityTaskName" -Detail "(state: $taskState)"
    } catch {
        return New-VerifyCheckResult -Status "MISSING" -Label "Scheduled task: $CS2_AffinityTaskName" -Detail "not readable"
    }
}

function Test-VerifyNvidiaDrsProfile {
    $classPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\$CFG_GUID_Display"
    $hasNvidiaGpu = $false
    if (Test-Path $classPath) {
        $subkeys = Get-ChildItem $classPath -ErrorAction SilentlyContinue |
            Where-Object { $_.PSChildName -match "^\d{4}$" }
        foreach ($subkey in $subkeys) {
            $props = Get-ItemProperty $subkey.PSPath -ErrorAction SilentlyContinue
            if ($props.ProviderName -match "NVIDIA" -or $props.DriverDesc -match "NVIDIA") {
                $hasNvidiaGpu = $true
                break
            }
        }
    }
    if (-not $hasNvidiaGpu) {
        return New-VerifyCheckResult -Status "INFO" -Label "NVIDIA DRS profile" -Detail "N/A (no NVIDIA GPU detected)"
    }
    if (-not (Initialize-NvApiDrs)) {
        return New-VerifyCheckResult -Status "MISSING" -Label "NVIDIA DRS profile" -Detail "nvapi64.dll unavailable"
    }

    try {
        $drsState = @{
            ProfileFound = $false
            MismatchCount = 0
        }
        Invoke-DrsSession -Action {
            param($session)

            $drsProfile = [NvApiDrs]::FindApplicationProfile($session, "cs2.exe")
            if ($drsProfile -eq [IntPtr]::Zero) {
                foreach ($profileName in @("Counter-strike 2", "Counter-Strike 2")) {
                    $drsProfile = [NvApiDrs]::FindProfileByName($session, $profileName)
                    if ($drsProfile -ne [IntPtr]::Zero) { break }
                }
            }
            if ($drsProfile -eq [IntPtr]::Zero) { return }

            $drsState.ProfileFound = $true
            foreach ($setting in $NV_DRS_SETTINGS) {
                [uint32]$currentValue = 0
                $status = [NvApiDrs]::GetDwordSetting($session, $drsProfile, [uint32]$setting.Id, [ref]$currentValue)
                if ($status -ne 0 -or $currentValue -ne [uint32]$setting.Value) {
                    $drsState.MismatchCount++
                }
            }
        }

        if (-not $drsState.ProfileFound) {
            return New-VerifyCheckResult -Status "MISSING" -Label "NVIDIA DRS profile" -Detail "CS2 profile not found"
        }
        if ($drsState.MismatchCount -eq 0) {
            return New-VerifyCheckResult -Status "OK" -Label "NVIDIA DRS profile" -Detail "($($NV_DRS_SETTINGS.Count) settings match)"
        }
        return New-VerifyCheckResult -Status "CHANGED" -Label "NVIDIA DRS profile" -Detail "($($drsState.MismatchCount) setting(s) differ)"
    } catch {
        return New-VerifyCheckResult -Status "MISSING" -Label "NVIDIA DRS profile" -Detail "verification failed"
    }
}

function Update-LastVerifiedTimestamp {
    try {
        $state = if (Test-Path $CFG_StateFile) {
            Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
        } else {
            [PSCustomObject]@{
                mode    = $SCRIPT:Mode
                profile = $SCRIPT:Profile
            }
        }
        $state | Add-Member -NotePropertyName "last_verified" -NotePropertyValue ((Get-Date).ToString("o")) -Force
        Save-JsonAtomic -Data $state -Path $CFG_StateFile
        Set-SecureAcl -Path $CFG_StateFile
    } catch {
        Write-DebugLog "Could not persist last_verified timestamp: $_"
    }
}

function Test-VerifyRuntimeCompatibility {
    if (-not (Test-HostIsWindows)) {
        return [PSCustomObject]@{
            Supported = $false
            Message   = "Verify-Settings is only supported on Windows. Use Windows PowerShell 5.1 on the target host for the full verification pass."
        }
    }

    if (-not (Get-Command Get-Service -ErrorAction SilentlyContinue)) {
        return [PSCustomObject]@{
            Supported = $false
            Message   = "Get-Service is unavailable in this runtime. Run Verify-Settings from Windows PowerShell 5.1 on the optimized machine."
        }
    }

    return [PSCustomObject]@{
        Supported = $true
        Message   = ""
    }
}

function Invoke-VerifySettings {
    $compat = Test-VerifyRuntimeCompatibility
    if (-not $compat.Supported) {
        Write-LogoBanner "Settings Verifier  ·  Read-Only Scan"
        Write-Host "  $([char]0x2139) $($compat.Message)" -ForegroundColor Cyan
        Write-Host "  START.bat -> [6] remains the supported launcher for this verifier." -ForegroundColor DarkGray
        Write-Blank
        return
    }

    Initialize-ScriptDefaults
    Write-LogoBanner "Settings Verifier  ·  Read-Only Scan"
    Write-Host "  Checks whether Windows Updates have reset your optimizations." -ForegroundColor DarkGray
    Write-Host "  $("─" * 60)" -ForegroundColor DarkGray
    Write-Blank

    # ── Counters ──────────────────────────────────────────────────────────────
    Initialize-VerifyCounters

    # ══════════════════════════════════════════════════════════════════════════
    # REGISTRY CHECKS
    # ══════════════════════════════════════════════════════════════════════════

Write-Host "`n  ═══ FULLSCREEN / GAME STORE ═══" -ForegroundColor Cyan

Test-RegistryCheck "HKCU:\System\GameConfigStore" "GameDVR_DXGIHonorFSEWindowsCompatible" 1 "FSE Windows Compatible"
Test-RegistryCheck "HKCU:\System\GameConfigStore" "GameDVR_FSEBehavior" 2 "FSE Behavior"
Test-RegistryCheck "HKCU:\System\GameConfigStore" "GameDVR_FSEBehaviorMode" 2 "FSE Behavior Mode"
Test-RegistryCheck "HKCU:\System\GameConfigStore" "GameDVR_HonorUserFSEBehaviorMode" 1 "Honor User FSE Mode"

Write-Host "`n  ═══ GAME BAR / DVR / GAME MODE ═══" -ForegroundColor Cyan

Test-RegistryCheck "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\GameDVR" "AppCaptureEnabled" 0 "App Capture"
Test-RegistryCheck "HKCU:\SOFTWARE\Microsoft\GameBar" "UseNexusForGameBarEnabled" 0 "Game Bar Nexus"
Test-RegistryCheck "HKLM:\SOFTWARE\Policies\Microsoft\Windows\GameDVR" "AllowGameDVR" 0 "Game DVR Policy"
Test-RegistryCheck "HKCU:\System\GameConfigStore" "GameDVR_Enabled" 0 "Game DVR master switch"
Test-RegistryCheck "HKCU:\SOFTWARE\Microsoft\GameBar" "AllowAutoGameMode" 1 "Auto Game Mode (enabled)"
Test-RegistryCheck "HKCU:\SOFTWARE\Microsoft\GameBar" "AutoGameModeEnabled" 1 "Game Mode (enabled)"

Write-Host "`n  ═══ GPU / DISPLAY ═══" -ForegroundColor Cyan

Test-RegistryCheck "HKLM:\SOFTWARE\Microsoft\Windows\Dwm" "OverlayTestMode" 5 "MPO disabled"
# HAGS is setup-dependent (0 or 2), just show current value
try {
    $hagsVal = (Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers" -Name "HwSchMode" -ErrorAction Stop).HwSchMode
    $hagsLabel = switch ($hagsVal) { 0 {"OFF"} 1 {"OFF (default)"} 2 {"ON"} default {"Unknown ($hagsVal)"} }
    Write-Host "  ✓  INFO      HAGS = $hagsLabel  (setup-dependent, no target value)" -ForegroundColor Cyan
    $Script:_verifyInfoCount++
} catch {
    Write-Host "  ?  MISSING   HAGS key not found" -ForegroundColor DarkGray
    $Script:_verifyMissingCount++
}

# NVIDIA GPU class registry keys — PerfLevelSrc + DisableDynamicPstate (P-state locks)
# Only checked if an NVIDIA GPU is detected in the device class registry
$_nvClassPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\$CFG_GUID_Display"
$_nvKeyPath = $null
if (Test-Path $_nvClassPath) {
    $_nvSubkeys = Get-ChildItem $_nvClassPath -ErrorAction SilentlyContinue |
        Where-Object { $_.PSChildName -match "^\d{4}$" }
    foreach ($_nvKey in $_nvSubkeys) {
        $_nvProps = Get-ItemProperty $_nvKey.PSPath -ErrorAction SilentlyContinue
        if ($_nvProps.ProviderName -match "NVIDIA" -or $_nvProps.DriverDesc -match "NVIDIA") {
            $_nvKeyPath = $_nvKey.PSPath
            break
        }
    }
}
if ($_nvKeyPath) {
    Test-RegistryCheck $_nvKeyPath "PerfLevelSrc" 0x2222 "NVIDIA PerfLevelSrc (P-state: Max Performance)"
    Test-RegistryCheck $_nvKeyPath "DisableDynamicPstate" 1 "NVIDIA DisableDynamicPstate (lock P0)"
} else {
    Write-Host "  ✓  INFO      NVIDIA GPU class key: N/A (no NVIDIA GPU detected)" -ForegroundColor Cyan
    $Script:_verifyInfoCount++
}

Write-Host "`n  ═══ SYSTEM PROFILE / GAMING ═══" -ForegroundColor Cyan

$mmPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile"
Test-RegistryCheck $mmPath "SystemResponsiveness" 10 "SystemResponsiveness"
Test-RegistryCheck $mmPath "NoLazyMode" 1 "MMCSS NoLazyMode (realtime-only)"
# NetworkThrottlingIndex is NOT checked — deliberately left at Windows default (10)
# djdallmann xperf: 0xFFFFFFFF increases NDIS.sys DPC latency vs. default
Test-RegistryCheck "$mmPath\Tasks\Games" "Priority" 6 "Gaming Priority"
Test-RegistryCheck "$mmPath\Tasks\Games" "Scheduling Category" "High" "Gaming Scheduling Category"
Test-RegistryCheck "$mmPath\Tasks\Games" "GPU Priority" 8 "Gaming GPU Priority"
Test-RegistryCheck "HKLM:\SYSTEM\CurrentControlSet\Control\PriorityControl" "Win32PrioritySeparation" 0x2A "Win32PrioritySeparation (short fixed quantum, max foreground boost)"
Test-RegistryCheck "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management" "DisablePagingExecutive" 1 "DisablePagingExecutive"

Write-Host "`n  ═══ TIMER / KERNEL ═══" -ForegroundColor Cyan

Test-RegistryCheck "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\kernel" "GlobalTimerResolutionRequests" 1 "Timer Resolution"

# PowerThrottlingOff — Intel 12th gen+ only (E-core mismatch frametime spikes)
$_intelHybridName = Get-IntelHybridCpuName
if ($null -eq $_intelHybridName) {
    Write-Host "  ?  WARN      Power Throttling: could not detect CPU (skipping Intel-specific check)" -ForegroundColor Yellow
    $Script:_verifyMissingCount++
} elseif ($_intelHybridName) {
    Test-RegistryCheck "HKLM:\SYSTEM\CurrentControlSet\Control\Power\PowerThrottling" "PowerThrottlingOff" 1 "Intel Power Throttling disabled (E-core fix)"
} else {
    Write-Host "  ✓  INFO      Power Throttling: N/A (not Intel 12th gen+)" -ForegroundColor Cyan
    $Script:_verifyInfoCount++
}

Write-Host "`n  ═══ SYSTEM LATENCY TWEAKS ═══" -ForegroundColor Cyan

Test-RegistryCheck "HKLM:\SOFTWARE\Microsoft\FTH" "Enabled" 0 "Fault Tolerant Heap disabled"
Test-RegistryCheck "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Device Installer" "DisableCoInstallers" 1 "PnP co-installers disabled"
Test-RegistryCheck "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\Maintenance" "MaintenanceDisabled" 1 "Automatic Maintenance disabled"
# 0x80000001 stored as DWORD reads back as [int32]-2147483647 in PowerShell
# (0x80000001 literal is [int64]2147483649 — int32/int64 mismatch would fail -eq)
Test-RegistryCheck "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" "NtfsDisableLastAccessUpdate" (-2147483647) "NTFS last-access update disabled"
Test-RegistryCheck "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" "NtfsDisable8dot3NameCreation" 1 "NTFS 8.3 name creation disabled"

Write-Host "`n  ═══ PROCESS PRIORITY ═══" -ForegroundColor Cyan

# IFEO PerfOptions: persistent High CPU priority for cs2.exe (set by Phase 3 Step 10)
$_ifeoPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\cs2.exe\PerfOptions"
Test-RegistryCheck $_ifeoPath "CpuPriorityClass" 3 "CS2 High CPU priority (IFEO PerfOptions)"

Write-Host "`n  ═══ MOUSE ═══" -ForegroundColor Cyan

Test-RegistryCheck "HKCU:\Control Panel\Mouse" "MouseSpeed" "0" "Mouse Acceleration"
Test-RegistryCheck "HKCU:\Control Panel\Mouse" "MouseThreshold1" "0" "Mouse Threshold 1"
Test-RegistryCheck "HKCU:\Control Panel\Mouse" "MouseThreshold2" "0" "Mouse Threshold 2"
Test-RegistryCheck "HKLM:\SYSTEM\CurrentControlSet\Services\mouclass\Parameters" "MouseDataQueueSize" 50 "mouclass kernel queue size (50)"

Write-Host "`n  ═══ NETWORK ═══" -ForegroundColor Cyan

$nicGuid = Get-ActiveNicGuid
if ($nicGuid) {
    $tcpBase = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\$nicGuid"
    Test-RegistryCheck $tcpBase "TcpNoDelay" 1 "Nagle's Algorithm (TcpNoDelay)"
    Test-RegistryCheck $tcpBase "TcpAckFrequency" 1 "TCP Ack Frequency"
} else {
    Write-Host "  ?  MISSING   Active NIC not found — Nagle check skipped" -ForegroundColor Red
    $Script:_verifyMissingCount += 2
}
Test-RegistryCheck "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\QoS" "Do not use NLA" "1" "QoS NLA bypass (DSCP prerequisite)"
# IPv6 intentionally left ENABLED (2026 reversal: Steam prefers IPv6 when faster; disabling forces CGNAT)
Write-Host "  ✓  INFO      IPv6: enabled (intentional — Steam prefers IPv6 when faster)" -ForegroundColor Cyan
$Script:_verifyInfoCount++

Write-Host "`n  ═══ FAST STARTUP ═══" -ForegroundColor Cyan

Test-RegistryCheck "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power" "HiberbootEnabled" 0 "Fast Startup disabled"

Write-Host "`n  ═══ AUDIO ═══" -ForegroundColor Cyan

Test-RegistryCheck "HKCU:\Software\Microsoft\Multimedia\Audio" "UserDuckingPreference" 3 "Audio ducking disabled"

Write-Host "`n  ═══ OVERLAY ═══" -ForegroundColor Cyan

Test-RegistryCheck "HKCU:\Software\Valve\Steam" "GameOverlayDisabled" 1 "Steam Overlay disabled"

Write-Host "`n  ═══ VISUAL EFFECTS / WIN11 ═══" -ForegroundColor Cyan

Test-RegistryCheck "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects" "VisualFXSetting" 2 "Visual Effects (Best Performance)"
# UserPreferencesMask: inline binary comparison via Compare-Object -SyncWindow 0.
# -SyncWindow 0 = positional (element-by-element) comparison, not set-like.
# Test-RegistryCheck uses -eq which is reference equality for byte[] (always false).
# This is the only binary check in Verify-Settings; extract to a helper if more are added.
try {
    $upmDesktop = "HKCU:\Control Panel\Desktop"
    $upmVal = (Get-ItemProperty -Path $upmDesktop -Name "UserPreferencesMask" -ErrorAction Stop).UserPreferencesMask
    $upmExpected = [byte[]](0x90,0x12,0x03,0x80,0x10,0x00,0x00,0x00)
    # Win11 may return 12 bytes; compare only the first $upmExpected.Length bytes
    $upmTrimmed = if ($upmVal -and $upmVal.Length -ge $upmExpected.Length) { $upmVal[0..($upmExpected.Length - 1)] } else { $upmVal }
    if ($null -eq $upmVal) {
        Write-Host "  ?  MISSING   UserPreferencesMask (Best Performance + ClearType)" -ForegroundColor Red
        Write-Host "               $upmDesktop\UserPreferencesMask" -ForegroundColor DarkGray
        $Script:_verifyMissingCount++
    } elseif (@(Compare-Object $upmTrimmed $upmExpected -SyncWindow 0).Count -eq 0) {
        Write-Host "  ✓  OK        UserPreferencesMask (Best Performance + ClearType)" -ForegroundColor Green
        $Script:_verifyOkCount++
    } else {
        $hexVal = ($upmVal | ForEach-Object { '{0:X2}' -f $_ }) -join ' '
        Write-Host "  ✗  CHANGED   UserPreferencesMask (is: $hexVal, expected: 90 12 03 80 10 00 00 00)" -ForegroundColor Yellow
        Write-Host "               $upmDesktop\UserPreferencesMask" -ForegroundColor DarkGray
        $Script:_verifyChangedCount++
    }
} catch {
    Write-Host "  ?  MISSING   UserPreferencesMask (Best Performance + ClearType)" -ForegroundColor Red
    Write-Host "               HKCU:\Control Panel\Desktop\UserPreferencesMask" -ForegroundColor DarkGray
    $Script:_verifyMissingCount++
}
Test-RegistryCheck "HKCU:\Control Panel\Desktop" "FontSmoothing" "2" "ClearType font smoothing enabled"
Test-RegistryCheck "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\VideoSettings" "AutoHDREnabled" 0 "Win11 Auto HDR disabled"

# ══════════════════════════════════════════════════════════════════════════════
# BCDEDIT CHECKS
# ══════════════════════════════════════════════════════════════════════════════

Write-Host "`n  ═══ BOOT CONFIG (bcdedit) ═══" -ForegroundColor Cyan

try {
    # Use /v flag to get raw hex element IDs instead of localized key names.
    # Without /v, "disabledynamictick" is localized (e.g., German: "Dynamischer Tick deaktiviert")
    # and English-only matching would fail on non-English Windows.
    $bcdOutput = bcdedit /enum "{current}" /v 2>&1 | Out-String
    # Match on hex element IDs (locale-independent) + any truthy value.
    # Boolean values are localized (Yes/Ja/Oui/Sí/Да/是/Tak/etc.) — hex IDs are not.
    # 0x26000060 = disabledynamictick, 0x26000092 = useplatformtick
    if ($bcdOutput -match "0x26000060\s+\S+") {
        Write-Host "  ✓  OK        Dynamic Tick disabled" -ForegroundColor Green
        $Script:_verifyOkCount++
    } else {
        Write-Host "  ✗  CHANGED   Dynamic Tick is ACTIVE (expected: disabled)" -ForegroundColor Yellow
        $Script:_verifyChangedCount++
    }
    if ($bcdOutput -match "0x26000092\s+\S+") {
        Write-Host "  ✓  OK        Platform Tick active" -ForegroundColor Green
        $Script:_verifyOkCount++
    } else {
        Write-Host "  ✗  CHANGED   Platform Tick is INACTIVE (expected: active)" -ForegroundColor Yellow
        $Script:_verifyChangedCount++
    }
} catch {
    Write-Host "  ?  MISSING   bcdedit not readable" -ForegroundColor Red
    $Script:_verifyMissingCount += 2
}

# ══════════════════════════════════════════════════════════════════════════════
# SERVICE CHECKS
# ══════════════════════════════════════════════════════════════════════════════

Write-Host "`n  ═══ SERVICES ═══" -ForegroundColor Cyan

Test-ServiceCheck "SysMain" "Disabled" "SysMain (Superfetch)"
Test-ServiceCheck "WSearch" "Disabled" "Windows Search"
Test-ServiceCheck "qWave"   "Disabled" "qWave (QoS network probes)"
foreach ($xSvc in $CFG_XboxServices) {
    $xLabel = if ($xSvc -eq "XboxGipSvc") {
        "$xSvc (Xbox wireless accessories — re-enable if using Xbox controller/headset)"
    } else {
        "$xSvc (Xbox background service)"
    }
    Test-ServiceCheck $xSvc "Disabled" $xLabel
}
# Windows Update services (Step 15, CRITICAL risk — most users skip this step)
# Only report if they were actually disabled; if still at default, show INFO not MISSING.
foreach ($_wuSvc in @("wuauserv", "UsoSvc", "WaaSMedicSvc")) {
    $_wuObj = Get-Service $_wuSvc -ErrorAction SilentlyContinue
    if ($_wuObj -and $_wuObj.StartType -eq 'Disabled') {
        Write-Host "  ✓  OK        $_wuSvc = Disabled (Step 15 — Windows Update Blocker)" -ForegroundColor Green
        $Script:_verifyOkCount++
    } elseif ($_wuObj) {
        # Not disabled = user likely skipped Step 15 (expected for most users)
        Write-Host "  ✓  INFO      $_wuSvc = $($_wuObj.StartType) (Step 15 skipped — normal)" -ForegroundColor Cyan
        $Script:_verifyInfoCount++
    }
}

# ══════════════════════════════════════════════════════════════════════════════
# EXTENDED VERIFICATION
# ══════════════════════════════════════════════════════════════════════════════

Write-Host "`n  ═══ NIC ADVANCED PROPERTIES ═══" -ForegroundColor Cyan
foreach ($result in @(Test-VerifyNicAdvancedProperties)) {
    Write-VerifyCheckResult $result
}

Write-Host "`n  ═══ POWER PLAN ═══" -ForegroundColor Cyan
Write-VerifyCheckResult (Test-VerifyPowerPlan)

Write-Host "`n  ═══ QOS POLICIES ═══" -ForegroundColor Cyan
foreach ($result in @(Test-VerifyQosPolicies)) {
    Write-VerifyCheckResult $result
}

Write-Host "`n  ═══ DNS CONFIGURATION ═══" -ForegroundColor Cyan
foreach ($result in @(Test-VerifyDnsConfiguration)) {
    Write-VerifyCheckResult $result
}

Write-Host "`n  ═══ STORAGE HEALTH ═══" -ForegroundColor Cyan
Write-VerifyCheckResult (Test-VerifyTrimConfiguration)

Write-Host "`n  ═══ SCHEDULED TASKS ═══" -ForegroundColor Cyan
Write-VerifyCheckResult (Test-VerifyScheduledTasks)

Write-Host "`n  ═══ NVIDIA DRS PROFILE ═══" -ForegroundColor Cyan
Write-VerifyCheckResult (Test-VerifyNvidiaDrsProfile)

# ══════════════════════════════════════════════════════════════════════════════
# SUMMARY
# ══════════════════════════════════════════════════════════════════════════════

$counts = Get-VerifyCounters
$total = $counts.okCount + $counts.changedCount + $counts.missingCount
Write-Blank
Write-Host "  $(("$([char]0x2550)") * 60)" -ForegroundColor DarkGray
Write-Host "  VERIFICATION RESULT:  $total settings checked$(if($counts.infoCount){", $($counts.infoCount) info"})" -ForegroundColor White
Write-Host "  $([char]0x2714) OK:       $($counts.okCount)  — working as intended" -ForegroundColor Green
Write-Host "  $([char]0x2718) CHANGED:  $($counts.changedCount)  — something reset these" -ForegroundColor Yellow
Write-Host "  ?  MISSING:  $($counts.missingCount)  — not applied yet" -ForegroundColor Red
Write-Host "  $(("$([char]0x2550)") * 60)" -ForegroundColor DarkGray

if ($counts.changedCount -gt 0 -or $counts.missingCount -gt 0) {
    Write-Blank
    if ($counts.changedCount -gt 0) {
        Write-Host "  $([char]0x26A0) Windows Update likely reset $($counts.changedCount) of your optimizations." -ForegroundColor Yellow
        Write-Host "    This is normal — Windows does this after major updates." -ForegroundColor DarkGray
    }
    if ($counts.missingCount -gt 0) {
        Write-Host "  $([char]0x2139) $($counts.missingCount) setting(s) were never applied (you may have skipped those steps)." -ForegroundColor DarkGray
    }
    Write-Host ""
    Write-Host "  $([char]0x2139) What to do: Run Phase 1 again (START.bat -> [1])." -ForegroundColor Cyan
    Write-Host "    It will detect your previous progress and only re-apply what changed." -ForegroundColor DarkGray
} else {
    Write-Blank
    Write-Host "  $([char]0x2714) All settings intact — your optimizations are still active!" -ForegroundColor Green
}

    Update-LastVerifiedTimestamp
    Write-Blank
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-VerifySettings
}
