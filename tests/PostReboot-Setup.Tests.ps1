# ==============================================================================
#  tests/PostReboot-Setup.Tests.ps1  --  direct shipped-entrypoint contract tests
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path
    $script:TargetScript = Join-Path $script:ProjectRoot "PostReboot-Setup.ps1"
    if (-not (Get-Command Apply-NvidiaCS2Profile -ErrorAction SilentlyContinue)) {
        function global:Apply-NvidiaCS2Profile {}
    }
    function global:shutdown { param([Parameter(ValueFromRemainingArguments)]$CmdArgs) }
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "PostReboot-Setup.ps1 shipped smoke contract" {

    It "supports -SmokeTest as a clean short-circuit" -Skip:(-not $IsWindows) {
        $records = & powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File $script:TargetScript -SmokeTest 2>&1
        $exitCode = $LASTEXITCODE
        $output = ($records | ForEach-Object { $_.ToString() }) -join "`n"
        $errorRecords = @($records | Where-Object { $_ -is [System.Management.Automation.ErrorRecord] })

        $exitCode | Should -Be 0
        $errorRecords | Should -BeNullOrEmpty
        $output | Should -Match 'SMOKE TEST OK'
    }
}

Describe "PostReboot-Setup.ps1 entrypoint wrapper" {

    BeforeAll {
        . $script:TargetScript
    }

    It "bypasses administrator validation before orchestration for smoke tests" {
        Mock Assert-PostRebootSetupAdministrator { throw "Administrator check should not run" }
        Mock Invoke-PostRebootSetup { throw "Orchestration should not run" }
        Mock Write-Host {}

        { Invoke-PostRebootSetupEntryPoint -SmokeTest } | Should -Not -Throw
        Should -Invoke Assert-PostRebootSetupAdministrator -Exactly 0
        Should -Invoke Invoke-PostRebootSetup -Exactly 0
    }

    It "blocks orchestration when administrator validation fails" {
        Mock Assert-PostRebootSetupAdministrator { throw "Not elevated" }
        Mock Invoke-PostRebootSetup {}

        { Invoke-PostRebootSetupEntryPoint } | Should -Throw "Not elevated"
        Should -Invoke Assert-PostRebootSetupAdministrator -Exactly 1
        Should -Invoke Invoke-PostRebootSetup -Exactly 0
    }

    It "runs administrator validation before orchestration" {
        $script:CallOrder = [System.Collections.Generic.List[string]]::new()
        Mock Assert-PostRebootSetupAdministrator { $script:CallOrder.Add("assert") }
        Mock Invoke-PostRebootSetup { $script:CallOrder.Add("orchestrate") }

        Invoke-PostRebootSetupEntryPoint

        $script:CallOrder | Should -Be @("assert", "orchestrate")
    }
}

