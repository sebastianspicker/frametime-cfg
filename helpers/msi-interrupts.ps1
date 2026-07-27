# ==============================================================================
#  helpers/msi-interrupts.ps1  -  MSI Interrupts + NIC Interrupt Affinity
# ==============================================================================

function Get-InterruptOperationResult {
    param(
        [Parameter(Mandatory)][string]$Status,
        [Parameter(Mandatory)][bool]$CanCompleteStep,
        [Parameter(Mandatory)][string]$Message,
        [bool]$Applied = $false
    )

    return [PSCustomObject]@{
        Status          = $Status
        Applied         = $Applied
        CanCompleteStep = $CanCompleteStep
        Message         = $Message
    }
}

function Enable-DeviceMSI {
    <#
    .SYNOPSIS  Requests Message Signaled Interrupts (MSI) for selected PCI devices.
    .DESCRIPTION
        Writes the Windows registry policy used by capable GPU, NIC, and audio
        devices. The effective interrupt mode is negotiated after device
        initialization and is not established by the registry value alone.
    #>
    [CmdletBinding()]
    param()

    Write-Step "Writing MSI support policy for PCI devices..."

    $fullDryRunActive = (Get-Variable -Name FullDryRun -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:FullDryRun
    if ($SCRIPT:DryRun -and $fullDryRunActive) {
        Write-ConsoleLine "  [DRY-RUN] Would enumerate PCI display, network, and audio devices." -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN] Would request MSI support and set the GPU vector-limit policy." -ForegroundColor Magenta
        return (Get-InterruptOperationResult -Status "DryRun" -CanCompleteStep $true -Message "MSI device discovery and registry writes previewed.")
    }

    $deviceClasses = @(
        @{ Class = "Display";       Label = "GPU";     MsiLimit = 16 },  # Suite default heuristic, not a universal vendor recommendation
        @{ Class = "Net";           Label = "NIC";     MsiLimit = 0  },  # Default (no vector limit)
        @{ Class = "Media";        Label = "Audio";   MsiLimit = 0  }   # Default (no vector limit)
    )

    $modified = 0
    $writeResults = [System.Collections.Generic.List[object]]::new()
    foreach ($dc in $deviceClasses) {
        $devices = Get-PnpDevice -Class $dc.Class -Status OK -ErrorAction SilentlyContinue
        if (-not $devices) {
            Write-Warn "No $($dc.Label) devices found in class $($dc.Class) - skipping."
            continue
        }

        foreach ($dev in $devices) {
            # Skip virtual and software devices
            if ($dev.InstanceId -notmatch "^PCI\\") { continue }

            $regBase = "HKLM:\SYSTEM\CurrentControlSet\Enum\$($dev.InstanceId)"
            $msiPath = "$regBase\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties"

            # Route through Set-RegistryValue for consistent DRY-RUN interception and auto-backup
            $msiWrite = Set-RegistryValue $msiPath "MSISupported" 1 "DWord" `
                "Request MSI support for $($dc.Label): $($dev.FriendlyName)" -PassThru
            $writeResults.Add($msiWrite)

            # Set MSI vector count limit for GPU using the suite's default heuristic.
            # Windows documents the key, but the exact value remains workload/device-specific.
            if ($dc.MsiLimit -gt 0) {
                $limitWrite = Set-RegistryValue $msiPath "MessageNumberLimit" $dc.MsiLimit "DWord" `
                    "MSI vector limit ($($dc.MsiLimit)) for $($dc.Label): $($dev.FriendlyName)" -PassThru
                $writeResults.Add($limitWrite)
            }

            $modified++
        }
    }

    if ($modified -eq 0) {
        Write-Warn "No applicable PCI devices were found for the MSI policy."
        return (Get-InterruptOperationResult -Status "Skipped" -CanCompleteStep $false -Message "No applicable PCI devices were found.")
    }

    $failedWrites = @($writeResults | Where-Object { $null -eq $_ -or $_.Status -notin @("Success", "DryRun") })
    if ($failedWrites.Count -gt 0) {
        $message = "$($failedWrites.Count) required MSI registry write(s) failed or were not applied."
        Write-Warn $message
        return (Get-InterruptOperationResult -Status "Failed" -CanCompleteStep $false -Message $message)
    }

    if ($SCRIPT:DryRun) {
        return (Get-InterruptOperationResult -Status "DryRun" -CanCompleteStep $true -Message "MSI registry writes previewed for $modified device(s).")
    }

    Write-OK "MSI registry policy verified for $modified device(s)."
    Write-Info "Restart, then verify the negotiated interrupt mode for each device."
    return (Get-InterruptOperationResult -Status "Success" -CanCompleteStep $true -Applied $true -Message "MSI registry writes applied for $modified device(s).")
}

