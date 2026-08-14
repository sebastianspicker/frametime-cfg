# ==============================================================================
#  tests/helpers/gui-panels.Tests.ps1  --  Non-WPF GUI logic
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"

    $Script:Root = (Resolve-Path "$PSScriptRoot/../..").Path

    if (-not ("System.Windows.MessageBox" -as [type])) {
        Add-Type -TypeDefinition @'
namespace System.Windows {
    public static class MessageBox {
        public static object Show(string messageBoxText, string caption) { return null; }
        public static object Show(string messageBoxText, string caption, string button) { return null; }
        public static object Show(string messageBoxText, string caption, string button, string icon) { return null; }
    }
}
'@
    }

    if (-not ("System.Windows.Automation.AutomationProperties" -as [type])) {
        Add-Type -TypeDefinition @'
namespace System.Windows.Automation {
    public static class AutomationProperties {
        public static void SetItemStatus(object element, string value) { }
    }
}
'@
    }

    . "$Script:Root/helpers/step-catalog.ps1"

    function New-FakeGuiElement {
        param()

        $element = [PSCustomObject]@{
            Name         = ""
            Visibility   = "Collapsed"
            Style        = $null
            Text         = ""
            SelectedItem = $null
            SelectedIndex = -1
            IsChecked    = $false
            IsEnabled    = $true
            Content      = ""
            ToolTip      = ""
            Foreground   = $null
            ItemsSource  = $null
            Items        = [System.Collections.ArrayList]::new()
            Children     = [System.Collections.ArrayList]::new()
            ActualWidth  = 600
            ActualHeight = 130
            ClickHandler = $null
            SelectionChangedHandler = $null
        }
        $element | Add-Member -MemberType ScriptMethod -Name Add_Click -Value {
            param($Handler)
            [void]($this.ClickHandler = $Handler)
            $script:GuiClickHandlers[$this.Name] = $Handler
        }
        $element | Add-Member -MemberType ScriptMethod -Name Add_SelectionChanged -Value {
            param($Handler)
            [void]($this.SelectionChangedHandler = $Handler)
            $script:GuiSelectionChangedHandlers[$this.Name] = $Handler
        }
        $element | Add-Member -MemberType ScriptMethod -Name Add_Checked -Value { param($Handler) }
        $element | Add-Member -MemberType ScriptMethod -Name Add_Unchecked -Value { param($Handler) }
        $element | Add-Member -MemberType ScriptMethod -Name UpdateLayout -Value { }
        return $element
    }

    $script:GuiElements = @{}
    $script:GuiClickHandlers = @{}
    $script:GuiSelectionChangedHandlers = @{}
    function El {
        param([string]$Name)
        if (-not $script:GuiElements.ContainsKey($Name)) {
            $element = New-FakeGuiElement
            $element.Name = $Name
            if ($script:GuiClickHandlers.ContainsKey($Name)) {
                $element.ClickHandler = $script:GuiClickHandlers[$Name]
            }
            if ($script:GuiSelectionChangedHandlers.ContainsKey($Name)) {
                $element.SelectionChangedHandler = $script:GuiSelectionChangedHandlers[$Name]
            }
            $script:GuiElements[$Name] = $element
        }
        return $script:GuiElements[$Name]
    }

    function New-Brush {
        param([string]$Color)
        $Color
    }
    function Invoke-Async {}
    function Launch-Terminal {}
    function Load-Dashboard {}
    function Start-Analysis {
        param()
    }
    function Load-Optimize {}
    function Start-InlineVerify {
        param()
    }
    function Load-Backup {}
    function Load-Benchmark {}
    function Load-Video {}
    function Load-Settings {}
    function Add-BenchmarkResult {}
    function Get-BenchmarkHistory { @() }
    function Stop-AsyncOperation {
        [CmdletBinding(SupportsShouldProcess)]
        param([hashtable]$Operation)

        if (-not $PSCmdlet.ShouldProcess("asynchronous operation", "Stop")) { return }
    }
    function Write-DebugLog {}
    function global:shutdown { param([Parameter(ValueFromRemainingArguments)]$CmdArgs) }

    $Script:UISync = @{}
    $Script:AllPanels = @("PanelDashboard", "PanelAnalyze", "PanelOptimize", "PanelNetwork")
    $Script:NavMap = @{
        "PanelDashboard" = "NavDashboard"
        "PanelAnalyze"   = "NavAnalyze"
        "PanelOptimize"  = "NavOptimize"
        "PanelNetwork"   = "NavNetwork"
    }
    $ActiveStyle = "ACTIVE"
    $InactiveStyle = "INACTIVE"

    . "$PSScriptRoot/../../helpers/gui-panels.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Switch-Panel" {

    BeforeEach {
        $script:GuiElements = @{}
        $Script:AllPanels = @("PanelDashboard", "PanelAnalyze", "PanelOptimize", "PanelNetwork")
        $Script:NavMap = @{
            "PanelDashboard" = "NavDashboard"
            "PanelAnalyze"   = "NavAnalyze"
            "PanelOptimize"  = "NavOptimize"
            "PanelNetwork"   = "NavNetwork"
        }
        $Script:ActivePanel = "PanelDashboard"
    }

    It "updates panel visibility and active nav style" {
        Switch-Panel -PanelName "PanelAnalyze"

        (El "PanelDashboard").Visibility | Should -Be "Collapsed"
        (El "PanelAnalyze").Visibility   | Should -Be "Visible"
        (El "NavDashboard").Style        | Should -Be "INACTIVE"
        (El "NavAnalyze").Style          | Should -Be "ACTIVE"
        $Script:ActivePanel              | Should -Be "PanelAnalyze"
    }

    It "runs the OnSwitch callback after updating state" {
        $called = $false

        Switch-Panel -PanelName "PanelOptimize" -OnSwitch { $script:called = $true }

        $script:called | Should -Be $true
        $Script:ActivePanel | Should -Be "PanelOptimize"
    }
}

