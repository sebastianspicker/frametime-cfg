# ==============================================================================
#  helpers/gui-network.ps1  -  Network panel functions and event handlers
#  Loaded by helpers/gui-panels.ps1 after the shared GUI helpers are defined.
# ==============================================================================

# ==============================================================================
# NETWORK
# ==============================================================================
function Get-NetSelectedRegion {
    $selected = (El "NetDiagRegionPicker").SelectedItem
    if ($selected) { return [string]$selected }
    return ""
}

function Get-NetSortMode {
    $selected = (El "NetDiagSortPicker").SelectedItem
    if ($selected) { return [string]$selected }
    return "Ping"
}

function Initialize-NetSortPicker {
    $picker = El "NetDiagSortPicker"
    if ($picker.Items.Count -gt 0) { return }
    foreach ($mode in @("Ping", "Region", "Delta", "Timeouts", "Blocked")) {
        [void]$picker.Items.Add($mode)
    }
    $picker.SelectedItem = "Ping"
}

function Update-NetRegionPicker {
    param($ComparisonRows)

    $picker = El "NetDiagRegionPicker"
    $current = Get-NetSelectedRegion
    $regions = @($ComparisonRows | ForEach-Object { $_.TargetLabel } | Where-Object { $_ } | Select-Object -Unique)

    $Script:NetworkRegionPickerUpdating = $true
    try {
        $picker.Items.Clear()
        foreach ($region in $regions) {
            [void]$picker.Items.Add($region)
        }

        if ($regions.Count -eq 0) {
            $picker.SelectedIndex = -1
            return ""
        }

        if ($current -and $current -in $regions) {
            $picker.SelectedItem = $current
            return $current
        }

        $picker.SelectedIndex = 0
        return [string]$regions[0]
    } finally {
        $Script:NetworkRegionPickerUpdating = $false
    }
}

function Update-NetRegionSummary {
    param(
        [string]$SelectedRegion,
        $ComparisonRows
    )

    if ([string]::IsNullOrWhiteSpace($SelectedRegion)) {
        (El "NetDiagRegionSummary").Text = "Run a baseline test to choose a region."
        return
    }

    $row = @($ComparisonRows | Where-Object { $_.TargetLabel -eq $SelectedRegion } | Select-Object -First 1)
    if (-not $row) {
        (El "NetDiagRegionSummary").Text = "Selected region is not present in the latest run."
        return
    }

    $baseline = if ($null -ne $row.BaselineAvgMs) { "$($row.BaselineAvgMs) ms" } else { "timeout" }
    $post = if ($null -ne $row.PostAvgMs) { "$($row.PostAvgMs) ms" } else { "not run" }
    $delta = if ($null -ne $row.DeltaMs) {
        $sign = if ($row.DeltaMs -gt 0) { "+" } else { "" }
        "  |  Delta: $sign$($row.DeltaMs) ms"
    } else { "" }
    (El "NetDiagRegionSummary").Text = "$SelectedRegion  |  Baseline: $baseline  |  Post: $post$delta"
}

function Update-NetFirewallSummary {
    try {
        $blocked = @(Get-BlockedValveRelayRegions)
        if ($blocked.Count -gt 0) {
            (El "NetDiagFirewallSummary").Text = "Blocked: $(@($blocked.RegionName) -join ', ')"
        } else {
            (El "NetDiagFirewallSummary").Text = "No CS2 network blocks active."
        }
    } catch {
        (El "NetDiagFirewallSummary").Text = "Firewall state unavailable."
    }
}

