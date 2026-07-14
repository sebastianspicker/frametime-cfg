# ==============================================================================
#  tests/Optimize-Hardware.Tests.ps1  --  Phase-1 hardware module contracts
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    if (-not (Get-Command Invoke-BenchmarkCapture -ErrorAction SilentlyContinue)) {
        function global:Invoke-BenchmarkCapture { param([string]$Label) }
    }
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Optimize-Hardware Step 10" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $script:BootWrites = @()
        $script:ActionError = $null

        Mock Write-Section {}
        Mock Write-Info {}
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Sub {}
        Mock Write-DebugLog {}
        Mock Complete-Step {}
        Mock Skip-Step {}

        Mock Set-BootConfig {
            param($Key, $Val, $Why, [switch]$PassThru)
            $script:BootWrites += [PSCustomObject]@{
                Key   = $Key
                Value = $Val
                Why   = $Why
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

            if ($Title -match "Dynamic Tick") {
                try {
                    & $Action
                } catch {
                    $script:ActionError = $_
                }
            }
        }

        $startStep = 10
        $PHASE = 1
        $gpuInput = "0"
        $state = $null
    }

    It "applies disabledynamictick without forcing useplatformtick" {
        . "$PSScriptRoot/../Optimize-Hardware.ps1"

        $script:BootWrites | Should -HaveCount 1
        $script:BootWrites[0].Key | Should -Be "disabledynamictick"
        $script:BootWrites[0].Value | Should -Be "yes"
        $script:BootWrites[0].PassThru | Should -Be $true
        ($script:BootWrites | Where-Object Key -eq "useplatformtick") | Should -BeNullOrEmpty
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 10 -and $stepName -eq "Timer"
        }
    }

    It "does not complete Step 10 when the required boot write fails" {
        Mock Set-BootConfig {
            param($Key, $Val, $Why, [switch]$PassThru)
            $script:BootWrites += [PSCustomObject]@{
                Key   = $Key
                Value = $Val
                Why   = $Why
                PassThru = [bool]$PassThru
            }
            [PSCustomObject]@{
                Status = "Failed"
                Applied = $false
                Message = "bcdedit failed"
            }
        }

        . "$PSScriptRoot/../Optimize-Hardware.ps1"

        $script:BootWrites | Should -HaveCount 1
        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "Required boot config write failed"
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 10 -and $stepName -eq "Timer"
        }
    }
}

Describe "Optimize-Hardware baseline benchmark completion" {

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
        Mock Write-Sub {}
        Mock Write-DebugLog {}
        Mock Complete-Step {}
        Mock Skip-Step {}
        Mock Read-Host { "y" }
        Mock Set-ClipboardSafe {}
        Mock Invoke-BenchmarkCapture { $null }

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

            if ($Title -match "Baseline benchmark") {
                try {
                    & $Action
                } catch {
                    $script:ActionError = $_
                }
            }
        }

        $startStep = 17
        $PHASE = 1
        $gpuInput = "0"
        $state = $null
    }

    It "does not complete the baseline when capture returns no usable result" {
        . "$PSScriptRoot/../Optimize-Hardware.ps1"

        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Invoke-BenchmarkCapture -Exactly 1 -ParameterFilter { $Label -eq "Baseline (before optimizations)" }
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 17 -and $stepName -eq "CapFrameX-Baseline"
        }
    }

    It "does not complete the baseline when state persistence fails" {
        Mock Invoke-BenchmarkCapture {
            [PSCustomObject]@{ Avg = 400.0; P1 = 250.0; Cap = 364 }
        }
        Mock Get-Content { '{"gpuInput":"0"}' }
        Mock Save-SuiteState { throw "state write failed" }

        . "$PSScriptRoot/../Optimize-Hardware.ps1"

        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Save-SuiteState -Exactly 1
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 17 -and $stepName -eq "CapFrameX-Baseline"
        }
    }
}