Describe "PostReboot-Setup.ps1 Safe Mode recovery" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $env:SAFEBOOT_OPTION = "MINIMAL"
        Mock Load-State {
            [PSCustomObject]@{
                gpuInput = "2"
                mode = "CONTROL"
                logLevel = "NORMAL"
                profile = "RECOMMENDED"
                fpsCap = 0
                avgFps = 0
            }
        }
        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Read-Host { "" }
        Mock Test-YoloProfile { $false }
        Mock Start-Sleep {}
        Mock shutdown {}
        Mock Complete-Step {}
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
        Mock Set-RunOnce {
            [PSCustomObject]@{
                Status = "Success"
                Applied = $true
                Message = "RunOnce set"
            }
        }
    }

    AfterEach {
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue
    }

    It "keeps the session running when SafeBoot verification fails" {
        Mock Clear-SafeBootVerified {
            [PSCustomObject]@{
                Status = "Failed"
                Verified = $false
                Applied = $true
                DeleteExitCode = 0
                EnumExitCode = 5
                Message = "enum failed"
            }
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        Should -Invoke Set-RunOnce -Exactly 0
        Should -Invoke shutdown -Exactly 0
        Should -Invoke Complete-Step -Exactly 0
        Should -Invoke Read-Host -Exactly 1
    }

    It "keeps the session running when Phase 3 RunOnce registration fails" {
        Mock Set-RunOnce {
            [PSCustomObject]@{
                Status = "Failed"
                Applied = $false
                Message = "registry write failed"
            }
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        Should -Invoke Set-RunOnce -Exactly 1 -ParameterFilter { $PassThru }
        Should -Invoke shutdown -Exactly 0
        Should -Invoke Start-Sleep -Exactly 0
        Should -Invoke Complete-Step -Exactly 0
        Should -Invoke Read-Host -Exactly 1
    }

    It "restarts only after verified SafeBoot removal and an applied Phase 3 handoff" {
        . $script:TargetScript
        Invoke-PostRebootSetup

        Should -Invoke Clear-SafeBootVerified -Exactly 1
        Should -Invoke Set-RunOnce -Exactly 1 -ParameterFilter { $PassThru }
        Should -Invoke Start-Sleep -Exactly 1
        Should -Invoke shutdown -Exactly 1
        Should -Invoke Complete-Step -Exactly 0
        Should -Invoke Read-Host -Exactly 0
    }

    It "does not remove another process lock when backup initialization fails" {
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue
        Mock Initialize-Backup { throw "backup lock is already held" }
        Mock Remove-BackupLock {}

        . $script:TargetScript
        Invoke-PostRebootSetup

        Should -Invoke Remove-BackupLock -Exactly 0
        Should -Invoke shutdown -Exactly 0
        Should -Invoke Complete-Step -Exactly 0
    }
}

Describe "PostReboot-Setup.ps1 NVIDIA profile Step 4" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $SCRIPT:Profile = "YOLO"
        $SCRIPT:LogLevel = "NORMAL"
        $script:ActionError = $null
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue

        Mock Load-State {
            [PSCustomObject]@{
                gpuInput = "2"
                mode = "CONTROL"
                logLevel = "NORMAL"
                profile = "YOLO"
                fpsCap = 0
                avgFps = 0
                rollbackDriver = $null
                nvidiaDriverPath = $null
                baselineAvg = $null
                baselineP1 = $null
            }
        }
        Mock Save-SuiteState {}
        Mock Initialize-Backup {}
        Mock Initialize-PhaseCounters {}
        Mock Ensure-Dir {}
        Mock Initialize-Log {}
        Mock Write-Banner {}
        Mock Write-Section {}
        Mock Write-TierBadge {}
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-Info {}
        Mock Write-Warn {}
        Mock Write-Err {}
        Mock Write-DebugLog {}
        Mock Read-Host { "n" }
        Mock Remove-BackupLock {}
        Mock Complete-Step {}
        Mock Skip-Step {}
        Mock Show-ResumePrompt { 4 }
        Mock Invoke-TieredStep {
            param(
                [int]$Tier,
                [string]$Title,
                [string]$Why,
                [string]$Evidence,
                [string]$Caveat,
                [string]$Risk,
                [string]$Depth,
                [string]$Improvement,
                [string]$SideEffects,
                [string]$Undo,
                [scriptblock]$Action,
                [scriptblock]$SkipAction
            )

            if ($Title -match "NVIDIA CS2 profile") {
                try {
                    & $Action
                } catch {
                    $script:ActionError = $_
                }
                throw "StopAfterNvidiaProfileTest"
            }
        }
    }

    It "does not complete Step 4 when the NVIDIA profile result is partial" {
        Mock Apply-NvidiaCS2Profile {
            [PSCustomObject]@{
                Status = "Partial"
                CanCompleteStep = $false
                Message = "Only 51 of 52 DRS settings applied."
            }
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "NVIDIA CS2 profile did not complete"
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 4 -and $stepName -eq "NVProfile"
        }
    }

    It "completes Step 4 when the NVIDIA profile result can complete" {
        Mock Apply-NvidiaCS2Profile {
            [PSCustomObject]@{
                Status = "Success"
                CanCompleteStep = $true
                Message = "NVIDIA DRS profile and required registry locks applied."
            }
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 4 -and $stepName -eq "NVProfile"
        }
    }
}