function Load-NetworkDiagnostics {
    Initialize-NetSortPicker
    $summary = Get-NetworkDiagnosticSummary
    if (-not $summary.AdapterFound) {
        (El "NetDiagAdapterSummary").Text = "Adapter: no active adapter found"
        (El "NetDiagDnsSummary").Text = "DNS: unavailable"
    } else {
        $dnsText = if (@($summary.DnsServers).Count -gt 0) { @($summary.DnsServers) -join ', ' } else { "automatic / DHCP" }
        (El "NetDiagAdapterSummary").Text = "Adapter: $($summary.AdapterName)  |  $($summary.AdapterType)"
        (El "NetDiagDnsSummary").Text = "DNS: $($summary.DnsProvider)  |  $dnsText"
    }

    $comparisonRows = @(Get-ValveLatencyComparisonRows -SortBy (Get-NetSortMode))
    $selectedRegion = Update-NetRegionPicker -ComparisonRows $comparisonRows
    Update-NetRegionSummary -SelectedRegion $selectedRegion -ComparisonRows $comparisonRows

    $historyRows = @(Get-LatencyHistoryRows -SelectedRegion $selectedRegion)
    (El "NetDiagHistoryGrid").ItemsSource = $historyRows
    (El "NetDiagComparisonGrid").ItemsSource = $comparisonRows
    (El "NetDiagHistorySummary").Text = if ($historyRows.Count -gt 0) {
        $latest = $historyRows[-1]
        $latestRegion = if ($latest.PSObject.Properties['SelectedRegion'] -and $latest.SelectedRegion) { [string]$latest.SelectedRegion } else { $selectedRegion }
        $latestRegionRtt = if ($latest.PSObject.Properties['RegionRttMs']) { $latest.RegionRttMs } else { $null }
        $rtt = if ($null -ne $latestRegionRtt) { "$latestRegionRtt ms" } else { "timeout" }
        "Latest run: $($latest.Timestamp)  |  $($latest.Kind)  |  ${latestRegion}: $rtt"
    } else {
        "No latency diagnostics recorded yet."
    }
    Update-NetFirewallSummary
}

function Invoke-GuiValveRelayBlock {
    param(
        [ValidateSet("block", "unblock", "unblockAll")][string]$Action
    )

    try {
        if ($Action -eq "unblockAll") {
            $confirm = [System.Windows.MessageBox]::Show(
                "Remove all firewall rules created by frametime.cfg for Valve network blocking?",
                "Valve Network Blocks", "YesNo", "Question")
            if ($confirm -ne "Yes") { return }
            $result = Unblock-AllValveRelayRegions
            Load-NetworkDiagnostics
            [System.Windows.MessageBox]::Show("Removed $($result.Count) network block rule(s).", "Valve Network Blocks")
            return
        }

        $region = Get-NetSelectedRegion
        if ([string]::IsNullOrWhiteSpace($region)) {
            [System.Windows.MessageBox]::Show("Select a focus region first.", "Valve Network Blocks", "OK", "Warning")
            return
        }

        if ($Action -eq "block") {
            $confirm = [System.Windows.MessageBox]::Show(
                "Block outbound traffic to Valve relay/server targets for:`n$region`n`nThis may prevent CS2 from using that region until you unblock it.",
                "Block Focus Region", "YesNo", "Warning")
            if ($confirm -ne "Yes") { return }
            $result = Block-ValveRelayRegion -RegionName $region
            Load-NetworkDiagnostics
            [System.Windows.MessageBox]::Show("Blocked $($result.AddressCount) address(es) for $region.", "Valve Network Blocks")
        } else {
            $result = Unblock-ValveRelayRegion -RegionName $region
            Load-NetworkDiagnostics
            if ($result.Changed) {
                [System.Windows.MessageBox]::Show("Unblocked $region.", "Valve Network Blocks")
            } else {
                [System.Windows.MessageBox]::Show("$region was not blocked by frametime.cfg.", "Valve Network Blocks")
            }
        }
    } catch {
        [System.Windows.MessageBox]::Show("Firewall update failed:`n$($_.Exception.Message)`n`nRun the GUI as Administrator and make sure Windows Firewall is available.", "Valve Network Blocks", "OK", "Error")
    }
}

