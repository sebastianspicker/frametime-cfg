# ==============================================================================
#  tests/Optimize-SystemBase.Tests.ps1  --  Phase-1 system step contracts
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Optimize-SystemBase Step 4" {

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
        Mock Get-CS2InstallPath { "C:\Games\Counter-Strike 2" }
        Mock Test-Path { $true }

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

            if ($Title -match "Fullscreen Optimizations") {
                try {
                    & $Action
                } catch {
                    $script:ActionError = $_
                }
            }
        }

        $startStep = 4
        $PHASE = 1
        $gpuInput = "0"
        $state = $null
    }

    It "completes Step 4 only after the path-shaped registry write is applied" {
        . "$PSScriptRoot/../Optimize-SystemBase.ps1"

        $script:ActionError | Should -BeNullOrEmpty
        $script:RegistryWrites | Should -HaveCount 1
        $script:RegistryWrites[0].PassThru | Should -BeTrue
        $script:RegistryWrites[0].Path | Should -Be "HKCU:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers"
        $script:RegistryWrites[0].Name | Should -Be "C:\Games\Counter-Strike 2\game\bin\win64\cs2.exe"
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 4 -and $stepName -eq "FSO"
        }
    }

    It "does not complete Step 4 when the registry write fails" {
        Mock Set-RegistryValue {
            param($Path, $Name, $Value, $Type, $Why, [switch]$PassThru)
            $script:RegistryWrites += [PSCustomObject]@{
                Path = $Path; Name = $Name; Value = $Value; Type = $Type; Why = $Why; PassThru = [bool]$PassThru
            }
            [PSCustomObject]@{ Status = "Failed"; Applied = $false; Message = "registry denied" }
        }

        . "$PSScriptRoot/../Optimize-SystemBase.ps1"

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "Fullscreen Optimizations registry write did not complete"
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter { $stepNum -eq 4 }
    }

    It "records Step 4 as skipped when CS2 is not installed" {
        Mock Get-CS2InstallPath { $null }

        . "$PSScriptRoot/../Optimize-SystemBase.ps1"

        $script:ActionError | Should -BeNullOrEmpty
        $script:RegistryWrites | Should -BeNullOrEmpty
        Should -Invoke Skip-Step -Exactly 1 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 4 -and $stepName -eq "FSO"
        }
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter { $stepNum -eq 4 }
    }

    It "allows Step 4 completion bookkeeping for a dry-run preview" {
        $SCRIPT:DryRun = $true
        Mock Set-RegistryValue {
            param($Path, $Name, $Value, $Type, $Why, [switch]$PassThru)
            [PSCustomObject]@{ Status = "DryRun"; Applied = $false; Message = "previewed" }
        }

        . "$PSScriptRoot/../Optimize-SystemBase.ps1"

        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter { $stepNum -eq 4 }
    }
}
