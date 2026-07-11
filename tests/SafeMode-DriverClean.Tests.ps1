# ==============================================================================
#  tests/SafeMode-DriverClean.Tests.ps1  --  Phase 2 driver-clean handoff safety
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    . "$PSScriptRoot/../helpers/gpu-driver-clean.ps1"

    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path
    $script:OriginalSafebootOption = $env:SAFEBOOT_OPTION
    function global:shutdown { param([Parameter(ValueFromRemainingArguments)]$CmdArgs) }
}

AfterAll {
    if ($null -eq $script:OriginalSafebootOption) {
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue
    } else {
        $env:SAFEBOOT_OPTION = $script:OriginalSafebootOption
    }

    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "SafeMode-DriverClean.ps1 entrypoint wrapper" {

    BeforeAll {
        . "$script:ProjectRoot/SafeMode-DriverClean.ps1"
    }

    It "bypasses administrator validation before orchestration for smoke tests" {
        Mock Assert-SafeModeDriverCleanAdministrator { throw "Administrator check should not run" }
        Mock Invoke-SafeModeDriverClean { throw "Orchestration should not run" }
        Mock Write-Host {}

        { Invoke-SafeModeDriverCleanEntryPoint -SmokeTest } | Should -Not -Throw
        Should -Invoke Assert-SafeModeDriverCleanAdministrator -Exactly 0
        Should -Invoke Invoke-SafeModeDriverClean -Exactly 0
    }

    It "blocks orchestration when administrator validation fails" {
        Mock Assert-SafeModeDriverCleanAdministrator { throw "Not elevated" }
        Mock Invoke-SafeModeDriverClean {}

        { Invoke-SafeModeDriverCleanEntryPoint } | Should -Throw "Not elevated"
        Should -Invoke Assert-SafeModeDriverCleanAdministrator -Exactly 1
        Should -Invoke Invoke-SafeModeDriverClean -Exactly 0
    }

    It "runs administrator validation before orchestration" {
        $script:CallOrder = [System.Collections.Generic.List[string]]::new()
        Mock Assert-SafeModeDriverCleanAdministrator { $script:CallOrder.Add("assert") }
        Mock Invoke-SafeModeDriverClean { $script:CallOrder.Add("orchestrate") }

        Invoke-SafeModeDriverCleanEntryPoint

        $script:CallOrder | Should -Be @("assert", "orchestrate")
    }
}

Describe "SafeMode-DriverClean Phase 2 completion" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $env:SAFEBOOT_OPTION = "MINIMAL"
        $script:DriverCleanResult = [PSCustomObject]@{
            Status = "Success"
            Applied = $true
            CanCompleteStep = $true
            Message = "Driver cleanup removed 1 package(s)."
        }

        Mock Load-State {
            [PSCustomObject]@{
                gpuInput = "2"
                mode = "CONTROL"
                logLevel = "NORMAL"
                profile = "RECOMMENDED"
                fpsCap = 0
                avgFps = 0
                rollbackDriver = $null
                nvidiaDriverPath = $null
                baselineAvg = $null
                baselineP1 = $null
            }
        }
        Mock Save-SuiteState {}
        Mock Initialize-Log {}
        Mock Initialize-Backup {}
        Mock Remove-BackupLock {}
        Mock Write-Banner {}
        Mock Write-Info {}
        Mock Write-Section {}
        Mock Write-Step {}
        Mock Write-Host {}
        Mock Write-Err {}
        Mock Write-Warn {}
        Mock Write-OK {}
        Mock Write-DebugLog {}
        Mock Write-Blank {}
        Mock Test-YoloProfile { $true }
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
        Mock shutdown {}
        Mock Complete-Step {}
        Mock Skip-Step {}
        Mock Test-StepDone { $false }
        Mock Set-RunOnce {
            [PSCustomObject]@{
                Status = "Success"
                Applied = $true
                Message = "RunOnce set"
            }
        }
        Mock Remove-GpuDriverClean { $script:DriverCleanResult }
    }

    It "does nothing unsafe when backup initialization fails" {
        Mock Initialize-Backup { throw "backup init failed" }

        . "$script:ProjectRoot/SafeMode-DriverClean.ps1"
        Invoke-SafeModeDriverClean

        Should -Invoke Clear-SafeBootVerified -Exactly 0
        Should -Invoke Complete-Step -Exactly 0
        Should -Invoke Remove-GpuDriverClean -Exactly 0
        Should -Invoke Set-RunOnce -Exactly 0
        Should -Invoke shutdown -Exactly 0
        Should -Invoke Remove-BackupLock -Exactly 0
    }

    It "fails closed before Step 1 completion when SafeBoot cannot be verified absent" {
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

        . "$script:ProjectRoot/SafeMode-DriverClean.ps1"
        Invoke-SafeModeDriverClean

        Should -Invoke Complete-Step -Exactly 0
        Should -Invoke Remove-GpuDriverClean -Exactly 0
        Should -Invoke Set-RunOnce -Exactly 0
        Should -Invoke shutdown -Exactly 0
    }

    It "does not mutate boot state or continue from a normal-mode dry run" {
        $SCRIPT:DryRun = $true
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue

        . "$script:ProjectRoot/SafeMode-DriverClean.ps1"
        Invoke-SafeModeDriverClean

        Should -Invoke Clear-SafeBootVerified -Exactly 0
        Should -Invoke Complete-Step -Exactly 0
        Should -Invoke Remove-GpuDriverClean -Exactly 0
        Should -Invoke Set-RunOnce -Exactly 0
        Should -Invoke shutdown -Exactly 0
    }

    It "does not complete Phase 2 or register Phase 3 when driver cleanup cannot complete" {
        $script:DriverCleanResult = [PSCustomObject]@{
            Status = "Failed"
            Applied = $false
            CanCompleteStep = $false
            Message = "No display driver packages were removed."
        }

        . "$script:ProjectRoot/SafeMode-DriverClean.ps1"
        Invoke-SafeModeDriverClean

        Should -Invoke Remove-GpuDriverClean -Exactly 1 -ParameterFilter { $GpuVendor -eq "NVIDIA" -and $PassThru }
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 2 -and $stepNum -eq 2 -and $stepName -eq "DriverClean"
        }
        Should -Invoke Set-RunOnce -Exactly 0
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 2 -and $stepNum -eq 3 -and $stepName -eq "RunOnce Phase3"
        }
        Should -Invoke shutdown -Exactly 0
    }

    It "stays running when Phase 3 RunOnce registration is not applied" {
        Mock Set-RunOnce {
            [PSCustomObject]@{
                Status = "Failed"
                Applied = $false
                Message = "registry write failed"
            }
        }

        . "$script:ProjectRoot/SafeMode-DriverClean.ps1"
        Invoke-SafeModeDriverClean

        Should -Invoke Remove-GpuDriverClean -Exactly 1
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 2 -and $stepNum -eq 2 -and $stepName -eq "DriverClean"
        }
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 2 -and $stepNum -eq 3 -and $stepName -eq "RunOnce Phase3"
        }
        Should -Invoke shutdown -Exactly 0
    }

    It "completes Phase 2 and registers Phase 3 only when driver cleanup can complete" {
        . "$script:ProjectRoot/SafeMode-DriverClean.ps1"
        Invoke-SafeModeDriverClean

        Should -Invoke Remove-GpuDriverClean -Exactly 1 -ParameterFilter { $GpuVendor -eq "NVIDIA" -and $PassThru }
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 2 -and $stepNum -eq 2 -and $stepName -eq "DriverClean"
        }
        Should -Invoke Set-RunOnce -Exactly 1 -ParameterFilter { $PassThru }
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 2 -and $stepNum -eq 3 -and $stepName -eq "RunOnce Phase3"
        }
        Should -Invoke shutdown -Exactly 1
    }

    It "registers recovery after an exception only when verified cleanup had begun, without rebooting" {
        Mock Remove-GpuDriverClean { throw "cleanup crashed" }

        . "$script:ProjectRoot/SafeMode-DriverClean.ps1"
        Invoke-SafeModeDriverClean

        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 2 -and $stepNum -eq 1 -and $stepName -eq "SafeMode off"
        }
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter { $stepNum -in @(2, 3) }
        Should -Invoke Set-RunOnce -Exactly 1 -ParameterFilter { $PassThru }
        Should -Invoke shutdown -Exactly 0
    }
}
