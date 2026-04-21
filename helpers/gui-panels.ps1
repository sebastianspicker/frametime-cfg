# ==============================================================================
#  helpers/gui-panels.ps1  —  GUI Panel Functions & Event Handlers
# ==============================================================================
#
#  Extracted from CS2-Optimize-GUI.ps1 to keep the main file under 800 lines.
#  Dot-sourced into the same scope — all functions have access to $Window, El(),
#  $Script:UISync, $Script:Root, and all helper modules.
#
#  Panels: Dashboard, Analyze, Optimize, Backup, Benchmark, Video, Settings
#  Shared: Launch-Terminal, Save-SettingsToState

$Script:DashboardLastLoad = [datetime]::MinValue
$Script:StartupDriftChecked = $false

function Get-StateDataSafe {
    try {
        if (Test-Path $CFG_StateFile) {
            return Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
        }
    } catch {
        Write-DebugLog "State load failed: $($_.Exception.Message)"
    }
    return $null
}

function Save-StateDataSafe {
    param([Parameter(Mandatory)]$State)
    Save-JsonAtomic -Data $State -Path $CFG_StateFile
    Set-SecureAcl -Path $CFG_StateFile
}

function New-DefaultState {
    return [PSCustomObject]@{
        mode    = "AUTO"
        profile = "RECOMMENDED"
    }
}

function Should-SkipStartupDriftCheck {
    param(
        $State,
        [datetime]$Now = (Get-Date)
    )
    if (-not $State -or -not $State.PSObject.Properties['startup_last_verified']) { return $false }
    try {
        $lastVerified = [datetime]::Parse([string]$State.startup_last_verified)
        return (($Now - $lastVerified).TotalMinutes -lt 60)
    } catch {
        return $false
    }
}

