Set-StrictMode -Version Latest

$Script:GuiDnsBackupStepPrefix = "GUI DNS Change"

function Get-ValveRegionTargets {
    [CmdletBinding()]
    param(
        [string]$Path = $CFG_LatencyTargetsFile
    )

    if (-not (Test-Path $Path)) {
        throw "Latency target definition file not found at '$Path'."
    }

    $raw = Get-Content $Path -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    $targets = @($raw.targets)
    if ($targets.Count -eq 0) {
        throw "Latency target definition file '$Path' does not contain any targets."
    }

    return @($targets | ForEach-Object {
        if (-not $_.Label) { throw "Latency target missing Label in '$Path'." }
        if (-not $_.Candidates -or @($_.Candidates).Count -eq 0) {
            throw "Latency target '$($_.Label)' has no candidates in '$Path'."
        }
        [PSCustomObject]@{
            Label              = [string]$_.Label
            ProtocolPreference = if ($_.ProtocolPreference) { [string]$_.ProtocolPreference } else { "ICMP" }
            Notes              = if ($_.Notes) { [string]$_.Notes } else { "" }
            Provenance         = if ($_.Provenance) { [string]$_.Provenance } else { "" }
            Candidates         = @($_.Candidates | ForEach-Object {
                if (-not $_.Host) { throw "Latency target '$($_.Label)' contains a candidate without Host." }
                [PSCustomObject]@{
                    Host  = [string]$_.Host
                    Notes = if ($_.Notes) { [string]$_.Notes } else { "" }
                }
            })
        }
    })
}

function Get-NetworkDiagnosticAdapterType {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Adapter
    )

    $description = [string]($Adapter.InterfaceDescription)
    $name = [string]($Adapter.Name)
    $haystack = "$name $description"
    if ($haystack -match $CFG_VirtualAdapterFilter) { return "VPN-like / virtual" }
    if ($description -match 'Wi-?Fi|Wireless|802\.11' -or $name -match 'Wi-?Fi|Wireless|WLAN') { return "Wireless" }
    return "Physical / wired"
}

function Get-ActiveNetworkDiagnosticAdapter {
    [CmdletBinding()]
    param()

    $adapters = @(
        Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object {
            $_.Status -eq "Up"
        }
    )
    if ($adapters.Count -eq 0) { return $null }

    $preferred = @(
        $adapters | Where-Object { $_.InterfaceDescription -notmatch $CFG_VirtualAdapterFilter }
    )
    $adapter = if ($preferred.Count -gt 0) { $preferred | Select-Object -First 1 } else { $adapters | Select-Object -First 1 }
    if (-not $adapter) { return $null }

    return [PSCustomObject]@{
        Name                 = [string]$adapter.Name
        InterfaceIndex       = if ($adapter.PSObject.Properties['ifIndex']) { [int]$adapter.ifIndex } else { [int]$adapter.InterfaceIndex }
        InterfaceDescription = [string]$adapter.InterfaceDescription
        Status               = [string]$adapter.Status
        AdapterType          = Get-NetworkDiagnosticAdapterType -Adapter $adapter
    }
}

function Get-NetworkDiagnosticDnsState {
    [CmdletBinding()]
    param(
        [int]$InterfaceIndex
    )

    $dnsInfo = Get-DnsClientServerAddress -InterfaceIndex $InterfaceIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue
    $servers = if ($dnsInfo -and $dnsInfo.ServerAddresses) { [string[]]@($dnsInfo.ServerAddresses) } else { @() }
    $provider = if ($servers.Count -eq 0) {
        "DHCP"
    } elseif (($servers -join ',') -eq ($CFG_DNS_Cloudflare -join ',')) {
        "Cloudflare"
    } elseif (($servers -join ',') -eq ($CFG_DNS_Google -join ',')) {
        "Google"
    } else {
        "Custom"
    }

    return [PSCustomObject]@{
        Provider = $provider
        Servers  = [string[]]$servers
    }
}