Describe "Video path trust" {

    BeforeEach {
        $Script:VideoTxtPath = $null
        $Script:VideoSteamPath = $null
    }

    It "trusts a video.txt path under the recorded Steam root" {
        $Script:VideoSteamPath = "C:\Program Files (x86)\Steam"
        $Script:VideoTxtPath = "C:\Program Files (x86)\Steam\userdata\123\730\local\cfg\video.txt"

        Test-CurrentVideoTxtPathTrusted | Should -BeTrue
    }

    It "rejects a video.txt path outside the recorded Steam root" {
        $Script:VideoSteamPath = "C:\Program Files (x86)\Steam"
        $Script:VideoTxtPath = "C:\Users\Public\userdata\123\730\local\cfg\video.txt"

        Test-CurrentVideoTxtPathTrusted | Should -BeFalse
    }
}

Describe "Network panel helpers" {

    BeforeEach {
        $script:GuiElements = @{}
    }

    It "renders the current adapter, DNS state, comparison rows, and history rows" {
        Mock Get-NetworkDiagnosticSummary {
            [PSCustomObject]@{
                AdapterFound = $true
                AdapterName  = "Ethernet"
                AdapterType  = "Physical / wired"
                DnsProvider  = "Cloudflare"
                DnsServers   = @("1.1.1.1", "1.0.0.1")
            }
        }
        Mock Get-LatencyHistoryRows {
            @(
                [PSCustomObject]@{ Timestamp = "2026-04-15 12:00:00"; Kind = "baseline"; AdapterName = "Ethernet"; DnsProvider = "Cloudflare"; AvgRttMs = 18.3; RegionsOk = 4 }
            )
        }
        Mock Get-ValveLatencyComparisonRows {
            @(
                [PSCustomObject]@{ TargetLabel = "Frankfurt"; BaselineAvgMs = 18.3; PostAvgMs = 16.1; DeltaMs = -2.2; TimeoutSummary = "0 → 0"; ProtocolUsed = "ICMP"; Endpoint = "155.133.232.10" }
            )
        }

        Load-NetworkDiagnostics

        (El "NetDiagAdapterSummary").Text | Should -Match "Ethernet"
        (El "NetDiagDnsSummary").Text | Should -Match "Cloudflare"
        @((El "NetDiagComparisonGrid").ItemsSource).Count | Should -Be 1
        @((El "NetDiagHistoryGrid").ItemsSource).Count | Should -Be 1
    }

    It "reads missing async result keys as null under StrictMode" {
        $Script:UISync = [hashtable]::Synchronized(@{})

        Get-UISyncValue -Store $Script:UISync -Name "LatencyError" | Should -BeNullOrEmpty

        Set-UISyncValue -Store $Script:UISync -Name "LatencyError" -Value "boom"
        Get-UISyncValue -Store $Script:UISync -Name "LatencyError" | Should -Be "boom"
    }
}

