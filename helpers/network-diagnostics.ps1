Set-StrictMode -Version Latest

$Script:GuiDnsBackupStepPrefix = "GUI DNS Change"
$Script:ValveSdrConfigUrl = "https://api.steampowered.com/ISteamApps/GetSDRConfig/v1/?appid=730"
$Script:ValveRelayFirewallRulePrefix = "CS2 Optimize Relay Block - "
$Script:ValveKnownHostedRegionTargets = @(
    [PSCustomObject]@{
        RegionCode         = "fsn-hetz"
        Label              = "Falkenstein (Germany) - Hetzner hosted"
        ProtocolPreference = "ICMP"
        Notes              = "Known Valve-hosted Source server IPs on Hetzner Falkenstein. Not currently exposed as CS2 SDR relays."
        Provenance         = "Public server listings for srcds1001-1007-fsn-hetz; confirmed hosts 138.199.142.208-214."
        Candidates         = @(
            [PSCustomObject]@{ Host = "138.199.142.208"; Port = $null; Notes = "srcds1007-fsn-hetz" }
            [PSCustomObject]@{ Host = "138.199.142.209"; Port = $null; Notes = "srcds1006-fsn-hetz" }
            [PSCustomObject]@{ Host = "138.199.142.210"; Port = $null; Notes = "srcds1005-fsn-hetz" }
            [PSCustomObject]@{ Host = "138.199.142.211"; Port = $null; Notes = "srcds1004-fsn-hetz" }
            [PSCustomObject]@{ Host = "138.199.142.212"; Port = $null; Notes = "srcds1003-fsn-hetz" }
            [PSCustomObject]@{ Host = "138.199.142.213"; Port = $null; Notes = "srcds1002-fsn-hetz" }
            [PSCustomObject]@{ Host = "138.199.142.214"; Port = $null; Notes = "srcds1001-fsn-hetz" }
        )
    }
)

function Get-KnownValveHostedRegionTargets {
    [CmdletBinding()]
    param()

    return @($Script:ValveKnownHostedRegionTargets | ForEach-Object {
        [PSCustomObject]@{
            RegionCode         = [string]$_.RegionCode
            Label              = [string]$_.Label
            ProtocolPreference = [string]$_.ProtocolPreference
            Notes              = [string]$_.Notes
            Provenance         = [string]$_.Provenance
            Candidates         = @($_.Candidates | ForEach-Object {
                [PSCustomObject]@{
                    Host  = [string]$_.Host
                    Port  = if ($_.PSObject.Properties['Port'] -and $_.Port) { [int]$_.Port } else { $null }
                    Notes = if ($_.PSObject.Properties['Notes']) { [string]$_.Notes } else { "" }
                }
            })
        }
    })
}

function ConvertFrom-ValveSdrConfig {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$SdrConfig
    )

    if (-not $SdrConfig.PSObject.Properties['revision'] -or -not $SdrConfig.PSObject.Properties['pops']) {
        throw "Valve SDR config response did not contain revision/pops."
    }

    $targets = [System.Collections.Generic.List[object]]::new()
    foreach ($popProperty in $SdrConfig.pops.PSObject.Properties) {
        $pop = $popProperty.Value
        if (-not $pop.PSObject.Properties['relays'] -or -not $pop.relays) { continue }

        $description = if ($pop.PSObject.Properties['desc']) { [string]$pop.desc } else { [string]$popProperty.Name }
        if ($description -match "China") { continue }

        $candidates = @($pop.relays | ForEach-Object {
            if ($_.PSObject.Properties['ipv4'] -and $_.ipv4) {
                [PSCustomObject]@{
                    Host  = [string]$_.ipv4
                    Port  = $null
                    Notes = "Valve SDR relay"
                }
            }
        })
        if ($candidates.Count -eq 0) { continue }

        $targets.Add([PSCustomObject]@{
            RegionCode         = [string]$popProperty.Name
            Label              = $description
            ProtocolPreference = "ICMP"
            Notes              = "Live Valve SDR relay set. Route-quality proxy only; not guaranteed in-match CS2 ping."
            Provenance         = "Valve ISteamApps/GetSDRConfig appid 730 revision $($SdrConfig.revision)"
            Candidates         = @($candidates)
        }) | Out-Null
    }

    $targetLabels = @{}
    foreach ($target in $targets) {
        $targetLabels[[string]$target.Label] = $true
    }
    foreach ($knownTarget in @(Get-KnownValveHostedRegionTargets)) {
        if (-not $targetLabels.ContainsKey([string]$knownTarget.Label)) {
            $targets.Add($knownTarget) | Out-Null
        }
    }

    return @($targets | Sort-Object Label)
}

