# ==============================================================================
#  tests/helpers/network-diagnostics.Tests.ps1
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Get-ValveRegionTargets" {

    BeforeEach {
        Reset-TestState
    }

    It "loads the repo-owned latency target definitions" {
        $targets = @(Get-ValveRegionTargets -SkipLiveFetch)

        $targets.Count | Should -BeGreaterThan 0
        $targets[0].PSObject.Properties.Name | Should -Contain "Label"
        @($targets[0].Candidates).Count | Should -BeGreaterThan 0
        $targets[0].Candidates[0].PSObject.Properties.Name | Should -Contain "Port"
    }

    It "builds live targets from Valve SDR config and excludes China pops" {
        $sdrConfig = [PSCustomObject]@{
            revision = 42
            pops = [PSCustomObject]@{
                fra = [PSCustomObject]@{
                    desc = "Frankfurt (Germany)"
                    relays = @(
                        [PSCustomObject]@{ ipv4 = "155.133.226.68" },
                        [PSCustomObject]@{ ipv4 = "155.133.226.69" }
                    )
                }
                pw = [PSCustomObject]@{
                    desc = "China Perfect World"
                    relays = @([PSCustomObject]@{ ipv4 = "1.2.3.4" })
                }
            }
        }

        $targets = @(ConvertFrom-ValveSdrConfig -SdrConfig $sdrConfig)

        $targets.Count | Should -Be 2
        $frankfurt = @($targets | Where-Object { $_.RegionCode -eq "fra" } | Select-Object -First 1)
        $frankfurt.Label | Should -Be "Frankfurt (Germany)"
        $frankfurt.ProtocolPreference | Should -Be "ICMP"
        $frankfurt.Candidates[0].Host | Should -Be "155.133.226.68"
        $frankfurt.Provenance | Should -Match "revision 42"

        $falkenstein = @($targets | Where-Object { $_.RegionCode -eq "fsn-hetz" } | Select-Object -First 1)
        $falkenstein.Label | Should -Be "Falkenstein (Germany) - Hetzner hosted"
        @($falkenstein.Candidates).Count | Should -Be 7
        $falkenstein.Candidates[-1].Host | Should -Be "138.199.142.214"
    }
}

Describe "Measure-ValveRegionLatency" {

    BeforeEach {
        Reset-TestState
    }

    It "marks fallback-used when the second candidate is the first responder" {
        Mock Invoke-LatencyCandidateProbe {
            param($TargetHost, $SampleCount, $TimeoutSeconds)
            if ($TargetHost -eq "first.invalid") {
                return [PSCustomObject]@{ Samples = @(); TimeoutCount = 3 }
            }
            return [PSCustomObject]@{ Samples = @(18.2, 19.3, 20.1); TimeoutCount = 0 }
        }

        $target = [PSCustomObject]@{
            Label      = "Frankfurt"
            Notes      = "proxy"
            Provenance = "test"
            Candidates = @(
                [PSCustomObject]@{ Host = "first.invalid"; Notes = "" },
                [PSCustomObject]@{ Host = "second.valid"; Notes = "" }
            )
        }

        $result = Measure-ValveRegionLatency -Target $target -SampleCount 3

        $result.TargetLabel | Should -Be "Frankfurt"
        $result.ResolvedEndpoint | Should -Be "second.valid"
        $result.FallbackUsed | Should -Be $true
        $result.TimeoutCount | Should -Be 0
        $result.AvgRttMs | Should -BeGreaterThan 18
    }

    It "returns timeout-only results when no candidate responds" {
        Mock Invoke-LatencyCandidateProbe {
            param($TargetHost, $SampleCount, $TimeoutSeconds)
            [PSCustomObject]@{ Samples = @(); TimeoutCount = 3 }
        }

        $target = [PSCustomObject]@{
            Label      = "Madrid"
            Notes      = "proxy"
            Provenance = "test"
            Candidates = @([PSCustomObject]@{ Host = "timeout.invalid"; Notes = "" })
        }

        $result = Measure-ValveRegionLatency -Target $target -SampleCount 3

        $result.AvgRttMs | Should -BeNullOrEmpty
        $result.SuccessfulSamples | Should -Be 0
        $result.TimeoutCount | Should -Be 3
        $result.FallbackUsed | Should -Be $false
    }

    It "uses TCP timing when a target asks for TCP probes" {
        Mock Invoke-TcpLatencyCandidateProbe {
            [PSCustomObject]@{ Samples = @(12.0, 14.0, 16.0); TimeoutCount = 0 }
        }

        $target = [PSCustomObject]@{
            Label = "Frankfurt"
            ProtocolPreference = "TCP"
            Notes = "proxy"
            Provenance = "test"
            Candidates = @([PSCustomObject]@{ Host = "cm.test"; Port = 27017; Notes = "" })
        }

        $result = Measure-ValveRegionLatency -Target $target -SampleCount 3

        $result.AvgRttMs | Should -Be 14.0
        $result.ProtocolUsed | Should -Be "TCP/27017"
        Should -Invoke Invoke-TcpLatencyCandidateProbe -Exactly 1
    }

    It "ignores failed Test-Connection objects that still carry a latency property" {
        $samples = ConvertTo-LatencySamples -ProbeResult ([PSCustomObject]@{
            Status  = "DestinationNetworkUnreachable"
            Latency = 0
        })

        @($samples).Count | Should -Be 0
    }
}

