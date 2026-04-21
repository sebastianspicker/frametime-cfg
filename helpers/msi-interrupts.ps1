# ==============================================================================
#  helpers/msi-interrupts.ps1  —  MSI Interrupts + NIC Interrupt Affinity
# ==============================================================================

function Enable-DeviceMSI {
    <#
    .SYNOPSIS  Enables Message Signaled Interrupts (MSI) for GPU, NIC, and Audio
               devices via direct registry writes. Replaces MSI Utility v3.
    #>
    [CmdletBinding()]
    param()

    Write-Step "Enabling MSI interrupts for PCI devices..."

    $deviceClasses = @(
        @{ Class = "Display";       Label = "GPU";     MsiLimit = 16 },  # Suite default heuristic, not a universal vendor recommendation
        @{ Class = "Net";           Label = "NIC";     MsiLimit = 0  },  # Default (no vector limit)
        @{ Class = "Media";        Label = "Audio";   MsiLimit = 0  }   # Default (no vector limit)
    )

    $modified = 0
    foreach ($dc in $deviceClasses) {
        $devices = Get-PnpDevice -Class $dc.Class -Status OK -ErrorAction SilentlyContinue
        if (-not $devices) {
            Write-Warn "No $($dc.Label) devices found in class $($dc.Class) — skipping."
            continue
        }

        foreach ($dev in $devices) {
            # Skip virtual and software devices
            if ($dev.InstanceId -notmatch "^PCI\\") { continue }

            $regBase = "HKLM:\SYSTEM\CurrentControlSet\Enum\$($dev.InstanceId)"
            $msiPath = "$regBase\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties"

            # Route through Set-RegistryValue for consistent DRY-RUN interception and auto-backup
            Set-RegistryValue $msiPath "MSISupported" 1 "DWord" `
                "MSI enabled for $($dc.Label): $($dev.FriendlyName)"

            # Set MSI vector count limit for GPU using the suite's default heuristic.
            # Windows documents the key, but the exact value remains workload/device-specific.
            if ($dc.MsiLimit -gt 0) {
                Set-RegistryValue $msiPath "MessageNumberLimit" $dc.MsiLimit "DWord" `
                    "MSI vector limit ($($dc.MsiLimit)) for $($dc.Label): $($dev.FriendlyName)"
            }

            $modified++
        }
    }

    if ($modified -eq 0) {
        Write-Warn "No PCI devices found to enable MSI."
    } else {
        Write-OK "$modified device(s) verified for MSI mode."
        if (-not $SCRIPT:DryRun) {
            Write-Info "Restart required for MSI changes to take effect."
        }
    }
}