function Get-NetworkDiagnosticSummary {
    [CmdletBinding()]
    param()

    $adapter = Get-ActiveNetworkDiagnosticAdapter
    if (-not $adapter) {
        return [PSCustomObject]@{
            AdapterFound = $false
            AdapterName  = ""
            AdapterType  = ""
            DnsProvider  = "Unknown"
            DnsServers   = @()
        }
    }

    $dns = Get-NetworkDiagnosticDnsState -InterfaceIndex $adapter.InterfaceIndex
    return [PSCustomObject]@{
        AdapterFound       = $true
        AdapterName        = $adapter.Name
        InterfaceIndex     = $adapter.InterfaceIndex
        AdapterDescription = $adapter.InterfaceDescription
        AdapterType        = $adapter.AdapterType
        DnsProvider        = $dns.Provider
        DnsServers         = [string[]]$dns.Servers
    }
}

function ConvertTo-LatencySamples {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$ProbeResult
    )

    if ($null -eq $ProbeResult) { return @() }
    if ($ProbeResult -is [array]) {
        return @($ProbeResult | ForEach-Object {
            if ($_.PSObject.Properties['ResponseTime']) { [double]$_.ResponseTime }
            elseif ($_.PSObject.Properties['Latency']) { [double]$_.Latency }
            elseif ($_ -is [double] -or $_ -is [int]) { [double]$_ }
        } | Where-Object { $null -ne $_ })
    }

    if ($ProbeResult.PSObject.Properties['ResponseTime']) { return @([double]$ProbeResult.ResponseTime) }
    if ($ProbeResult.PSObject.Properties['Latency']) { return @([double]$ProbeResult.Latency) }
    if ($ProbeResult -is [double] -or $ProbeResult -is [int]) { return @([double]$ProbeResult) }
    return @()
}

function Invoke-LatencyCandidateProbe {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$TargetHost,
        [int]$SampleCount = 3,
        [int]$TimeoutSeconds = 1
    )

    $samples = [System.Collections.Generic.List[double]]::new()
    $timeouts = 0
    for ($i = 0; $i -lt $SampleCount; $i++) {
        try {
            $probe = Test-Connection -ComputerName $TargetHost -Count 1 -Quiet:$false -ErrorAction Stop -TimeoutSeconds $TimeoutSeconds
            $responseTimes = @(ConvertTo-LatencySamples -ProbeResult $probe)
            if ($responseTimes.Count -gt 0) {
                foreach ($sample in $responseTimes) { $samples.Add($sample) | Out-Null }
            } else {
                $timeouts++
            }
        } catch {
            $timeouts++
        }
    }

    return [PSCustomObject]@{
        Samples      = @($samples)
        TimeoutCount = $timeouts
    }
}

function Get-LatencyStatisticMedian {
    [CmdletBinding()]
    param(
        [double[]]$Samples
    )

    if (-not $Samples -or $Samples.Count -eq 0) { return $null }
    $sorted = @($Samples | Sort-Object)
    $middle = [int][math]::Floor($sorted.Count / 2)
    if (($sorted.Count % 2) -eq 1) {
        return [math]::Round($sorted[$middle], 1)
    }
    return [math]::Round((($sorted[$middle - 1] + $sorted[$middle]) / 2), 1)
}

function Measure-ValveRegionLatency {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Target,
        [int]$SampleCount = 3,
        [int]$TimeoutSeconds = 1
    )

    $candidates = @($Target.Candidates)
    $attemptedHost = if ($candidates.Count -gt 0) { [string]$candidates[0].Host } else { "" }
    for ($candidateIndex = 0; $candidateIndex -lt $candidates.Count; $candidateIndex++) {
        $candidate = $candidates[$candidateIndex]
        $attemptedHost = [string]$candidate.Host
        $probe = Invoke-LatencyCandidateProbe -TargetHost $attemptedHost -SampleCount $SampleCount -TimeoutSeconds $TimeoutSeconds
        $samples = [double[]]@($probe.Samples)
        if ($samples.Count -gt 0) {
            return [PSCustomObject]@{
                TargetLabel       = [string]$Target.Label
                ResolvedEndpoint  = $attemptedHost
                ProtocolUsed      = "ICMP"
                SampleCount       = $SampleCount
                SuccessfulSamples = $samples.Count
                MinRttMs          = [math]::Round((($samples | Measure-Object -Minimum).Minimum), 1)
                MedianRttMs       = Get-LatencyStatisticMedian -Samples $samples
                AvgRttMs          = [math]::Round((($samples | Measure-Object -Average).Average), 1)
                TimeoutCount      = [int]$probe.TimeoutCount
                FallbackUsed      = ($candidateIndex -gt 0)
                Notes             = [string]$Target.Notes
                Provenance        = [string]$Target.Provenance
            }
        }
    }

    return [PSCustomObject]@{
        TargetLabel       = [string]$Target.Label
        ResolvedEndpoint  = $attemptedHost
        ProtocolUsed      = "ICMP"
        SampleCount       = $SampleCount
        SuccessfulSamples = 0
        MinRttMs          = $null
        MedianRttMs       = $null
        AvgRttMs          = $null
        TimeoutCount      = $SampleCount
        FallbackUsed      = $false
        Notes             = [string]$Target.Notes
        Provenance        = [string]$Target.Provenance
    }
}