function Set-NicRssConfig {
    <#
    .SYNOPSIS  Adds missing RSS (Receive Side Scaling) registry entries to NIC driver key.
    .DESCRIPTION
        Some NIC drivers omit explicit RSS registry entries. This helper writes a
        repository-defined policy only for values that are absent. Effective RSS
        behavior depends on the adapter, driver, Windows, and processor topology.

        This function adds *RSSProfile, *RssBaseProcNumber, *MaxRssProcessors, *NumRssQueues
        ONLY if they are absent from the driver key. Existing values are never overwritten
        (respects manual or driver-written configuration).

        The selected queue and processor values are heuristics. The repository does
        not contain an NDIS trace establishing a general latency result.
    #>

    [CmdletBinding(SupportsShouldProcess)]
    param()

    Write-Step "Configuring NIC RSS (Receive Side Scaling) distribution..."

    $nic = Get-ActiveNicAdapter

    if (-not $nic) {
        Write-Warn "RSS: no active wired NIC found - skipping."
        return
    }

    # Locate the NIC's driver subkey under the Network class GUID.
    # Each installed NIC driver is registered as a numbered subkey (0000, 0001, ...)
    # under HKLM:\...\Control\Class\{4d36e972-...}. DriverDesc matches the adapter
    # description reported by Get-NetAdapter.
    $classPath    = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\$CFG_GUID_Network"
    $driverKey    = $null

    try {
        $subkeys = Get-ChildItem $classPath -ErrorAction Stop |
            Where-Object { $_.PSChildName -match "^\d{4}$" }
    } catch {
        # Fallback: parent-level ACL may block Get-ChildItem but individual subkeys
        # are often accessible. Try numbered subkeys 0000–0030 directly.
        Write-DebugLog "RSS: Get-ChildItem failed ($_), trying direct subkey access..."
        $subkeys = @()
        for ($i = 0; $i -le 30; $i++) {
            $subPath = "$classPath\$($i.ToString('D4'))"
            try { $subkeys += Get-Item $subPath -ErrorAction Stop } catch { $null = $_ }
        }
        if (-not $subkeys) {
            Write-Warn "RSS: could not access network class registry keys - skipping RSS configuration."
            Write-Sub "This does not affect other NIC optimizations (adapter properties, URO, QoS)."
            return
        }
        Write-DebugLog "RSS: fallback found $($subkeys.Count) subkey(s)"
    }

    # Prefer exact match, fallback to substring only if no exact match found
    $substringMatch = $null
    foreach ($key in $subkeys) {
        $desc = (Get-ItemProperty $key.PSPath -Name "DriverDesc" -ErrorAction SilentlyContinue).DriverDesc
        if ($desc -and $desc -eq $nic.InterfaceDescription) {
            $driverKey = $key.PSPath
            break
        }
        if (-not $substringMatch -and $desc -and (
            $nic.InterfaceDescription -like "*$desc*" -or
            $desc -like "*$($nic.InterfaceDescription)*")) {
            $substringMatch = $key.PSPath
        }
    }
    if (-not $driverKey -and $substringMatch) {
        $driverKey = $substringMatch
        Write-DebugLog "RSS: using substring match for NIC driver key (no exact match found)"
    }

    if (-not $driverKey) {
        Write-Warn "RSS: driver registry key not found for '$($nic.InterfaceDescription)' - skipping."
        return
    }

    Write-Info "RSS NIC: $($nic.InterfaceDescription)"
    if (-not $PSCmdlet.ShouldProcess($driverKey, "Configure NIC RSS registry values for $($nic.InterfaceDescription)")) {
        return
    }

    # Request the RSS master switch before writing sub-parameters. Driver behavior
    # for an absent keyword is adapter-specific.
    $rssKeyword = Get-ItemProperty $driverKey -Name "*RSS" -ErrorAction SilentlyContinue
    $rssValue = if ($null -ne $rssKeyword) { $rssKeyword."*RSS" } else { $null }
    if ($null -eq $rssValue) {
        Set-RegistryValue $driverKey "*RSS" 1 "DWord" "Enable RSS master switch (was absent)"
        Write-Info "RSS master switch (*RSS) was absent - created and enabled. Sub-parameters below require this."
    } elseif ([int]$rssValue -eq 0) {
        Set-RegistryValue $driverKey "*RSS" 1 "DWord" "Enable RSS master switch (was disabled)"
        Write-Info "RSS master switch (*RSS) was 0 - enabled. Sub-parameters below require this."
    }

    # Repository queue-count heuristic based on reported link speed. This does
    # not establish the adapter's supported or optimal queue count.
    $rssQueueCount = 4
    if ($nic.Speed -and $nic.Speed -ge 5000000000) {
        $rssQueueCount = 8
        Write-Info "RSS: 5+ GbE NIC detected ($('{0:N1}' -f ($nic.Speed / 1e9)) Gbps) - using $rssQueueCount queues"
    }

    # Validate RSS settings against actual processor count to avoid assigning
    # queues to non-existent cores (causes driver errors or silent fallback).
    $processorCount = [Environment]::ProcessorCount
    $rssBaseProcNumber = 2
    if ($processorCount -le 2) {
        $rssBaseProcNumber = 0
        $rssQueueCount = [math]::Max(1, $processorCount)
        Write-Info "RSS: Only $processorCount logical processors - base proc 0, $rssQueueCount queue(s)"
    } elseif ($processorCount - $rssBaseProcNumber -lt $rssQueueCount) {
        $rssBaseProcNumber = 1
        Write-Info "RSS: Only $processorCount logical processors - reduced base proc to $rssBaseProcNumber"
        if ($processorCount - $rssBaseProcNumber -lt $rssQueueCount) {
            $rssQueueCount = [math]::Max(1, $processorCount - $rssBaseProcNumber)
            Write-Info "RSS: Clamped queue count to $rssQueueCount (processor count: $processorCount)"
        }
    }

    # RSS entries and their rationale:
    #
    #   *RSSProfile = 1 (ClosestProcessor)
    #     Requests the Windows ClosestProcessor RSS profile. Effective placement is
    #     determined by Windows and the adapter driver.
    #
    #   *RssBaseProcNumber = 2
    #     Requests the first logical processor for RSS queues. Starting above zero
    #     is a repository heuristic and must be checked against the target topology.
    #
    #   *MaxRssProcessors = 4 (or 8 for 5+ GbE)
    #     Requests the upper bound selected by the repository heuristic.
    #
    #   *NumRssQueues = 4 (or 8 for 5+ GbE)
    #     Requests a queue count matching MaxRssProcessors.
    #
    $rssDefaults = [ordered]@{
        "*RSSProfile"        = @{ Value = 1; Type = "DWord";
            Note = "Request the Windows ClosestProcessor RSS profile" }
        "*RssBaseProcNumber" = @{ Value = $rssBaseProcNumber; Type = "DWord";
            Note = "Request RSS base processor $rssBaseProcNumber (suite heuristic)" }
        "*MaxRssProcessors"  = @{ Value = $rssQueueCount; Type = "DWord";
            Note = "Cap RSS spread at $rssQueueCount processors" }
        "*NumRssQueues"      = @{ Value = $rssQueueCount; Type = "DWord";
            Note = "$rssQueueCount RSS queues matching processor cap" }
    }

    $added   = 0
    $skipped = 0

    foreach ($entry in $rssDefaults.GetEnumerator()) {
        $existing = Get-ItemProperty -LiteralPath $driverKey -Name $entry.Key -ErrorAction SilentlyContinue
        if ($null -ne $existing -and $null -ne $existing.($entry.Key)) {
            Write-Sub "$($entry.Key) = $($existing.($entry.Key)) (already set - preserved)"
            $skipped++
        } else {
            Set-RegistryValue $driverKey $entry.Key $entry.Value.Value $entry.Value.Type $entry.Value.Note
            $added++
        }
    }

    if ($added -gt 0) {
        Write-OK "RSS: added $added missing entries ($skipped existing preserved). Restart required."
        Write-Info "Requested RSS queue distribution through the added registry values."
        # Report model-pattern matches without claiming a vendor default.
        if ($nic.InterfaceDescription -match "I225|I226|I219") {
            Write-Info "Intel $($Matches[0]) pattern detected; missing RSS entries were added."
        } elseif ($nic.InterfaceDescription -match "RTL8125|RTL8126|Realtek.*Gaming.*2\.5|Realtek.*5\s*GbE") {
            Write-Info "Realtek $($Matches[0]) detected - missing RSS entries were added."
        }
    } else {
        Write-OK "RSS: all $skipped entries already present - no changes needed."
    }
}

