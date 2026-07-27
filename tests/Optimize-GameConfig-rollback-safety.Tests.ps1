# ==============================================================================
#  tests/Optimize-GameConfig-rollback-safety.Tests.ps1
#  Step 37 backup durability contracts.
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Optimize-GameConfig Step 37 rollback barrier" {
    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $script:ActionError = $null
        $script:ServiceOrder = [System.Collections.Generic.List[string]]::new()

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
        Mock Get-Service { [PSCustomObject]@{ Name = $Name } }
        Mock Backup-ServiceState {
            $script:ServiceOrder.Add("backup:$ServiceName") | Out-Null
            [PSCustomObject]@{
                Captured = $true
                Entry = [PSCustomObject]@{ name = $ServiceName }
                Message = "captured"
            }
        }
        Mock Flush-BackupBuffer { $script:ServiceOrder.Add("flush") | Out-Null }
        Mock Get-BackupDataRaw {
            [PSCustomObject]@{
                entries = @((@("SysMain", "WSearch", "qWave") + $CFG_XboxServices) | ForEach-Object {
                    [PSCustomObject]@{
                        type = "service"
                        step = "Disable SysMain + Search + QWAVE + Xbox"
                        name = $_
                    }
                })
            }
        }
        Mock Set-Service { $script:ServiceOrder.Add("set:$Name") | Out-Null }
        Mock Stop-Service { $script:ServiceOrder.Add("stop:$Name") | Out-Null }
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

    It "flushes every required service capture before the first service mutation" {
        . "$PSScriptRoot/../Optimize-GameConfig.ps1"

        $script:ActionError | Should -BeNullOrEmpty
        $order = @($script:ServiceOrder)
        $backupIndices = @(for ($i = 0; $i -lt $order.Count; $i++) {
            if ($order[$i] -like "backup:*") { $i }
        })
        $flushIndices = @(for ($i = 0; $i -lt $order.Count; $i++) {
            if ($order[$i] -eq "flush") { $i }
        })
        $mutationIndices = @(for ($i = 0; $i -lt $order.Count; $i++) {
            if ($order[$i] -like "set:*" -or $order[$i] -like "stop:*") { $i }
        })

        $backupIndices.Count | Should -Be 7
        $flushIndices.Count | Should -BeGreaterThan 0
        $mutationIndices.Count | Should -Be 14
        ($backupIndices | Measure-Object -Maximum).Maximum | Should -BeLessThan ($flushIndices | Measure-Object -Maximum).Maximum
        ($flushIndices | Measure-Object -Maximum).Maximum | Should -BeLessThan ($mutationIndices | Measure-Object -Minimum).Minimum
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter { $stepNum -eq 37 }
    }

    It "does not mutate services when any required capture fails" {
        Mock Backup-ServiceState {
            $captured = $ServiceName -ne "WSearch"
            [PSCustomObject]@{
                Captured = $captured
                Entry = if ($captured) { [PSCustomObject]@{ name = $ServiceName } } else { $null }
                Message = if ($captured) { "captured" } else { "capture failed for WSearch" }
            }
        }

        . "$PSScriptRoot/../Optimize-GameConfig.ps1"

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "WSearch"
        Should -Invoke Set-Service -Exactly 0
        Should -Invoke Stop-Service -Exactly 0
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter { $stepNum -eq 37 }
    }
}
