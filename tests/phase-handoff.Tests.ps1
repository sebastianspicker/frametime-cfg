# ==============================================================================
#  tests/phase-handoff.Tests.ps1  --  reboot handoff safety contracts
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Optimize-GameConfig Step 38 Safe Mode handoff" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $SCRIPT:SafebootReady = $null
        $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path

        Mock Write-Section {}
        Mock Write-TierBadge {}
        Mock Write-Info {}
        Mock Write-Host {}
        Mock Write-Err {}
        Mock Write-Warn {}
        Mock Write-DebugLog {}
        Mock Write-Blank {}
        Mock Copy-PhaseRuntimePayload {}
        Mock Set-Phase1SafeModeReadyFlag {}
        Mock Complete-Step {}
        Mock Test-BootConfigSet { $false }
        Mock bcdedit {
            $global:LASTEXITCODE = 0
            "ok"
        }

        $startStep = 38
        $PHASE = 1
        $ScriptRoot = $script:ProjectRoot
    }

    It "does not set Safe Mode, readiness, or progress when Phase 2 RunOnce registration fails" {
        Mock Set-RunOnce {
            [PSCustomObject]@{
                Status = "Failed"
                Applied = $false
                Message = "RunOnce failed"
            }
        }

        . "$PSScriptRoot/../Optimize-GameConfig.ps1"

        Should -Invoke Copy-PhaseRuntimePayload -Exactly 1
        Should -Invoke Set-RunOnce -Exactly 1 -ParameterFilter { $SafeMode -and $PassThru }
        Should -Invoke bcdedit -Exactly 0
        Should -Invoke Set-Phase1SafeModeReadyFlag -Exactly 0
        Should -Invoke Complete-Step -Exactly 0
        $SCRIPT:SafebootReady | Should -Be $false
    }

    It "sets readiness and completes the step only after RunOnce and Safe Mode boot flag succeed" {
        Mock Set-RunOnce {
            [PSCustomObject]@{
                Status = "Success"
                Applied = $true
                Message = "RunOnce set"
            }
        }

        . "$PSScriptRoot/../Optimize-GameConfig.ps1"

        Should -Invoke Set-RunOnce -Exactly 1 -ParameterFilter { $SafeMode -and $PassThru }
        Should -Invoke bcdedit -Exactly 1
        Should -Invoke Set-Phase1SafeModeReadyFlag -Exactly 1 -ParameterFilter { $Path -eq $CFG_StateFile }
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 38 -and $stepName -eq "SafeMode"
        }
        $SCRIPT:SafebootReady | Should -Be $true
    }
}
