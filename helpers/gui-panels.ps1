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

function Get-GuiSemanticBrush {
    param(
        [Parameter(Mandatory)][string]$ResourceName,
        [Parameter(Mandatory)][string]$FallbackColor
    )
    try {
        if ($Window -and $Window.Resources -and $Window.Resources[$ResourceName]) {
            return $Window.Resources[$ResourceName]
        }
    } catch { $null = $_ }
    return New-Brush $FallbackColor
}
$Script:StartupDriftChecked = $false
$Script:GuiObservedStepKeys = @()

function Get-StateDataSafe {
    try {
        Ensure-SecureWorkDir -Path (Split-Path $CFG_StateFile -Parent)
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
    Save-SuiteState -State $State
}

function New-DefaultState {
    param()

    return [PSCustomObject]@{
        mode    = Get-ModeForProfile -Profile "RECOMMENDED"
        profile = "RECOMMENDED"
    }
}

function Get-UISyncValue {
    param(
        [Parameter(Mandatory)]$Store,
        [Parameter(Mandatory)][string]$Name
    )
    if ($Store -is [System.Collections.IDictionary]) {
        if ($Store.Contains($Name)) { return $Store[$Name] }
        return $null
    }
    if ($Store.PSObject.Properties[$Name]) { return $Store.$Name }
    return $null
}

function Set-UISyncValue {
    param(
        [Parameter(Mandatory)]$Store,
        [Parameter(Mandatory)][string]$Name,
        $Value
    )
    if ($Store -is [System.Collections.IDictionary]) {
        $Store[$Name] = $Value
        return
    }
    $Store | Add-Member -NotePropertyName $Name -NotePropertyValue $Value -Force
}

function Enter-GuiBackupOperation {
    <# Atomically acquires the backup lock for a GUI recovery operation.  The
       preliminary check gives a useful message for the common case, while the
       caught CreateNew failure closes the race with another contender. #>
    if (Test-BackupLock) {
        [System.Windows.MessageBox]::Show(
            "Another CS2 Optimization process is running. Wait for it to finish first.",
            "Locked", "OK", "Warning") | Out-Null
        return $false
    }
    try {
        Set-BackupLock
        return $true
    } catch {
        Write-DebugLog "Backup lock acquisition lost to another process: $($_.Exception.Message)"
        [System.Windows.MessageBox]::Show(
            "Another CS2 Optimization process acquired the recovery lock first. Wait for it to finish, then try again.",
            "Locked", "OK", "Warning") | Out-Null
        return $false
    }
}

function Should-SkipStartupDriftCheck {
    param(
        $State,
        [datetime]$Now = (Get-Date)
    )
    if (-not $State -or -not $State.PSObject.Properties['startup_last_verified']) { return $false }
    try {
        $lastValue = $State.startup_last_verified
        $lastVerified = if ($lastValue -is [datetime]) {
            [datetime]$lastValue
        } else {
            [datetime]::Parse(
                [string]$lastValue,
                [System.Globalization.CultureInfo]::InvariantCulture,
                [System.Globalization.DateTimeStyles]::RoundtripKind)
        }
        return (($Now - $lastVerified).TotalMinutes -lt 60)
    } catch {
        return $false
    }
}

function Test-StartupConfigDrift {
    param([switch]$Force)

    $state = Get-StateDataSafe
    $now = Get-Date
    if (-not $Force -and (Should-SkipStartupDriftCheck -State $state -Now $now)) {
        return [PSCustomObject]@{
            Skipped      = $true
            Status       = "Unknown"
            HasDrift     = $null
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
        Status       = if ($driftLabels.Count -gt 0) { "Drift" } else { "Clean" }
        HasDrift     = ($driftLabels.Count -gt 0)
        DriftCount   = $driftLabels.Count
        CheckedCount = $checks.Count
        DriftLabels  = @($driftLabels)
        CheckedAt    = $now.ToString("yyyy-MM-dd HH:mm")
    }
}

function Update-StartupDriftBanner {
    [CmdletBinding(SupportsShouldProcess)]
    param()

    if (-not $PSCmdlet.ShouldProcess("dashboard startup drift banner", "Update GUI banner")) { return }
    if ($Script:StartupDriftChecked) { return }
    $Script:StartupDriftChecked = $true

    $result = Test-StartupConfigDrift
    if ($result.Skipped -or $result.Status -eq "Unknown") {
        (El "DashDriftBannerTitle").Text = "Configuration Drift Not Checked"
        (El "DashDriftBannerText").Text = "Quick startup drift check was skipped because it ran recently. Current drift state is unknown; run Verify-Settings for a fresh review."
        (El "DashDriftBanner").Visibility = "Visible"
        return
    }

    if (-not $result.HasDrift) {
        (El "DashDriftBanner").Visibility = "Collapsed"
        return
    }

    (El "DashDriftBannerTitle").Text = "Configuration Drift Detected"
    (El "DashDriftBannerText").Text = "$($result.DriftCount) of $($result.CheckedCount) quick checks drifted. Run Verify-Settings to review and repair the full set."
    (El "DashDriftBanner").Visibility = "Visible"
}

function Get-BenchmarkCapFromText {
    param([string]$Text)
    if ($Text -notmatch "Avg\s*=\s*(\S+?)(?:\s*,\s*P1|\s+P1|\s|$)") { return $null }

    $avg = ConvertTo-BenchmarkNumber $Matches[1]
    if ($null -eq $avg) { return $null }
    $cap = [math]::Max($CFG_FpsCap_Min, [math]::Floor($avg - [math]::Floor($avg * $CFG_FpsCap_Percent)))
    return [PSCustomObject]@{
        AvgFps = $avg
        Cap    = $cap
    }
}

function Get-BenchmarkResultFromText {
    param([string]$Text)
    if ($Text -notmatch "Avg\s*=\s*(\S+?)(?:\s*,\s*P1|\s+P1)\s*=\s*(\S+)") { return $null }

    $avg = ConvertTo-BenchmarkNumber $Matches[1]
    $p1 = ConvertTo-BenchmarkNumber $Matches[2] -AllowZero
    if ($null -eq $avg -or $null -eq $p1) { return $null }

    return [PSCustomObject]@{
        AvgFps = $avg
        P1Fps  = $p1
    }
}

function Switch-Panel {
    param([string]$PanelName, [scriptblock]$OnSwitch = $null)
    foreach ($p in $Script:AllPanels) {
        (El $p).Visibility = if ($p -eq $PanelName) { "Visible" } else { "Collapsed" }
    }
    foreach ($kv in $Script:NavMap.GetEnumerator()) {
        $navElement = El $kv.Value
        $isActive = $kv.Key -eq $PanelName
        $navElement.Style = if ($isActive) { $ActiveStyle } else { $InactiveStyle }
        [System.Windows.Automation.AutomationProperties]::SetItemStatus(
            $navElement,
            $(if ($isActive) { "Current page" } else { "" })
        )
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
                (El "DashPerfDelta"   ).Foreground = if ($dAvg -gt 0) { Get-GuiSemanticBrush "Success" "#22C55E" } elseif ($dAvg -lt 0) { Get-GuiSemanticBrush "Danger" "#F87171" } else { Get-GuiSemanticBrush "TextMuted" "#9AA5B4" }
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
            $vtxt = if ($stPath) {
                Get-ChildItem "$stPath\userdata\*\730\local\cfg\video.txt" -ErrorAction SilentlyContinue |
                    Where-Object { Test-TrustedVideoTxtPath -Path $_.FullName -SteamPath $stPath } |
                    Select-Object -First 1
            } else { $null }
            $optExists = if ($cs2) { Test-Path "$cs2\game\csgo\cfg\optimization.cfg" } else { $false }
            $UISync["Hw"] = @{
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
        } catch { $UISync["HwErr"] = $_.Exception.Message }
        $UISync["HwDone"] = $true
    } -WorkArgs @($Script:Root, $Script:UISync) -OnDone {
        $hw = Get-UISyncValue -Store $Script:UISync -Name "Hw"
        if (-not $hw) {
            $hwErr = Get-UISyncValue -Store $Script:UISync -Name "HwErr"
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
        (El "CardRamXmp"  ).Foreground = if ($hw.RamXmpOk) { Get-GuiSemanticBrush "Success" "#22C55E" } else { Get-GuiSemanticBrush "Warning" "#FBBF24" }
        (El "CardNicName" ).Text = $hw.NicName
        (El "CardNicSpeed").Text = $hw.NicSpeed
        (El "CardNicType" ).Text = $hw.NicType
        (El "CardNicType" ).Foreground = if ($hw.NicOk) { Get-GuiSemanticBrush "Success" "#22C55E" } else { Get-GuiSemanticBrush "Warning" "#FBBF24" }
        (El "CardOsName"  ).Text = $hw.OsName
        (El "CardOsBuild" ).Text = $hw.OsBuild
        (El "CardOsHags"  ).Text = $hw.HagsStr
        (El "CardCs2Status").Text = $hw.Cs2Path
        (El "CardCs2Status").Foreground = if ($hw.Cs2Found) { Get-GuiSemanticBrush "Success" "#22C55E" } else { Get-GuiSemanticBrush "Danger" "#F87171" }
        (El "CardCs2Cfg"  ).Text = $hw.OptCfg
        (El "CardCs2Cfg"  ).Foreground = if ($hw.OptOk)  { Get-GuiSemanticBrush "Success" "#22C55E" } else { Get-GuiSemanticBrush "Warning" "#FBBF24" }
        (El "CardCs2Video").Text = $hw.VideoTxt
        (El "CardCs2Video").Foreground = if ($hw.VtxtOk) { Get-GuiSemanticBrush "Success" "#22C55E" } else { Get-GuiSemanticBrush "Warning" "#FBBF24" }
    }
}

# Quick action buttons
(El "BtnDashAnalyze"  ).Add_Click({ Switch-Panel "PanelAnalyze"; Start-Analysis })
(El "BtnDashVerify"   ).Add_Click({ Switch-Panel "PanelOptimize"; Load-Settings; Load-Optimize; Start-InlineVerify })
(El "BtnDashBackup"   ).Add_Click({ Switch-Panel "PanelBackup"; Load-Backup })
(El "BtnDashPhase1"   ).Add_Click({ Launch-Terminal "Run-Optimize.ps1" })
(El "BtnDashLaunchCs2").Add_Click({ Start-Process "steam://rungameid/730" })

# ══════════════════════════════════════════════════════════════════════════════
# ANALYZE
# ══════════════════════════════════════════════════════════════════════════════
function Start-Analysis {
    [CmdletBinding(SupportsShouldProcess)]
    param()

    if ($Script:AnalysisInFlight) { return }
    if (-not $PSCmdlet.ShouldProcess("system analysis panel", "Start GUI analysis")) { return }
    $Script:AnalysisInFlight = $true
    (El "BtnRunAnalysis").IsEnabled = $false
    (El "BtnRunAnalysis").Content   = "Scanning…"
    (El "BtnCancelAnalysis").IsEnabled = $true
    (El "AnalyzeScanTime").Text     = "Scanning…"
    (El "AnalysisGrid").ItemsSource = $null
    Set-UISyncValue -Store $Script:UISync -Name "AnalysisError" -Value $null
    Set-UISyncValue -Store $Script:UISync -Name "AnalysisResults" -Value @()

    $Script:AnalysisOperation = Invoke-Async -Work {
        param($ScriptRoot, $UISync)
        . "$ScriptRoot\config.env.ps1"
        . "$ScriptRoot\helpers.ps1"
        . "$ScriptRoot\helpers\system-analysis.ps1"
        try { $UISync["AnalysisResults"] = @(Invoke-SystemAnalysis) }
        catch { $UISync["AnalysisError"] = $_.Exception.Message }
    } -WorkArgs @($Script:Root, $Script:UISync) -OnDone {
        $analysisErr = Get-UISyncValue -Store $Script:UISync -Name "AnalysisError"
        $res = @(Get-UISyncValue -Store $Script:UISync -Name "AnalysisResults")
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
        if ($warn + $err -gt 0) {
            (El "DashIssueHint").Text = "⚠  $($warn+$err) item(s) need attention — see Assess"
        }
        Refresh-StorageHealthCard
        # Clear for next run
        Set-UISyncValue -Store $Script:UISync -Name "AnalysisError" -Value $null
        Set-UISyncValue -Store $Script:UISync -Name "AnalysisResults" -Value $null
    } -OnError {
        param($asyncError)
        (El "AnalyzeScanTime").Text = "Scan error: $asyncError"
        (El "AnalysisGrid").ItemsSource = @()
    } -OnFinally {
        $Script:AnalysisInFlight = $false
        $Script:AnalysisOperation = $null
        (El "BtnRunAnalysis").IsEnabled = $true
        (El "BtnRunAnalysis").Content = "Run full scan"
        (El "BtnCancelAnalysis").IsEnabled = $false
    }
}

(El "BtnCancelAnalysis").Add_Click({
    if ($Script:AnalysisOperation) {
        (El "AnalyzeScanTime").Text = "Cancelling scan…"
        (El "BtnCancelAnalysis").IsEnabled = $false
        Stop-AsyncOperation -Operation $Script:AnalysisOperation
    }
})

(El "BtnRunAnalysis"   ).Add_Click({ Start-Analysis })
(El "BtnAnalyzeGotoOpt").Add_Click({ Switch-Panel "PanelOptimize"; Load-Settings; Load-Optimize })
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
    $observed  = @($Script:GuiObservedStepKeys)

    $rows = foreach ($s in $SCRIPT:StepCatalog) {
        $stepKey = "P$($s.Phase):$($s.Step)"
        $isDone  = $stepKey -in $completed
        $isSkip  = $stepKey -in $skipped
        $isObserved = $stepKey -in $observed

        $statusKey   = if ($s.CheckOnly) { "Check" } elseif ($isDone) { "Done" } elseif ($isSkip) { "Skipped" } elseif ($isObserved) { "Observed" } else { "Pending" }
        $statusLabel = if ($s.CheckOnly) { "—  Check" } elseif ($isDone) { "✓  Done" } elseif ($isSkip) { "—  Skipped" } elseif ($isObserved) { "◦  Observed" } else { "○  Pending" }
        $statusColor = if ($s.CheckOnly) { Get-GuiSemanticBrush "TextMuted" "#9AA5B4" } elseif ($isDone) { Get-GuiSemanticBrush "Success" "#22C55E" } elseif ($isSkip) { Get-GuiSemanticBrush "TextMuted" "#9AA5B4" } elseif ($isObserved) { Get-GuiSemanticBrush "Info" "#38BDF8" } else { Get-GuiSemanticBrush "Warning" "#FBBF24" }

        $tierColor = switch ($s.Tier) { 1 { Get-GuiSemanticBrush "Success" "#22C55E" } 2 { Get-GuiSemanticBrush "Warning" "#FBBF24" } 3 { Get-GuiSemanticBrush "Accent" "#E8520A" } default { Get-GuiSemanticBrush "TextMuted" "#9AA5B4" } }
        $riskColor = switch ($s.Risk) {
            "SAFE"       { Get-GuiSemanticBrush "Success" "#22C55E" } "MODERATE"   { Get-GuiSemanticBrush "Warning" "#FBBF24" }
            "AGGRESSIVE" { Get-GuiSemanticBrush "Accent" "#E8520A" } "CRITICAL"   { Get-GuiSemanticBrush "Danger" "#F87171" }
            default      { Get-GuiSemanticBrush "TextMuted" "#9AA5B4" }
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
            _Step       = $s
        }
    }

    $SCRIPT:OptimizeAllRows = $rows
    (El "OptimizeGrid").ItemsSource = $rows

    # Populate category filter
    $cats = @("All") + ($rows | Select-Object -ExpandProperty Category -Unique | Sort-Object)
    (El "OptFilterCat").ItemsSource   = $cats
    (El "OptFilterCat").SelectedIndex = 0

    $statuses = @("All", "Pending", "Done", "Skipped", "Observed", "Check")
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
# Checks actual system state (registry/services) and records UI-only observed
# state without marking runtime steps complete for resume purposes.
function Start-InlineVerify {
    [CmdletBinding(SupportsShouldProcess)]
    param()

    if ($Script:VerifyInFlight) { return }
    if (-not $PSCmdlet.ShouldProcess("optimize panel", "Start inline verification")) { return }
    $Script:VerifyInFlight = $true
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
                         @{P="HKCU:\Control Panel\Mouse"; N="MouseThreshold2"; E="0"},
                         @{P="HKLM:\SYSTEM\CurrentControlSet\Services\mouclass\Parameters"; N="MouseDataQueueSize"; E=50} )
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
        } catch {
            Write-DebugLog "Inline verification service check failed: $($_.Exception.Message)"
        }

        # ── NIC check (P1:25 - Disable Nagle) ─────────────────────────
        try {
            $nicGuid = Get-ActiveNicGuid
            if ($nicGuid) {
                $tcpBase = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\$nicGuid"
                $r = Test-RegistryCheck $tcpBase "TcpNoDelay" 1 "" -Quiet
                if ($r.Status -eq "OK") { $verified.Add("P1:25") }
            }
        } catch {
            Write-DebugLog "Inline verification NIC check failed: $($_.Exception.Message)"
        }

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
        } catch {
            Write-DebugLog "Inline verification NVIDIA check failed: $($_.Exception.Message)"
        }

        $UISync["VerifyResults"] = @($verified)
    } -WorkArgs @($Script:Root, $Script:UISync) -OnDone {
        $verified = Get-UISyncValue -Store $Script:UISync -Name "VerifyResults"
        if (-not $verified) { $verified = @() }

        $Script:GuiObservedStepKeys = @($verified | Sort-Object -Unique)

        # Reload grid with UI-only observed status; do not mutate progress.json.
        Load-Optimize

        $total = @($verified).Count
        $msg = "Observed: $total step(s) match expected state.`nRuntime progress was not changed."
        [System.Windows.MessageBox]::Show($msg, "Verification Complete")
        Set-UISyncValue -Store $Script:UISync -Name "VerifyResults" -Value $null
    } -OnError {
        param($asyncError)
        [System.Windows.MessageBox]::Show("Verification failed: $asyncError", "Verification Failed", "OK", "Error")
    } -OnFinally {
        $Script:VerifyInFlight = $false
        (El "BtnOptVerify").IsEnabled = $true
        (El "BtnOptVerify").Content = "Verify supported settings"
    }
}