function Start-LatencyDiagnostic {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [ValidateSet("baseline", "post")][string]$Kind
    )

    if ($Script:LatencyInFlight) { return }
    if (-not $PSCmdlet.ShouldProcess("network diagnostics panel", "Start $Kind latency diagnostic")) { return }
    $Script:LatencyInFlight = $true
    $buttonName = if ($Kind -eq "baseline") { "BtnNetBaseline" } else { "BtnNetPost" }
    (El "BtnNetBaseline").IsEnabled = $false
    (El "BtnNetPost").IsEnabled = $false
    (El $buttonName).Content = if ($Kind -eq "baseline") { "Running..." } else { "Retesting..." }
    Set-UISyncValue -Store $Script:UISync -Name "LatencyError" -Value $null
    Set-UISyncValue -Store $Script:UISync -Name "LatencyRun" -Value $null

    Invoke-Async -Work {
        param($ScriptRoot, $UISync, $RunKind)
        . "$ScriptRoot\config.env.ps1"
        . "$ScriptRoot\helpers.ps1"
        try {
            $UISync["LatencyRun"] = Invoke-ValveRegionLatencyDiagnostic -Kind $RunKind
        } catch {
            $UISync["LatencyError"] = $_.Exception.Message
        }
    } -WorkArgs @($Script:Root, $Script:UISync, $Kind) -OnDone {
        $err = Get-UISyncValue -Store $Script:UISync -Name "LatencyError"
        Load-NetworkDiagnostics
        if ($err) {
            [System.Windows.MessageBox]::Show("Latency diagnostic failed:`n$err", "Network Diagnostic", "OK", "Error")
        } else {
            $run = Get-UISyncValue -Store $Script:UISync -Name "LatencyRun"
            $okRegions = @($run.Results | Where-Object { $null -ne $_.AvgRttMs }).Count
            [System.Windows.MessageBox]::Show("Saved $($run.Kind) run at $($run.Timestamp).`nResponsive regions: $okRegions / $(@($run.Results).Count)", "Network Diagnostic")
        }
        Set-UISyncValue -Store $Script:UISync -Name "LatencyRun" -Value $null
        Set-UISyncValue -Store $Script:UISync -Name "LatencyError" -Value $null
    } -OnError {
        param($asyncError)
        [System.Windows.MessageBox]::Show("Latency diagnostic failed: $asyncError", "Network Diagnostic", "OK", "Error")
    } -OnFinally {
        $Script:LatencyInFlight = $false
        (El "BtnNetBaseline").IsEnabled = $true
        (El "BtnNetPost").IsEnabled = $true
        (El "BtnNetBaseline").Content = "Run baseline test"
        (El "BtnNetPost").Content = "Run post-change retest"
    }
}

function Invoke-GuiDnsProfileChange {
    param(
        [ValidateSet("Cloudflare", "Google", "DHCP")][string]$Provider
    )

    try {
        $result = Set-NetworkDiagnosticDnsProfile -Provider $Provider
        Load-NetworkDiagnostics
        if ($result.Changed) {
            [System.Windows.MessageBox]::Show("DNS updated on $($result.AdapterName): $Provider", "DNS Updated")
        } else {
            [System.Windows.MessageBox]::Show("DNS is already set to $Provider on $($result.AdapterName).", "DNS Unchanged")
        }
    } catch {
        [System.Windows.MessageBox]::Show("DNS update failed:`n$($_.Exception.Message)", "DNS Error", "OK", "Error")
    }
}

(El "BtnNetRefresh").Add_Click({ Load-NetworkDiagnostics })
(El "NetDiagSortPicker").Add_SelectionChanged({ Load-NetworkDiagnostics })
(El "NetDiagRegionPicker").Add_SelectionChanged({
    if (-not $Script:NetworkRegionPickerUpdating) { Load-NetworkDiagnostics }
})
(El "BtnNetBaseline").Add_Click({ Start-LatencyDiagnostic -Kind baseline })
(El "BtnNetPost").Add_Click({ Start-LatencyDiagnostic -Kind post })
(El "BtnNetBlockRegion").Add_Click({ Invoke-GuiValveRelayBlock -Action block })
(El "BtnNetUnblockRegion").Add_Click({ Invoke-GuiValveRelayBlock -Action unblock })
(El "BtnNetUnblockAllRegions").Add_Click({ Invoke-GuiValveRelayBlock -Action unblockAll })
(El "BtnNetDnsCloudflare").Add_Click({ Invoke-GuiDnsProfileChange -Provider Cloudflare })
(El "BtnNetDnsGoogle").Add_Click({ Invoke-GuiDnsProfileChange -Provider Google })
(El "BtnNetDnsDhcp").Add_Click({ Invoke-GuiDnsProfileChange -Provider DHCP })
(El "BtnNetDnsRestore").Add_Click({
    try {
        $ok = Restore-LatestDnsBackup
        Load-NetworkDiagnostics
        if ($ok) {
            [System.Windows.MessageBox]::Show("Restored the latest GUI DNS backup.", "DNS Restore")
        } else {
            [System.Windows.MessageBox]::Show("No GUI DNS backup was found.", "DNS Restore", "OK", "Warning")
        }
    } catch {
        [System.Windows.MessageBox]::Show("DNS restore failed:`n$($_.Exception.Message)", "DNS Restore", "OK", "Error")
    }
})