function Set-NicInterruptAffinity {
    <#
    .SYNOPSIS  Sets NIC interrupt affinity using the suite's last-core heuristic (avoids Core 0).
    .DESCRIPTION
        Implements the repository's native registry policy. Its scope is not
        claimed to match third-party affinity tools.
    #>
    [CmdletBinding(SupportsShouldProcess)]
    param()

    Write-Step "Setting NIC interrupt affinity..."

    $fullDryRunActive = (Get-Variable -Name FullDryRun -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:FullDryRun
    if ($SCRIPT:DryRun -and $fullDryRunActive) {
        Write-ConsoleLine "  [DRY-RUN] Would identify the active wired PCI NIC and CPU topology." -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN] Would calculate a non-Core-0 interrupt mask and write its affinity policy." -ForegroundColor Magenta
        return (Get-InterruptOperationResult -Status "DryRun" -CanCompleteStep $true -Message "NIC interrupt affinity discovery and writes previewed.")
    }

    # Use the active wired adapter (consistent with Set-NicRssConfig)
    $activeNic = Get-ActiveNicAdapter
    if (-not $activeNic) {
        Write-Warn "No active wired NIC found - skipping affinity."
        return (Get-InterruptOperationResult -Status "Skipped" -CanCompleteStep $false -Message "No active wired NIC was found.")
    }

    # Match PnP device for registry path.
    # NIC FriendlyName may include an instance suffix (e.g., "Intel I226-V #2") that
    # differs from InterfaceDescription. Try exact match first, then substring match
    # in both directions, then match by PCI hardware path segment as last resort.
    $friendlyName = $activeNic.InterfaceDescription
    $allPciNics = @(Get-PnpDevice -Class Net -Status OK -ErrorAction SilentlyContinue |
        Where-Object { $_.InstanceId -match "^PCI\\" })

    # Strategy 1: Exact match
    $nic = $allPciNics | Where-Object { $_.FriendlyName -eq $friendlyName } | Select-Object -First 1

    if (-not $nic) {
        # Strategy 2: Substring match (either direction) - handles instance suffixes like "#2"
        $nic = $allPciNics |
            Where-Object { $friendlyName -like "*$($_.FriendlyName)*" -or $_.FriendlyName -like "*$friendlyName*" } |
            Select-Object -First 1
    }

    if (-not $nic) {
        # Strategy 3: Match by PCI hardware path segment - extracts VEN/DEV from InstanceId
        # and compares against the active NIC's PnP device ID (most reliable for multi-instance NICs)
        try {
            $activeHwId = (Get-NetAdapter -Name $activeNic.Name -ErrorAction SilentlyContinue).PnpDeviceID
            if ($activeHwId) {
                $nic = $allPciNics |
                    Where-Object { $activeHwId -eq $_.InstanceId } |
                    Select-Object -First 1
            }
        } catch { Write-DebugLog "PCI path matching failed for NIC affinity: $_" }
    }

    if (-not $nic) {
        Write-Warn "Active NIC '$($activeNic.InterfaceDescription)' not found as PCI device - skipping affinity."
        return (Get-InterruptOperationResult -Status "Skipped" -CanCompleteStep $false -Message "The active NIC was not found as a PCI device.")
    }

    # Ensure single device (not array)
    if ($nic -is [array]) { $nic = $nic | Select-Object -First 1 }

    # Get physical core count (need physical, not logical, for correct core targeting)
    try {
        $coreCount = (Get-CachedCpuInfo).NumberOfCores
        if (-not $coreCount) { throw "NumberOfCores returned null" }
    } catch {
        # Fallback: estimate physical cores as half of logical count
        $logicalCount = [Environment]::ProcessorCount
        $coreCount = [math]::Max(2, [math]::Floor($logicalCount / 2))
        Write-DebugLog "CIM failed - using $coreCount logical processors as core count"
    }

    if ($coreCount -lt 2) {
        Write-Warn "Only 1 core detected - cannot set affinity away from Core 0."
        return (Get-InterruptOperationResult -Status "Skipped" -CanCompleteStep $false -Message "At least two physical cores are required.")
    }

    # Calculate affinity mask for target core.
    # On detected hybrid CPUs, the suite heuristically targets the last-core region.
    # On other CPUs it uses the last reported physical core. This is not a
    # topology measurement or a Microsoft recommendation.
    # Clamp to 63 - processor group 0 supports max 64 logical processors;
    # systems with >64 LPs require GROUP_AFFINITY which needs a different API.
    # Select the last reported physical core as the repository heuristic.
    $hybridCpu = Get-IntelHybridCpuName
    if ($null -eq $hybridCpu) { Write-DebugLog "CPU hybrid detection returned null - defaulting to non-hybrid" }
    # SMT-aware affinity: on AMD sequential SMT, physical core N maps to LP N (thread 0)
    # and LP N+coreCount (thread 1). We target the last physical core's LP(s).
    $logicalCount = [Environment]::ProcessorCount
    $smtEnabled   = ($logicalCount -gt $coreCount)
    $targetLP     = [math]::Min($coreCount - 1, 63)
    $mask         = [uint64]1 -shl $targetLP
    # SMT sibling calculation depends on CPU topology:
    # - AMD: sequential - core N threads at LP N and LP N+coreCount
    # - Intel (non-hybrid): interleaved - core N threads at LP 2N and LP 2N+1
    # - Hybrid (Intel 12th+): E-cores have no SMT sibling
    $vendor = Get-ChipsetVendor
    $siblingLP = -1
    $physCoreIdx = $targetLP   # preserve before possible mutation
    if ($smtEnabled -and (-not $hybridCpu)) {
        if ($vendor -eq "AMD") {
            $siblingLP = $targetLP + $coreCount
        } else {
            # Intel interleaved: core's two threads are adjacent (2N, 2N+1)
            # targetLP is the physical core index; in interleaved layout, its threads
            # are at LP targetLP*2 and LP targetLP*2+1. But we need the actual LP index.
            # Since targetLP = coreCount-1 and logical = coreCount*2, the sibling is targetLP+coreCount
            # for sequential, or for interleaved: if targetLP is even, sibling = targetLP+1; if odd, targetLP-1.
            # However, the $targetLP we computed is a physical core index mapped to LP space.
            # For interleaved Intel: thread 0 of core N = LP 2N, thread 1 = LP 2N+1
            $primaryLP = $targetLP * 2
            if ($primaryLP -lt 64) {
                $mask = [uint64]1 -shl $primaryLP
                $targetLP = $primaryLP
                $siblingLP = $primaryLP + 1
            }
        }
        if ($siblingLP -ge 0 -and $siblingLP -lt 64) {
            $mask = $mask -bor ([uint64]1 -shl $siblingLP)
        }
    }
    if ($hybridCpu) { Write-DebugLog "Hybrid CPU ($hybridCpu) - applying suite heuristic: target last-core/E-core region for NIC affinity" }

    # Convert mask to 8-byte array for registry (binary value)
    $maskBytes = [BitConverter]::GetBytes([uint64]$mask)

    $regBase = "HKLM:\SYSTEM\CurrentControlSet\Enum\$($nic.InstanceId)"
    $affinityPath = "$regBase\Device Parameters\Interrupt Management\Affinity Policy"
    if (-not $PSCmdlet.ShouldProcess($affinityPath, "Configure NIC interrupt affinity for $($nic.FriendlyName)")) {
        return (Get-InterruptOperationResult -Status "Skipped" -CanCompleteStep $false -Message "NIC interrupt affinity was not approved.")
    }

    # Route DevicePolicy through Set-RegistryValue for consistent DRY-RUN/backup handling
    $policyWrite = Set-RegistryValue $affinityPath "DevicePolicy" 4 "DWord" `
        "NIC interrupt affinity policy (Specified Processors): $($nic.FriendlyName)" -PassThru

    # AssignmentSetOverride is Binary - Set-RegistryValue supports this via -Type passthrough
    $lpIndices = @($targetLP)
    if ($siblingLP -ge 0 -and $siblingLP -le 63) { $lpIndices += $siblingLP }
    $lpLabel = ($lpIndices | ForEach-Object { "LP $_" }) -join ", "
    $maskWrite = Set-RegistryValue $affinityPath "AssignmentSetOverride" ([byte[]]$maskBytes) "Binary" `
        "NIC affinity mask 0x$($mask.ToString('X')) -> ${lpLabel}: $($nic.FriendlyName)" -PassThru

    $writeResults = @($policyWrite, $maskWrite)
    $failedWrites = @($writeResults | Where-Object { $null -eq $_ -or $_.Status -notin @("Success", "DryRun") })
    if ($failedWrites.Count -gt 0) {
        $message = "$($failedWrites.Count) required NIC affinity registry write(s) failed or were not applied."
        Write-Warn $message
        return (Get-InterruptOperationResult -Status "Failed" -CanCompleteStep $false -Message $message)
    }

    if ($SCRIPT:DryRun) {
        return (Get-InterruptOperationResult -Status "DryRun" -CanCompleteStep $true -Message "NIC affinity registry writes previewed.")
    }

    Write-OK "NIC affinity registry policy written: $($nic.FriendlyName) -> $lpLabel (physical core $physCoreIdx)"
    Write-Info "Affinity mask: 0x$($mask.ToString('X')) ($lpLabel of $logicalCount LPs, $coreCount physical cores$(if($smtEnabled){', SMT enabled'}))"
    Write-Info "Restart, then verify the effective affinity on the target device."
    return (Get-InterruptOperationResult -Status "Success" -CanCompleteStep $true -Applied $true -Message "NIC interrupt affinity applied and verified by registry write results.")
}