Describe "Invoke-ValveRegionLatencyDiagnostic" {

    BeforeEach {
        Reset-TestState
    }

    It "persists a timestamped diagnostic run in latency_history.json" {
        Mock Get-NetworkDiagnosticSummary {
            [PSCustomObject]@{
                AdapterFound = $true
                AdapterName  = "Ethernet"
                AdapterType  = "Physical / wired"
                DnsProvider  = "Cloudflare"
                DnsServers   = @("1.1.1.1", "1.0.0.1")
            }
        }
        Mock Get-ValveRegionTargets {
            @(
                [PSCustomObject]@{
                    Label = "Stockholm"
                    Notes = "proxy"
                    Provenance = "test"
                    Candidates = @([PSCustomObject]@{ Host = "stockholm.host"; Notes = "" })
                }
            )
        }
        Mock Measure-ValveRegionLatency {
            [PSCustomObject]@{
                TargetLabel       = $Target.Label
                ResolvedEndpoint  = $Target.Candidates[0].Host
                ProtocolUsed      = "ICMP"
                SampleCount       = 3
                SuccessfulSamples = 3
                MinRttMs          = 12.1
                MedianRttMs       = 12.5
                AvgRttMs          = 12.8
                TimeoutCount      = 0
                FallbackUsed      = $false
                Notes             = $Target.Notes
                Provenance        = $Target.Provenance
            }
        }

        $run = Invoke-ValveRegionLatencyDiagnostic -Kind baseline
        $history = Get-Content $CFG_LatencyHistoryFile -Raw | ConvertFrom-Json

        $run.Kind | Should -Be "baseline"
        @($history.Runs).Count | Should -Be 1
        $history.Runs[0].AdapterName | Should -Be "Ethernet"
        $history.Runs[0].Disclaimer | Should -Match "route-quality proxy"
    }

    It "builds side-by-side comparison rows from baseline and post runs" {
        $baseline = [PSCustomObject]@{
            Results = @(
                [PSCustomObject]@{ TargetLabel = "Frankfurt"; AvgRttMs = 18.4; TimeoutCount = 0; ProtocolUsed = "ICMP"; ResolvedEndpoint = "a"; FallbackUsed = $false }
            )
        }
        $post = [PSCustomObject]@{
            Results = @(
                [PSCustomObject]@{ TargetLabel = "Frankfurt"; AvgRttMs = 15.9; TimeoutCount = 1; ProtocolUsed = "ICMP"; ResolvedEndpoint = "b"; FallbackUsed = $true }
            )
        }

        $rows = @(Get-ValveLatencyComparisonRows -BaselineRun $baseline -PostRun $post)

        $rows.Count | Should -Be 1
        $rows[0].DeltaMs | Should -Be -2.5
        $rows[0].BaselineSort | Should -Be 18.4
        $rows[0].PostSort | Should -Be 15.9
        $rows[0].DeltaSort | Should -Be -2.5
        $rows[0].FallbackUsed | Should -Be $true
        $rows[0].TimeoutSummary | Should -Be "0 -> 1"
    }

    It "shows baseline-only rows before a post-change run exists" {
        $baseline = [PSCustomObject]@{
            Results = @(
                [PSCustomObject]@{ TargetLabel = "Timeout"; AvgRttMs = $null; TimeoutCount = 3; SampleCount = 3; ProtocolUsed = "ICMP"; ResolvedEndpoint = "a"; FallbackUsed = $false },
                [PSCustomObject]@{ TargetLabel = "Frankfurt"; AvgRttMs = 8.0; TimeoutCount = 0; SampleCount = 3; ProtocolUsed = "ICMP"; ResolvedEndpoint = "b"; FallbackUsed = $false },
                [PSCustomObject]@{ TargetLabel = "London"; AvgRttMs = 21.0; TimeoutCount = 0; SampleCount = 3; ProtocolUsed = "ICMP"; ResolvedEndpoint = "c"; FallbackUsed = $false }
            )
        }

        $rows = @(Get-ValveLatencyComparisonRows -BaselineRun $baseline -PostRun $null)

        $rows.Count | Should -Be 3
        $rows[0].TargetLabel | Should -Be "Frankfurt"
        $rows[0].BaselineSort | Should -Be 8.0
        $rows[1].TargetLabel | Should -Be "London"
        $rows[2].TargetLabel | Should -Be "Timeout"
        $rows[2].BaselineSort | Should -Be ([double]::PositiveInfinity)
        $rows[2].TimeoutSummary | Should -Be "3 / 3"
    }

    It "sorts baseline-vs-post rows by the lowest post RTT" {
        $baseline = [PSCustomObject]@{
            Results = @(
                [PSCustomObject]@{ TargetLabel = "Frankfurt"; AvgRttMs = 8.0; TimeoutCount = 0; ProtocolUsed = "ICMP"; ResolvedEndpoint = "a"; FallbackUsed = $false },
                [PSCustomObject]@{ TargetLabel = "London"; AvgRttMs = 21.0; TimeoutCount = 0; ProtocolUsed = "ICMP"; ResolvedEndpoint = "b"; FallbackUsed = $false }
            )
        }
        $post = [PSCustomObject]@{
            Results = @(
                [PSCustomObject]@{ TargetLabel = "London"; AvgRttMs = 20.0; TimeoutCount = 0; ProtocolUsed = "ICMP"; ResolvedEndpoint = "b"; FallbackUsed = $false },
                [PSCustomObject]@{ TargetLabel = "Timeout"; AvgRttMs = $null; TimeoutCount = 3; ProtocolUsed = "ICMP"; ResolvedEndpoint = "c"; FallbackUsed = $false },
                [PSCustomObject]@{ TargetLabel = "Frankfurt"; AvgRttMs = 6.0; TimeoutCount = 0; ProtocolUsed = "ICMP"; ResolvedEndpoint = "a"; FallbackUsed = $false }
            )
        }

        $rows = @(Get-ValveLatencyComparisonRows -BaselineRun $baseline -PostRun $post)

        $rows[0].TargetLabel | Should -Be "Frankfurt"
        $rows[1].TargetLabel | Should -Be "London"
        $rows[2].TargetLabel | Should -Be "Timeout"
    }

    It "can sort comparison rows by region name" {
        $post = [PSCustomObject]@{
            Results = @(
                [PSCustomObject]@{ TargetLabel = "London"; AvgRttMs = 20.0; TimeoutCount = 0; SampleCount = 3; ProtocolUsed = "ICMP"; ResolvedEndpoint = "b"; FallbackUsed = $false },
                [PSCustomObject]@{ TargetLabel = "Amsterdam"; AvgRttMs = 7.0; TimeoutCount = 0; SampleCount = 3; ProtocolUsed = "ICMP"; ResolvedEndpoint = "a"; FallbackUsed = $false }
            )
        }

        $rows = @(Get-ValveLatencyComparisonRows -BaselineRun $null -PostRun $post -SortBy Region)

        $rows[0].TargetLabel | Should -Be "Amsterdam"
        $rows[1].TargetLabel | Should -Be "London"
    }

    It "can sort comparison rows by blocked status" {
        Mock Get-BlockedValveRelayRegions {
            @([PSCustomObject]@{ RegionName = "London" })
        }
        $post = [PSCustomObject]@{
            Results = @(
                [PSCustomObject]@{ TargetLabel = "Amsterdam"; AvgRttMs = 7.0; TimeoutCount = 0; SampleCount = 3; ProtocolUsed = "ICMP"; ResolvedEndpoint = "a"; FallbackUsed = $false },
                [PSCustomObject]@{ TargetLabel = "London"; AvgRttMs = 20.0; TimeoutCount = 0; SampleCount = 3; ProtocolUsed = "ICMP"; ResolvedEndpoint = "b"; FallbackUsed = $false }
            )
        }

        $rows = @(Get-ValveLatencyComparisonRows -BaselineRun $null -PostRun $post -SortBy Blocked)

        $rows[0].TargetLabel | Should -Be "London"
    }
}

