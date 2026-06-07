# ==============================================================================
#  tests/Verify-Settings.Tests.ps1  --  Settings verifier coverage
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    . "$PSScriptRoot/../Verify-Settings.ps1"

    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path

    function Get-VerificationSourceText {
        param([string]$RelativePath)
        Get-Content (Join-Path $script:ProjectRoot $RelativePath) -Raw
    }

    function Test-VerifySettingsRegistryDefinition {
        param(
            [string]$Source,
            [string]$Path,
            [string]$Name,
            [string]$ExpectedPattern
        )
        $pattern = 'Test-RegistryCheck\s+"' + [regex]::Escape($Path) + '"\s+"' + [regex]::Escape($Name) + '"\s+' + $ExpectedPattern
        return $Source -match $pattern
    }

    function Test-GuiStartupRegistryDefinition {
        param(
            [string]$Source,
            [string]$Path,
            [string]$Name,
            [string]$ExpectedPattern
        )
        $pattern = '@\{\s*Path\s*=\s*"' + [regex]::Escape($Path) + '";\s*Name\s*=\s*"' + [regex]::Escape($Name) + '";\s*Expected\s*=\s*' + $ExpectedPattern
        return $Source -match $pattern
    }

    function Test-GuiInlineRegistryDefinition {
        param(
            [string]$Source,
            [string]$Path,
            [string]$Name,
            [string]$ExpectedPattern
        )
        $pattern = '@\{\s*P\s*=\s*"' + [regex]::Escape($Path) + '";\s*N\s*=\s*"' + [regex]::Escape($Name) + '";\s*E\s*=\s*' + $ExpectedPattern
        return $Source -match $pattern
    }

    function Test-SystemAnalysisRegistryDefinition {
        param(
            [string]$Source,
            [string]$Path,
            [string]$Name,
            [string]$ExpectedPattern
        )
        $readPattern = 'Get-RegVal\s+"' + [regex]::Escape($Path) + '"\s+"' + [regex]::Escape($Name) + '"'
        $match = [regex]::Match($Source, $readPattern, [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
        if (-not $match.Success) { return $false }

        $remainingLength = [math]::Min(900, $Source.Length - $match.Index)
        $nearbySource = $Source.Substring($match.Index, $remainingLength)
        return $nearbySource -match ('-eq\s+' + $ExpectedPattern)
    }

    function Initialize-VerifyBaselineState {
        $script:VerifyLabelStatus = @{}
        $script:VerifyServiceStatus = @{}
        $script:VerifyOutput = [System.Collections.Generic.List[string]]::new()
        $script:BcdOutput = @(
            "Windows Boot Loader",
            "identifier              {current}",
            "0x26000060              Yes"
        )
        $script:HagsValue = 2
        $script:UserPreferencesMask = [byte[]](0x90, 0x12, 0x03, 0x80, 0x10, 0x00, 0x00, 0x00)
        $script:VerifyNicResults = @(
            (New-VerifyCheckResult -Status "OK" -Label "NIC tweak: EEE" -Detail "(Disabled)")
        )
        $script:VerifyPowerPlanResult = New-VerifyCheckResult -Status "OK" -Label "Active power plan = CS2 Optimized" -Detail "(cs2-guid)"
        $script:VerifyQosResults = @(
            (New-VerifyCheckResult -Status "OK" -Label "QoS policy: CS2_UDP_Ports" -Detail "(DSCP 46)"),
            (New-VerifyCheckResult -Status "OK" -Label "QoS policy: CS2_App" -Detail "(DSCP 46)")
        )
        $script:VerifyDnsResults = @(
            (New-VerifyCheckResult -Status "OK" -Label "DNS: Ethernet" -Detail "(Cloudflare = 1.1.1.1, 1.0.0.1)")
        )
        $script:VerifyTrimResult = New-VerifyCheckResult -Status "OK" -Label "Storage maintenance: TRIM" -Detail "(NTFS: enabled)"
        $script:VerifyScheduledTaskResult = New-VerifyCheckResult -Status "INFO" -Label "Scheduled task: CS2 CCD affinity" -Detail "N/A (not a dual-CCD X3D system)"
        $script:VerifyDrsResult = New-VerifyCheckResult -Status "INFO" -Label "NVIDIA DRS profile" -Detail "N/A (no NVIDIA GPU detected)"
        $script:VerifyWindowsUpdateServices = @{
            wuauserv      = [PSCustomObject]@{ StartType = "Automatic"; Status = "Running" }
            UsoSvc        = [PSCustomObject]@{ StartType = "Automatic"; Status = "Running" }
            WaaSMedicSvc  = [PSCustomObject]@{ StartType = "Automatic"; Status = "Running" }
        }

        $script:VerifyServiceStatus["SysMain (Superfetch)"] = "OK"
        $script:VerifyServiceStatus["Windows Search"] = "OK"
        $script:VerifyServiceStatus["qWave (QoS network probes)"] = "OK"
        foreach ($xSvc in $CFG_XboxServices) {
            $label = if ($xSvc -eq "XboxGipSvc") {
                "$xSvc (Xbox wireless accessories - re-enable if using Xbox controller/headset)"
            } else {
                "$xSvc (Xbox background service)"
            }
            $script:VerifyServiceStatus[$label] = "OK"
        }
    }
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Verification table drift" {

    BeforeAll {
        $script:VerifySettingsSource = Get-VerificationSourceText "Verify-Settings.ps1"
        $script:GuiPanelsSource = Get-VerificationSourceText "helpers/gui-panels.ps1"
        $script:SystemAnalysisSource = Get-VerificationSourceText "helpers/system-analysis.ps1"
    }

    It "keeps shared registry checks aligned across active verification surfaces" {
        $checks = @(
            [PSCustomObject]@{ Id="mpo"; Path="HKLM:\SOFTWARE\Microsoft\Windows\Dwm"; Name="OverlayTestMode"; Expected="5"; Surfaces=@("verify","gui-startup","gui-inline","system-analysis") },
            [PSCustomObject]@{ Id="game-mode"; Path="HKCU:\SOFTWARE\Microsoft\GameBar"; Name="AutoGameModeEnabled"; Expected="1"; Surfaces=@("verify","gui-startup","gui-inline","system-analysis") },
            [PSCustomObject]@{ Id="game-dvr-capture"; Path="HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\GameDVR"; Name="AppCaptureEnabled"; Expected="0"; Surfaces=@("verify","gui-startup","gui-inline") },
            [PSCustomObject]@{ Id="game-dvr-master"; Path="HKCU:\System\GameConfigStore"; Name="GameDVR_Enabled"; Expected="0"; Surfaces=@("verify","gui-inline","system-analysis") },
            [PSCustomObject]@{ Id="fse-behavior"; Path="HKCU:\System\GameConfigStore"; Name="GameDVR_FSEBehavior"; Expected="2"; Surfaces=@("verify","gui-inline","system-analysis") },
            [PSCustomObject]@{ Id="fast-startup"; Path="HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power"; Name="HiberbootEnabled"; Expected="0"; Surfaces=@("verify","gui-startup","gui-inline","system-analysis") },
            [PSCustomObject]@{ Id="timer-resolution"; Path="HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\kernel"; Name="GlobalTimerResolutionRequests"; Expected="1"; Surfaces=@("verify","gui-startup","gui-inline","system-analysis") },
            [PSCustomObject]@{ Id="mouse-speed"; Path="HKCU:\Control Panel\Mouse"; Name="MouseSpeed"; Expected='"0"'; Surfaces=@("verify","gui-inline","system-analysis") },
            [PSCustomObject]@{ Id="mouse-threshold-1"; Path="HKCU:\Control Panel\Mouse"; Name="MouseThreshold1"; Expected='"0"'; Surfaces=@("verify","gui-inline","system-analysis") },
            [PSCustomObject]@{ Id="mouse-threshold-2"; Path="HKCU:\Control Panel\Mouse"; Name="MouseThreshold2"; Expected='"0"'; Surfaces=@("verify","gui-inline","system-analysis") },
            [PSCustomObject]@{ Id="mouse-queue"; Path="HKLM:\SYSTEM\CurrentControlSet\Services\mouclass\Parameters"; Name="MouseDataQueueSize"; Expected="50"; Surfaces=@("verify","gui-inline","system-analysis") },
            [PSCustomObject]@{ Id="audio-ducking"; Path="HKCU:\Software\Microsoft\Multimedia\Audio"; Name="UserDuckingPreference"; Expected="3"; Surfaces=@("verify","gui-inline") },
            [PSCustomObject]@{ Id="steam-overlay"; Path="HKCU:\Software\Valve\Steam"; Name="GameOverlayDisabled"; Expected="1"; Surfaces=@("verify","gui-inline","system-analysis") },
            [PSCustomObject]@{ Id="visual-effects"; Path="HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects"; Name="VisualFXSetting"; Expected="2"; Surfaces=@("verify","gui-inline","system-analysis") }
        )

        foreach ($check in $checks) {
            if ("verify" -in $check.Surfaces) {
                Test-VerifySettingsRegistryDefinition -Source $script:VerifySettingsSource -Path $check.Path -Name $check.Name -ExpectedPattern $check.Expected |
                    Should -BeTrue -Because "$($check.Id) must match in Verify-Settings.ps1"
            }
            if ("gui-startup" -in $check.Surfaces) {
                Test-GuiStartupRegistryDefinition -Source $script:GuiPanelsSource -Path $check.Path -Name $check.Name -ExpectedPattern $check.Expected |
                    Should -BeTrue -Because "$($check.Id) must match in GUI startup drift checks"
            }
            if ("gui-inline" -in $check.Surfaces) {
                Test-GuiInlineRegistryDefinition -Source $script:GuiPanelsSource -Path $check.Path -Name $check.Name -ExpectedPattern $check.Expected |
                    Should -BeTrue -Because "$($check.Id) must match in GUI inline verification"
            }
            if ("system-analysis" -in $check.Surfaces) {
                Test-SystemAnalysisRegistryDefinition -Source $script:SystemAnalysisSource -Path $check.Path -Name $check.Name -ExpectedPattern $check.Expected |
                    Should -BeTrue -Because "$($check.Id) must match in system analysis"
            }
        }
    }

    It "does not require the unapplied Platform Tick boot setting in verifier surfaces" {
        ($script:VerifySettingsSource -match "Platform Tick|0x26000092") | Should -BeFalse
        ($script:SystemAnalysisSource -match "Platform Tick|0x26000092") | Should -BeFalse
    }
}

Describe "Invoke-VerifySettings" {

    BeforeEach {
        Reset-TestState
        Initialize-VerifyBaselineState

        $writeHostCommand = [string]::Concat("Write", "-Host")
        Mock Write-LogoBanner {}
        Mock Write-Blank {}
        Mock -CommandName $writeHostCommand {
            param(
                [Parameter(Position = 0, ValueFromRemainingArguments)]
                [object[]]$Object,
                [ConsoleColor]$ForegroundColor,
                [ConsoleColor]$BackgroundColor,
                [switch]$NoNewline,
                [object]$Separator
            )
            if ($null -ne $Object) {
                $script:VerifyOutput.Add(($Object -join " ")) | Out-Null
            }
        }
        Mock Test-RegistryCheck {
            $status = if ($script:VerifyLabelStatus.ContainsKey($Label)) {
                $script:VerifyLabelStatus[$Label]
            } else {
                "OK"
            }

            switch ($status) {
                "OK" {
                    & $writeHostCommand "  OK  $Label"
                    $Script:_verifyOkCount++
                }
                "CHANGED" {
                    & $writeHostCommand "  CHANGED  $Label"
                    $Script:_verifyChangedCount++
                }
                "MISSING" {
                    & $writeHostCommand "  MISSING  $Label"
                    $Script:_verifyMissingCount++
                }
                default {
                    throw "Unexpected registry status '$status' for label '$Label'"
                }
            }
        }
        Mock Test-ServiceCheck {
            $status = if ($script:VerifyServiceStatus.ContainsKey($Label)) {
                $script:VerifyServiceStatus[$Label]
            } else {
                "OK"
            }

            switch ($status) {
                "OK" {
                    & $writeHostCommand "  OK  $Label"
                    $Script:_verifyOkCount++
                }
                "CHANGED" {
                    & $writeHostCommand "  CHANGED  $Label"
                    $Script:_verifyChangedCount++
                }
                "MISSING" {
                    & $writeHostCommand "  MISSING  $Label"
                    $Script:_verifyMissingCount++
                }
                default {
                    throw "Unexpected service status '$status' for label '$Label'"
                }
            }
        }
        Mock Get-IntelHybridCpuName { "Intel Core Ultra Test" }
        Mock Get-ActiveNicGuid { "TESTGUID" }
        Mock Test-VerifyNicAdvancedProperties { $script:VerifyNicResults }
        Mock Test-VerifyPowerPlan { $script:VerifyPowerPlanResult }
        Mock Test-VerifyQosPolicies { $script:VerifyQosResults }
        Mock Test-VerifyDnsConfiguration { $script:VerifyDnsResults }
        Mock Test-VerifyTrimConfiguration { $script:VerifyTrimResult }
        Mock Test-VerifyScheduledTasks { $script:VerifyScheduledTaskResult }
        Mock Test-VerifyNvidiaDrsProfile { $script:VerifyDrsResult }
        Mock Test-VerifyRuntimeCompatibility { [PSCustomObject]@{ Supported = $true; Message = "" } }
        Mock bcdedit {
            $global:LASTEXITCODE = 0
            $script:BcdOutput
        }
        Mock Test-Path { $false } -ParameterFilter { $Path -eq "HKLM:\SYSTEM\CurrentControlSet\Control\Class\$CFG_GUID_Display" }
        Mock Get-ItemProperty {
            if ($Path -eq "HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers" -and $Name -eq "HwSchMode") {
                if ($null -eq $script:HagsValue) {
                    throw "Missing HAGS"
                }
                return [PSCustomObject]@{ HwSchMode = $script:HagsValue }
            }
            if ($Path -eq "HKCU:\Control Panel\Desktop" -and $Name -eq "UserPreferencesMask") {
                if ($null -eq $script:UserPreferencesMask) {
                    throw "Missing UserPreferencesMask"
                }
                return [PSCustomObject]@{ UserPreferencesMask = $script:UserPreferencesMask }
            }
            return [PSCustomObject]@{
                HwSchMode = $null
                UserPreferencesMask = $null
            }
        }
        Mock Get-Service {
            if ($script:VerifyWindowsUpdateServices.ContainsKey($Name)) {
                return $script:VerifyWindowsUpdateServices[$Name]
            }
            return $null
        }
    }

    It "reports a clean summary when all categories are OK or INFO" {
        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.changedCount | Should -Be 0
        $counts.missingCount | Should -Be 0
        $counts.okCount | Should -BeGreaterThan 0
        (@($script:VerifyOutput) -join "`n") | Should -Match "All settings intact . your optimizations are still active!"
    }

    It "marks the Game Mode category as changed when AutoGameModeEnabled is disabled" {
        $script:VerifyLabelStatus["Game Mode (enabled)"] = "CHANGED"

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.changedCount | Should -Be 1
        (@($script:VerifyOutput) -join "`n") | Should -Match "Game Mode \(enabled\)"
    }

    It "marks the GPU section as missing when the HAGS key is absent" {
        $script:HagsValue = $null

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.missingCount | Should -Be 1
        (@($script:VerifyOutput) -join "`n") | Should -Match "HAGS key not found"
    }

    It "marks the timer and boot section as changed when bcdedit flags are absent" {
        $script:BcdOutput = @(
            "Windows Boot Loader",
            "identifier              {current}"
        )

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.changedCount | Should -Be 1
        (@($script:VerifyOutput) -join "`n") | Should -Match "Dynamic Tick is ACTIVE"
        (@($script:VerifyOutput) -join "`n") | Should -Not -Match "Platform Tick"
    }

    It "marks the network section as missing when no active NIC can be resolved" {
        Mock Get-ActiveNicGuid { $null }

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.missingCount | Should -Be 2
        (@($script:VerifyOutput) -join "`n") | Should -Match "Active NIC not found"
    }

    It "aggregates CHANGED and MISSING counters into the summary output" {
        $script:VerifyServiceStatus["SysMain (Superfetch)"] = "CHANGED"
        $script:VerifyServiceStatus["Windows Search"] = "MISSING"

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.changedCount | Should -Be 1
        $counts.missingCount | Should -Be 1
        (@($script:VerifyOutput) -join "`n") | Should -Match "CHANGED:\s+1"
        (@($script:VerifyOutput) -join "`n") | Should -Match "MISSING:\s+1"
    }

    It "accepts DHCP DNS as informational instead of missing" {
        $script:VerifyDnsResults = @(
            New-VerifyCheckResult -Status "INFO" -Label "DNS: Ethernet" -Detail "set to automatic/DHCP"
        )

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.missingCount | Should -Be 0
        $counts.infoCount | Should -BeGreaterThan 0
        (@($script:VerifyOutput) -join "`n") | Should -Match "automatic/DHCP"
    }

    It "marks TRIM as changed when storage maintenance is disabled" {
        $script:VerifyTrimResult = New-VerifyCheckResult -Status "CHANGED" -Label "Storage maintenance: TRIM" -Detail "(disabled on: ReFS)"

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.changedCount | Should -Be 1
        (@($script:VerifyOutput) -join "`n") | Should -Match "Storage maintenance: TRIM"
    }

    It "marks NIC advanced properties as changed when a tweak differs" {
        $script:VerifyNicResults = @(
            New-VerifyCheckResult -Status "CHANGED" -Label "NIC tweak: EEE" -Detail "(is: Enabled, expected: Disabled)"
        )

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.changedCount | Should -Be 1
        (@($script:VerifyOutput) -join "`n") | Should -Match "NIC tweak: EEE"
    }

    It "marks the power plan category as changed when the CS2 plan is not active" {
        $script:VerifyPowerPlanResult = New-VerifyCheckResult -Status "CHANGED" -Label "Active power plan = CS2 Optimized" -Detail "(active: balanced-guid, expected: cs2-guid)"

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.changedCount | Should -Be 1
        (@($script:VerifyOutput) -join "`n") | Should -Match "Active power plan = CS2 Optimized"
    }

    It "marks QoS policies as missing when a required policy is absent" {
        $script:VerifyQosResults = @(
            New-VerifyCheckResult -Status "MISSING" -Label "QoS policy: CS2_UDP_Ports" -Detail "not found"
        )

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.missingCount | Should -Be 1
        (@($script:VerifyOutput) -join "`n") | Should -Match "QoS policy: CS2_UDP_Ports"
    }

    It "marks DNS as changed when the adapter is no longer using the optimized servers" {
        $script:VerifyDnsResults = @(
            New-VerifyCheckResult -Status "CHANGED" -Label "DNS: Ethernet" -Detail "(is: 9.9.9.9, expected Cloudflare or Google)"
        )

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.changedCount | Should -Be 1
        (@($script:VerifyOutput) -join "`n") | Should -Match "DNS: Ethernet"
    }

    It "marks the scheduled task category as missing when the affinity task is absent" {
        $script:VerifyScheduledTaskResult = New-VerifyCheckResult -Status "MISSING" -Label "Scheduled task: CS2_Optimize_CCD_Affinity" -Detail "not found"

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.missingCount | Should -Be 1
        (@($script:VerifyOutput) -join "`n") | Should -Match "CS2_Optimize_CCD_Affinity"
    }

    It "marks the NVIDIA DRS category as changed when profile settings differ" {
        $script:VerifyDrsResult = New-VerifyCheckResult -Status "CHANGED" -Label "NVIDIA DRS profile" -Detail "(3 setting(s) differ)"

        Invoke-VerifySettings

        $counts = Get-VerifyCounters
        $counts.changedCount | Should -Be 1
        (@($script:VerifyOutput) -join "`n") | Should -Match "NVIDIA DRS profile"
    }

    It "returns a clean compatibility message instead of running the full verifier on unsupported runtimes" {
        Mock Test-VerifyRuntimeCompatibility {
            [PSCustomObject]@{
                Supported = $false
                Message = "Verify-Settings is only supported on Windows."
            }
        }

        Invoke-VerifySettings

        (@($script:VerifyOutput) -join "`n") | Should -Match "only supported on Windows"
        Should -Invoke Test-RegistryCheck -Exactly 0
    }
}

Describe "Test-VerifyNicAdvancedProperties" {

    BeforeEach {
        Reset-TestState
    }

    It "treats missing NIC properties as informational" {
        Mock Get-ActiveNicAdapter {
            [PSCustomObject]@{ Name = "Ethernet" }
        }
        Mock Get-NetAdapterAdvancedProperty { $null }

        $results = @(Test-VerifyNicAdvancedProperties)

        ($results | Select-Object -First 1).Status | Should -Be "INFO"
    }
}

Describe "Test-VerifyPowerPlan" {

    BeforeEach {
        Reset-TestState
    }

    It "returns changed when the active CS2 plan has drifted subsettings" {
        Mock powercfg {
            param([Parameter(ValueFromRemainingArguments)]$CmdArgs)
            $joined = @($CmdArgs) -join ' '
            if ($joined -eq '/list') {
                return @"
Power Scheme GUID: 11111111-1111-1111-1111-111111111111  (Balanced)
Power Scheme GUID: 22222222-2222-2222-2222-222222222222  (CS2 Optimized)
"@
            }
            if ($joined -eq '/getactivescheme') {
                return 'Power Scheme GUID: 22222222-2222-2222-2222-222222222222  (CS2 Optimized)'
            }
            if ($joined -match '^/query 22222222-2222-2222-2222-222222222222 ') {
                if ($joined -match [regex]::Escape($PP_USBSS)) {
                    return 'Current AC Power Setting Index: 0x00000000'
                }
                return 'Current AC Power Setting Index: 0x00000001'
            }
            throw "Unexpected powercfg call: $joined"
        }

        $result = Test-VerifyPowerPlan

        $result.Status | Should -Be "CHANGED"
        $result.Detail | Should -Match 'USB selective suspend=0'
    }
}

Describe "Test-VerifyScheduledTasks" {

    BeforeEach {
        Reset-TestState
        Mock Get-X3DCcdInfo {
            @{
                IsX3D = $true
                DualCCD = $true
                CpuName = '7950X3D'
            }
        }
    }

    It "returns changed when the affinity task action payload does not match" {
        Mock Get-ScheduledTask {
            [PSCustomObject]@{
                State = 'Ready'
                Actions = @(
                    [PSCustomObject]@{
                        Execute = '%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe'
                        Arguments = '-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "C:\CS2_OPTIMIZE\wrong.ps1"'
                    }
                )
            }
        }

        $result = Test-VerifyScheduledTasks

        $result.Status | Should -Be "CHANGED"
        $result.Detail | Should -Match 'action mismatch'
    }

    It "returns changed when the affinity task state is unhealthy" {
        Mock Get-ScheduledTask {
            [PSCustomObject]@{
                State = 'Unknown'
                Actions = @(
                    [PSCustomObject]@{
                        Execute = '%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe'
                        Arguments = "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$CS2_AffinityScriptPath`""
                    }
                )
            }
        }

        $result = Test-VerifyScheduledTasks

        $result.Status | Should -Be "CHANGED"
        $result.Detail | Should -Match 'state: Unknown'
    }
}