(El "BtnOptPhase1"   ).Add_Click({ Launch-Terminal "Run-Optimize.ps1" })
(El "BtnOptPhase2"   ).Add_Click({ Start-PublishedPhaseRuntime "SafeMode-DriverClean.ps1" })
(El "BtnOptPhase3"   ).Add_Click({ Start-PublishedPhaseRuntime "PostReboot-Setup.ps1" })
(El "BtnOptFullSetup").Add_Click({ Launch-Terminal "Run-Optimize.ps1" })
(El "BtnOptVerify"   ).Add_Click({ Start-InlineVerify })

# Phase 2 button: only enabled in Safe Mode (driver files are locked in Normal Mode)
if (-not $env:SAFEBOOT_OPTION) {
    (El "BtnOptPhase2").IsEnabled = $false
    (El "BtnOptPhase2").ToolTip   = "Phase 2 requires Safe Mode (use 'Boot to Safe Mode' first)"
}

function Invoke-GuiSafeModeExit {
    <#  Remove SafeBoot and verify its absence before allowing the GUI to
        restart. A successful delete exit code alone is not authoritative: an
        enum failure or a remaining BCD element must keep the current Safe Mode
        session alive for manual recovery.  #>
    [CmdletBinding(SupportsShouldProcess)]
    param()

    if (-not $PSCmdlet.ShouldProcess("Safe Mode boot configuration", "Remove SafeBoot and restart into Normal Mode")) {
        return $false
    }

    $safeBootResult = Clear-SafeBootVerified
    if (-not $safeBootResult.Verified) {
        [System.Windows.MessageBox]::Show(
            "Safe Mode could not be verified disabled.`n`n$($safeBootResult.Message)`n`nReboot aborted — remain in this session and run the documented manual recovery commands.",
            "Safe Mode Recovery Failed", "OK", "Error") | Out-Null
        return $false
    }

    $global:LASTEXITCODE = $null
    try {
        $shutdownOutput = shutdown /r /t 5 /f 2>&1
        $shutdownExitCode = $LASTEXITCODE
    } catch {
        $shutdownOutput = $_
        $shutdownExitCode = $null
    }
    if ($null -eq $shutdownExitCode -or $shutdownExitCode -ne 0) {
        $exitText = if ($null -eq $shutdownExitCode) { "not available" } else { [string]$shutdownExitCode }
        [System.Windows.MessageBox]::Show(
            "Safe Mode was disabled, but Windows rejected the restart request (exit code: $exitText).`n`n$shutdownOutput`n`nRestart manually when ready; the next boot will use Normal Mode.",
            "Restart Failed", "OK", "Error") | Out-Null
        return $false
    }
    return $true
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
            $null = Invoke-GuiSafeModeExit
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
            (El "BtnBackupExport").IsEnabled = $false
            (El "BtnRestoreAll").IsEnabled = $false
            (El "BtnRestoreStep").IsEnabled = $false
            (El "BtnClearBackup").IsEnabled = $false
            return
        }
        $entries = $bd.entries
        if ($entries.Count -eq 0) {
            (El "BackupSummary").Text = "No backups found in backup.json"
            (El "BackupGrid").ItemsSource = $null
            (El "BtnBackupExport").IsEnabled = $false
            (El "BtnRestoreAll").IsEnabled = $false
            (El "BtnRestoreStep").IsEnabled = $false
            (El "BtnClearBackup").IsEnabled = $false
            return
        }
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
        (El "BtnBackupExport").IsEnabled = $true
        (El "BtnRestoreAll").IsEnabled = $true
        (El "BtnRestoreStep").IsEnabled = $false
        (El "BtnClearBackup").IsEnabled = $true
    } catch {
        (El "BackupSummary").Text = "Error loading backup.json: $($_.Exception.Message)"
        (El "BackupGrid").ItemsSource = $null
        (El "BtnBackupExport").IsEnabled = $false
        (El "BtnRestoreAll").IsEnabled = $false
        (El "BtnRestoreStep").IsEnabled = $false
        (El "BtnClearBackup").IsEnabled = $false
    }
}

