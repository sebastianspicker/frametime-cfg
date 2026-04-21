# ==============================================================================
#  tests/Verify-Settings.Tests.ps1  --  Settings verifier coverage
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    . "$PSScriptRoot/../Verify-Settings.ps1"

    function Initialize-VerifyBaselineState {
        $script:VerifyLabelStatus = @{}
        $script:VerifyServiceStatus = @{}
        $script:VerifyOutput = [System.Collections.Generic.List[string]]::new()
        $script:BcdOutput = @(
            "Windows Boot Loader",
            "identifier              {current}",
            "0x26000060              Yes",
            "0x26000092              Yes"
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

Describe "Invoke-VerifySettings" {

    BeforeEach {
        Reset-TestState
        Initialize-VerifyBaselineState

        Mock Write-LogoBanner {}
        Mock Write-Blank {}
        Mock Write-Host {
            if ($null -ne $Object) {
                $script:VerifyOutput.Add("$Object") | Out-Null
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
                    Write-Host "  OK  $Label"
                    $Script:_verifyOkCount++
                }
                "CHANGED" {
                    Write-Host "  CHANGED  $Label"
                    $Script:_verifyChangedCount++
                }
                "MISSING" {
                    Write-Host "  MISSING  $Label"
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
                    Write-Host "  OK  $Label"
                    $Script:_verifyOkCount++
                }
                "CHANGED" {
                    Write-Host "  CHANGED  $Label"
                    $Script:_verifyChangedCount++
                }
                "MISSING" {
                    Write-Host "  MISSING  $Label"
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
        Mock Update-LastVerifiedTimestamp {}
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
        $counts.changedCount | Should -Be 2
        (@($script:VerifyOutput) -join "`n") | Should -Match "Dynamic Tick is ACTIVE"
        (@($script:VerifyOutput) -join "`n") | Should -Match "Platform Tick is INACTIVE"
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
        Should -Invoke Update-LastVerifiedTimestamp -Exactly 0
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
