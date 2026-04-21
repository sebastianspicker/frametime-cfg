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
        $targets = @(Get-ValveRegionTargets)

        $targets.Count | Should -BeGreaterThan 0
        $targets[0].PSObject.Properties.Name | Should -Contain "Label"
        @($targets[0].Candidates).Count | Should -BeGreaterThan 0
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
        $rows[0].FallbackUsed | Should -Be $true
        $rows[0].TimeoutSummary | Should -Be "0 → 1"
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