Describe "Analyze storage helpers" {

    BeforeEach {
        $script:GuiElements = @{}
    }

    It "shows storage maintenance status without framing it as performance meta" {
        Mock Get-TrimHealthStatus {
            [PSCustomObject]@{
                Summary = "NTFS: enabled"
                AnyTrimDisabled = $false
                RetrimAvailable = $true
                RetrimmableVolumes = @("C")
            }
        }

        Refresh-StorageHealthCard

        (El "AnalyzeStorageHealth").Text | Should -Match "Storage maintenance"
        (El "BtnAnalyzeTrimEnable").IsEnabled | Should -Be $false
        (El "BtnAnalyzeRetrim").IsEnabled | Should -Be $true
    }
}

Describe "Inline verification provenance" {

    BeforeEach {
        Reset-TestState
        $script:GuiElements = @{}
        $Script:GuiObservedStepKeys = @()
        $Script:UISync = @{}
        $Script:VerifyInFlight = $false
        New-TestProgressFile -Phase 1 -LastStep 0 -CompletedSteps @() -SkippedSteps @() | Out-Null
    }

    It "shows observed state without completing runtime progress" {
        Mock Invoke-Async {
            param($Work, $WorkArgs, $OnDone, $OnError, $OnFinally)
            $Script:UISync.VerifyResults = @("P1:4")
            & $OnDone
            & $OnFinally
        }
        Mock Save-Progress { throw "Inline verification must not save progress." }
        Mock Save-StateDataSafe { throw "Inline verification must not save state." }

        Start-InlineVerify

        $prog = Load-Progress
        @($prog.completedSteps) | Should -Not -Contain "P1:4"
        Test-StepDone -phase 1 -stepNum 4 | Should -BeFalse

        $row = @((El "OptimizeGrid").ItemsSource) |
            Where-Object { $_._Step.Phase -eq 1 -and $_._Step.Step -eq 4 } |
            Select-Object -First 1
        $row.StatusKey | Should -Be "Observed"
        $Script:VerifyInFlight | Should -BeFalse
        (El "BtnOptVerify").IsEnabled | Should -BeTrue
    }
}

Describe "Benchmark parsing helpers" {

    BeforeEach {
        $script:GuiElements = @{}
        $Script:UISync = @{}
    }

    It "extracts an FPS cap from VProf text" {
        $result = Get-BenchmarkCapFromText "Noise [VProf] Avg=300.5 P1=220.4"

        $result.AvgFps | Should -Be 300.5
        $result.Cap    | Should -BeGreaterThan 0
    }

    It "extracts an FPS cap from comma-separated VProf text" {
        $result = Get-BenchmarkCapFromText "[VProf] FPS: Avg=300.5, P1=220.4"

        $result.AvgFps | Should -Be 300.5
        $result.Cap | Should -BeGreaterThan 0
    }

    It "returns null when no Avg FPS token exists" {
        Get-BenchmarkCapFromText "No FPS data here" | Should -BeNullOrEmpty
    }

    It "returns null instead of throwing for malformed FPS cap values" {
        $huge = "9" * 400
        foreach ($value in @("300..0", ".", "300.", "300,5", $huge)) {
            { $script:capParseResult = Get-BenchmarkCapFromText "[VProf] FPS: Avg=$value P1=200" } |
                Should -Not -Throw
            $script:capParseResult | Should -BeNullOrEmpty
        }
    }

    It "extracts Avg and P1 values for benchmark result imports" {
        $result = Get-BenchmarkResultFromText "[VProf] FPS: Avg=280.0, P1=190.0"

        $result.AvgFps | Should -Be 280.0
        $result.P1Fps  | Should -Be 190.0
    }

    It "returns null when the benchmark result text is incomplete" {
        Get-BenchmarkResultFromText "[VProf] Avg=280.0 only" | Should -BeNullOrEmpty
    }

    It "returns null instead of writing history for malformed benchmark result values" {
        Remove-Item $CFG_BenchmarkFile -Force -ErrorAction SilentlyContinue
        foreach ($value in @("300..0", ".", "300.", "300,5")) {
            { $script:benchmarkParseResult = Get-BenchmarkResultFromText "[VProf] FPS: Avg=$value, P1=190.0" } |
                Should -Not -Throw
            $script:benchmarkParseResult | Should -BeNullOrEmpty
        }

        Test-Path $CFG_BenchmarkFile | Should -Be $false
    }

    It "clears a stale cap and disables Copy Cap when parsing fails" {
        Set-UISyncValue -Store $Script:UISync -Name "LastCap" -Value 240
        (El "BtnBenchCopy").IsEnabled = $true
        (El "BenchVprof").Text = "not benchmark output"

        & (El "BtnBenchParse").ClickHandler

        Get-UISyncValue -Store $Script:UISync -Name "LastCap" | Should -BeNullOrEmpty
        (El "BtnBenchCopy").IsEnabled | Should -BeFalse
    }

    It "does not record a benchmark result when its label is empty" {
        (El "BenchVprof").Text = "[VProf] FPS: Avg=280.0, P1=190.0"
        Mock Get-BenchmarkResultLabel { "" }
        Mock Add-BenchmarkResult {}

        & (El "BtnBenchAdd").ClickHandler

        Should -Invoke Add-BenchmarkResult -Times 0 -Exactly
    }
}