function Set-NicRssConfig {
    <#
    .SYNOPSIS  Adds missing RSS (Receive Side Scaling) registry entries to NIC driver key.
    .DESCRIPTION
        Many NIC drivers — notably Intel I225-V and I226-V (shipped as the onboard NIC on
        most Ryzen 5000/7000 and Intel 12th+ gen boards) — omit RSS indirection table
        registry entries. Without them, all receive-side interrupt processing defaults to
        Core 0. When Core 0 is also handling game threads, NIC DPCs contend with them,
        producing irregular packet delivery timing (jitter).

        This function adds *RSSProfile, *RssBaseProcNumber, *MaxRssProcessors, *NumRssQueues
        ONLY if they are absent from the driver key. Existing values are never overwritten
        (respects manual or driver-written configuration).

        Source: djdallmann/GamingPCSetup — "many vendor drivers omit these entries;
        Intel I219-V missing these by default; notable NDIS DPC latency improvements
        vs. default Core 0 allocation."
    #>

    Write-Step "Configuring NIC RSS (Receive Side Scaling) distribution..."

    $nic = Get-ActiveNicAdapter

    if (-not $nic) {
        Write-Warn "RSS: no active wired NIC found — skipping."
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
            try { $subkeys += Get-Item $subPath -ErrorAction Stop } catch { }
        }
        if (-not $subkeys) {
            Write-Warn "RSS: could not access network class registry keys — skipping RSS configuration."
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
        Write-Warn "RSS: driver registry key not found for '$($nic.InterfaceDescription)' — skipping."
        return
    }

    Write-Info "RSS NIC: $($nic.InterfaceDescription)"

    # Ensure RSS master switch is enabled — some Realtek drivers ship with *RSS=0
    # which silently ignores all RSS sub-parameters (queues, base proc, etc.).
    # Also enable if *RSS doesn't exist at all (missing = disabled on many drivers).
    $rssKeyword = Get-ItemProperty $driverKey -Name "*RSS" -ErrorAction SilentlyContinue
    $rssValue = if ($null -ne $rssKeyword) { $rssKeyword."*RSS" } else { $null }
    if ($null -eq $rssValue) {
        Set-RegistryValue $driverKey "*RSS" 1 "DWord" "Enable RSS master switch (was absent)"
        Write-Info "RSS master switch (*RSS) was absent — created and enabled. Sub-parameters below require this."
    } elseif ([int]$rssValue -eq 0) {
        Set-RegistryValue $driverKey "*RSS" 1 "DWord" "Enable RSS master switch (was disabled)"
        Write-Info "RSS master switch (*RSS) was 0 — enabled. Sub-parameters below require this."
    }

    # Speed-aware RSS queue count: 5+ GbE NICs benefit from 8 queues to handle
    # higher packet rates without saturating individual cores.
    $rssQueueCount = 4
    if ($nic.Speed -and $nic.Speed -ge 5000000000) {
        $rssQueueCount = 8
        Write-Info "RSS: 5+ GbE NIC detected ($('{0:N1}' -f ($nic.Speed / 1e9)) Gbps) — using $rssQueueCount queues"
    }

    # Validate RSS settings against actual processor count to avoid assigning
    # queues to non-existent cores (causes driver errors or silent fallback).
    $processorCount = [Environment]::ProcessorCount
    $rssBaseProcNumber = 2
    if ($processorCount -le 2) {
        $rssBaseProcNumber = 0
        $rssQueueCount = [math]::Max(1, $processorCount)
        Write-Info "RSS: Only $processorCount logical processors — base proc 0, $rssQueueCount queue(s)"
    } elseif ($processorCount - $rssBaseProcNumber -lt $rssQueueCount) {
        $rssBaseProcNumber = 1
        Write-Info "RSS: Only $processorCount logical processors — reduced base proc to $rssBaseProcNumber"
        if ($processorCount - $rssBaseProcNumber -lt $rssQueueCount) {
            $rssQueueCount = [math]::Max(1, $processorCount - $rssBaseProcNumber)
            Write-Info "RSS: Clamped queue count to $rssQueueCount (processor count: $processorCount)"
        }
    }

    # RSS entries and their rationale:
    #
    #   *RSSProfile = 1 (ClosestProcessor)
    #     Each RSS queue is processed on the CPU whose cache is closest to where the NIC
    #     DMA-landed the packet data. On single-NUMA desktop systems this minimises
    #     cross-core cache misses. Value 2 (ClosestProcessorStatic) gives fixed assignment
    #     which may be more predictable; 1 is appropriate for most gaming systems.
    #
    #   *RssBaseProcNumber = 2
    #     First logical processor to assign RSS queues to. Core 0 handles the majority of
    #     OS interrupt bookkeeping; Core 1 is its HT/SMT sibling on most CPUs. Starting
    #     at Core 2 keeps NIC DPCs off the most-contended cores.
    #
    #   *MaxRssProcessors = 4 (or 8 for 5+ GbE)
    #     Upper bound on how many processors RSS will spread queues across. 4 covers all
    #     gaming scenarios at 1-2.5 GbE; 8 for 5+ GbE to handle higher packet rates.
    #
    #   *NumRssQueues = 4 (or 8 for 5+ GbE)
    #     Explicit queue count matching MaxRssProcessors. Without this, drivers may default
    #     to 1 queue regardless of the processor count setting.
    #
    $rssDefaults = [ordered]@{
        "*RSSProfile"        = @{ Value = 1; Type = "DWord";
            Note = "ClosestProcessor: NIC DPCs on cache-local core" }
        "*RssBaseProcNumber" = @{ Value = $rssBaseProcNumber; Type = "DWord";
            Note = "Start RSS from Core $rssBaseProcNumber — avoids Core 0 OS overhead" }
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
            Write-Sub "$($entry.Key) = $($existing.($entry.Key)) (already set — preserved)"
            $skipped++
        } else {
            Set-RegistryValue $driverKey $entry.Key $entry.Value.Value $entry.Value.Type $entry.Value.Note
            $added++
        }
    }

    if ($added -gt 0) {
        Write-OK "RSS: added $added missing entries ($skipped existing preserved). Restart required."
        Write-Info "Effect: NIC receive DPCs distributed away from Core 0."
        # Flag NICs known to omit RSS entries by default
        if ($nic.InterfaceDescription -match "I225|I226|I219") {
            Write-Info "Intel $($Matches[0]) detected — this NIC is known to omit RSS entries by default."
        } elseif ($nic.InterfaceDescription -match "RTL8125|RTL8126|Realtek.*Gaming.*2\.5|Realtek.*5\s*GbE") {
            Write-Info "Realtek $($Matches[0]) detected — RSS entries added for optimal DPC distribution."
        }
    } else {
        Write-OK "RSS: all $skipped entries already present — no changes needed."
    }
}