function Get-ValveRegionTargets {
    [CmdletBinding()]
    param(
        [string]$Path = $CFG_LatencyTargetsFile,
        [switch]$SkipLiveFetch
    )

    if (-not $SkipLiveFetch) {
        try {
            $sdrConfig = Invoke-RestMethod -Uri $Script:ValveSdrConfigUrl -UseBasicParsing -TimeoutSec 10 -ErrorAction Stop
            $liveTargets = @(ConvertFrom-ValveSdrConfig -SdrConfig $sdrConfig)
            if ($liveTargets.Count -gt 0) { return $liveTargets }
        } catch {
            if (Get-Command Write-DebugLog -ErrorAction SilentlyContinue) {
                Write-DebugLog "Valve SDR config fetch failed; using local latency targets: $($_.Exception.Message)"
            }
        }
    }

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
            RegionCode         = if ($_.PSObject.Properties['RegionCode']) { [string]$_.RegionCode } else { "" }
            Label              = [string]$_.Label
            ProtocolPreference = if ($_.ProtocolPreference) { [string]$_.ProtocolPreference } else { "ICMP" }
            Notes              = if ($_.Notes) { [string]$_.Notes } else { "" }
            Provenance         = if ($_.Provenance) { [string]$_.Provenance } else { "" }
            Candidates         = @($_.Candidates | ForEach-Object {
                if (-not $_.Host) { throw "Latency target '$($_.Label)' contains a candidate without Host." }
                [PSCustomObject]@{
                    Host  = [string]$_.Host
                    Port  = if ($_.PSObject.Properties['Port'] -and $_.Port) { [int]$_.Port } else { $null }
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
    $servers = @(if ($dnsInfo -and $dnsInfo.ServerAddresses) { [string[]]@($dnsInfo.ServerAddresses) } else { @() })
    $provider = if (@($servers).Count -eq 0) {
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
        Servers  = [string[]]@($servers)
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

function Set-VerifiedDnsProfileForAdapter {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)][string]$AdapterName,
        [Parameter(Mandatory)][int]$InterfaceIndex,
        [ValidateSet("Cloudflare", "Google", "DHCP")][string]$Provider,
        [string[]]$CurrentServers = @(),
        [string]$BackupStep = $null,
        [switch]$SkipBackup
    )

    $targetServers = switch ($Provider) {
        "Cloudflare" { [string[]]$CFG_DNS_Cloudflare }
        "Google"     { [string[]]$CFG_DNS_Google }
        "DHCP"       { @() }
    }

    if (-not $PSCmdlet.ShouldProcess($AdapterName, "Set DNS profile to $Provider on interface $InterfaceIndex")) {
        return [PSCustomObject]@{
            Changed = $false
            AdapterName = $AdapterName
            InterfaceIndex = $InterfaceIndex
            Provider = $Provider
            DnsServers = [string[]]$CurrentServers
            VerifiedServers = [string[]]$CurrentServers
            BackupStep = $null
        }
    }

    if (-not $SkipBackup) {
        if ([string]::IsNullOrWhiteSpace($BackupStep)) {
            throw "DNS backup step title is required before changing DNS on '$AdapterName'."
        }
        Backup-DnsConfig -AdapterName $AdapterName -InterfaceIndex $InterfaceIndex -OriginalDnsServers $CurrentServers -StepTitle $BackupStep
    }

    if ($Provider -eq "DHCP") {
        Set-DnsClientServerAddress -InterfaceIndex $InterfaceIndex -ResetServerAddresses -ErrorAction Stop
    } else {
        Set-DnsClientServerAddress -InterfaceIndex $InterfaceIndex -ServerAddresses $targetServers -ErrorAction Stop
    }

    $postDns = Get-DnsClientServerAddress -InterfaceIndex $InterfaceIndex -AddressFamily IPv4 -ErrorAction Stop
    $postServers = if ($postDns -and $postDns.ServerAddresses) { [string[]]@($postDns.ServerAddresses) } else { @() }
    if ($Provider -ne "DHCP" -and (($postServers -join ',') -ne ($targetServers -join ','))) {
        throw "DNS post-check failed for '$AdapterName': expected [$($targetServers -join ', ')], got [$($postServers -join ', ')]."
    }

    return [PSCustomObject]@{
        Changed = $true
        AdapterName = $AdapterName
        InterfaceIndex = $InterfaceIndex
        Provider = $Provider
        DnsServers = $targetServers
        VerifiedServers = $postServers
        BackupStep = if ($SkipBackup) { $null } else { $BackupStep }
    }
}

function ConvertTo-LatencySamples {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$ProbeResult
    )

    if ($null -eq $ProbeResult) { return @() }
    if ($ProbeResult.PSObject.Properties['Status'] -and [string]$ProbeResult.Status -ne "Success") {
        return @()
    }
    if ($ProbeResult.PSObject.Properties['Reply'] -and $ProbeResult.Reply -and
        $ProbeResult.Reply.PSObject.Properties['Status'] -and [string]$ProbeResult.Reply.Status -ne "Success") {
        return @()
    }
    if ($ProbeResult -is [array]) {
        return @($ProbeResult | ForEach-Object {
            if ($_.PSObject.Properties['Status'] -and [string]$_.Status -ne "Success") { return }
            if ($_.PSObject.Properties['Reply'] -and $_.Reply -and
                $_.Reply.PSObject.Properties['Status'] -and [string]$_.Reply.Status -ne "Success") { return }
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
    $timeoutMs = [math]::Max(200, $TimeoutSeconds * 1000)
    for ($i = 0; $i -lt $SampleCount; $i++) {
        $ping = $null
        try {
            $ping = [System.Net.NetworkInformation.Ping]::new()
            $reply = $ping.Send($TargetHost, $timeoutMs)
            if ($reply -and $reply.Status -eq [System.Net.NetworkInformation.IPStatus]::Success) {
                $samples.Add([double]$reply.RoundtripTime) | Out-Null
            } else {
                $timeouts++
            }
        } catch {
            $timeouts++
        } finally {
            if ($ping) { $ping.Dispose() }
        }
    }

    return [PSCustomObject]@{
        Samples      = @($samples)
        TimeoutCount = $timeouts
    }
}

function Invoke-TcpLatencyCandidateProbe {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$TargetHost,
        [Parameter(Mandatory)][int]$Port,
        [int]$SampleCount = 3,
        [int]$TimeoutSeconds = 1
    )

    $samples = [System.Collections.Generic.List[double]]::new()
    $timeouts = 0
    $timeoutMs = [math]::Max(200, $TimeoutSeconds * 1000)
    for ($i = 0; $i -lt $SampleCount; $i++) {
        $client = [System.Net.Sockets.TcpClient]::new()
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        try {
            $async = $client.BeginConnect($TargetHost, $Port, $null, $null)
            if ($async.AsyncWaitHandle.WaitOne($timeoutMs) -and $client.Connected) {
                $client.EndConnect($async)
                $sw.Stop()
                $samples.Add([double]$sw.ElapsedMilliseconds) | Out-Null
            } else {
                $sw.Stop()
                $timeouts++
            }
        } catch {
            $sw.Stop()
            $timeouts++
        } finally {
            try { $client.Close() } catch {
                Write-DebugLog "Latency probe socket close failed: $($_.Exception.Message)"
            }
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
    $protocolPreference = if ($Target.PSObject.Properties['ProtocolPreference'] -and $Target.ProtocolPreference) {
        [string]$Target.ProtocolPreference
    } else {
        "ICMP"
    }
    for ($candidateIndex = 0; $candidateIndex -lt $candidates.Count; $candidateIndex++) {
        $candidate = $candidates[$candidateIndex]
        $attemptedHost = [string]$candidate.Host
        $protocolUsed = "ICMP"
        if ($protocolPreference -eq "TCP" -and $candidate.Port) {
            $protocolUsed = "TCP/$($candidate.Port)"
            $probe = Invoke-TcpLatencyCandidateProbe -TargetHost $attemptedHost -Port $candidate.Port -SampleCount $SampleCount -TimeoutSeconds $TimeoutSeconds
        } else {
            $probe = Invoke-LatencyCandidateProbe -TargetHost $attemptedHost -SampleCount $SampleCount -TimeoutSeconds $TimeoutSeconds
        }
        $samples = [double[]]@($probe.Samples)
        if ($samples.Count -gt 0) {
            return [PSCustomObject]@{
                RegionCode       = if ($Target.PSObject.Properties['RegionCode']) { [string]$Target.RegionCode } else { "" }
                TargetLabel       = [string]$Target.Label
                ResolvedEndpoint  = $attemptedHost
                ProtocolUsed      = $protocolUsed
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
        RegionCode       = if ($Target.PSObject.Properties['RegionCode']) { [string]$Target.RegionCode } else { "" }
        TargetLabel       = [string]$Target.Label
        ResolvedEndpoint  = $attemptedHost
        ProtocolUsed      = if ($protocolPreference -eq "TCP") { "TCP" } else { "ICMP" }
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
        $PostRun = $(Get-LatestLatencyRun -Kind "post"),
        [ValidateSet("Ping", "Region", "Delta", "Timeouts", "Blocked")][string]$SortBy = "Ping"
    )

    if (-not $BaselineRun -and -not $PostRun) { return @() }
    if ($BaselineRun -and -not $PostRun) {
        return @(@($BaselineRun.Results) | ForEach-Object {
            $baselineSort = if ($null -ne $_.AvgRttMs) { [double]$_.AvgRttMs } else { [double]::PositiveInfinity }
            [PSCustomObject]@{
                TargetLabel    = [string]$_.TargetLabel
                RegionCode     = if ($_.PSObject.Properties['RegionCode']) { [string]$_.RegionCode } else { "" }
                BaselineAvgMs  = $_.AvgRttMs
                PostAvgMs      = $null
                DeltaMs        = $null
                BaselineSort   = $baselineSort
                PostSort       = [double]::PositiveInfinity
                DeltaSort      = [double]::PositiveInfinity
                TimeoutSort    = Get-TimeoutSortValue -TimeoutSummary "$($_.TimeoutCount) / $($_.SampleCount)"
                ProtocolUsed   = [string]$_.ProtocolUsed
                Endpoint       = [string]$_.ResolvedEndpoint
                TimeoutSummary = "$($_.TimeoutCount) / $($_.SampleCount)"
                FallbackUsed   = [bool]$_.FallbackUsed
            }
        } | Sort-ValveLatencyComparisonRows -SortBy $SortBy)
    }
    if (-not $BaselineRun -and $PostRun) {
        return @(@($PostRun.Results) | ForEach-Object {
            $postSort = if ($null -ne $_.AvgRttMs) { [double]$_.AvgRttMs } else { [double]::PositiveInfinity }
            [PSCustomObject]@{
                TargetLabel    = [string]$_.TargetLabel
                RegionCode     = if ($_.PSObject.Properties['RegionCode']) { [string]$_.RegionCode } else { "" }
                BaselineAvgMs  = $null
                PostAvgMs      = $_.AvgRttMs
                DeltaMs        = $null
                BaselineSort   = [double]::PositiveInfinity
                PostSort       = $postSort
                DeltaSort      = [double]::PositiveInfinity
                TimeoutSort    = Get-TimeoutSortValue -TimeoutSummary "$($_.TimeoutCount) / $($_.SampleCount)"
                ProtocolUsed   = [string]$_.ProtocolUsed
                Endpoint       = [string]$_.ResolvedEndpoint
                TimeoutSummary = "$($_.TimeoutCount) / $($_.SampleCount)"
                FallbackUsed   = [bool]$_.FallbackUsed
            }
        } | Sort-ValveLatencyComparisonRows -SortBy $SortBy)
    }

    $baselineByLabel = @{}
    foreach ($result in @($BaselineRun.Results)) {
        $baselineByLabel[[string]$result.TargetLabel] = $result
    }

    return @(@($PostRun.Results) | ForEach-Object {
        $post = $_
        $baseline = $baselineByLabel[[string]$post.TargetLabel]
        $timeoutSummary = if ($baseline) { "$($baseline.TimeoutCount) -> $($post.TimeoutCount)" } else { "$($post.TimeoutCount)" }
        $delta = if ($baseline -and $null -ne $baseline.AvgRttMs -and $null -ne $post.AvgRttMs) {
            [math]::Round(([double]$post.AvgRttMs - [double]$baseline.AvgRttMs), 1)
        } else {
            $null
        }
        [PSCustomObject]@{
            TargetLabel    = [string]$post.TargetLabel
            RegionCode     = if ($post.PSObject.Properties['RegionCode']) { [string]$post.RegionCode } elseif ($baseline -and $baseline.PSObject.Properties['RegionCode']) { [string]$baseline.RegionCode } else { "" }
            BaselineAvgMs  = if ($baseline) { $baseline.AvgRttMs } else { $null }
            PostAvgMs      = $post.AvgRttMs
            DeltaMs        = $delta
            BaselineSort   = if ($baseline -and $null -ne $baseline.AvgRttMs) { [double]$baseline.AvgRttMs } else { [double]::PositiveInfinity }
            PostSort       = if ($null -ne $post.AvgRttMs) { [double]$post.AvgRttMs } else { [double]::PositiveInfinity }
            DeltaSort      = if ($null -ne $delta) { [double]$delta } else { [double]::PositiveInfinity }
            TimeoutSort    = Get-TimeoutSortValue -TimeoutSummary $timeoutSummary
            ProtocolUsed   = [string]$post.ProtocolUsed
            Endpoint       = [string]$post.ResolvedEndpoint
            TimeoutSummary = $timeoutSummary
            FallbackUsed   = [bool]$post.FallbackUsed
        }
    } | Sort-ValveLatencyComparisonRows -SortBy $SortBy)
}

function Sort-ValveLatencyComparisonRows {
    [CmdletBinding()]
    param(
        [Parameter(ValueFromPipeline)]$Row,
        [ValidateSet("Ping", "Region", "Delta", "Timeouts", "Blocked")][string]$SortBy = "Ping"
    )

    begin {
        $rows = [System.Collections.Generic.List[object]]::new()
    }
    process {
        if ($null -ne $Row) { $rows.Add($Row) | Out-Null }
    }
    end {
        $blocked = @{}
        if ($SortBy -eq "Blocked") {
            foreach ($region in @(Get-BlockedValveRelayRegions)) {
                $blocked[[string]$region.RegionName] = $true
            }
        }

        switch ($SortBy) {
            "Region" {
                return @($rows | Sort-Object TargetLabel)
            }
            "Delta" {
                return @($rows | Sort-Object DeltaSort, TargetLabel)
            }
            "Timeouts" {
                return @($rows | Sort-Object TimeoutSort, TargetLabel)
            }
            "Blocked" {
                return @($rows | Sort-Object @{ Expression = { if ($blocked.ContainsKey([string]$_.TargetLabel)) { 0 } else { 1 } } }, TargetLabel)
            }
            default {
                return @($rows | Sort-Object @{ Expression = {
                    if ($_.PSObject.Properties['PostSort'] -and $_.PostSort -ne [double]::PositiveInfinity) { [double]$_.PostSort }
                    elseif ($_.PSObject.Properties['BaselineSort']) { [double]$_.BaselineSort }
                    else { [double]::PositiveInfinity }
                } }, TargetLabel)
            }
        }
    }
}

function Get-TimeoutSortValue {
    [CmdletBinding()]
    param(
        [AllowEmptyString()][string]$TimeoutSummary
    )

    $numbers = @([regex]::Matches([string]$TimeoutSummary, '\d+') | ForEach-Object { [int]$_.Value })
    if ($numbers.Count -eq 0) { return 0 }
    return ($numbers | Measure-Object -Sum).Sum
}

function Get-LatencyHistoryRows {
    [CmdletBinding()]
    param(
        [AllowEmptyString()][string]$SelectedRegion = ""
    )

    $history = Get-LatencyHistoryData
    return @(@($history.Runs | Select-Object -Last 12) | ForEach-Object {
        $run = $_
        $successful = @($run.Results | Where-Object { $null -ne $_.AvgRttMs })
        $avg = if ($successful.Count -gt 0) {
            [math]::Round((($successful | Measure-Object -Property AvgRttMs -Average).Average), 1)
        } else {
            $null
        }
        $regionResult = $null
        if (-not [string]::IsNullOrWhiteSpace($SelectedRegion)) {
            $regionResult = @($run.Results | Where-Object { $_.TargetLabel -eq $SelectedRegion } | Select-Object -First 1)
        }
        if (-not $regionResult -and $successful.Count -gt 0) {
            $regionResult = @($successful | Sort-Object AvgRttMs, TargetLabel | Select-Object -First 1)
        }
        [PSCustomObject]@{
            Timestamp      = [string]$run.Timestamp
            Kind           = [string]$run.Kind
            AdapterName    = [string]$run.AdapterName
            DnsProvider    = [string]$run.DnsProvider
            AvgRttMs       = $avg
            SelectedRegion = if ($regionResult) { [string]$regionResult.TargetLabel } elseif ($SelectedRegion) { $SelectedRegion } else { "" }
            RegionRttMs    = if ($regionResult) { $regionResult.AvgRttMs } else { $null }
            RegionsOk      = $successful.Count
            RunId          = [string]$run.RunId
        }
    })
}

function Set-NetworkDiagnosticDnsProfile {
    [CmdletBinding(SupportsShouldProcess)]
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

    $currentServers = @($summary.DnsServers)
    $sameDns = (($currentServers -join ',') -eq ($targetServers -join ','))
    if ($Provider -eq "DHCP" -and @($currentServers).Count -eq 0) {
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

    if (-not $PSCmdlet.ShouldProcess($summary.AdapterName, "Set network diagnostic DNS profile to $Provider")) {
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
            $dnsResult = Set-VerifiedDnsProfileForAdapter `
                -AdapterName $summary.AdapterName `
                -InterfaceIndex $summary.InterfaceIndex `
                -Provider $Provider `
                -CurrentServers $currentServers `
                -BackupStep $backupStep
        } else {
            $dnsResult = Set-VerifiedDnsProfileForAdapter `
                -AdapterName $summary.AdapterName `
                -InterfaceIndex $summary.InterfaceIndex `
                -Provider $Provider `
                -CurrentServers $currentServers `
                -SkipBackup
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
        DnsServers   = [string[]]$dnsResult.DnsServers
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

function Get-ValveRelayFirewallRuleName {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$RegionName
    )

    $safeName = ($RegionName -replace '[^\w\s().-]', '').Trim()
    if ([string]::IsNullOrWhiteSpace($safeName)) { $safeName = "Unknown Region" }
    $ruleName = "$Script:ValveRelayFirewallRulePrefix$safeName"
    if ($ruleName.Length -gt 190) { $ruleName = $ruleName.Substring(0, 190) }
    return $ruleName
}

function Get-ValveRegionRelayAddresses {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$RegionName
    )

    $target = @(Get-ValveRegionTargets | Where-Object { $_.Label -eq $RegionName } | Select-Object -First 1)
    if (-not $target) { throw "Region '$RegionName' was not found in the current Valve SDR target list." }

    $addresses = @($target.Candidates | ForEach-Object { $_.Host } | Where-Object { $_ } | Sort-Object -Unique)
    if ($addresses.Count -eq 0) { throw "Region '$RegionName' has no relay IPv4 addresses to block." }
    return @($addresses)
}

function Get-BlockedValveRelayRegions {
    [CmdletBinding()]
    param()

    $rules = @(Get-NetFirewallRule -DisplayName "$Script:ValveRelayFirewallRulePrefix*" -ErrorAction SilentlyContinue)
    return @($rules | Sort-Object DisplayName | ForEach-Object {
        [PSCustomObject]@{
            RegionName = ([string]$_.DisplayName).Substring($Script:ValveRelayFirewallRulePrefix.Length)
            RuleName   = [string]$_.DisplayName
            Enabled    = [string]$_.Enabled
        }
    })
}

function Block-ValveRelayRegion {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$RegionName
    )

    $addresses = @(Get-ValveRegionRelayAddresses -RegionName $RegionName)
    $ruleName = Get-ValveRelayFirewallRuleName -RegionName $RegionName
    Remove-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue
    New-NetFirewallRule `
        -DisplayName $ruleName `
        -Description "Blocks outbound traffic to Valve relay/server targets for $RegionName. Created by CS2 Optimize GUI." `
        -Direction Outbound `
        -Action Block `
        -RemoteAddress $addresses `
        -Protocol Any `
        -Profile Any `
        -Enabled True `
        -ErrorAction Stop | Out-Null

    return [PSCustomObject]@{
        Changed     = $true
        RegionName  = $RegionName
        RuleName    = $ruleName
        AddressCount = $addresses.Count
    }
}

function Unblock-ValveRelayRegion {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$RegionName
    )

    $ruleName = Get-ValveRelayFirewallRuleName -RegionName $RegionName
    $existing = @(Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue)
    if ($existing.Count -eq 0) {
        return [PSCustomObject]@{
            Changed    = $false
            RegionName = $RegionName
            RuleName   = $ruleName
        }
    }

    Remove-NetFirewallRule -DisplayName $ruleName -ErrorAction Stop
    return [PSCustomObject]@{
        Changed    = $true
        RegionName = $RegionName
        RuleName   = $ruleName
    }
}

function Unblock-AllValveRelayRegions {
    [CmdletBinding()]
    param()

    $rules = @(Get-NetFirewallRule -DisplayName "$Script:ValveRelayFirewallRulePrefix*" -ErrorAction SilentlyContinue)
    foreach ($rule in $rules) {
        Remove-NetFirewallRule -DisplayName $rule.DisplayName -ErrorAction Stop
    }
    return [PSCustomObject]@{
        Changed = ($rules.Count -gt 0)
        Count   = $rules.Count
    }
}