Describe "Backup panel loading" {

    BeforeEach {
        $script:GuiElements = @{}
    }

    It "clears stale rows and disables backup actions when no entries exist" {
        (El "BackupGrid").ItemsSource = @([pscustomobject]@{ Step = "stale" })
        Mock Get-BackupData { [pscustomobject]@{ entries = @(); created = "2026-07-11" } }

        Load-Backup

        (El "BackupGrid").ItemsSource | Should -BeNullOrEmpty
        foreach ($name in "BtnBackupExport", "BtnRestoreAll", "BtnRestoreStep", "BtnClearBackup") {
            (El $name).IsEnabled | Should -BeFalse
        }
    }

    It "clears stale rows and disables backup actions when loading fails" {
        (El "BackupGrid").ItemsSource = @([pscustomobject]@{ Step = "stale" })
        Mock Get-BackupData { throw "corrupt backup" }

        Load-Backup

        (El "BackupGrid").ItemsSource | Should -BeNullOrEmpty
        (El "BackupSummary").Text | Should -Match "Error loading backup.json"
        foreach ($name in "BtnBackupExport", "BtnRestoreAll", "BtnRestoreStep", "BtnClearBackup") {
            (El $name).IsEnabled | Should -BeFalse
        }
    }

    It "enables selected-step recovery only after a row is selected" {
        $grid = El "BackupGrid"
        (El "BtnRestoreStep").IsEnabled = $false
        $grid.SelectedItem = [pscustomobject]@{ Step = "Phase 1" }

        & $grid.SelectionChangedHandler

        (El "BtnRestoreStep").IsEnabled | Should -BeTrue
    }
}

Describe "Benchmark panel loading" {

    It "clears the chart when benchmark history is empty" {
        $script:GuiElements = @{}
        [void](El "BenchChart").Children.Add("stale point")
        Mock Get-BenchmarkHistory { @() }

        Load-Benchmark

        (El "BenchChart").Children.Count | Should -Be 0
    }
}

Describe "Analyze async lifecycle" {

    BeforeEach {
        $script:GuiElements = @{}
        $Script:UISync = @{}
        $Script:AnalysisInFlight = $false
    }

    It "does not start a second analysis while the first owns the shared result slots" {
        Mock Invoke-Async {}

        Start-Analysis
        Start-Analysis

        Should -Invoke Invoke-Async -Times 1 -Exactly
        $Script:AnalysisInFlight | Should -BeTrue
    }

    It "releases the analysis guard and restores its button after an async failure" {
        Mock Invoke-Async {
            param($Work, $WorkArgs, $OnDone, $OnError, $OnFinally)
            & $OnError "runspace failed"
            & $OnFinally
        }

        Start-Analysis

        $Script:AnalysisInFlight | Should -BeFalse
        (El "BtnRunAnalysis").IsEnabled | Should -BeTrue
        (El "BtnRunAnalysis").Content | Should -Be "Run full scan"
        (El "BtnCancelAnalysis").IsEnabled | Should -BeFalse
        (El "AnalyzeScanTime").Text | Should -Match "runspace failed"
    }

    It "cancels the owned analysis operation and prevents duplicate cancel requests" {
        $Script:AnalysisOperation = @{ Cancelled = $false }
        (El "BtnCancelAnalysis").IsEnabled = $true
        Mock Stop-AsyncOperation {}

        & (El "BtnCancelAnalysis").ClickHandler

        Should -Invoke Stop-AsyncOperation -Times 1 -Exactly
        (El "BtnCancelAnalysis").IsEnabled | Should -BeFalse
        (El "AnalyzeScanTime").Text | Should -Be "Cancelling scan…"
    }
}