Describe "PostReboot-Setup.ps1 DNS Step 9" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $SCRIPT:Profile = "YOLO"
        $SCRIPT:LogLevel = "NORMAL"
        $script:ActionError = $null
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue

        Mock Load-State {
            [PSCustomObject]@{
                gpuInput = "2"
                mode = "CONTROL"
                logLevel = "NORMAL"
                profile = "YOLO"
                fpsCap = 0
                avgFps = 0
                rollbackDriver = $null
                nvidiaDriverPath = $null
                baselineAvg = $null
                baselineP1 = $null
            }
        }
        Mock Save-SuiteState {}
        Mock Initialize-Backup {}
        Mock Initialize-PhaseCounters {}
        Mock Ensure-Dir {}
        Mock Initialize-Log {}
        Mock Write-Banner {}
        Mock Write-Section {}
        Mock Write-TierBadge {}
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-Info {}
        Mock Write-Warn {}
        Mock Write-Err {}
        Mock Write-DebugLog {}
        Mock Read-Host { "n" }
        Mock Remove-BackupLock {}
        Mock Complete-Step {}
        Mock Skip-Step {}
        Mock Show-ResumePrompt { 9 }
        Mock Get-DnsClientServerAddress {
            [PSCustomObject]@{ ServerAddresses = @("8.8.8.8") }
        }
        Mock Set-VerifiedDnsProfileForAdapter {
            [PSCustomObject]@{
                Changed = $true
                AdapterName = $AdapterName
                Provider = $Provider
            }
        }
        Mock Invoke-TieredStep {
            param(
                [int]$Tier,
                [string]$Title,
                [string]$Why,
                [string]$Evidence,
                [string]$Caveat,
                [string]$Risk,
                [string]$Depth,
                [string]$Improvement,
                [string]$SideEffects,
                [string]$Undo,
                [scriptblock]$Action,
                [scriptblock]$SkipAction
            )

            if ($Title -match "DNS server") {
                try {
                    & $Action
                } catch {
                    $script:ActionError = $_
                }
                throw "StopAfterDnsStepTest"
            }
        }
    }

    It "does not complete Step 9 when the only selected adapter fails" {
        Mock Get-NetAdapter {
            [PSCustomObject]@{
                Name = "Ethernet"
                Status = "Up"
                InterfaceDescription = "Intel Ethernet"
                ifIndex = 7
            }
        }
        Mock Set-VerifiedDnsProfileForAdapter { throw "DNS post-check failed" }

        . $script:TargetScript
        Invoke-PostRebootSetup

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "DNS post-check failed"
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 9 -and $stepName -eq "DNS"
        }
    }

    It "does not complete Step 9 when the second selected adapter fails" {
        Mock Get-NetAdapter {
            @(
                [PSCustomObject]@{
                    Name = "Ethernet"
                    Status = "Up"
                    InterfaceDescription = "Intel Ethernet"
                    ifIndex = 7
                }
                [PSCustomObject]@{
                    Name = "Wi-Fi"
                    Status = "Up"
                    InterfaceDescription = "Intel Wi-Fi"
                    ifIndex = 8
                }
            )
        }
        Mock Set-VerifiedDnsProfileForAdapter {
            if ($AdapterName -eq "Wi-Fi") { throw "DNS post-check failed" }
            [PSCustomObject]@{ Changed = $true; AdapterName = $AdapterName; Provider = $Provider }
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "DNS post-check failed"
        Should -Invoke Set-VerifiedDnsProfileForAdapter -Exactly 2
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 9 -and $stepName -eq "DNS"
        }
    }

    It "does not complete Step 9 when no active physical adapter is available" {
        Mock Get-NetAdapter { @() }

        . $script:TargetScript
        Invoke-PostRebootSetup

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "No active network adapter"
        Should -Invoke Set-VerifiedDnsProfileForAdapter -Exactly 0
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 9 -and $stepName -eq "DNS"
        }
    }

    It "completes Step 9 after all selected adapters verify" {
        Mock Get-NetAdapter {
            @(
                [PSCustomObject]@{
                    Name = "Ethernet"
                    Status = "Up"
                    InterfaceDescription = "Intel Ethernet"
                    ifIndex = 7
                }
                [PSCustomObject]@{
                    Name = "Wi-Fi"
                    Status = "Up"
                    InterfaceDescription = "Intel Wi-Fi"
                    ifIndex = 8
                }
            )
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Set-VerifiedDnsProfileForAdapter -Exactly 2
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 9 -and $stepName -eq "DNS"
        }
    }

    It "does not write DNS during dry-run" {
        $SCRIPT:DryRun = $true
        $SCRIPT:Profile = "YOLO"
        Mock Get-NetAdapter {
            [PSCustomObject]@{
                Name = "Ethernet"
                Status = "Up"
                InterfaceDescription = "Intel Ethernet"
                ifIndex = 7
            }
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        Should -Invoke Set-VerifiedDnsProfileForAdapter -Exactly 0
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 9 -and $stepName -eq "DNS"
        }
    }
}