Describe "Get-NetworkDiagnosticDnsState" {

    It "handles a single DNS server returned as a scalar under StrictMode" {
        Mock Get-DnsClientServerAddress {
            [PSCustomObject]@{
                ServerAddresses = "1.1.1.1"
            }
        }

        $state = Get-NetworkDiagnosticDnsState -InterfaceIndex 7

        $state.Provider | Should -Be "Custom"
        @($state.Servers).Count | Should -Be 1
        $state.Servers[0] | Should -Be "1.1.1.1"
    }
}

Describe "Get-LatencyHistoryRows" {

    BeforeEach {
        Reset-TestState
    }

    It "uses the selected region RTT instead of the global average headline" {
        $run = [PSCustomObject]@{
            RunId = "run-1"
            Kind = "baseline"
            Timestamp = "2026-05-15 13:10:00"
            AdapterName = "Ethernet"
            DnsProvider = "Custom"
            Results = @(
                [PSCustomObject]@{ TargetLabel = "Frankfurt"; AvgRttMs = 6.0 },
                [PSCustomObject]@{ TargetLabel = "London"; AvgRttMs = 21.0 }
            )
        }
        Save-LatencyHistoryRun -Run $run | Out-Null

        $rows = @(Get-LatencyHistoryRows -SelectedRegion "London")

        $rows[0].AvgRttMs | Should -Be 13.5
        $rows[0].SelectedRegion | Should -Be "London"
        $rows[0].RegionRttMs | Should -Be 21.0
    }
}