Describe "Startup drift helpers" {

    BeforeEach {
        $script:GuiElements = @{}
        $Script:StartupDriftChecked = $false
        Reset-TestState
    }

    It "returns the canonical default state" {
        $state = New-DefaultState

        $state.profile | Should -Be "RECOMMENDED"
        $state.mode | Should -Be "AUTO"
    }

    It "skips the startup drift probe when startup_last_verified is recent" {
        $state = [PSCustomObject]@{
            startup_last_verified = (Get-Date).AddMinutes(-10).ToString("o")
        }

        Should-SkipStartupDriftCheck -State $state | Should -Be $true
    }

    It "returns unknown instead of clean when the startup drift check is throttled" {
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = "RECOMMENDED"
            mode = "AUTO"
            startup_last_verified = (Get-Date).AddMinutes(-10).ToString("o")
        }) -Path $CFG_StateFile
        Mock Test-RegistryCheck { throw "Throttled startup drift must not read registry state." }

        $result = Test-StartupConfigDrift

        $result.Skipped | Should -Be $true
        $result.Status | Should -Be "Unknown"
        $result.HasDrift | Should -BeNullOrEmpty
        $result.CheckedCount | Should -Be 0
        Should -Invoke Test-RegistryCheck -Times 0
    }

    It "records startup_last_verified and reports drift counts from the quick startup check" {
        Mock Test-RegistryCheck {
            if ($Name -eq "OverlayTestMode") {
                return @{ Status = "CHANGED"; Value = 0 }
            }
            return @{ Status = "OK"; Value = $Expected }
        }
        Mock Save-JsonAtomic { $script:SavedState = $Data }
        Mock Set-SecureAcl {}

        $result = Test-StartupConfigDrift

        $result.Skipped | Should -Be $false
        $result.Status | Should -Be "Drift"
        $result.HasDrift | Should -Be $true
        $result.DriftCount | Should -Be 1
        Should -Invoke Save-JsonAtomic -Exactly 1
        $script:SavedState.PSObject.Properties.Name | Should -Contain "startup_last_verified"
        $script:SavedState.PSObject.Properties.Name | Should -Not -Contain "last_verified"
    }

    It "can force a fresh drift check even when startup_last_verified is recent" {
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = "RECOMMENDED"
            mode = "AUTO"
            startup_last_verified = (Get-Date).AddMinutes(-10).ToString("o")
        }) -Path $CFG_StateFile
        Mock Test-RegistryCheck {
            if ($Name -eq "OverlayTestMode") {
                return @{ Status = "CHANGED"; Value = 0 }
            }
            return @{ Status = "OK"; Value = $Expected }
        }
        Mock Save-StateDataSafe {}

        $result = Test-StartupConfigDrift -Force

        $result.Skipped | Should -Be $false
        $result.Status | Should -Be "Drift"
        $result.HasDrift | Should -Be $true
        $result.DriftCount | Should -Be 1
        Should -Invoke Test-RegistryCheck -Times 5
    }

    It "uses AUTO as the fallback mode for the recommended profile" {
        Mock Test-RegistryCheck { @{ Status = "OK"; Value = $Expected } }
        Mock Save-JsonAtomic { $script:SavedState = $Data }
        Mock Set-SecureAcl {}

        Test-StartupConfigDrift | Out-Null

        $script:SavedState.profile | Should -Be "RECOMMENDED"
        $script:SavedState.mode | Should -Be "AUTO"
    }

    It "returns drift results even when saving startup_last_verified fails" {
        Mock Test-RegistryCheck { @{ Status = "OK"; Value = $Expected } }
        Mock Save-StateDataSafe { throw "disk full" }

        { $script:SaveFailureResult = Test-StartupConfigDrift } | Should -Not -Throw

        $script:SaveFailureResult.Skipped | Should -Be $false
        $script:SaveFailureResult.Status | Should -Be "Clean"
        $script:SaveFailureResult.CheckedCount | Should -Be 5
    }

    It "shows the drift banner when startup drift is detected" {
        Mock Test-StartupConfigDrift {
            [PSCustomObject]@{
                Skipped = $false
                Status = "Drift"
                HasDrift = $true
                DriftCount = 2
                CheckedCount = 5
                DriftLabels = @("MPO disabled", "Game Mode enabled")
                CheckedAt = "2026-04-08 14:00"
            }
        }

        Update-StartupDriftBanner

        (El "DashDriftBanner").Visibility | Should -Be "Visible"
        (El "DashDriftBannerText").Text | Should -Match "2 of 5 quick checks drifted"
    }

    It "shows an unknown banner when the startup check is skipped" {
        Mock Test-StartupConfigDrift {
            [PSCustomObject]@{
                Skipped = $true
                Status = "Unknown"
                HasDrift = $null
                DriftCount = 0
                CheckedCount = 0
                DriftLabels = @()
                CheckedAt = ""
            }
        }

        Update-StartupDriftBanner

        (El "DashDriftBanner").Visibility | Should -Be "Visible"
        (El "DashDriftBannerTitle").Text | Should -Match "Not Checked"
        (El "DashDriftBannerText").Text | Should -Match "unknown"
    }

    It "hides the drift banner when a fresh startup check is clean" {
        Mock Test-StartupConfigDrift {
            [PSCustomObject]@{
                Skipped = $false
                Status = "Clean"
                HasDrift = $false
                DriftCount = 0
                CheckedCount = 5
                DriftLabels = @()
                CheckedAt = "2026-04-08 14:00"
            }
        }

        Update-StartupDriftBanner

        (El "DashDriftBanner").Visibility | Should -Be "Collapsed"
    }
}