function Test-StartupConfigDrift {
    $state = Get-StateDataSafe
    $now = Get-Date
    if (Should-SkipStartupDriftCheck -State $state -Now $now) {
        return [PSCustomObject]@{
            Skipped      = $true
            HasDrift     = $false
            DriftCount   = 0
            CheckedCount = 0
            DriftLabels  = @()
            CheckedAt    = [string]$state.startup_last_verified
        }
    }

    $checks = @(
        @{ Path = "HKLM:\SOFTWARE\Microsoft\Windows\Dwm"; Name = "OverlayTestMode"; Expected = 5; Label = "MPO disabled" }
        @{ Path = "HKCU:\SOFTWARE\Microsoft\GameBar"; Name = "AutoGameModeEnabled"; Expected = 1; Label = "Game Mode enabled" }
        @{ Path = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\GameDVR"; Name = "AppCaptureEnabled"; Expected = 0; Label = "Game DVR capture disabled" }
        @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\kernel"; Name = "GlobalTimerResolutionRequests"; Expected = 1; Label = "Timer Resolution enabled" }
        @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power"; Name = "HiberbootEnabled"; Expected = 0; Label = "Fast Startup disabled" }
    )

    $driftLabels = [System.Collections.Generic.List[string]]::new()
    foreach ($check in $checks) {
        $result = Test-RegistryCheck $check.Path $check.Name $check.Expected $check.Label -Quiet
        if ($result.Status -ne "OK") {
            $driftLabels.Add($check.Label) | Out-Null
        }
    }

    if (-not $state) {
        $state = New-DefaultState
    }
    $state | Add-Member -NotePropertyName "startup_last_verified" -NotePropertyValue ($now.ToString("o")) -Force
    try {
        Save-StateDataSafe -State $state
    } catch {
        Write-DebugLog "Startup drift state save failed: $($_.Exception.Message)"
    }

    return [PSCustomObject]@{
        Skipped      = $false
        HasDrift     = ($driftLabels.Count -gt 0)
        DriftCount   = $driftLabels.Count
        CheckedCount = $checks.Count
        DriftLabels  = @($driftLabels)
        CheckedAt    = $now.ToString("yyyy-MM-dd HH:mm")
    }
}

function Update-StartupDriftBanner {
    if ($Script:StartupDriftChecked) { return }
    $Script:StartupDriftChecked = $true

    $result = Test-StartupConfigDrift
    if ($result.Skipped -or -not $result.HasDrift) {
        (El "DashDriftBanner").Visibility = "Collapsed"
        return
    }

    (El "DashDriftBannerTitle").Text = "Configuration Drift Detected"
    (El "DashDriftBannerText").Text = "$($result.DriftCount) of $($result.CheckedCount) quick checks drifted. Run Verify-Settings to review and repair the full set."
    (El "DashDriftBanner").Visibility = "Visible"
}

function Get-BenchmarkCapFromText {
    param([string]$Text)
    if ($Text -notmatch "Avg\s*=\s*([\d.]+)") { return $null }

    $avg = [double]$Matches[1]
    $cap = [math]::Max($CFG_FpsCap_Min, [math]::Floor($avg - [math]::Floor($avg * $CFG_FpsCap_Percent)))
    return [PSCustomObject]@{
        AvgFps = $avg
        Cap    = $cap
    }
}

function Get-BenchmarkResultFromText {
    param([string]$Text)
    if ($Text -notmatch "Avg\s*=\s*([\d.]+).*P1\s*=\s*([\d.]+)") { return $null }

    return [PSCustomObject]@{
        AvgFps = [double]$Matches[1]
        P1Fps  = [double]$Matches[2]
    }
}

function Switch-Panel {
    param([string]$PanelName, [scriptblock]$OnSwitch = $null)
    foreach ($p in $Script:AllPanels) {
        (El $p).Visibility = if ($p -eq $PanelName) { "Visible" } else { "Collapsed" }
    }
    foreach ($kv in $Script:NavMap.GetEnumerator()) {
        (El $kv.Value).Style = if ($kv.Key -eq $PanelName) { $ActiveStyle } else { $InactiveStyle }
    }
    $Script:ActivePanel = $PanelName
    if ($OnSwitch) { & $OnSwitch }
}

# ══════════════════════════════════════════════════════════════════════════════
# DASHBOARD
# ══════════════════════════════════════════════════════════════════════════════
function Load-Dashboard {
    if (([datetime]::Now - $Script:DashboardLastLoad).TotalSeconds -lt 30) { return }
    $Script:DashboardLastLoad = [datetime]::Now

    # Progress from progress.json
    try {
        $prog = Load-Progress
        if ($prog) {
            $allDone = @($prog.completedSteps) + @($prog.skippedSteps)
            $p1Done = if ($prog.phase -ge 1) {
                @($allDone | Where-Object { $_ -match "^P1:" }).Count
            } else { 0 }
            $p2Done = @($allDone | Where-Object { $_ -match "^P2:" }).Count
            $p3Done = if ($prog.phase -ge 3) {
                @($allDone | Where-Object { $_ -match "^P3:" }).Count
            } else { 0 }
            $Window.Dispatcher.Invoke({
                (El "ProgressP1").Value   = $p1Done
                (El "ProgressP1Txt").Text = "$p1Done / 38"
                (El "ProgressP2").Value   = $p2Done
                (El "ProgressP2Txt").Text = "$p2Done / 3"
                (El "ProgressP3").Value   = $p3Done
                (El "ProgressP3Txt").Text = "$p3Done / 13"
            })
        }
    } catch { Write-DebugLog "Dashboard progress load failed: $($_.Exception.Message)" }

    # Benchmark history
    try {
        $hist = @(Get-BenchmarkHistory)
        if ($hist -and $hist.Count -ge 2) {
            $first = $hist[0]; $last = $hist[-1]
            $dAvg  = if ($first.avgFps -gt 0) { [math]::Round(($last.avgFps - $first.avgFps) / $first.avgFps * 100, 1) } else { 0 }
            $dP1   = if ($first.p1Fps -gt 0)  { [math]::Round(($last.p1Fps  - $first.p1Fps)  / $first.p1Fps  * 100, 1) } else { 0 }
            $Window.Dispatcher.Invoke({
                (El "DashPerfBaseline").Text = "Baseline:  avg $($first.avgFps) fps   1%low $($first.p1Fps) fps"
                (El "DashPerfLatest"  ).Text = "Latest:    avg $($last.avgFps) fps   1%low $($last.p1Fps) fps"
                $sign   = if ($dAvg -gt 0) { "+" } else { "" }
                $signP1 = if ($dP1  -gt 0) { "+" } else { "" }
                (El "DashPerfDelta"   ).Text = "Δ avg: ${sign}${dAvg}%   Δ 1%low: ${signP1}${dP1}%"
                (El "DashPerfDelta"   ).Foreground = if ($dAvg -gt 0) { New-Brush "#22c55e" } elseif ($dAvg -lt 0) { New-Brush "#ef4444" } else { New-Brush "#6b7280" }
            })
        } elseif ($hist -and $hist.Count -eq 1) {
            $Window.Dispatcher.Invoke({
                (El "DashPerfBaseline").Text = "Baseline: avg $($hist[0].avgFps) fps  1%low $($hist[0].p1Fps) fps"
                (El "DashPerfLatest").Text = ""
                (El "DashPerfDelta").Text = ""
            })
        }
    } catch { Write-DebugLog "Dashboard benchmark history load failed: $($_.Exception.Message)" }

    # Hardware (async)
    Invoke-Async -Work {
        param($ScriptRoot, $UISync)
        . "$ScriptRoot\config.env.ps1"
        . "$ScriptRoot\helpers.ps1"
        try {
            $cpu  = (Get-CachedCpuInfo).Name
            $gpu  = Get-NvidiaDriverVersion
            $gpuN = if ($gpu) { $gpu.Name } else {
                (Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue | Select-Object -First 1).Caption }
            $gpuD = if ($gpu) { "Driver $($gpu.Version)" } else { "" }
            $ram  = Get-RamInfo
            $dc   = Test-DualChannel
            $nic  = Get-ActiveNicAdapter
            $os   = Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue
            $hags = try { (Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers" "HwSchMode" -ErrorAction Stop).HwSchMode } catch { $null }
            $cs2  = Get-CS2InstallPath
            $stPath = Get-SteamPath
            $vtxt = if ($stPath) { Get-ChildItem "$stPath\userdata\*\730\local\cfg\video.txt" -ErrorAction SilentlyContinue | Select-Object -First 1 } else { $null }
            $optExists = if ($cs2) { Test-Path "$cs2\game\csgo\cfg\optimization.cfg" } else { $false }
            $UISync.Hw = @{
                CpuName  = $cpu
                GpuName  = $gpuN; GpuDriver = $gpuD; GpuVendor = (Get-ChipsetVendor)
                RamGb    = if ($ram) { "$($ram.TotalGB) GB" } else { "?" }
                RamSpeed = if ($ram) { "$($ram.ActiveMhz) MT/s$(if (-not $ram.AtRatedSpeed) {' (below rated)'})" } else { "" }
                RamXmp   = if ($ram) { if ($ram.AtRatedSpeed) { "✓ Running at rated speed" } else { "⚠ Below rated speed — enable XMP/EXPO" } } else { "" }
                RamXmpOk = if ($ram) { $ram.AtRatedSpeed } else { $false }
                DualCh   = if ($dc) { $dc.Reason } else { "" }
                DualChOk = if ($dc) { $dc.DualChannel } else { $false }
                NicName  = if ($nic) { $nic.Name } else { "Not found" }
                NicSpeed = if ($nic) { "$([math]::Round($nic.Speed/1e6)) Mbps" } else { "" }
                NicType  = if ($nic) { "✓ Wired" } else { "⚠ No active wired NIC" }
                NicOk    = ($null -ne $nic)
                OsName   = if ($os) { $os.Caption -replace "Microsoft Windows ", "Windows " } else { "?" }
                OsBuild  = if ($os) { "Build $($os.BuildNumber)" } else { "" }
                HagsStr  = switch ($hags) { 2 {"HAGS: Enabled"} 1 {"HAGS: Disabled"} 0 {"HAGS: Disabled"} $null {"HAGS: Not set"} default {"HAGS: $hags"} }
                Cs2Found = ($null -ne $cs2)
                Cs2Path  = if ($cs2) { "CS2 installed" } else { "CS2 not found" }
                OptCfg   = if ($optExists) { "optimization.cfg: present" } else { "optimization.cfg: missing" }
                VideoTxt = if ($vtxt) { "video.txt: present" } else { "video.txt: missing" }
                OptOk    = $optExists
                VtxtOk   = ($null -ne $vtxt)
            }
        } catch { $UISync.HwErr = $_.Exception.Message }
        $UISync.HwDone = $true
    } -WorkArgs @($Script:Root, $Script:UISync) -OnDone {
        $hw = $Script:UISync.Hw
        if (-not $hw) {
            $hwErr = $Script:UISync.HwErr
            (El "CardCpuName").Text = if ($hwErr) { "Error: $hwErr" } else { "Detection failed" }
            return
        }
        (El "CardCpuName" ).Text = if ($hw.CpuName) { $hw.CpuName } else { "Unknown CPU" }
        (El "CardGpuName"  ).Text = if ($hw.GpuName) { $hw.GpuName } else { "Unknown GPU" }
        (El "CardGpuVendor").Text = if ($hw.GpuVendor) { $hw.GpuVendor } else { "" }
        (El "CardGpuDriver").Text = $hw.GpuDriver
        (El "CardRamSize" ).Text = $hw.RamGb
        (El "CardRamSpeed").Text = $hw.RamSpeed
        (El "CardRamXmp"  ).Text = $hw.RamXmp
        (El "CardRamXmp"  ).Foreground = if ($hw.RamXmpOk) { New-Brush "#22c55e" } else { New-Brush "#fbbf24" }
        (El "CardNicName" ).Text = $hw.NicName
        (El "CardNicSpeed").Text = $hw.NicSpeed
        (El "CardNicType" ).Text = $hw.NicType
        (El "CardNicType" ).Foreground = if ($hw.NicOk) { New-Brush "#22c55e" } else { New-Brush "#fbbf24" }
        (El "CardOsName"  ).Text = $hw.OsName
        (El "CardOsBuild" ).Text = $hw.OsBuild
        (El "CardOsHags"  ).Text = $hw.HagsStr
        (El "CardCs2Status").Text = $hw.Cs2Path
        (El "CardCs2Status").Foreground = if ($hw.Cs2Found) { New-Brush "#22c55e" } else { New-Brush "#ef4444" }
        (El "CardCs2Cfg"  ).Text = $hw.OptCfg
        (El "CardCs2Cfg"  ).Foreground = if ($hw.OptOk)  { New-Brush "#22c55e" } else { New-Brush "#fbbf24" }
        (El "CardCs2Video").Text = $hw.VideoTxt
        (El "CardCs2Video").Foreground = if ($hw.VtxtOk) { New-Brush "#22c55e" } else { New-Brush "#fbbf24" }
    }
}

# Quick action buttons
(El "BtnDashAnalyze"  ).Add_Click({ Switch-Panel "PanelAnalyze"; Start-Analysis })
(El "BtnDashVerify"   ).Add_Click({ Switch-Panel "PanelOptimize"; Load-Optimize; Start-InlineVerify })
(El "BtnDashBackup"   ).Add_Click({ Switch-Panel "PanelBackup"; Load-Backup })
(El "BtnDashPhase1"   ).Add_Click({ Launch-Terminal "Run-Optimize.ps1" })
(El "BtnDashLaunchCs2").Add_Click({ Start-Process "steam://rungameid/730" })

# ══════════════════════════════════════════════════════════════════════════════
# ANALYZE
# ══════════════════════════════════════════════════════════════════════════════
function Start-Analysis {
    (El "BtnRunAnalysis").IsEnabled = $false
    (El "BtnRunAnalysis").Content   = "Scanning…"
    (El "AnalyzeScanTime").Text     = "Scanning…"
    (El "AnalysisGrid").ItemsSource = $null

    Invoke-Async -Work {
        param($ScriptRoot, $UISync)
        . "$ScriptRoot\config.env.ps1"
        . "$ScriptRoot\helpers.ps1"
        . "$ScriptRoot\helpers\system-analysis.ps1"
        try { $UISync.AnalysisResults = Invoke-SystemAnalysis }
        catch { $UISync.AnalysisError = $_.Exception.Message }
    } -WorkArgs @($Script:Root, $Script:UISync) -OnDone {
        $analysisErr = $Script:UISync.AnalysisError
        $res = $Script:UISync.AnalysisResults
        if (-not $res) { $res = @() }
        (El "AnalysisGrid").ItemsSource = $res
        $ok   = @($res | Where-Object Status -eq "OK").Count
        $warn = @($res | Where-Object Status -eq "WARN").Count
        $err  = @($res | Where-Object Status -eq "ERR").Count
        (El "AnalyzeSummary" ).Text = "✓ $ok   ⚠ $warn   ✗ $err"
        if ($analysisErr) {
            (El "AnalyzeScanTime").Text = "Scan error: $analysisErr"
        } else {
            (El "AnalyzeScanTime").Text = "Last scan: $(Get-Date -Format 'HH:mm  dd-MMM-yyyy')  ·  $($res.Count) checks"
        }
        (El "BtnRunAnalysis" ).IsEnabled = $true
        (El "BtnRunAnalysis" ).Content   = "▶  Run Full Scan"
        if ($warn + $err -gt 0) {
            (El "DashIssueHint").Text = "⚠  $($warn+$err) item(s) need attention — see Analyze panel"
        }
        Refresh-StorageHealthCard
        # Clear for next run
        $Script:UISync.AnalysisError   = $null
        $Script:UISync.AnalysisResults = $null
    }
}

(El "BtnRunAnalysis"   ).Add_Click({ Start-Analysis })
(El "BtnAnalyzeGotoOpt").Add_Click({ Switch-Panel "PanelOptimize"; Load-Optimize })
(El "BtnAnalyzeExport" ).Add_Click({
    $res = (El "AnalysisGrid").ItemsSource
    if (-not $res) { return }
    $dlg = [Microsoft.Win32.SaveFileDialog]::new()
    $dlg.Filter = "CSV files (*.csv)|*.csv|All files (*.*)|*.*"
    $dlg.FileName = "cs2-analyze-$(Get-Date -Format 'yyyyMMdd-HHmm').csv"
    if ($dlg.ShowDialog() -eq $true) {
        try {
            $res | Export-Csv -Path $dlg.FileName -NoTypeInformation -Encoding UTF8
            [System.Windows.MessageBox]::Show("Exported to:`n$($dlg.FileName)", "Export Complete")
        } catch {
            [System.Windows.MessageBox]::Show("Export failed:`n$_`n`nCheck that the file is not open in another program.", "Export Error", "OK", "Error")
        }
    }
})

function Refresh-StorageHealthCard {
    try {
        $trim = Get-TrimHealthStatus
        (El "AnalyzeStorageHealth").Text = "Storage maintenance: $($trim.Summary)"
        (El "BtnAnalyzeTrimEnable").IsEnabled = $trim.AnyTrimDisabled
        (El "BtnAnalyzeRetrim").IsEnabled = $trim.RetrimAvailable
        if ($trim.RetrimAvailable) {
            (El "BtnAnalyzeRetrim").ToolTip = "ReTrim available on: $(@($trim.RetrimmableVolumes) -join ', ')"
        }
    } catch {
        (El "AnalyzeStorageHealth").Text = "Storage maintenance: state not readable"
        (El "BtnAnalyzeTrimEnable").IsEnabled = $false
        (El "BtnAnalyzeRetrim").IsEnabled = $false
    }
}

(El "BtnAnalyzeStorageRefresh").Add_Click({ Refresh-StorageHealthCard })
(El "BtnAnalyzeTrimEnable").Add_Click({
    try {
        $result = Enable-TrimSupport
        if ($result.Success) {
            [System.Windows.MessageBox]::Show("TRIM support enabled. This is storage maintenance/correctness, not a gaming-meta claim.", "Storage Health")
        } else {
            [System.Windows.MessageBox]::Show("Enable TRIM failed.`n$(@($result.Output) -join [Environment]::NewLine)", "Storage Health", "OK", "Warning")
        }
    } catch {
        [System.Windows.MessageBox]::Show("Enable TRIM failed:`n$($_.Exception.Message)", "Storage Health", "OK", "Error")
    }
    Refresh-StorageHealthCard
})
(El "BtnAnalyzeRetrim").Add_Click({
    try {
        $trim = Get-TrimHealthStatus
        $volumes = @($trim.RetrimmableVolumes)
        if ($volumes.Count -eq 0) {
            [System.Windows.MessageBox]::Show("No eligible fixed volumes were detected for ReTrim.", "Storage Health", "OK", "Warning")
            return
        }
        $confirm = [System.Windows.MessageBox]::Show(
            "Run ReTrim on: $($volumes -join ', ')?`n`nThis is a storage-maintenance action, not an FPS optimization.",
            "Storage Health", "YesNo", "Question")
        if ($confirm -ne "Yes") { return }
        foreach ($drive in $volumes) {
            Invoke-StorageRetrim -DriveLetter $drive
        }
        [System.Windows.MessageBox]::Show("ReTrim completed for: $($volumes -join ', ')", "Storage Health")
    } catch {
        [System.Windows.MessageBox]::Show("ReTrim failed:`n$($_.Exception.Message)", "Storage Health", "OK", "Error")
    }
    Refresh-StorageHealthCard
})

# ══════════════════════════════════════════════════════════════════════════════
# OPTIMIZE
# ══════════════════════════════════════════════════════════════════════════════
function Load-Optimize {
    $prog = $null
    try { $prog = Load-Progress } catch { Write-DebugLog "Optimize progress load failed: $($_.Exception.Message)" }
    $completed = if ($prog) { $prog.completedSteps } else { @() }
    $skipped   = if ($prog) { $prog.skippedSteps }   else { @() }

    $estimates = $CFG_ImprovementEstimates

    $rows = foreach ($s in $SCRIPT:StepCatalog) {
        $stepKey = "P$($s.Phase):$($s.Step)"
        $isDone  = $stepKey -in $completed
        $isSkip  = $stepKey -in $skipped

        $statusKey   = if ($s.CheckOnly) { "Check" } elseif ($isDone) { "Done" } elseif ($isSkip) { "Skipped" } else { "Pending" }
        $statusLabel = if ($s.CheckOnly) { "—  Check" } elseif ($isDone) { "✓  Done" } elseif ($isSkip) { "—  Skipped" } else { "○  Pending" }
        $statusColor = if ($s.CheckOnly) { "#6b7280" } elseif ($isDone) { "#22c55e" } elseif ($isSkip) { "#374151" } else { "#fbbf24" }

        $tierColor = switch ($s.Tier) { 1 { "#22c55e" } 2 { "#fbbf24" } 3 { "#e8520a" } default { "#6b7280" } }
        $riskColor = switch ($s.Risk) {
            "SAFE"       { "#22c55e" } "MODERATE"   { "#fbbf24" }
            "AGGRESSIVE" { "#e8520a" } "CRITICAL"   { "#ef4444" }
            default      { "#6b7280" }
        }

        $est = ""
        if ($s.EstKey -and $estimates.ContainsKey($s.EstKey)) {
            $e = $estimates[$s.EstKey]
            if ($e.P1LowMin -ne 0 -or $e.P1LowMax -ne 0) {
                $est = "+$($e.P1LowMin)-$($e.P1LowMax)% P1"
            }
        }

        [PSCustomObject]@{
            PhLabel     = "P$($s.Phase)"
            StepLabel   = "$($s.Step)"
            Category    = $s.Category
            Title       = $s.Title
            Tier        = $s.Tier
            TierLabel   = "T$($s.Tier)"
            TierColor   = $tierColor
            Risk        = $s.Risk
            RiskColor   = $riskColor
            StatusKey   = $statusKey
            StatusLabel = $statusLabel
            StatusColor = $statusColor
            RebootLabel = if ($s.Reboot) { "Yes" } else { "" }
            EstLabel    = $est
            _Step       = $s
        }
    }

    $SCRIPT:OptimizeAllRows = $rows
    (El "OptimizeGrid").ItemsSource = $rows

    # Populate category filter
    $cats = @("All") + ($rows | Select-Object -ExpandProperty Category -Unique | Sort-Object)
    (El "OptFilterCat").ItemsSource   = $cats
    (El "OptFilterCat").SelectedIndex = 0

    $statuses = @("All", "Pending", "Done", "Skipped", "Check")
    (El "OptFilterStatus").ItemsSource   = $statuses
    (El "OptFilterStatus").SelectedIndex = 0
}

(El "OptFilterCat"   ).Add_SelectionChanged({ Filter-OptimizeGrid })
(El "OptFilterStatus").Add_SelectionChanged({ Filter-OptimizeGrid })

function Filter-OptimizeGrid {
    $cat    = (El "OptFilterCat").SelectedItem
    $status = (El "OptFilterStatus").SelectedItem
    $all    = $SCRIPT:OptimizeAllRows
    if (-not $all) { return }
    $filtered = $all | Where-Object {
        ($cat    -eq "All" -or $_.Category -eq $cat) -and
        ($status -eq "All" -or $_.StatusKey -eq $status)
    }
    (El "OptimizeGrid").ItemsSource = @($filtered)
}

# ── Inline Verification ──────────────────────────────────────────────────────
# Checks actual system state (registry/services) and updates progress.json
# so the Optimize grid reflects which optimizations are actually applied,
# even if progress.json was lost or corrupted.
function Start-InlineVerify {
    (El "BtnOptVerify").IsEnabled = $false
    (El "BtnOptVerify").Content   = "Verifying…"

    Invoke-Async -Work {
        param($ScriptRoot, $UISync)
        . "$ScriptRoot\config.env.ps1"
        . "$ScriptRoot\helpers.ps1"

        $verified = [System.Collections.Generic.List[string]]::new()

        # ── Registry checks mapped to optimization steps ───────────────
        # Each entry: stepKey -> array of @{P=Path; N=Name; E=Expected}
        # Step is "verified" only if ALL checks pass.
        $checks = [ordered]@{
            # Phase 1
            "P1:4"  = @( @{P="HKCU:\System\GameConfigStore"; N="GameDVR_DXGIHonorFSEWindowsCompatible"; E=1} )
            "P1:11" = @( @{P="HKLM:\SOFTWARE\Microsoft\Windows\Dwm"; N="OverlayTestMode"; E=5} )
            "P1:12" = @( @{P="HKCU:\SOFTWARE\Microsoft\GameBar"; N="AllowAutoGameMode"; E=1},
                         @{P="HKCU:\SOFTWARE\Microsoft\GameBar"; N="AutoGameModeEnabled"; E=1} )
            "P1:23" = @( @{P="HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power"; N="HiberbootEnabled"; E=0} )
            "P1:26" = @( @{P="HKCU:\System\GameConfigStore"; N="GameDVR_FSEBehavior"; E=2},
                         @{P="HKCU:\System\GameConfigStore"; N="GameDVR_FSEBehaviorMode"; E=2},
                         @{P="HKCU:\System\GameConfigStore"; N="GameDVR_HonorUserFSEBehaviorMode"; E=1} )
            "P1:27" = @( @{P="HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile"; N="SystemResponsiveness"; E=10},
                         @{P="HKLM:\SYSTEM\CurrentControlSet\Control\PriorityControl"; N="Win32PrioritySeparation"; E=0x2A} )
            "P1:28" = @( @{P="HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\kernel"; N="GlobalTimerResolutionRequests"; E=1} )
            "P1:29" = @( @{P="HKCU:\Control Panel\Mouse"; N="MouseSpeed"; E="0"},
                         @{P="HKCU:\Control Panel\Mouse"; N="MouseThreshold1"; E="0"},
                         @{P="HKCU:\Control Panel\Mouse"; N="MouseThreshold2"; E="0"} )
            "P1:31" = @( @{P="HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\GameDVR"; N="AppCaptureEnabled"; E=0},
                         @{P="HKCU:\SOFTWARE\Microsoft\GameBar"; N="UseNexusForGameBarEnabled"; E=0},
                         @{P="HKCU:\System\GameConfigStore"; N="GameDVR_Enabled"; E=0} )
            "P1:32" = @( @{P="HKCU:\Software\Valve\Steam"; N="GameOverlayDisabled"; E=1} )
            "P1:33" = @( @{P="HKCU:\Software\Microsoft\Multimedia\Audio"; N="UserDuckingPreference"; E=3} )
            "P1:36" = @( @{P="HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects"; N="VisualFXSetting"; E=2} )
            # Phase 3
            "P3:7"  = @( @{P="HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity"; N="Enabled"; E=0} )
            "P3:10" = @( @{P="HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\cs2.exe\PerfOptions"; N="CpuPriorityClass"; E=3} )
        }

        foreach ($stepKey in $checks.Keys) {
            $allOk = $true
            foreach ($c in $checks[$stepKey]) {
                $r = Test-RegistryCheck $c.P $c.N $c.E "" -Quiet
                if ($r.Status -ne "OK") { $allOk = $false; break }
            }
            if ($allOk) { $verified.Add($stepKey) }
        }

        # ── Service checks ─────────────────────────────────────────────
        # P1:37 - Disable Bloat Services (SysMain + WSearch)
        try {
            $sm = Get-Service -Name "SysMain" -ErrorAction Stop
            $ws = Get-Service -Name "WSearch" -ErrorAction Stop
            if ($sm.StartType -eq 'Disabled' -and $ws.StartType -eq 'Disabled') { $verified.Add("P1:37") }
        } catch {}

        # ── NIC check (P1:25 - Disable Nagle) ─────────────────────────
        try {
            $nicGuid = Get-ActiveNicGuid
            if ($nicGuid) {
                $tcpBase = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\$nicGuid"
                $r = Test-RegistryCheck $tcpBase "TcpNoDelay" 1 "" -Quiet
                if ($r.Status -eq "OK") { $verified.Add("P1:25") }
            }
        } catch {}

        # ── NVIDIA GPU check (P3:4 - DRS Profile / PerfLevelSrc) ──────
        try {
            $classPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\$CFG_GUID_Display"
            if (Test-Path $classPath) {
                $subkeys = Get-ChildItem $classPath -ErrorAction SilentlyContinue |
                    Where-Object { $_.PSChildName -match "^\d{4}$" }
                foreach ($key in $subkeys) {
                    $props = Get-ItemProperty $key.PSPath -ErrorAction SilentlyContinue
                    if ($props.ProviderName -match "NVIDIA" -or $props.DriverDesc -match "NVIDIA") {
                        $r = Test-RegistryCheck $key.PSPath "PerfLevelSrc" 0x2222 "" -Quiet
                        if ($r.Status -eq "OK") { $verified.Add("P3:4") }
                        break
                    }
                }
            }
        } catch {}

        $UISync.VerifyResults = @($verified)
    } -WorkArgs @($Script:Root, $Script:UISync) -OnDone {
        $verified = $Script:UISync.VerifyResults
        if (-not $verified) { $verified = @() }

        # Update progress.json with verified steps
        $prog = Load-Progress
        if (-not $prog) {
            $prog = [PSCustomObject]@{ phase=0; lastCompletedStep=0; completedSteps=@(); skippedSteps=@(); timestamps=[PSCustomObject]@{} }
        }

        $added = 0
        foreach ($stepKey in $verified) {
            if ($stepKey -notin @($prog.completedSteps)) {
                $prog.completedSteps = @($prog.completedSteps) + $stepKey
                $added++
            }
        }
        if ($added -gt 0) {
            if (Test-BackupLock) {
                Write-DebugLog "Inline verify: skipping progress save — backup lock held by another process"
            } else {
                $maxPhase = ($verified | ForEach-Object {
                    if ($_ -match "^P(\d+):") { [int]$Matches[1] } else { 0 }
                } | Measure-Object -Maximum).Maximum
                if ($maxPhase -gt $prog.phase) { $prog.phase = $maxPhase }
                Save-Progress $prog
                $state = Get-StateDataSafe
                if (-not $state) {
                    $state = New-DefaultState
                }
                $state | Add-Member -NotePropertyName "startup_last_verified" -NotePropertyValue ((Get-Date).ToString("o")) -Force
                Save-StateDataSafe -State $state
            }
        }

        # Reload grid with updated progress
        Load-Optimize

        (El "BtnOptVerify").IsEnabled = $true
        (El "BtnOptVerify").Content   = "✓  Verify All"

        $total = @($verified).Count
        $msg = if ($added -gt 0) {
            "Verified: $total steps applied.`n$added step(s) recovered to progress."
        } else {
            "Verified: $total steps applied.`nProgress already up to date."
        }
        [System.Windows.MessageBox]::Show($msg, "Verification Complete")
        $Script:UISync.VerifyResults = $null
    }
}

(El "BtnOptPhase1"   ).Add_Click({ Launch-Terminal "Run-Optimize.ps1" })
(El "BtnOptPhase2"   ).Add_Click({ Launch-Terminal "SafeMode-DriverClean.ps1" })
(El "BtnOptPhase3"   ).Add_Click({ Launch-Terminal "PostReboot-Setup.ps1" })
(El "BtnOptFullSetup").Add_Click({ Launch-Terminal "Run-Optimize.ps1" })
(El "BtnOptVerify"   ).Add_Click({ Start-InlineVerify })

# Phase 2 button: only enabled in Safe Mode (driver files are locked in Normal Mode)
if (-not $env:SAFEBOOT_OPTION) {
    (El "BtnOptPhase2").IsEnabled = $false
    (El "BtnOptPhase2").ToolTip   = "Phase 2 requires Safe Mode (use 'Boot to Safe Mode' first)"
}

# Boot to Safe Mode / Normal Mode button — context-aware failsafe
if ($env:SAFEBOOT_OPTION) {
    # In Safe Mode: offer to exit back to Normal Mode
    (El "BtnBootSafeMode").Content = "Boot to Normal Mode"
    (El "BtnBootSafeMode").ToolTip = "Remove Safe Mode boot flag and restart into Normal Mode"
    (El "BtnBootSafeMode").Add_Click({
        $confirm = [System.Windows.MessageBox]::Show(
            "This will remove the Safe Mode boot flag and restart into Normal Mode.`n`nRestart now?",
            "Boot to Normal Mode", "YesNo", "Question")
        if ($confirm -eq "Yes") {
            $bcdOut = bcdedit /deletevalue safeboot 2>&1
            if ($LASTEXITCODE -ne 0) {
                [System.Windows.MessageBox]::Show(
                    "Failed to remove Safe Mode flag (bcdedit exit $LASTEXITCODE).`n`n$($bcdOut | Out-String)`nReboot aborted — you are still in Safe Mode.",
                    "bcdedit Error", "OK", "Error")
                return
            }
            shutdown /r /t 5 /f
        }
    })
} else {
    # In Normal Mode: offer to boot into Safe Mode
    (El "BtnBootSafeMode").Add_Click({
        try {
            Launch-Terminal "Boot-SafeMode.ps1"
        } catch {
            [System.Windows.MessageBox]::Show("Boot-SafeMode failed: $_", "Error", "OK", "Error")
        }
    })
}

# ══════════════════════════════════════════════════════════════════════════════
# BACKUP
# ══════════════════════════════════════════════════════════════════════════════
function Load-Backup {
    try {
        $bd = Get-BackupData
        if (-not $bd -or -not $bd.entries) {
            (El "BackupSummary").Text = "No backups found in backup.json"
            (El "BackupGrid").ItemsSource = $null
            return
        }
        $entries = $bd.entries
        (El "BackupSummary").Text = "$($entries.Count) backup entries  ·  Created $($bd.created)"

        $rows = foreach ($e in $entries) {
            $key = switch ($e.type) {
                "registry"      { "$($e.path)  →  $($e.name)" }
                "service"       { $e.name }
                "bootconfig"    { $e.key }
                "powerplan"     { "Power Plan: $($e.originalName)" }
                "drs"           { "DRS Profile: $($e.profile)  ($($e.settings.Count) settings)" }
                "scheduledtask" { "Task: $($e.taskName)" }
                default         { "$($e.type)" }
            }
            $orig = switch ($e.type) {
                "registry"      { if ($e.existed) { "$($e.originalValue)" } else { "(new key)" } }
                "service"       { "$($e.originalStartType) / $($e.originalStatus)" }
                "bootconfig"    { if ($e.existed) { $e.originalValue } else { "(new)" } }
                "powerplan"     { $e.originalGuid }
                "drs"           { "$($e.settings.Count) settings" }
                "scheduledtask" { if ($e.existed) { "existed" } else { "(new)" } }
                default         { "" }
            }
            [PSCustomObject]@{
                Step      = $e.step
                Type      = $e.type
                Key       = $key
                Original  = $orig
                Timestamp = $e.timestamp
                _Entry    = $e
            }
        }
        (El "BackupGrid").ItemsSource = $rows
    } catch {
        (El "BackupSummary").Text = "Error loading backup.json: $($_.Exception.Message)"
    }
}

(El "BtnBackupRefresh").Add_Click({ Load-Backup })

(El "BtnBackupExport").Add_Click({
    $src = "$CFG_WorkDir\backup.json"
    if (-not (Test-Path $src)) { [System.Windows.MessageBox]::Show("backup.json not found.","Export"); return }
    $dlg = [Microsoft.Win32.SaveFileDialog]::new()
    $dlg.Filter = "JSON files (*.json)|*.json|All files (*.*)|*.*"
    $dlg.FileName = "cs2-backup-$(Get-Date -Format 'yyyyMMdd-HHmm').json"
    if ($dlg.ShowDialog() -eq $true) { Copy-Item $src $dlg.FileName -Force; [System.Windows.MessageBox]::Show("Exported to:`n$($dlg.FileName)","Export Complete") }
})

(El "BtnRestoreAll").Add_Click({
    if (Test-BackupLock) {
        [System.Windows.MessageBox]::Show(
            "Another CS2 Optimization process is running. Wait for it to finish first.",
            "Locked", "OK", "Warning")
        return
    }
    $r = [System.Windows.MessageBox]::Show("Restore ALL backed-up settings?`nThis will undo every change the suite made.","Restore All","YesNo","Warning")
    if ($r -eq "Yes") {
        Set-BackupLock
        try {
            # Call step restores directly — Restore-AllChanges uses Read-Host which blocks GUI (no console)
            $bd = Get-BackupData
            if ($bd.entries -and $bd.entries.Count -gt 0) {
                $stepNames = @(($bd.entries | Group-Object -Property step).Name)
                $failures = 0
                foreach ($sn in $stepNames) { if (-not (Restore-StepChanges -StepTitle $sn)) { $failures++ } }
                if ($failures -gt 0) { throw "$failures step group(s) had restore failures." }
            }
            [System.Windows.MessageBox]::Show("All settings restored successfully.","Restore Complete")
            Load-Backup
        } catch {
            [System.Windows.MessageBox]::Show("Restore error: $($_.Exception.Message)","Restore Failed","OK","Error")
        } finally {
            Remove-BackupLock
        }
    }
})

(El "BtnRestoreStep").Add_Click({
    if (Test-BackupLock) {
        [System.Windows.MessageBox]::Show(
            "Another CS2 Optimization process is running. Wait for it to finish first.",
            "Locked", "OK", "Warning")
        return
    }
    $sel = (El "BackupGrid").SelectedItem
    if (-not $sel) { [System.Windows.MessageBox]::Show("Select a row first.","Restore Step"); return }
    $stepTitle = $sel.Step
    $r = [System.Windows.MessageBox]::Show("Restore all changes from:`n`"$stepTitle`"?","Restore Step","YesNo","Question")
    if ($r -eq "Yes") {
        Set-BackupLock
        try {
            $ok = Restore-StepChanges $stepTitle
            if ($ok) {
                [System.Windows.MessageBox]::Show("Restore complete for:`n$stepTitle","Done")
            } else {
                [System.Windows.MessageBox]::Show("Restore partially failed for:`n$stepTitle`n`nSome entries could not be restored. Check the log for details.","Restore Incomplete","OK","Warning")
            }
            Load-Backup
        } catch { [System.Windows.MessageBox]::Show("Error: $($_.Exception.Message)","Restore Failed") }
        finally { Remove-BackupLock }
    }
})

(El "BtnClearBackup").Add_Click({
    $r = [System.Windows.MessageBox]::Show("Delete all backup data?`nThis cannot be undone.","Clear Backups","YesNo","Warning")
    if ($r -eq "Yes") {
        if (Test-Path "$CFG_WorkDir\backup.json") { Remove-Item "$CFG_WorkDir\backup.json" -Force }
        Load-Backup
    }
})

# ══════════════════════════════════════════════════════════════════════════════
# BENCHMARK
# ══════════════════════════════════════════════════════════════════════════════
function Load-Benchmark {
    try {
        $hist = @(Get-BenchmarkHistory)
        if (-not $hist -or $hist.Count -eq 0) {
            (El "BenchGrid").ItemsSource = $null
            return
        }

        $rows = for ($i = 0; $i -lt $hist.Count; $i++) {
            $h = $hist[$i]
            $dAvg = if ($i -eq 0) { "—" } else {
                $prev = $hist[$i - 1]
                $d = if ($prev.avgFps -gt 0) { [math]::Round(($h.avgFps - $prev.avgFps) / $prev.avgFps * 100, 1) } else { 0 }
                if ($d -gt 0) { "+$d%" } else { "$d%" }
            }
            $dP1 = if ($i -eq 0) { "—" } else {
                $prev = $hist[$i - 1]
                $d = if ($prev.p1Fps -gt 0) { [math]::Round(($h.p1Fps - $prev.p1Fps) / $prev.p1Fps * 100, 1) } else { 0 }
                if ($d -gt 0) { "+$d%" } else { "$d%" }
            }
            $dc = if ($i -eq 0 -or $dAvg -eq "—") { "#6b7280" } elseif ($dAvg.StartsWith("+")) { "#22c55e" } elseif ($dAvg -eq "0%" -or $dAvg -eq "0.0%") { "#6b7280" } else { "#ef4444" }
            $dp1c = if ($i -eq 0 -or $dP1 -eq "—") { "#6b7280" } elseif ($dP1.StartsWith("+")) { "#22c55e" } elseif ($dP1 -eq "0%" -or $dP1 -eq "0.0%") { "#6b7280" } else { "#ef4444" }
            $dateStr = try { [datetime]::ParseExact($h.timestamp,"yyyy-MM-dd HH:mm:ss",$null).ToString("dd-MMM HH:mm") } catch { $h.timestamp }
            [PSCustomObject]@{
                Index        = $i + 1
                Date         = $dateStr
                Label        = $h.label
                AvgFps       = [math]::Round($h.avgFps, 0)
                P1Fps        = [math]::Round($h.p1Fps,  0)
                DeltaAvg     = $dAvg
                DeltaP1      = $dP1
                DeltaColor   = $dc
                DeltaP1Color = $dp1c
            }
        }
        (El "BenchGrid").ItemsSource = $rows
        Draw-BenchChart $hist
    } catch { Write-DebugLog "Benchmark history load failed: $($_.Exception.Message)" }
}

function Draw-BenchChart {
    param($hist)
    $canvas = El "BenchChart"
    $canvas.Children.Clear()
    if (-not $hist -or $hist.Count -lt 2) { return }

    # Wait for layout
    $canvas.UpdateLayout()
    $w = if ($canvas.ActualWidth  -gt 0) { $canvas.ActualWidth  } else { 600 }
    $h = if ($canvas.ActualHeight -gt 0) { $canvas.ActualHeight } else { 130 }

    $allFps = ($hist | ForEach-Object { $_.avgFps, $_.p1Fps }) | Measure-Object -Maximum -Minimum
    $maxF = $allFps.Maximum * 1.08
    $minF = $allFps.Minimum * 0.92
    $range = $maxF - $minF
    if ($range -le 0) { $range = 1 }

    $xStep = $w / ($hist.Count - 1)
    $toY   = { param($v) $h - (($v - $minF) / $range * $h) }

    # Grid lines
    foreach ($pct in @(0.25, 0.5, 0.75)) {
        $y = $h * $pct
        $gl = [System.Windows.Shapes.Line]::new()
        $gl.X1 = 0; $gl.X2 = $w; $gl.Y1 = $y; $gl.Y2 = $y
        $gl.Stroke = New-Brush "#252525"; $gl.StrokeThickness = 1
        $canvas.Children.Add($gl) | Out-Null
    }

    # Build point collections
    $avgPts = [System.Windows.Media.PointCollection]::new()
    $p1Pts  = [System.Windows.Media.PointCollection]::new()
    for ($i = 0; $i -lt $hist.Count; $i++) {
        $x = $i * $xStep
        $avgPts.Add([System.Windows.Point]::new($x, (& $toY $hist[$i].avgFps))) | Out-Null
        $p1Pts.Add( [System.Windows.Point]::new($x, (& $toY $hist[$i].p1Fps ))) | Out-Null
    }

    # Avg line
    $avgLine = [System.Windows.Shapes.Polyline]::new()
    $avgLine.Points = $avgPts
    $avgLine.Stroke = New-Brush "#e8520a"; $avgLine.StrokeThickness = 2
    $canvas.Children.Add($avgLine) | Out-Null

    # P1 line
    $p1Line = [System.Windows.Shapes.Polyline]::new()
    $p1Line.Points = $p1Pts
    $p1Line.Stroke = New-Brush "#22c55e"; $p1Line.StrokeThickness = 2; $p1Line.StrokeDashArray = [System.Windows.Media.DoubleCollection]@(4, 3)
    $canvas.Children.Add($p1Line) | Out-Null

    # Dots + x-axis labels
    for ($i = 0; $i -lt $hist.Count; $i++) {
        $x = $i * $xStep
        foreach ($pts in @($avgPts, $p1Pts)) {
            $dot = [System.Windows.Shapes.Ellipse]::new()
            $dot.Width = 6; $dot.Height = 6
            $dot.Fill = if ($pts -eq $avgPts) { New-Brush "#e8520a" } else { New-Brush "#22c55e" }
            [System.Windows.Controls.Canvas]::SetLeft($dot, $pts[$i].X - 3)
            [System.Windows.Controls.Canvas]::SetTop( $dot, $pts[$i].Y - 3)
            $canvas.Children.Add($dot) | Out-Null
        }
        # x-label
        $lbl = [System.Windows.Controls.TextBlock]::new()
        $lbl.Text = try { [datetime]::ParseExact($hist[$i].timestamp,"yyyy-MM-dd HH:mm:ss",$null).ToString("d-MMM") } catch { "$($i+1)" }
        $lbl.FontSize = 9; $lbl.Foreground = New-Brush "#4b5563"
        [System.Windows.Controls.Canvas]::SetLeft($lbl, $x - 16)
        [System.Windows.Controls.Canvas]::SetTop( $lbl, $h + 4)
        $canvas.Children.Add($lbl) | Out-Null
    }

    # Y-axis label
    $yLbl = [System.Windows.Controls.TextBlock]::new()
    $yLbl.Text = "FPS"; $yLbl.FontSize = 9; $yLbl.Foreground = New-Brush "#4b5563"
    [System.Windows.Controls.Canvas]::SetLeft($yLbl, -28)
    [System.Windows.Controls.Canvas]::SetTop( $yLbl, $h / 2 - 8)
    $canvas.Children.Add($yLbl) | Out-Null

    # Legend
    $leg = [System.Windows.Controls.TextBlock]::new()
    $leg.Text = "— Avg FPS   - - 1% Low"; $leg.FontSize = 9; $leg.Foreground = New-Brush "#6b7280"
    [System.Windows.Controls.Canvas]::SetLeft($leg, $w - 120)
    [System.Windows.Controls.Canvas]::SetTop( $leg, -16)
    $canvas.Children.Add($leg) | Out-Null
}

# FPS Cap
(El "BtnBenchParse").Add_Click({
    $raw = (El "BenchVprof").Text.Trim()
    $parsed = Get-BenchmarkCapFromText $raw
    if ($parsed) {
        (El "BenchCapLabel").Text = "→  Cap:"
        (El "BenchCapValue").Text = "$($parsed.Cap)"
        $Script:UISync.LastCap = $parsed.Cap
    } else {
        (El "BenchCapLabel").Text = "⚠  No [VProf] FPS line detected"
        (El "BenchCapValue").Text = ""
    }
})

(El "BtnBenchCopy").Add_Click({
    $cap = $Script:UISync.LastCap
    if ($cap) {
        try { [System.Windows.Clipboard]::SetText("$cap") }
        catch { Write-DebugLog "Clipboard copy failed: $_" }
    }
})

(El "BtnBenchAdd").Add_Click({
    $raw = (El "BenchVprof").Text.Trim()
    $parsed = Get-BenchmarkResultFromText $raw
    if ($parsed) {
        $lbl = [Microsoft.VisualBasic.Interaction]::InputBox("Label for this benchmark result:", "Add Result", "")
        Add-BenchmarkResult -AvgFps $parsed.AvgFps -P1Fps $parsed.P1Fps -Label $lbl -Runs 1
        Load-Benchmark
    } else {
        [System.Windows.MessageBox]::Show("Paste a [VProf] FPS: Avg=… P1=… line first.","Add Result")
    }
})

# ══════════════════════════════════════════════════════════════════════════════
# NETWORK
# ══════════════════════════════════════════════════════════════════════════════
function Load-NetworkDiagnostics {
    $summary = Get-NetworkDiagnosticSummary
    if (-not $summary.AdapterFound) {
        (El "NetDiagAdapterSummary").Text = "Adapter: no active adapter found"
        (El "NetDiagDnsSummary").Text = "DNS: unavailable"
    } else {
        $dnsText = if (@($summary.DnsServers).Count -gt 0) { @($summary.DnsServers) -join ', ' } else { "automatic / DHCP" }
        (El "NetDiagAdapterSummary").Text = "Adapter: $($summary.AdapterName)  ·  $($summary.AdapterType)"
        (El "NetDiagDnsSummary").Text = "DNS: $($summary.DnsProvider)  ·  $dnsText"
    }

    $historyRows = @(Get-LatencyHistoryRows)
    (El "NetDiagHistoryGrid").ItemsSource = $historyRows
    (El "NetDiagComparisonGrid").ItemsSource = @(Get-ValveLatencyComparisonRows)
    (El "NetDiagHistorySummary").Text = if ($historyRows.Count -gt 0) {
        "Latest run: $($historyRows[-1].Timestamp)  ·  $($historyRows[-1].Kind)"
    } else {
        "No latency diagnostics recorded yet."
    }
}

function Start-LatencyDiagnostic {
    param(
        [ValidateSet("baseline", "post")][string]$Kind
    )

    $buttonName = if ($Kind -eq "baseline") { "BtnNetBaseline" } else { "BtnNetPost" }
    (El "BtnNetBaseline").IsEnabled = $false
    (El "BtnNetPost").IsEnabled = $false
    (El $buttonName).Content = if ($Kind -eq "baseline") { "Running…" } else { "Retesting…" }

    Invoke-Async -Work {
        param($ScriptRoot, $UISync, $RunKind)
        . "$ScriptRoot\config.env.ps1"
        . "$ScriptRoot\helpers.ps1"
        try {
            $UISync.LatencyRun = Invoke-ValveRegionLatencyDiagnostic -Kind $RunKind
        } catch {
            $UISync.LatencyError = $_.Exception.Message
        }
    } -WorkArgs @($Script:Root, $Script:UISync, $Kind) -OnDone {
        $err = $Script:UISync.LatencyError
        Load-NetworkDiagnostics
        (El "BtnNetBaseline").IsEnabled = $true
        (El "BtnNetPost").IsEnabled = $true
        (El "BtnNetBaseline").Content = "Baseline Test"
        (El "BtnNetPost").Content = "Post-Change Retest"
        if ($err) {
            [System.Windows.MessageBox]::Show("Latency diagnostic failed:`n$err", "Network Diagnostic", "OK", "Error")
        } else {
            $run = $Script:UISync.LatencyRun
            $okRegions = @($run.Results | Where-Object { $null -ne $_.AvgRttMs }).Count
            [System.Windows.MessageBox]::Show("Saved $($run.Kind) run at $($run.Timestamp).`nResponsive regions: $okRegions / $(@($run.Results).Count)", "Network Diagnostic")
        }
        $Script:UISync.LatencyRun = $null
        $Script:UISync.LatencyError = $null
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
(El "BtnNetBaseline").Add_Click({ Start-LatencyDiagnostic -Kind baseline })
(El "BtnNetPost").Add_Click({ Start-LatencyDiagnostic -Kind post })
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

# ══════════════════════════════════════════════════════════════════════════════
# VIDEO
# ══════════════════════════════════════════════════════════════════════════════
$Script:VideoTxtPath = $null

function Load-Video {
    # Populate tier picker
    if ((El "VideoTierPicker").Items.Count -eq 0) {
        foreach ($t in @("Auto","HIGH","MID","LOW")) { (El "VideoTierPicker").Items.Add($t) | Out-Null }
        (El "VideoTierPicker").SelectedIndex = 0
    }

    $steamPath = Get-SteamPath
    $vtxt = if ($steamPath) {
        Get-ChildItem "$steamPath\userdata\*\730\local\cfg\video.txt" -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
    }

    if ($vtxt) {
        $Script:VideoTxtPath = $vtxt.FullName
        (El "VideoTxtPath").Text = $vtxt.FullName
        (El "BtnVideoWrite").IsEnabled = $true
        (El "BtnVideoWriteFooter").IsEnabled = $true
    } else {
        (El "VideoTxtPath").Text = "video.txt not found — launch CS2 once to generate it"
        (El "BtnVideoWrite").IsEnabled = $false
        (El "BtnVideoWriteFooter").IsEnabled = $false
        return
    }

    Refresh-VideoGrid
}

# Single source of truth for video tier presets (V=value, N=note for display)
$Script:VideoPresets = @{
    "HIGH" = @{
        "setting.msaa_samples"              = @{ V="4";  N="4x MSAA — better 1% lows than None (ThourCS2)" }
        "setting.mat_vsync"                 = @{ V="0";  N="Always OFF — adds render queue latency" }
        "setting.fullscreen"                = @{ V="1";  N="Exclusive fullscreen — bypasses DWM compositor" }
        "setting.r_low_latency"             = @{ V="1";  N="NVIDIA Reflex On — saves 3-4ms input latency" }
        "setting.r_csgo_fsr_upsample"       = @{ V="0";  N="FSR OFF — artifacts harm enemy recognition" }
        "setting.shaderquality"             = @{ V="1";  N="High — GPU has headroom at this tier" }
        "setting.r_texturefilteringquality" = @{ V="5";  N="AF16x — near-zero cost on modern GPUs" }
        "setting.r_csgo_cmaa_enable"        = @{ V="0";  N="Off — MSAA handles AA" }
        "setting.r_aoproxy_enable"          = @{ V="0";  N="AO off — purely cosmetic, up to 6% FPS cost" }
        "setting.sc_hdr_enabled_override"   = @{ V="3";  N="Performance — Quality washes out sun/window areas" }
        "setting.r_particle_max_detail_level"=@{ V="0";  N="Low particles — no competitive disadvantage" }
        "setting.csm_enabled"               = @{ V="1";  N="Shadows ON — foot shadows reveal enemy positions" }
    }
    "MID" = @{
        "setting.msaa_samples"              = @{ V="4";  N="4x — or 2x if below 200 avg FPS" }
        "setting.mat_vsync"                 = @{ V="0";  N="Always OFF" }
        "setting.fullscreen"                = @{ V="1";  N="Exclusive fullscreen" }
        "setting.r_low_latency"             = @{ V="1";  N="NVIDIA Reflex On" }
        "setting.r_csgo_fsr_upsample"       = @{ V="0";  N="FSR OFF" }
        "setting.shaderquality"             = @{ V="0";  N="Low — saves GPU headroom on mid-tier" }
        "setting.r_texturefilteringquality" = @{ V="5";  N="AF16x" }
        "setting.r_csgo_cmaa_enable"        = @{ V="0";  N="Off — MSAA handles AA" }
        "setting.r_aoproxy_enable"          = @{ V="0";  N="AO off" }
        "setting.sc_hdr_enabled_override"   = @{ V="3";  N="Performance" }
        "setting.r_particle_max_detail_level"=@{ V="0";  N="Low" }
        "setting.csm_enabled"               = @{ V="1";  N="Shadows ON" }
    }
    "LOW" = @{
        "setting.msaa_samples"              = @{ V="0";  N="None + CMAA2 — free AA alternative" }
        "setting.mat_vsync"                 = @{ V="0";  N="Always OFF" }
        "setting.fullscreen"                = @{ V="1";  N="Exclusive fullscreen — critical for FPS" }
        "setting.r_low_latency"             = @{ V="1";  N="NVIDIA Reflex On" }
        "setting.r_csgo_fsr_upsample"       = @{ V="0";  N="FSR OFF" }
        "setting.shaderquality"             = @{ V="0";  N="Low" }
        "setting.r_texturefilteringquality" = @{ V="0";  N="Bilinear — legacy for max FPS" }
        "setting.r_csgo_cmaa_enable"        = @{ V="1";  N="CMAA2 ON — near-zero cost AA when MSAA=0" }
        "setting.r_aoproxy_enable"          = @{ V="0";  N="AO off" }
        "setting.sc_hdr_enabled_override"   = @{ V="3";  N="Performance" }
        "setting.r_particle_max_detail_level"=@{ V="0";  N="Low" }
        "setting.csm_enabled"               = @{ V="1";  N="Shadows ON — keep even on low-end" }
    }
}

function Get-ResolvedVideoTier {
    # Auto tier: HIGH for NVIDIA (detected via driver version), MID for AMD/Intel
    # The suite is NVIDIA-focused; AMD/Intel users should select tier manually
    param([string]$TierSel)
    if ($TierSel -eq "Auto") {
        $nv = Get-NvidiaDriverVersion
        if ($nv) { return "HIGH" }
        return "MID"
    }
    return $TierSel
}

function Refresh-VideoGrid {
    $tier = Get-ResolvedVideoTier (El "VideoTierPicker").SelectedItem
    $recommended = $Script:VideoPresets[$tier]

    $current = @{}
    if ($Script:VideoTxtPath -and (Test-Path $Script:VideoTxtPath)) {
        Get-Content $Script:VideoTxtPath | ForEach-Object {
            if ($_ -match '^\s*"([^"]+)"\s+"([^"]*)"') { $current[$Matches[1]] = $Matches[2] }
        }
    }

    $rows = foreach ($kv in $recommended.GetEnumerator() | Sort-Object Key) {
        $cur  = $current[$kv.Key]
        $rec  = $kv.Value.V
        $note = $kv.Value.N
        $st   = if ($null -eq $cur) { "—  Missing" } elseif ($cur -eq $rec) { "✓  OK" } else { "⚠  Differs" }
        $sc   = if ($st -match "OK") { "#22c55e" } elseif ($st -match "Missing") { "#6b7280" } else { "#fbbf24" }
        [PSCustomObject]@{
            Setting     = $kv.Key -replace "^setting\.",""
            YourValue   = if ($null -eq $cur) { "(not set)" } else { $cur }
            Recommended = $rec
            StatusLabel = $st
            StatusColor = $sc
            Notes       = $note
        }
    }

    (El "VideoGrid").ItemsSource = $rows
    $diffs = @($rows | Where-Object { $_.StatusLabel -notmatch "OK" }).Count
    (El "VideoSummary").Text = "$diffs setting(s) need attention for $tier-tier recommendation"
}

(El "VideoTierPicker").Add_SelectionChanged({ if ((El "VideoTierPicker").SelectedItem) { Refresh-VideoGrid } })

$writeVideo = {
    if (-not $Script:VideoTxtPath) { [System.Windows.MessageBox]::Show("video.txt not found.","Write"); return }

    $tier = Get-ResolvedVideoTier (El "VideoTierPicker").SelectedItem

    # Derive values-only hashtable from shared presets
    $managed = @{}
    foreach ($kv in $Script:VideoPresets[$tier].GetEnumerator()) { $managed[$kv.Key] = $kv.Value.V }

    # Read existing file — preserve unmanaged keys (resolution, Hz, etc.)
    $existing = [System.Collections.Generic.Dictionary[string,string]]::new([StringComparer]::OrdinalIgnoreCase)
    if (Test-Path $Script:VideoTxtPath) {
        Get-Content $Script:VideoTxtPath | ForEach-Object {
            if ($_ -match '^\s*"([^"]+)"\s+"([^"]*)"') { $existing[$Matches[1]] = $Matches[2] }
        }
    }

    # Merge: apply managed overrides onto existing keys
    foreach ($kv in $managed.GetEnumerator()) { $existing[$kv.Key] = $kv.Value }

    $summary = ($managed.Keys | ForEach-Object { "$($_ -replace '^setting\.',''): $($managed[$_])" }) -join "`n"
    $r = [System.Windows.MessageBox]::Show(
        "Write optimized video.txt ($tier tier)?`n`nOriginal → video.txt.bak`n`nSettings:`n$summary",
        "Confirm Write","YesNo","Question")
    if ($r -ne "Yes") { return }

    try {
        $bakPath = "$Script:VideoTxtPath.bak"
        # Only create backup if one doesn't already exist — preserve the original
        $bakMade = $false
        if ((Test-Path $Script:VideoTxtPath) -and -not (Test-Path $bakPath)) {
            Copy-Item $Script:VideoTxtPath $bakPath -Force
            $bakMade = $true
        }

        $dir = Split-Path $Script:VideoTxtPath
        if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force -ErrorAction SilentlyContinue | Out-Null }

        $lines = @(
            '"VideoConfig"'
            '{'
            "    // CS2-Optimize Suite — $(Get-Date -Format 'yyyy-MM-dd HH:mm')  Tier: $tier"
            "    // Original backed up as video.txt.bak"
            ""
        )
        foreach ($kv in $existing.GetEnumerator() | Sort-Object Key) {
            $lines += "    `"$($kv.Key)`"`t`"$($kv.Value)`""
        }
        $lines += "}"
        # Steam Cloud can set video.txt read-only — clear the flag before writing
        if ((Test-Path $Script:VideoTxtPath) -and (Get-Item $Script:VideoTxtPath).IsReadOnly) {
            try { (Get-Item $Script:VideoTxtPath).IsReadOnly = $false }
            catch {
                [System.Windows.MessageBox]::Show(
                    "video.txt is read-only (Steam Cloud may be syncing).`n`nTry disabling Steam Cloud sync for CS2:`nSteam → CS2 → Properties → General → Steam Cloud",
                    "Read-Only File", "OK", "Warning")
                return
            }
        }
        [System.IO.File]::WriteAllLines($Script:VideoTxtPath, [string[]]$lines, [System.Text.UTF8Encoding]::new($false))

        $backupMsg = if ($bakMade) { "Original saved as video.txt.bak" } else { "Backup preserved as video.txt.bak (from first run)" }
        [System.Windows.MessageBox]::Show("video.txt written ($tier tier).`n$backupMsg`n`n$Script:VideoTxtPath","Done")
        Load-Video
    } catch { [System.Windows.MessageBox]::Show("Error: $($_.Exception.Message)","Write Failed") }
}
(El "BtnVideoWrite"      ).Add_Click($writeVideo)
(El "BtnVideoWriteFooter").Add_Click($writeVideo)

# ══════════════════════════════════════════════════════════════════════════════
# SETTINGS
# ══════════════════════════════════════════════════════════════════════════════
function Load-Settings {
    $state = $null
    try { if (Test-Path $CFG_StateFile) { $state = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop } } catch { Write-DebugLog "Settings state load failed: $($_.Exception.Message)" }

    $prof = if ($state) { $state.profile } else { "RECOMMENDED" }
    switch ($prof) {
        "SAFE"        { (El "RadioSafe"       ).IsChecked = $true }
        "COMPETITIVE" { (El "RadioCompetitive").IsChecked = $true }
        "CUSTOM"      { (El "RadioCustom"     ).IsChecked = $true }
        "YOLO"        { (El "RadioYolo"       ).IsChecked = $true }
        default       { (El "RadioRecommended").IsChecked = $true }
    }

    $dry = if ($state) { $state.mode -eq "DRY-RUN" } else { $false }
    (El "ChkDryRun").IsChecked = $dry
}

function Save-SettingsToState {
    $prof = if ((El "RadioSafe").IsChecked)        { "SAFE"
            } elseif ((El "RadioYolo").IsChecked)        { "YOLO"
            } elseif ((El "RadioCompetitive").IsChecked) { "COMPETITIVE"
            } elseif ((El "RadioCustom").IsChecked)      { "CUSTOM"
            } else                                        { "RECOMMENDED" }
    $dry  = (El "ChkDryRun").IsChecked -eq $true
    $mode = if ($dry) { "DRY-RUN" } else {
        switch ($prof) { "SAFE" {"AUTO"} "RECOMMENDED" {"AUTO"} "COMPETITIVE" {"CONTROL"} "CUSTOM" {"INFORMED"} "YOLO" {"YOLO"} }
    }
    try {
        $state = $null
        if (Test-Path $CFG_StateFile) { $state = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop }
        # Skip write if nothing changed
        if ($state -and $state.PSObject.Properties['profile'] -and $state.PSObject.Properties['mode'] -and $state.profile -eq $prof -and $state.mode -eq $mode) { return }
        if (-not $state) { $state = [PSCustomObject]@{ mode = $mode; profile = $prof } }
        $state | Add-Member -NotePropertyName "profile" -NotePropertyValue $prof -Force
        $state | Add-Member -NotePropertyName "mode"    -NotePropertyValue $mode -Force
        Save-JsonAtomic -Data $state -Path $CFG_StateFile
        Set-SecureAcl -Path $CFG_StateFile
    } catch {
        Write-DebugLog "Settings state save failed: $($_.Exception.Message)"
        [System.Windows.MessageBox]::Show(
            "Failed to save settings:`n$($_.Exception.Message)`n`nYour profile/mode change was NOT persisted. Terminal operations may use the previous settings.",
            "Settings Save Error", "OK", "Warning")
    }
}

foreach ($rb in @("RadioSafe","RadioRecommended","RadioCompetitive","RadioCustom","RadioYolo")) {
    (El $rb).Add_Checked({
        $prof = if ((El "RadioSafe").IsChecked)        { "SAFE"
                } elseif ((El "RadioYolo").IsChecked)        { "YOLO"
                } elseif ((El "RadioCompetitive").IsChecked) { "COMPETITIVE"
                } elseif ((El "RadioCustom").IsChecked)      { "CUSTOM"
                } else                                        { "RECOMMENDED" }
        (El "SbProfile").Text = "Profile: $prof"
        Save-SettingsToState
    })
}

(El "ChkDryRun").Add_Checked({   (El "SbDryRun").Text = "DRY-RUN"; (El "SbDryRunBadge").Visibility = "Visible"; Save-SettingsToState })
(El "ChkDryRun").Add_Unchecked({ (El "SbDryRun").Text = "";          (El "SbDryRunBadge").Visibility = "Collapsed"; Save-SettingsToState })

(El "BtnSettingsPhase1").Add_Click({ Launch-Terminal "Run-Optimize.ps1" })

# ══════════════════════════════════════════════════════════════════════════════
# SHARED HELPERS
# ══════════════════════════════════════════════════════════════════════════════
function Launch-Terminal {
    param([string]$Script, [string]$ScriptArgs = "")
    $fileArg = "`"$Script:Root\$Script`""
    $allArgs = "-NoProfile -ExecutionPolicy Bypass -WindowStyle Normal -File $fileArg"
    if ($ScriptArgs) { $allArgs += " `"$ScriptArgs`"" }
    Start-Process powershell -ArgumentList $allArgs
}