(El "BtnBackupRefresh").Add_Click({ Load-Backup })
(El "BackupGrid").Add_SelectionChanged({
    (El "BtnRestoreStep").IsEnabled = $null -ne (El "BackupGrid").SelectedItem
})

(El "BtnBackupExport").Add_Click({
    $src = "$CFG_WorkDir\backup.json"
    if (-not (Test-Path $src)) { [System.Windows.MessageBox]::Show("backup.json not found.","Export"); return }
    $dlg = [Microsoft.Win32.SaveFileDialog]::new()
    $dlg.Filter = "JSON files (*.json)|*.json|All files (*.*)|*.*"
    $dlg.FileName = "cs2-backup-$(Get-Date -Format 'yyyyMMdd-HHmm').json"
    if ($dlg.ShowDialog() -eq $true) { Copy-Item $src $dlg.FileName -Force; [System.Windows.MessageBox]::Show("Exported to:`n$($dlg.FileName)","Export Complete") }
})

(El "BtnRestoreAll").Add_Click({
    $r = [System.Windows.MessageBox]::Show("Restore ALL backed-up settings?`nThis will undo every change the suite made.","Restore All","YesNo","Warning")
    if ($r -eq "Yes") {
        if (-not (Enter-GuiBackupOperation)) { return }
        $Script:CriticalOperation = "Recovery"
        (El "BackupSummary").Text = "Restoring all recorded changes…"
        foreach ($name in "BtnBackupRefresh", "BtnBackupExport", "BtnRestoreAll", "BtnRestoreStep", "BtnClearBackup") {
            (El $name).IsEnabled = $false
        }
        Set-UISyncValue -Store $Script:UISync -Name "RestoreError" -Value $null
        Invoke-Async -Work {
            param($ScriptRoot, $UISync)
            . "$ScriptRoot\config.env.ps1"
            . "$ScriptRoot\helpers.ps1"
            try {
                $bd = Get-BackupData
                if ($bd.entries -and $bd.entries.Count -gt 0) {
                    $stepNames = @(($bd.entries | Group-Object -Property step).Name)
                    $failures = 0
                    foreach ($sn in $stepNames) {
                        if (-not (Restore-StepChanges -StepTitle $sn)) { $failures++ }
                    }
                    if ($failures -gt 0) { throw "$failures step group(s) had restore failures." }
                }
            } catch {
                $UISync["RestoreError"] = $_.Exception.Message
            }
        } -WorkArgs @($Script:Root, $Script:UISync) -OnDone {
            $restoreError = Get-UISyncValue -Store $Script:UISync -Name "RestoreError"
            if ($restoreError) {
                [System.Windows.MessageBox]::Show("Restore error: $restoreError", "Restore Failed", "OK", "Error")
            } else {
                [System.Windows.MessageBox]::Show("All settings restored successfully.", "Restore Complete")
            }
        } -OnFinally {
            Set-UISyncValue -Store $Script:UISync -Name "RestoreError" -Value $null
            Remove-BackupLock
            $Script:CriticalOperation = $null
            Load-Backup
        }
    }
})