Describe "Invoke-GuiSafeModeExit" {

    BeforeEach {
        Mock shutdown { $global:LASTEXITCODE = 0 }
        Mock Clear-SafeBootVerified {
            [PSCustomObject]@{
                Status = "Success"
                Verified = $true
                Applied = $true
                DeleteExitCode = 0
                EnumExitCode = 0
                Message = "Safe Mode disabled and verified."
            }
        }
    }

    It "blocks restart when SafeBoot absence cannot be verified" {
        Mock Clear-SafeBootVerified {
            [PSCustomObject]@{
                Status = "Failed"
                Verified = $false
                Applied = $true
                DeleteExitCode = 0
                EnumExitCode = 5
                Message = "Safe Mode state could not be verified."
            }
        }

        $result = Invoke-GuiSafeModeExit

        $result | Should -Be $false
        Should -Invoke Clear-SafeBootVerified -Exactly 1
        Should -Invoke shutdown -Exactly 0
    }

    It "restarts only after verified SafeBoot removal" {
        $result = Invoke-GuiSafeModeExit

        $result | Should -Be $true
        Should -Invoke Clear-SafeBootVerified -Exactly 1
        Should -Invoke shutdown -Exactly 1
    }

    It "reports failure when Windows rejects the restart request" {
        Mock shutdown {
            $global:LASTEXITCODE = 5
            "Access is denied."
        }

        $result = Invoke-GuiSafeModeExit

        $result | Should -Be $false
        Should -Invoke Clear-SafeBootVerified -Exactly 1
        Should -Invoke shutdown -Exactly 1
    }
}

Describe "Enter-GuiBackupOperation" {

    BeforeEach {
        Mock Test-BackupLock { $false }
        Mock Set-BackupLock {}
        Mock Write-DebugLog {}
    }

    It "returns false when an existing owner holds the lock" {
        Mock Test-BackupLock { $true }

        Enter-GuiBackupOperation | Should -BeFalse

        Should -Invoke Set-BackupLock -Exactly 0
    }

    It "catches a contender winning between the check and atomic acquisition" {
        Mock Set-BackupLock { throw "file already exists" }

        { $script:LockResult = Enter-GuiBackupOperation } | Should -Not -Throw

        $script:LockResult | Should -BeFalse
        Should -Invoke Set-BackupLock -Exactly 1
    }

    It "returns true only after acquiring the lock" {
        Enter-GuiBackupOperation | Should -BeTrue

        Should -Invoke Set-BackupLock -Exactly 1
    }
}

