# ==============================================================================
#  tests/Optimize-GameConfig.Tests.ps1 - Phase 1 game and service contracts
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Optimize-GameConfig Step 37 completion" {
    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $script:ActionError = $null

        Mock Write-Section {}
        Mock Write-Info {}
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Err {}
        Mock Write-Sub {}
        Mock Write-DebugLog {}
        Mock Write-TierBadge {}
        Mock Complete-Step {}
        Mock Skip-Step {}
        Mock Backup-ServiceState {
            [PSCustomObject]@{ Captured = $true; Entry = [PSCustomObject]@{}; Message = "captured" }
        }
        Mock Flush-BackupBuffer {}
        Mock Get-BackupDataRaw {
            [PSCustomObject]@{
                entries = @((@("SysMain", "WSearch", "qWave") + $CFG_XboxServices) | ForEach-Object {
                    [PSCustomObject]@{
                        type = "service"; step = "Disable SysMain + Search + QWAVE + Xbox"; name = $_
                    }
                })
            }
        }
        Mock Get-Service { [PSCustomObject]@{ Name = $Name } }
        Mock Set-Service {}
        Mock Stop-Service {}
        Mock Enable-Phase2SafeModeTransaction {
            [PSCustomObject]@{ Applied = $false; Message = "not part of this test" }
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
            if ($Title -match "Disable SysMain") {
                try { & $Action } catch { $script:ActionError = $_ }
            }
        }

        $startStep = 37
        $PHASE = 1
        $gpuInput = "0"
        $state = $null
        $ScriptRoot = (Resolve-Path "$PSScriptRoot/..").Path
    }

    It "completes only after every present selected service is disabled and stopped" {
        . "$PSScriptRoot/../Optimize-GameConfig.ps1"

        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Set-Service -Exactly 7
        Should -Invoke Stop-Service -Exactly 7
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 37 -and $stepName -eq "SysMainSearch"
        }
    }

    It "does not complete when a required service stop fails" {
        Mock Stop-Service {
            if ($Name -eq "WSearch") { throw "service control denied" }
        }

        . "$PSScriptRoot/../Optimize-GameConfig.ps1"

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "Required service changes failed: WSearch"
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 37
        }
    }
}

Describe "Optimize-GameConfig Step 36 security boundary" {
    It "does not add Windows Defender exclusions" {
        $source = Get-Content -LiteralPath "$PSScriptRoot/../Optimize-GameConfig.ps1" -Raw

        $source | Should -Not -Match '\bAdd-MpPreference\b'
        $source | Should -Not -Match '\bBackup-DefenderExclusions\b'
        $source | Should -Match 'Step 36 - Visual Effects \+ Auto HDR'
    }
}
