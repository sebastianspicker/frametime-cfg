# ==============================================================================
#  tests/phase-handoff.Tests.ps1  --  reboot handoff safety contracts
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path
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
        Mock Complete-Step {}

        $startStep = 38
        $PHASE = 1
        $ScriptRoot = $script:ProjectRoot
    }

    It "does not set Safe Mode, readiness, or progress when Phase 2 RunOnce registration fails" {
        Mock Enable-Phase2SafeModeTransaction {
            [PSCustomObject]@{
                Status = "Failed"
                Applied = $false
                Message = "transaction failed"
            }
        }

        . "$PSScriptRoot/../Optimize-GameConfig.ps1"

        Should -Invoke Enable-Phase2SafeModeTransaction -Exactly 1
        Should -Invoke Complete-Step -Exactly 0
        $SCRIPT:SafebootReady | Should -Be $false
    }

    It "sets readiness and completes the step only after RunOnce and Safe Mode boot flag succeed" {
        Mock Enable-Phase2SafeModeTransaction {
            [PSCustomObject]@{
                Status = "Success"
                Applied = $true
                Message = "transaction ready"
            }
        }

        . "$PSScriptRoot/../Optimize-GameConfig.ps1"

        Should -Invoke Enable-Phase2SafeModeTransaction -Exactly 1 -ParameterFilter {
            $SourceRoot -eq $ScriptRoot -and $DestinationRoot -eq $CFG_WorkDir -and $StatePath -eq $CFG_StateFile
        }
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 38 -and $stepName -eq "SafeMode"
        }
        $SCRIPT:SafebootReady | Should -Be $true
    }

    It "does not mark Step 38 complete when the transaction cannot verify Safe Mode" {
        Mock Enable-Phase2SafeModeTransaction {
            [PSCustomObject]@{ Status = "Failed"; Applied = $false; Message = "Safe Mode verification failed" }
        }

        . "$PSScriptRoot/../Optimize-GameConfig.ps1"

        Should -Invoke Enable-Phase2SafeModeTransaction -Exactly 1
        Should -Invoke Complete-Step -Exactly 0
        $SCRIPT:SafebootReady | Should -Be $false
    }
}

Describe "top-level Safe Mode handoff failure contracts" {

    It "uses the transaction helper for both shortcut flows when Safe Mode setup is requested" {
        $bootSafeMode = Get-Content (Join-Path $script:ProjectRoot "Boot-SafeMode.ps1") -Raw
        $cleanup = Get-Content (Join-Path $script:ProjectRoot "Cleanup.ps1") -Raw

        $bootSafeMode | Should -Match 'Enable-Phase2SafeModeTransaction'
        $cleanup | Should -Match 'Enable-Phase2SafeModeTransaction'
    }

    It "uses the transaction helper before completed Phase 1 resume can restart" {
        $setupProfile = Get-Content (Join-Path $script:ProjectRoot "Setup-Profile.ps1") -Raw
        $runOptimize = Get-Content (Join-Path $script:ProjectRoot "Run-Optimize.ps1") -Raw

        $setupProfile | Should -Match '(?s)\$startStep -gt \$TOTAL_STEPS.*Enable-Phase2SafeModeTransaction.*if \(-not \$phase2Transaction\.Applied\)'
        $runOptimize | Should -Match '\$safebootConfirmed = \$SCRIPT:SafebootReady'
    }
}

Describe "manual Phase 3 recovery launcher" {

    It "resolves START [P] through the validated current-generation pointer" {
        $startLauncher = Get-Content (Join-Path $script:ProjectRoot "START.bat") -Raw

        $startLauncher | Should -Match 'Get-PhaseRuntimeRoot -DestinationRoot \$CFG_WorkDir'
        $startLauncher | Should -Match 'Test-PhaseRuntimePayload -RuntimeRoot \$runtimeRoot'
        $startLauncher | Should -Match 'Join-Path \$runtimeRoot ''PostReboot-Setup\.ps1'''
        $startLauncher | Should -Match '& \$phase3Runtime'
        $startLauncher | Should -Not -Match 'C:\\CS2_OPTIMIZE\\runtime\\PostReboot-Setup\.ps1'
        $startLauncher | Should -Not -Match '-File "%~dp0PostReboot-Setup\.ps1"'
    }
}