Describe "Set-NetworkDiagnosticDnsProfile" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()
        New-TestBackupFile -Entries @()
    }

    It "sets Cloudflare DNS and records a backup step" {
        Mock Get-NetworkDiagnosticSummary {
            [PSCustomObject]@{
                AdapterFound   = $true
                AdapterName    = "Ethernet"
                InterfaceIndex = 7
                AdapterType    = "Physical / wired"
                DnsProvider    = "DHCP"
                DnsServers     = @()
            }
        }
        Mock Test-BackupLock { $false }
        Mock Set-BackupLock {}
        Mock Remove-BackupLock {}
        Mock Backup-DnsConfig {}
        Mock Flush-BackupBuffer {}
        Mock Set-DnsClientServerAddress {}

        $result = Set-NetworkDiagnosticDnsProfile -Provider Cloudflare

        $result.Changed | Should -Be $true
        $result.Provider | Should -Be "Cloudflare"
        Should -Invoke Backup-DnsConfig -Exactly 1 -ParameterFilter { $AdapterName -eq "Ethernet" -and $InterfaceIndex -eq 7 }
        Should -Invoke Set-DnsClientServerAddress -Exactly 1 -ParameterFilter {
            $InterfaceIndex -eq 7 -and ($ServerAddresses -join ',') -eq ($CFG_DNS_Cloudflare -join ',')
        }
        Should -Invoke Flush-BackupBuffer -Exactly 1
    }

    It "resets DNS to DHCP when requested" {
        Mock Get-NetworkDiagnosticSummary {
            [PSCustomObject]@{
                AdapterFound   = $true
                AdapterName    = "Ethernet"
                InterfaceIndex = 7
                AdapterType    = "Physical / wired"
                DnsProvider    = "Google"
                DnsServers     = @("8.8.8.8", "8.8.4.4")
            }
        }
        Mock Test-BackupLock { $false }
        Mock Set-BackupLock {}
        Mock Remove-BackupLock {}
        Mock Backup-DnsConfig {}
        Mock Flush-BackupBuffer {}
        Mock Set-DnsClientServerAddress {}

        $result = Set-NetworkDiagnosticDnsProfile -Provider DHCP

        $result.Changed | Should -Be $true
        Should -Invoke Set-DnsClientServerAddress -Exactly 1 -ParameterFilter {
            $InterfaceIndex -eq 7 -and $ResetServerAddresses
        }
    }
}

