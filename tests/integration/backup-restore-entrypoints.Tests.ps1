# ==============================================================================
#  tests/integration/backup-restore-entrypoints.Tests.ps1
#  Integration coverage for Restore-AllChanges and Restore-Interactive.
# ==============================================================================

BeforeAll {
    $tempBase = if ($env:TEMP) { $env:TEMP } else { [System.IO.Path]::GetTempPath() }
    $script:FallbackTestTempRoot = Join-Path $tempBase "cs2opt-tests-integration-fallback"
    try {
        . "$PSScriptRoot/_IntegrationInit.ps1"
    } finally {
        if (-not $SCRIPT:TestTempRoot) {
            $SCRIPT:TestTempRoot = $script:FallbackTestTempRoot
        }
    }

    function Set-RestorePromptResponses {
        param([string[]]$Values)
        $script:RestorePromptResponses = [System.Collections.Generic.Queue[string]]::new()
        foreach ($value in $Values) {
            $script:RestorePromptResponses.Enqueue($value)
        }
    }
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
    if ($script:FallbackTestTempRoot -and
        $script:FallbackTestTempRoot -ne $SCRIPT:TestTempRoot -and
        (Test-Path $script:FallbackTestTempRoot)) {
        Remove-Item $script:FallbackTestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Restore-AllChanges integration" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false

        New-TestBackupFile -Entries @(
            [ordered]@{
                type = "defender"
                exclusionPaths = @("C:\Games\CS2")
                exclusionProcesses = @()
                step = "Step Alpha"
                timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
            },
            [ordered]@{
                type = "defender"
                exclusionPaths = @()
                exclusionProcesses = @("cs2.exe")
                step = "Step Beta"
                timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
            }
        )

        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
        Mock Remove-MpPreference {}
        Mock Test-BackupLock { $false }
        Mock Set-BackupLock {}
        Mock Remove-BackupLock {}
        Mock Read-Host { "y" }
    }

    It "processes all step groups and cleans up backup.json after completion" {
        Restore-AllChanges

        Should -Invoke Remove-MpPreference -Exactly 2
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 0
    }
}

Describe "Restore-Interactive integration" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false

        New-TestBackupFile -Entries @(
            [ordered]@{
                type = "defender"
                exclusionPaths = @("C:\Games\CS2")
                exclusionProcesses = @()
                step = "Step Alpha"
                timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
            },
            [ordered]@{
                type = "defender"
                exclusionPaths = @()
                exclusionProcesses = @("cs2.exe")
                step = "Step Beta"
                timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
            }
        )

        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
        Mock Remove-MpPreference {}
        Mock Test-BackupLock { $false }
        Mock Set-BackupLock {}
        Mock Remove-BackupLock {}
        Mock Read-Host {
            if ($script:RestorePromptResponses.Count -eq 0) {
                throw "No mocked Read-Host response left"
            }
            return $script:RestorePromptResponses.Dequeue()
        }
    }

    It "resumes through all step groups when the operator chooses Restore ALL and resume" {
        Set-RestorePromptResponses @("A", "R", "R")

        Restore-Interactive

        Should -Invoke Remove-MpPreference -Exactly 2
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 0
    }

    It "keeps skipped step groups in backup.json when the operator chooses skip" {
        Set-RestorePromptResponses @("A", "S", "R")

        Restore-Interactive

        Should -Invoke Remove-MpPreference -Exactly 1
        $remaining = @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries)
        $remaining.Count | Should -Be 1
        $remaining[0].step | Should -Be "Step Alpha"
    }

    It "does not claim a full restore when any step group is skipped" {
        Set-RestorePromptResponses @("A", "S", "R")

        Restore-Interactive

        Should -Invoke Write-OK -Exactly 0 -ParameterFilter { $t -match 'All settings restored to pre-optimization state' }
        Should -Invoke Write-Warn -Exactly 1 -ParameterFilter { $t -match 'skipped step group' }
    }

    It "leaves unprocessed step groups in backup.json when the operator aborts mid-run" {
        Set-RestorePromptResponses @("A", "R", "A")

        Restore-Interactive

        Should -Invoke Remove-MpPreference -Exactly 1
        $remaining = @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries)
        $remaining.Count | Should -Be 1
        $remaining[0].step | Should -Be "Step Beta"
    }
}