(El "BtnRestoreStep").Add_Click({
    $sel = (El "BackupGrid").SelectedItem
    if (-not $sel) { [System.Windows.MessageBox]::Show("Select a row first.","Restore Step"); return }
    $stepTitle = $sel.Step
    $r = [System.Windows.MessageBox]::Show("Restore all changes from:`n`"$stepTitle`"?","Restore Step","YesNo","Question")
    if ($r -eq "Yes") {
        if (-not (Enter-GuiBackupOperation)) { return }
        $Script:CriticalOperation = "Recovery"
        (El "BackupSummary").Text = "Restoring $stepTitle…"
        foreach ($name in "BtnBackupRefresh", "BtnBackupExport", "BtnRestoreAll", "BtnRestoreStep", "BtnClearBackup") {
            (El $name).IsEnabled = $false
        }
        Set-UISyncValue -Store $Script:UISync -Name "RestoreStepResult" -Value $null
        Set-UISyncValue -Store $Script:UISync -Name "RestoreError" -Value $null
        Set-UISyncValue -Store $Script:UISync -Name "RestoreStepTitle" -Value $stepTitle
        Invoke-Async -Work {
            param($ScriptRoot, $UISync, $StepTitle)
            . "$ScriptRoot\config.env.ps1"
            . "$ScriptRoot\helpers.ps1"
            try {
                $UISync["RestoreStepResult"] = Restore-StepChanges -StepTitle $StepTitle
            } catch {
                $UISync["RestoreError"] = $_.Exception.Message
            }
        } -WorkArgs @($Script:Root, $Script:UISync, $stepTitle) -OnDone {
            $restoreError = Get-UISyncValue -Store $Script:UISync -Name "RestoreError"
            $ok = Get-UISyncValue -Store $Script:UISync -Name "RestoreStepResult"
            $restoredStepTitle = Get-UISyncValue -Store $Script:UISync -Name "RestoreStepTitle"
            if ($restoreError) {
                [System.Windows.MessageBox]::Show("Restore error: $restoreError", "Restore Failed", "OK", "Error")
            } elseif ($ok) {
                [System.Windows.MessageBox]::Show("Restore complete for: $restoredStepTitle", "Done")
            } else {
                [System.Windows.MessageBox]::Show("Restore partially failed for: $restoredStepTitle. Some entries could not be restored; check the log.", "Restore Incomplete", "OK", "Warning")
            }
        } -OnFinally {
            Set-UISyncValue -Store $Script:UISync -Name "RestoreStepResult" -Value $null
            Set-UISyncValue -Store $Script:UISync -Name "RestoreError" -Value $null
            Set-UISyncValue -Store $Script:UISync -Name "RestoreStepTitle" -Value $null
            Remove-BackupLock
            $Script:CriticalOperation = $null
            Load-Backup
        }
    }
})

(El "BtnClearBackup").Add_Click({
    $r = [System.Windows.MessageBox]::Show("Delete all backup data?`nThis cannot be undone.","Clear Backups","YesNo","Warning")
    if ($r -eq "Yes") {
        if (-not (Enter-GuiBackupOperation)) { return }
        try {
            Save-BackupData (New-BackupDataObject)
            Load-Backup
        } finally {
            Remove-BackupLock
        }
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
            Draw-BenchChart @()
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
            $dc = if ($i -eq 0 -or $dAvg -eq "—") { Get-GuiSemanticBrush "TextMuted" "#9AA5B4" } elseif ($dAvg.StartsWith("+")) { Get-GuiSemanticBrush "Success" "#22C55E" } elseif ($dAvg -eq "0%" -or $dAvg -eq "0.0%") { Get-GuiSemanticBrush "TextMuted" "#9AA5B4" } else { Get-GuiSemanticBrush "Danger" "#F87171" }
            $dp1c = if ($i -eq 0 -or $dP1 -eq "—") { Get-GuiSemanticBrush "TextMuted" "#9AA5B4" } elseif ($dP1.StartsWith("+")) { Get-GuiSemanticBrush "Success" "#22C55E" } elseif ($dP1 -eq "0%" -or $dP1 -eq "0.0%") { Get-GuiSemanticBrush "TextMuted" "#9AA5B4" } else { Get-GuiSemanticBrush "Danger" "#F87171" }
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
        $gl.Stroke = Get-GuiSemanticBrush "Border" "#313943"; $gl.StrokeThickness = 1
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
    $avgLine.Stroke = Get-GuiSemanticBrush "Accent" "#E8520A"; $avgLine.StrokeThickness = 2
    $canvas.Children.Add($avgLine) | Out-Null

    # P1 line
    $p1Line = [System.Windows.Shapes.Polyline]::new()
    $p1Line.Points = $p1Pts
    $p1Line.Stroke = Get-GuiSemanticBrush "Success" "#22C55E"; $p1Line.StrokeThickness = 2; $p1Line.StrokeDashArray = [System.Windows.Media.DoubleCollection]@(4, 3)
    $canvas.Children.Add($p1Line) | Out-Null

    # Dots + x-axis labels
    for ($i = 0; $i -lt $hist.Count; $i++) {
        $x = $i * $xStep
        foreach ($pts in @($avgPts, $p1Pts)) {
            $dot = [System.Windows.Shapes.Ellipse]::new()
            $dot.Width = 6; $dot.Height = 6
            $dot.Fill = if ($pts -eq $avgPts) { Get-GuiSemanticBrush "Accent" "#E8520A" } else { Get-GuiSemanticBrush "Success" "#22C55E" }
            [System.Windows.Controls.Canvas]::SetLeft($dot, $pts[$i].X - 3)
            [System.Windows.Controls.Canvas]::SetTop( $dot, $pts[$i].Y - 3)
            $canvas.Children.Add($dot) | Out-Null
        }
        # x-label
        $lbl = [System.Windows.Controls.TextBlock]::new()
        $lbl.Text = try { [datetime]::ParseExact($hist[$i].timestamp,"yyyy-MM-dd HH:mm:ss",$null).ToString("d-MMM") } catch { "$($i+1)" }
        $lbl.FontSize = 10; $lbl.Foreground = Get-GuiSemanticBrush "TextMuted" "#9AA5B4"
        [System.Windows.Controls.Canvas]::SetLeft($lbl, $x - 16)
        [System.Windows.Controls.Canvas]::SetTop( $lbl, $h + 4)
        $canvas.Children.Add($lbl) | Out-Null
    }

    # Y-axis label
    $yLbl = [System.Windows.Controls.TextBlock]::new()
    $yLbl.Text = "FPS"; $yLbl.FontSize = 10; $yLbl.Foreground = Get-GuiSemanticBrush "TextMuted" "#9AA5B4"
    [System.Windows.Controls.Canvas]::SetLeft($yLbl, -28)
    [System.Windows.Controls.Canvas]::SetTop( $yLbl, $h / 2 - 8)
    $canvas.Children.Add($yLbl) | Out-Null

    # Legend
    $leg = [System.Windows.Controls.TextBlock]::new()
    $leg.Text = "— Avg FPS   - - 1% Low"; $leg.FontSize = 10; $leg.Foreground = Get-GuiSemanticBrush "TextMuted" "#9AA5B4"
    [System.Windows.Controls.Canvas]::SetLeft($leg, $w - 120)
    [System.Windows.Controls.Canvas]::SetTop( $leg, -16)
    $canvas.Children.Add($leg) | Out-Null
}

# FPS Cap
function Get-BenchmarkResultLabel {
    return (El "BenchLabel").Text.Trim()
}

(El "BtnBenchParse").Add_Click({
    $raw = (El "BenchVprof").Text.Trim()
    $parsed = Get-BenchmarkCapFromText $raw
    if ($parsed) {
        (El "BenchCapLabel").Text = "→  Cap:"
        (El "BenchCapValue").Text = "$($parsed.Cap)"
        Set-UISyncValue -Store $Script:UISync -Name "LastCap" -Value $parsed.Cap
        (El "BtnBenchCopy").IsEnabled = $true
    } else {
        (El "BenchCapLabel").Text = "⚠  No [VProf] FPS line detected"
        (El "BenchCapValue").Text = ""
        Set-UISyncValue -Store $Script:UISync -Name "LastCap" -Value $null
        (El "BtnBenchCopy").IsEnabled = $false
    }
})

(El "BtnBenchCopy").Add_Click({
    $cap = Get-UISyncValue -Store $Script:UISync -Name "LastCap"
    if ($cap) {
        try { [System.Windows.Clipboard]::SetText("$cap") }
        catch { Write-DebugLog "Clipboard copy failed: $_" }
    }
})

(El "BtnBenchAdd").Add_Click({
    $raw = (El "BenchVprof").Text.Trim()
    $parsed = Get-BenchmarkResultFromText $raw
    if ($parsed) {
        $lbl = Get-BenchmarkResultLabel
        if (-not [string]::IsNullOrWhiteSpace($lbl)) {
            Add-BenchmarkResult -AvgFps $parsed.AvgFps -P1Fps $parsed.P1Fps -Label $lbl -Runs 1
            (El "BenchLabel").Text = ""
            Load-Benchmark
        }
    } else {
        [System.Windows.MessageBox]::Show("Paste a [VProf] FPS: Avg=… P1=… line first.","Add Result")
    }
})

# ══════════════════════════════════════════════════════════════════════════════
# NETWORK
# ══════════════════════════════════════════════════════════════════════════════
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
        "  ·  Delta: $sign$($row.DeltaMs) ms"
    } else { "" }
    (El "NetDiagRegionSummary").Text = "$SelectedRegion  ·  Baseline: $baseline  ·  Post: $post$delta"
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
        (El "NetDiagAdapterSummary").Text = "Adapter: $($summary.AdapterName)  ·  $($summary.AdapterType)"
        (El "NetDiagDnsSummary").Text = "DNS: $($summary.DnsProvider)  ·  $dnsText"
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
        "Latest run: $($latest.Timestamp)  ·  $($latest.Kind)  ·  ${latestRegion}: $rtt"
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
                "Remove all firewall rules created by CS2 Optimize for Valve network blocking?",
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
                [System.Windows.MessageBox]::Show("$region was not blocked by CS2 Optimize.", "Valve Network Blocks")
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
    (El $buttonName).Content = if ($Kind -eq "baseline") { "Running…" } else { "Retesting…" }
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

# ══════════════════════════════════════════════════════════════════════════════
# VIDEO
# ══════════════════════════════════════════════════════════════════════════════
$Script:VideoTxtPath = $null
$Script:VideoSteamPath = $null

function Test-CurrentVideoTxtPathTrusted {
    if (-not $Script:VideoTxtPath) { return $false }
    $steamPath = if ($Script:VideoSteamPath) { $Script:VideoSteamPath } else { Get-SteamPath }
    return (Test-TrustedVideoTxtPath -Path $Script:VideoTxtPath -SteamPath $steamPath)
}

function Load-Video {
    # Populate tier picker
    if ((El "VideoTierPicker").Items.Count -eq 0) {
        foreach ($t in @("Auto","HIGH","MID","LOW")) { (El "VideoTierPicker").Items.Add($t) | Out-Null }
        (El "VideoTierPicker").SelectedIndex = 0
    }

    $steamPath = Get-SteamPath
    $vtxt = if ($steamPath) {
        Get-ChildItem "$steamPath\userdata\*\730\local\cfg\video.txt" -ErrorAction SilentlyContinue |
            Where-Object { Test-TrustedVideoTxtPath -Path $_.FullName -SteamPath $steamPath } |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
    }

    if ($vtxt) {
        $Script:VideoTxtPath = $vtxt.FullName
        $Script:VideoSteamPath = $steamPath
        (El "VideoTxtPath").Text = $vtxt.FullName
        (El "BtnVideoWrite").IsEnabled = $true
        (El "BtnVideoWriteFooter").IsEnabled = $true
    } else {
        $Script:VideoTxtPath = $null
        $Script:VideoSteamPath = $null
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
        "setting.msaa_samples"              = @{ V="4";  N="4x MSAA — high-end default; benchmark vs 2x/CMAA2" }
        "setting.mat_vsync"                 = @{ V="0";  N="Off — fixed-refresh low-latency default" }
        "setting.fullscreen"                = @{ V="1";  N="Exclusive fullscreen — bypasses DWM compositor" }
        "setting.r_low_latency"             = @{ V="1";  N="NVIDIA Reflex On — saves 3-4ms input latency" }
        "setting.r_csgo_fsr_upsample"       = @{ V="0";  N="FSR OFF — native clarity default" }
        "setting.shaderquality"             = @{ V="1";  N="High — quality default when GPU has headroom" }
        "setting.r_texturefilteringquality" = @{ V="5";  N="AF16x — near-zero cost on modern GPUs" }
        "setting.r_csgo_cmaa_enable"        = @{ V="0";  N="Off — MSAA handles AA" }
        "setting.r_aoproxy_enable"          = @{ V="0";  N="AO off — purely cosmetic, up to 6% FPS cost" }
        "setting.sc_hdr_enabled_override"   = @{ V="3";  N="Performance — suite default; compare visually" }
        "setting.r_particle_max_detail_level"=@{ V="0";  N="Low particles — no competitive disadvantage" }
        "setting.csm_enabled"               = @{ V="1";  N="Shadows ON — keep tactical shadow cues" }
        "setting.videocfg_dynamic_shadows"  = @{ V="1";  N="Dynamic Shadows All — current competitive cue default" }
    }
    "MID" = @{
        "setting.msaa_samples"              = @{ V="4";  N="4x — or 2x if FPS budget is tight" }
        "setting.mat_vsync"                 = @{ V="0";  N="Off — fixed-refresh default" }
        "setting.fullscreen"                = @{ V="1";  N="Exclusive fullscreen" }
        "setting.r_low_latency"             = @{ V="1";  N="NVIDIA Reflex On" }
        "setting.r_csgo_fsr_upsample"       = @{ V="0";  N="FSR OFF — native clarity default" }
        "setting.shaderquality"             = @{ V="0";  N="Low — saves GPU headroom on mid-tier" }
        "setting.r_texturefilteringquality" = @{ V="5";  N="AF16x" }
        "setting.r_csgo_cmaa_enable"        = @{ V="0";  N="Off — MSAA handles AA" }
        "setting.r_aoproxy_enable"          = @{ V="0";  N="AO off" }
        "setting.sc_hdr_enabled_override"   = @{ V="3";  N="Performance — suite default" }
        "setting.r_particle_max_detail_level"=@{ V="0";  N="Low" }
        "setting.csm_enabled"               = @{ V="1";  N="Shadows ON" }
        "setting.videocfg_dynamic_shadows"  = @{ V="1";  N="Dynamic Shadows All" }
    }
    "LOW" = @{
        "setting.msaa_samples"              = @{ V="0";  N="None + CMAA2 — free AA alternative" }
        "setting.mat_vsync"                 = @{ V="0";  N="Off — fixed-refresh default" }
        "setting.fullscreen"                = @{ V="1";  N="Exclusive fullscreen — critical for FPS" }
        "setting.r_low_latency"             = @{ V="1";  N="NVIDIA Reflex On" }
        "setting.r_csgo_fsr_upsample"       = @{ V="0";  N="FSR OFF — lower resolution first" }
        "setting.shaderquality"             = @{ V="0";  N="Low" }
        "setting.r_texturefilteringquality" = @{ V="0";  N="Bilinear — legacy for max FPS" }
        "setting.r_csgo_cmaa_enable"        = @{ V="1";  N="CMAA2 ON — near-zero cost AA when MSAA=0" }
        "setting.r_aoproxy_enable"          = @{ V="0";  N="AO off" }
        "setting.sc_hdr_enabled_override"   = @{ V="3";  N="Performance" }
        "setting.r_particle_max_detail_level"=@{ V="0";  N="Low" }
        "setting.csm_enabled"               = @{ V="1";  N="Shadows ON — keep even on low-end" }
        "setting.videocfg_dynamic_shadows"  = @{ V="1";  N="Dynamic Shadows All — lower other shadow quality first" }
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
    if ((Test-CurrentVideoTxtPathTrusted) -and (Test-Path $Script:VideoTxtPath)) {
        Get-Content $Script:VideoTxtPath | ForEach-Object {
            if ($_ -match '^\s*"([^"]+)"\s+"([^"]*)"') { $current[$Matches[1]] = $Matches[2] }
        }
    }

    $rows = foreach ($kv in $recommended.GetEnumerator() | Sort-Object Key) {
        $cur  = $current[$kv.Key]
        $rec  = $kv.Value.V
        $note = $kv.Value.N
        $st   = if ($null -eq $cur) { "—  Missing" } elseif ($cur -eq $rec) { "✓  OK" } else { "⚠  Differs" }
        $sc   = if ($st -match "OK") { Get-GuiSemanticBrush "Success" "#22C55E" } elseif ($st -match "Missing") { Get-GuiSemanticBrush "TextMuted" "#9AA5B4" } else { Get-GuiSemanticBrush "Warning" "#FBBF24" }
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
    if (-not (Test-CurrentVideoTxtPathTrusted)) {
        [System.Windows.MessageBox]::Show("video.txt path is outside the trusted Steam userdata tree.","Write","OK","Error")
        return
    }

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
# PROFILE AND PREVIEW MODE
# ══════════════════════════════════════════════════════════════════════════════
function Load-Settings {
    $state = $null
    $state = Get-StateDataSafe

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
    $mode = Get-ModeForProfile -Profile $prof -DryRun:$dry
    try {
        $state = $null
        $state = Get-StateDataSafe
        # Skip write if nothing changed
        if ($state -and $state.PSObject.Properties['profile'] -and $state.PSObject.Properties['mode'] -and $state.profile -eq $prof -and $state.mode -eq $mode) { return }
        if (-not $state) { $state = [PSCustomObject]@{ mode = $mode; profile = $prof } }
        $state | Add-Member -NotePropertyName "profile" -NotePropertyValue $prof -Force
        $state | Add-Member -NotePropertyName "mode"    -NotePropertyValue $mode -Force
        Save-SuiteState -State $state
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

function Start-PublishedPhaseRuntime {
    [CmdletBinding(SupportsShouldProcess)]
    param([Parameter(Mandatory)][ValidateSet("SafeMode-DriverClean.ps1", "PostReboot-Setup.ps1")][string]$Script)

    try {
        $runtimeRoot = Get-PhaseRuntimeRoot -DestinationRoot $CFG_WorkDir
    } catch {
        [System.Windows.MessageBox]::Show(
            "The published runtime pointer is invalid.`n`n$_`n`nRun Phase 1 again to publish a verified generation.",
            "Runtime pointer invalid", "OK", "Error") | Out-Null
        return $false
    }
    $runtimeScript = Join-Path $runtimeRoot $Script
    if (-not (Test-Path -LiteralPath $runtimeScript -PathType Leaf)) {
        [System.Windows.MessageBox]::Show(
            "The verified Phase 2/3 runtime payload is missing.`n`nRun Phase 1 again to publish a fresh immutable runtime generation, then retry.",
            "Runtime payload missing", "OK", "Warning") | Out-Null
        return $false
    }
    $payloadValidation = Test-PhaseRuntimePayload -RuntimeRoot $runtimeRoot
    if (-not $payloadValidation.Valid) {
        [System.Windows.MessageBox]::Show(
            "The Phase 2/3 runtime payload failed integrity validation.`n`n$($payloadValidation.Message)`n`nRun Phase 1 again to republish it, then retry.",
            "Runtime payload invalid", "OK", "Error") | Out-Null
        return $false
    }
    $fileArg = "`"$runtimeScript`""
    $allArgs = "-NoProfile -ExecutionPolicy Bypass -WindowStyle Normal -File $fileArg"
    if (-not $PSCmdlet.ShouldProcess($runtimeScript, "Start verified published Phase runtime")) {
        return $false
    }
    Start-Process powershell -ArgumentList $allArgs
    return $true
}
