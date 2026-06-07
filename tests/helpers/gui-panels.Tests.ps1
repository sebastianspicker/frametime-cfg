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

    . "$Script:Root/helpers/step-catalog.ps1"

    function New-FakeGuiElement {
        param()

        $element = [PSCustomObject]@{
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
        }
        $element | Add-Member -MemberType ScriptMethod -Name Add_Click -Value { param($Handler) }
        $element | Add-Member -MemberType ScriptMethod -Name Add_SelectionChanged -Value { param($Handler) }
        $element | Add-Member -MemberType ScriptMethod -Name Add_Checked -Value { param($Handler) }
        $element | Add-Member -MemberType ScriptMethod -Name Add_Unchecked -Value { param($Handler) }
        $element | Add-Member -MemberType ScriptMethod -Name UpdateLayout -Value { }
        return $element
    }

    $script:GuiElements = @{}
    function El {
        param([string]$Name)
        if (-not $script:GuiElements.ContainsKey($Name)) {
            $script:GuiElements[$Name] = New-FakeGuiElement
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
    function Write-DebugLog {}

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
        New-TestProgressFile -Phase 1 -LastStep 0 -CompletedSteps @() -SkippedSteps @() | Out-Null
    }

    It "shows observed state without completing runtime progress" {
        Mock Invoke-Async {
            param($Work, $WorkArgs, $OnDone)
            $Script:UISync.VerifyResults = @("P1:4")
            & $OnDone
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
    }
}

Describe "Benchmark parsing helpers" {

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
