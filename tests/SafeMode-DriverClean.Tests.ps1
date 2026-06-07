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
        Mock Write-DebugLog {}
        Mock Write-Blank {}
        Mock Test-YoloProfile { $true }
        Mock bcdedit {
            $global:LASTEXITCODE = 0
            "ok"
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

    It "does not complete Phase 2 or register Phase 3 when driver cleanup cannot complete" {
        $script:DriverCleanResult = [PSCustomObject]@{
            Status = "Failed"
            Applied = $false
            CanCompleteStep = $false
            Message = "No display driver packages were removed."
        }

        . "$script:ProjectRoot/SafeMode-DriverClean.ps1"

        Should -Invoke Remove-GpuDriverClean -Exactly 1 -ParameterFilter { $GpuVendor -eq "NVIDIA" -and $PassThru }
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 2 -and $stepNum -eq 2 -and $stepName -eq "DriverClean"
        }
        Should -Invoke Set-RunOnce -Exactly 0
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 2 -and $stepNum -eq 3 -and $stepName -eq "RunOnce Phase3"
        }
    }

    It "completes Phase 2 and registers Phase 3 only when driver cleanup can complete" {
        . "$script:ProjectRoot/SafeMode-DriverClean.ps1"

        Should -Invoke Remove-GpuDriverClean -Exactly 1 -ParameterFilter { $GpuVendor -eq "NVIDIA" -and $PassThru }
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 2 -and $stepNum -eq 2 -and $stepName -eq "DriverClean"
        }
        Should -Invoke Set-RunOnce -Exactly 1 -ParameterFilter { $PassThru }
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 2 -and $stepNum -eq 3 -and $stepName -eq "RunOnce Phase3"
        }
    }
}