Describe "Valve relay firewall blocks" {

    BeforeEach {
        Reset-TestState
    }

    It "creates an outbound block rule for all relay addresses in the selected region" {
        Mock Get-ValveRegionTargets {
            @(
                [PSCustomObject]@{
                    Label = "Frankfurt (Germany)"
                    Candidates = @(
                        [PSCustomObject]@{ Host = "155.133.226.68" },
                        [PSCustomObject]@{ Host = "155.133.226.69" },
                        [PSCustomObject]@{ Host = "155.133.226.68" }
                    )
                }
            )
        }
        Mock Remove-NetFirewallRule {}
        Mock New-NetFirewallRule {}

        $result = Block-ValveRelayRegion -RegionName "Frankfurt (Germany)"

        $result.AddressCount | Should -Be 2
        $result.RuleName | Should -Be "CS2 Optimize Relay Block - Frankfurt (Germany)"
        Should -Invoke Remove-NetFirewallRule -Exactly 1 -ParameterFilter {
            $DisplayName -eq "CS2 Optimize Relay Block - Frankfurt (Germany)"
        }
        Should -Invoke New-NetFirewallRule -Exactly 1 -ParameterFilter {
            $DisplayName -eq "CS2 Optimize Relay Block - Frankfurt (Germany)" -and
            $Direction -eq "Outbound" -and
            $Action -eq "Block" -and
            ($RemoteAddress -join ',') -eq "155.133.226.68,155.133.226.69"
        }
    }

    It "removes only CS2 Optimize relay block rules when unblocking all" {
        Mock Get-NetFirewallRule {
            @(
                [PSCustomObject]@{ DisplayName = "CS2 Optimize Relay Block - Frankfurt (Germany)" },
                [PSCustomObject]@{ DisplayName = "CS2 Optimize Relay Block - London (England)" }
            )
        }
        Mock Remove-NetFirewallRule {}

        $result = Unblock-AllValveRelayRegions

        $result.Count | Should -Be 2
        Should -Invoke Remove-NetFirewallRule -Exactly 2
    }

    It "can block the known Falkenstein Hetzner hosted target addresses" {
        Mock Get-ValveRegionTargets {
            Get-KnownValveHostedRegionTargets
        }
        Mock Remove-NetFirewallRule {}
        Mock New-NetFirewallRule {}

        $result = Block-ValveRelayRegion -RegionName "Falkenstein (Germany) - Hetzner hosted"

        $result.AddressCount | Should -Be 7
        Should -Invoke New-NetFirewallRule -Exactly 1 -ParameterFilter {
            $DisplayName -eq "CS2 Optimize Relay Block - Falkenstein (Germany) - Hetzner hosted" -and
            ($RemoteAddress -join ',') -eq "138.199.142.208,138.199.142.209,138.199.142.210,138.199.142.211,138.199.142.212,138.199.142.213,138.199.142.214"
        }
    }
}