Describe "Save-SettingsToState" {

    BeforeEach {
        Reset-TestState
        $script:GuiElements = @{}
        Mock Set-SecureAcl {}
        Mock Write-DebugLog {}
    }

    It "does not invent the Safe Mode readiness marker when GUI settings create the first state file" {
        (El "RadioRecommended").IsChecked = $true
        (El "ChkDryRun").IsChecked = $false

        Save-SettingsToState

        $saved = Get-Content $CFG_StateFile -Raw | ConvertFrom-Json
        $saved.PSObject.Properties.Name | Should -Not -Contain "phase1SafeModeReady"
    }

    It "preserves an existing Safe Mode readiness marker when updating profile settings" {
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = "RECOMMENDED"
            mode = "AUTO"
            phase1SafeModeReady = $true
        }) -Path $CFG_StateFile

        (El "RadioCompetitive").IsChecked = $true
        (El "ChkDryRun").IsChecked = $false

        Save-SettingsToState

        $saved = Get-Content $CFG_StateFile -Raw | ConvertFrom-Json
        $saved.profile | Should -Be "COMPETITIVE"
        $saved.mode | Should -Be "CONTROL"
        $saved.phase1SafeModeReady | Should -Be $true
    }

    It "persists DRY-RUN as a mode modifier for any selected profile" {
        (El "RadioCustom").IsChecked = $true
        (El "ChkDryRun").IsChecked = $true

        Save-SettingsToState

        $saved = Get-Content $CFG_StateFile -Raw | ConvertFrom-Json
        $saved.profile | Should -Be "CUSTOM"
        $saved.mode | Should -Be "DRY-RUN"
    }
}

Describe "published Phase runtime routing" {

    It "routes only the Phase 2 and Phase 3 GUI buttons through the published runtime helper" {
        $source = Get-Content -LiteralPath (Join-Path $Script:Root "helpers/gui-panels.ps1") -Raw

        $source | Should -Match 'BtnOptPhase2"\s*\)\.Add_Click\(\{\s*Start-PublishedPhaseRuntime "SafeMode-DriverClean\.ps1"'
        $source | Should -Match 'BtnOptPhase3"\s*\)\.Add_Click\(\{\s*Start-PublishedPhaseRuntime "PostReboot-Setup\.ps1"'
        $source | Should -Match '\$runtimeRoot\s*=\s*Get-PhaseRuntimeRoot -DestinationRoot \$CFG_WorkDir'
        $source | Should -Match 'Test-PhaseRuntimePayload -RuntimeRoot \$runtimeRoot'
        $source | Should -Not -Match 'BtnOptPhase[23]"\s*\)\.Add_Click\(\{\s*Launch-Terminal'
    }

    It "does not launch an existing runtime whose manifest validation fails" {
        Mock Get-PhaseRuntimeRoot { Join-Path $CFG_WorkDir "runtime-generations/0123456789abcdef0123456789abcdef" }
        Mock Test-Path { $true }
        Mock Test-PhaseRuntimePayload {
            [PSCustomObject]@{ Valid = $false; Message = "runtime hash mismatch" }
        }
        Mock Start-Process {}

        Start-PublishedPhaseRuntime "PostReboot-Setup.ps1" | Should -BeFalse

        Should -Invoke Test-PhaseRuntimePayload -Exactly 1
        Should -Invoke Start-Process -Exactly 0
    }

    It "launches only after the published runtime passes integrity validation" {
        Mock Get-PhaseRuntimeRoot { Join-Path $CFG_WorkDir "runtime-generations/0123456789abcdef0123456789abcdef" }
        Mock Test-Path { $true }
        Mock Test-PhaseRuntimePayload {
            [PSCustomObject]@{ Valid = $true; Message = "verified" }
        }
        Mock Get-TrustedWindowsToolPath { "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" }
        Mock Start-Process {}

        Start-PublishedPhaseRuntime "SafeMode-DriverClean.ps1" | Should -BeTrue

        Should -Invoke Test-PhaseRuntimePayload -Exactly 1
        Should -Invoke Start-Process -Exactly 1
    }
}