function Set-NicInterruptAffinity {
    <#
    .SYNOPSIS  Sets NIC interrupt affinity using the suite's last-core heuristic (avoids Core 0).
               Replaces GoInterruptPolicy for NIC interrupt binding.
    #>

    Write-Step "Setting NIC interrupt affinity..."

    # Use the active wired adapter (consistent with Set-NicRssConfig)
    $activeNic = Get-ActiveNicAdapter
    if (-not $activeNic) {
        Write-Warn "No active wired NIC found — skipping affinity."
        return
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
        # Strategy 2: Substring match (either direction) — handles instance suffixes like "#2"
        $nic = $allPciNics |
            Where-Object { $friendlyName -like "*$($_.FriendlyName)*" -or $_.FriendlyName -like "*$friendlyName*" } |
            Select-Object -First 1
    }

    if (-not $nic) {
        # Strategy 3: Match by PCI hardware path segment — extracts VEN/DEV from InstanceId
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
        Write-Warn "Active NIC '$($activeNic.InterfaceDescription)' not found as PCI device — skipping affinity."
        return
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
        Write-DebugLog "CIM failed — using $coreCount logical processors as core count"
    }

    if ($coreCount -lt 2) {
        Write-Warn "Only 1 core detected — cannot set affinity away from Core 0."
        return
    }

    # Calculate affinity mask for target core.
    # On hybrid CPUs (Intel 12th+), the suite heuristically targets the E-core region
    # to avoid contending with game threads on P-cores. On non-hybrid CPUs it uses the
    # last physical core to avoid Core 0. This is a tuning heuristic, not a universal
    # Microsoft recommendation for every topology.
    # Clamp to 63 — processor group 0 supports max 64 logical processors;
    # systems with >64 LPs require GROUP_AFFINITY which needs a different API.
    # Target last physical core to avoid Core 0 (OS-heavy); on hybrid CPUs this lands in the E-core region
    $hybridCpu = Get-IntelHybridCpuName
    if ($null -eq $hybridCpu) { Write-DebugLog "CPU hybrid detection returned null — defaulting to non-hybrid" }
    # SMT-aware affinity: on AMD sequential SMT, physical core N maps to LP N (thread 0)
    # and LP N+coreCount (thread 1). We target the last physical core's LP(s).
    $logicalCount = [Environment]::ProcessorCount
    $smtEnabled   = ($logicalCount -gt $coreCount)
    $targetLP     = [math]::Min($coreCount - 1, 63)
    $mask         = [uint64]1 -shl $targetLP
    # SMT sibling calculation depends on CPU topology:
    # - AMD: sequential — core N threads at LP N and LP N+coreCount
    # - Intel (non-hybrid): interleaved — core N threads at LP 2N and LP 2N+1
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
    if ($hybridCpu) { Write-DebugLog "Hybrid CPU ($hybridCpu) — applying suite heuristic: target last-core/E-core region for NIC affinity" }

    # Convert mask to 8-byte array for registry (binary value)
    $maskBytes = [BitConverter]::GetBytes([uint64]$mask)

    $regBase = "HKLM:\SYSTEM\CurrentControlSet\Enum\$($nic.InstanceId)"
    $affinityPath = "$regBase\Device Parameters\Interrupt Management\Affinity Policy"

    # Route DevicePolicy through Set-RegistryValue for consistent DRY-RUN/backup handling
    Set-RegistryValue $affinityPath "DevicePolicy" 4 "DWord" `
        "NIC interrupt affinity policy (Specified Processors): $($nic.FriendlyName)"

    # AssignmentSetOverride is Binary — Set-RegistryValue supports this via -Type passthrough
    $lpIndices = @($targetLP)
    if ($siblingLP -ge 0 -and $siblingLP -le 63) { $lpIndices += $siblingLP }
    $lpLabel = ($lpIndices | ForEach-Object { "LP $_" }) -join ", "
    Set-RegistryValue $affinityPath "AssignmentSetOverride" ([byte[]]$maskBytes) "Binary" `
        "NIC affinity mask 0x$($mask.ToString('X')) -> ${lpLabel}: $($nic.FriendlyName)"

    if (-not $SCRIPT:DryRun) {
        Write-OK "NIC affinity set: $($nic.FriendlyName) -> $lpLabel (physical core $physCoreIdx)"
        Write-Info "Affinity mask: 0x$($mask.ToString('X')) ($lpLabel of $logicalCount LPs, $coreCount physical cores$(if($smtEnabled){', SMT enabled'}))"
    }
    Write-Info "Restart required for affinity changes to take effect."
}