function Get-LatencyHistoryData {
    [CmdletBinding()]
    param(
        [string]$Path = $CFG_LatencyHistoryFile
    )

    if (-not (Test-Path $Path)) {
        return [PSCustomObject]@{
            Version = 1
            Runs    = @()
        }
    }

    $history = Get-Content $Path -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    if (-not $history.PSObject.Properties['Runs']) {
        $history | Add-Member -NotePropertyName Runs -NotePropertyValue @() -Force
    }
    if (-not $history.PSObject.Properties['Version']) {
        $history | Add-Member -NotePropertyName Version -NotePropertyValue 1 -Force
    }
    return $history
}

function Save-LatencyHistoryRun {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Run,
        [string]$Path = $CFG_LatencyHistoryFile
    )

    $history = Get-LatencyHistoryData -Path $Path
    $history.Runs = @($history.Runs) + @($Run)
    Save-JsonAtomic -Data $history -Path $Path
    Set-SecureAcl -Path $Path
    return $history
}

function Invoke-ValveRegionLatencyDiagnostic {
    [CmdletBinding()]
    param(
        [ValidateSet("baseline", "post")][string]$Kind,
        [int]$SampleCount = 3,
        [int]$TimeoutSeconds = 1
    )

    $summary = Get-NetworkDiagnosticSummary
    $targets = @(Get-ValveRegionTargets)
    $results = @($targets | ForEach-Object {
        Measure-ValveRegionLatency -Target $_ -SampleCount $SampleCount -TimeoutSeconds $TimeoutSeconds
    })

    $run = [PSCustomObject]@{
        RunId          = [guid]::NewGuid().Guid
        Kind           = $Kind
        Timestamp      = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        AdapterName    = if ($summary.AdapterFound) { $summary.AdapterName } else { "" }
        AdapterType    = if ($summary.AdapterFound) { $summary.AdapterType } else { "" }
        DnsProvider    = $summary.DnsProvider
        DnsServers     = [string[]]$summary.DnsServers
        Disclaimer     = "Valve Region Latency Diagnostic is a route-quality proxy, not a guaranteed in-match CS2 ping."
        Results        = $results
    }

    Save-LatencyHistoryRun -Run $run | Out-Null
    return $run
}

function Get-LatestLatencyRun {
    [CmdletBinding()]
    param(
        [ValidateSet("baseline", "post")][string]$Kind
    )

    $history = Get-LatencyHistoryData
    $runs = @($history.Runs | Where-Object { $_.Kind -eq $Kind })
    if ($runs.Count -eq 0) { return $null }
    return $runs[-1]
}

function Get-ValveLatencyComparisonRows {
    [CmdletBinding()]
    param(
        $BaselineRun = $(Get-LatestLatencyRun -Kind "baseline"),
        $PostRun = $(Get-LatestLatencyRun -Kind "post")
    )

    if (-not $BaselineRun -or -not $PostRun) { return @() }

    $baselineByLabel = @{}
    foreach ($result in @($BaselineRun.Results)) {
        $baselineByLabel[[string]$result.TargetLabel] = $result
    }

    return @(@($PostRun.Results) | ForEach-Object {
        $post = $_
        $baseline = $baselineByLabel[[string]$post.TargetLabel]
        $delta = if ($baseline -and $null -ne $baseline.AvgRttMs -and $null -ne $post.AvgRttMs) {
            [math]::Round(([double]$post.AvgRttMs - [double]$baseline.AvgRttMs), 1)
        } else {
            $null
        }
        [PSCustomObject]@{
            TargetLabel    = [string]$post.TargetLabel
            BaselineAvgMs  = if ($baseline) { $baseline.AvgRttMs } else { $null }
            PostAvgMs      = $post.AvgRttMs
            DeltaMs        = $delta
            ProtocolUsed   = [string]$post.ProtocolUsed
            Endpoint       = [string]$post.ResolvedEndpoint
            TimeoutSummary = if ($baseline) { "$($baseline.TimeoutCount) → $($post.TimeoutCount)" } else { "$($post.TimeoutCount)" }
            FallbackUsed   = [bool]$post.FallbackUsed
        }
    })
}

