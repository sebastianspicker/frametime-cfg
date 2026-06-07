# ==============================================================================
#  tests/Optimize-RegistryTweaks.Tests.ps1  --  registry step completion gates
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Optimize-RegistryTweaks Step 27" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $script:RegistryWrites = @()
        $script:ActionError = $null

        Mock Write-Section {}
        Mock Write-Info {}
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Sub {}
        Mock Write-ActionOK {}
        Mock Write-DebugLog {}
        Mock Complete-Step {}
        Mock Skip-Step {}
        Mock Get-IntelHybridCpuName { $null }

        Mock Set-RegistryValue {
            param($Path, $Name, $Value, $Type, $Why, [switch]$PassThru)
            $script:RegistryWrites += [PSCustomObject]@{
                Path = $Path
                Name = $Name
                Value = $Value
                Type = $Type
                Why = $Why
                PassThru = [bool]$PassThru
            }
            [PSCustomObject]@{
                Status = "Success"
                Applied = $true
                Message = "ok"
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

            if ($Title -match "Multimedia SystemProfile") {
                try {
                    & $Action
                } catch {
                    $script:ActionError = $_
                }
            }
        }

        $startStep = 27
        $PHASE = 1
    }

    It "completes Step 27 only after required registry writes report applied status" {
        . "$PSScriptRoot/../Optimize-RegistryTweaks.ps1"

        $script:RegistryWrites.Count | Should -BeGreaterThan 0
        @($script:RegistryWrites | Where-Object { -not $_.PassThru }) | Should -BeNullOrEmpty
        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 27 -and $stepName -eq "SystemResponsiveness"
        }
    }

    It "does not complete Step 27 when a required registry write fails" {
        Mock Set-RegistryValue {
            param($Path, $Name, $Value, $Type, $Why, [switch]$PassThru)
            $script:RegistryWrites += [PSCustomObject]@{
                Path = $Path
                Name = $Name
                Value = $Value
                Type = $Type
                Why = $Why
                PassThru = [bool]$PassThru
            }
            if ($Name -eq "SystemResponsiveness") {
                return [PSCustomObject]@{
                    Status = "Failed"
                    Applied = $false
                    Message = "registry denied"
                }
            }
            [PSCustomObject]@{
                Status = "Success"
                Applied = $true
                Message = "ok"
            }
        }

        . "$PSScriptRoot/../Optimize-RegistryTweaks.ps1"

        $script:RegistryWrites | Should -HaveCount 1
        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "Required registry write failed"
        Should -Invoke Complete-Step -Exactly 0
    }
}
