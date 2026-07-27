# ==============================================================================
#  tests/Optimize-Hardware.Tests.ps1  --  Phase-1 hardware module contracts
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    if (-not (Get-Command Invoke-BenchmarkCapture -ErrorAction SilentlyContinue)) {
        function global:Invoke-BenchmarkCapture { param([string]$Label) }
    }
    if (-not (Get-Command Get-LatestNvidiaDriver -ErrorAction SilentlyContinue)) {
        function global:Get-LatestNvidiaDriver { }
    }
    if (-not (Get-Command Test-NvidiaDriverSignature -ErrorAction SilentlyContinue)) {
        function global:Test-NvidiaDriverSignature {
            param([string]$FilePath)
        }
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

Describe "Optimize-Hardware NVIDIA driver download completion" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $script:ActionError = $null
        $script:SavedDriverState = $null

        Mock Write-Section {}
        Mock Write-Info {}
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Err {}
        Mock Write-Sub {}
        Mock Write-DebugLog {}
        Mock Complete-Step {}
        Mock Skip-Step {}
        Mock Set-ClipboardSafe {}
        Mock Get-CimInstance {
            [PSCustomObject]@{ Name = "NVIDIA GeForce RTX 4080" }
        } -ParameterFilter { $ClassName -eq "Win32_VideoController" }
        Mock Get-LatestNvidiaDriver {
            [PSCustomObject]@{
                Version = "572.42"
                Url = "https://us.download.nvidia.com/Windows/572.42/driver.exe"
                ManualDownload = $false
            }
        }
        Mock Invoke-Download { $true }
        Mock Test-NvidiaDriverSignature { $true }
        Mock Get-Content { '{}' }
        Mock Save-SuiteState {
            param($State)
            $script:SavedDriverState = $State
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

            if ($Title -match "Download NVIDIA driver") {
                try {
                    & $Action
                } catch {
                    $script:ActionError = $_
                }
            }
        }

        $startStep = 19
        $PHASE = 1
        $gpuInput = "1"
        $state = $null
    }

    It "completes Step 19 after download, signature verification, and state persistence" {
        . "$PSScriptRoot/../Optimize-Hardware.ps1"

        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Invoke-Download -Exactly 1
        Should -Invoke Test-NvidiaDriverSignature -Exactly 1
        Should -Invoke Save-SuiteState -Exactly 1
        $script:SavedDriverState.nvidiaGpuName | Should -Be "NVIDIA GeForce RTX 4080"
        $script:SavedDriverState.nvidiaDriverPath | Should -Be "$CFG_WorkDir\nvidia_driver.exe"
        $script:SavedDriverState.nvidiaDriverVersion | Should -Be "572.42"
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 19 -and $stepName -eq "NVDriver"
        }
    }

    It "throws and does not complete Step 19 when the download fails" {
        Mock Invoke-Download { $false }

        . "$PSScriptRoot/../Optimize-Hardware.ps1"

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "driver download failed"
        Should -Invoke Test-NvidiaDriverSignature -Exactly 0
        Should -Invoke Save-SuiteState -Exactly 0
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter { $stepNum -eq 19 }
    }

    It "throws and does not complete Step 19 when signature verification fails" {
        Mock Test-NvidiaDriverSignature { $false }

        . "$PSScriptRoot/../Optimize-Hardware.ps1"

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "signature verification failed"
        Should -Invoke Invoke-Download -Exactly 1
        Should -Invoke Save-SuiteState -Exactly 0
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter { $stepNum -eq 19 }
    }

    It "throws and does not complete Step 19 when state persistence fails" {
        Mock Save-SuiteState { throw "state write denied" }

        . "$PSScriptRoot/../Optimize-Hardware.ps1"

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "driver state persistence failed"
        Should -Invoke Invoke-Download -Exactly 1
        Should -Invoke Test-NvidiaDriverSignature -Exactly 1
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter { $stepNum -eq 19 }
    }

    It "records automatic-download fallback as a manual deferral" {
        Mock Get-LatestNvidiaDriver {
            [PSCustomObject]@{ ManualDownload = $true }
        }

        . "$PSScriptRoot/../Optimize-Hardware.ps1"

        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Invoke-Download -Exactly 0
        Should -Invoke Skip-Step -Exactly 1 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 19 -and $stepName -eq "NVDriver-manual"
        }
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter { $stepNum -eq 19 }
    }

    It "does not let legacy rollback metadata bypass validated automatic retrieval" {
        $state = [PSCustomObject]@{ rollbackDriver = "legacy-fixed-version" }

        . "$PSScriptRoot/../Optimize-Hardware.ps1"

        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Get-LatestNvidiaDriver -Exactly 1
        Should -Invoke Invoke-Download -Exactly 1
        Should -Invoke Test-NvidiaDriverSignature -Exactly 1
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 19 -and $stepName -eq "NVDriver"
        }
    }

    It "allows Step 19 completion bookkeeping for an automatic dry-run preview" {
        $SCRIPT:DryRun = $true

        . "$PSScriptRoot/../Optimize-Hardware.ps1"

        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Get-LatestNvidiaDriver -Exactly 1
        Should -Invoke Invoke-Download -Exactly 0
        Should -Invoke Test-NvidiaDriverSignature -Exactly 0
        Should -Invoke Save-SuiteState -Exactly 0
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 1 -and $stepNum -eq 19 -and $stepName -eq "NVDriver"
        }
    }
}