function Get-LatencyHistoryRows {
    [CmdletBinding()]
    param()

    $history = Get-LatencyHistoryData
    return @(@($history.Runs | Select-Object -Last 12) | ForEach-Object {
        $run = $_
        $successful = @($run.Results | Where-Object { $null -ne $_.AvgRttMs })
        $avg = if ($successful.Count -gt 0) {
            [math]::Round((($successful | Measure-Object -Property AvgRttMs -Average).Average), 1)
        } else {
            $null
        }
        [PSCustomObject]@{
            Timestamp   = [string]$run.Timestamp
            Kind        = [string]$run.Kind
            AdapterName = [string]$run.AdapterName
            DnsProvider = [string]$run.DnsProvider
            AvgRttMs    = $avg
            RegionsOk   = $successful.Count
            RunId       = [string]$run.RunId
        }
    })
}

function Set-NetworkDiagnosticDnsProfile {
    [CmdletBinding()]
    param(
        [ValidateSet("Cloudflare", "Google", "DHCP")][string]$Provider,
        [switch]$SkipBackup
    )

    $summary = Get-NetworkDiagnosticSummary
    if (-not $summary.AdapterFound) {
        throw "No active adapter available for DNS changes."
    }

    $targetServers = switch ($Provider) {
        "Cloudflare" { [string[]]$CFG_DNS_Cloudflare }
        "Google"     { [string[]]$CFG_DNS_Google }
        "DHCP"       { @() }
    }

    $currentServers = [string[]]$summary.DnsServers
    $sameDns = (($currentServers -join ',') -eq ($targetServers -join ','))
    if ($Provider -eq "DHCP" -and $currentServers.Count -eq 0) {
        $sameDns = $true
    }

    if ($sameDns) {
        return [PSCustomObject]@{
            Changed      = $false
            AdapterName  = $summary.AdapterName
            Provider     = $Provider
            DnsServers   = $currentServers
            BackupStep   = $null
        }
    }

    $backupStep = "$Script:GuiDnsBackupStepPrefix :: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
    $lockSet = $false
    try {
        if (-not (Test-BackupLock)) {
            Set-BackupLock
            $lockSet = $true
        }

        if (-not $SkipBackup) {
            Backup-DnsConfig -AdapterName $summary.AdapterName -InterfaceIndex $summary.InterfaceIndex -OriginalDnsServers $currentServers -StepTitle $backupStep
        }

        if ($Provider -eq "DHCP") {
            Set-DnsClientServerAddress -InterfaceIndex $summary.InterfaceIndex -ResetServerAddresses -ErrorAction Stop
        } else {
            Set-DnsClientServerAddress -InterfaceIndex $summary.InterfaceIndex -ServerAddresses $targetServers -ErrorAction Stop
        }

        if (-not $SkipBackup) {
            Flush-BackupBuffer
        }
    } finally {
        if ($lockSet) { Remove-BackupLock }
    }

    return [PSCustomObject]@{
        Changed      = $true
        AdapterName  = $summary.AdapterName
        Provider     = $Provider
        DnsServers   = $targetServers
        BackupStep   = if ($SkipBackup) { $null } else { $backupStep }
    }
}

function Restore-LatestDnsBackup {
    [CmdletBinding()]
    param()

    $backup = Get-BackupData
    $dnsEntries = @($backup.entries | Where-Object { $_.type -eq "dns" -and $_.step -like "$Script:GuiDnsBackupStepPrefix*" })
    if ($dnsEntries.Count -eq 0) { return $false }

    $latestStep = ($dnsEntries | Sort-Object timestamp | Select-Object -Last 1).step
    return (Restore-StepChanges -StepTitle $latestStep)
}
