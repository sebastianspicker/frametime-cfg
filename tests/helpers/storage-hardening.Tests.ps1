# ==============================================================================
#  tests/helpers/storage-hardening.Tests.ps1  --  Sensitive file hardening
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Initialize-Backup hardening" {

    BeforeEach {
        Reset-TestState
        Mock Test-BackupLock { $false }
        Mock Set-BackupLock {}
        Mock Set-SecureAcl {}
    }

    It "applies a secure ACL to backup.json after initialization" {
        Initialize-Backup

        Should -Invoke Set-SecureAcl -Exactly 1 -ParameterFilter { $Path -eq $CFG_BackupFile }
    }
}

Describe "Prune-BackupVersions" {

    BeforeEach { Reset-TestState }

    It "removes the oldest versioned backups beyond CFG_BackupMaxVersions" {
        $CFG_BackupMaxVersions = 2
        @(
            "backup.20260101-000001.json",
            "backup.20260101-000002.json",
            "backup.20260101-000003.json"
        ) | ForEach-Object {
            Set-Content (Join-Path $SCRIPT:TestTempRoot $_) -Value "{}" -Encoding UTF8
        }

        Prune-BackupVersions

        $remaining = @(
            Get-ChildItem $SCRIPT:TestTempRoot -Filter "backup.*.json" |
                Where-Object { $_.Name -match '^backup\.\d{8}-\d{6}\.json$' } |
                Sort-Object Name
        )
        @($remaining.Name) | Should -Be @(
            "backup.20260101-000002.json",
            "backup.20260101-000003.json"
        )
    }

    It "preserves at least one version when CFG_BackupMaxVersions is zero" {
        $CFG_BackupMaxVersions = 0
        @(
            "backup.20260101-000001.json",
            "backup.20260101-000002.json"
        ) | ForEach-Object {
            Set-Content (Join-Path $SCRIPT:TestTempRoot $_) -Value "{}" -Encoding UTF8
        }

        Prune-BackupVersions

        $remaining = @(
            Get-ChildItem $SCRIPT:TestTempRoot -Filter "backup.*.json" |
                Where-Object { $_.Name -match '^backup\.\d{8}-\d{6}\.json$' } |
                Sort-Object Name
        )
        @($remaining.Name) | Should -Be @("backup.20260101-000002.json")
    }
}

Describe "Get-BackupDataRaw corruption handling" {

    BeforeEach {
        Reset-TestState
        Mock Test-BackupLock { $false }
        Mock Set-BackupLock {}
    }

    It "preserves corrupted files with a non-versioned .corrupt name" {
        Set-Content $CFG_BackupFile -Value "not json" -Encoding UTF8

        Get-BackupDataRaw | Out-Null

        $corruptFiles = @(Get-ChildItem $SCRIPT:TestTempRoot -Filter "backup.corrupt.*.json")
        $corruptFiles.Count | Should -Be 1
        @(
            Get-ChildItem $SCRIPT:TestTempRoot -Filter "backup.*.json" |
                Where-Object { $_.Name -match '^backup\.\d{8}-\d{6}(?:\d{3})?\.json$' }
        ).Count | Should -Be 0
    }
}

Describe "Sensitive JSON ACL re-application" {

    BeforeEach {
        Reset-TestState
        Mock Set-SecureAcl {}
        Mock Write-DebugLog {}
    }

    It "Save-Progress reapplies the secure ACL to progress.json" {
        $progress = [PSCustomObject]@{
            phase = 1
            lastCompletedStep = 2
            completedSteps = @("P1:2")
            skippedSteps = @()
            timestamps = [PSCustomObject]@{}
        }

        Save-Progress $progress

        Should -Invoke Set-SecureAcl -Exactly 1 -ParameterFilter { $Path -eq $CFG_ProgressFile }
    }

    It "Save-AppliedSteps reapplies the secure ACL to state.json" {
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = "RECOMMENDED"
            mode = "CONTROL"
            logLevel = "NORMAL"
        }) -Path $CFG_StateFile
        $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()
        $SCRIPT:AppliedSteps.Add("Step A")

        Save-AppliedSteps | Should -Be $true

        Should -Invoke Set-SecureAcl -Exactly 1 -ParameterFilter { $Path -eq $CFG_StateFile }
    }
}

Describe "Set-RunOnce configurable ExecutionPolicy" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Err {}
        Mock Test-Path { $true } -ParameterFilter { $Path -eq "C:\CS2_OPTIMIZE\PostReboot-Setup.ps1" }
        Mock Set-ItemProperty {}
    }

    It "uses CFG_RunOnceExecutionPolicy in the RunOnce command line" {
        $CFG_RunOnceExecutionPolicy = "AllSigned"

        Set-RunOnce -name "CS2_Phase3" -scriptPath "C:\CS2_OPTIMIZE\PostReboot-Setup.ps1"

        Should -Invoke Set-ItemProperty -Exactly 1 -ParameterFilter {
            $Name -eq "CS2_Phase3" -and
            $Value -match "-ExecutionPolicy AllSigned"
        }
    }

    It "rejects invalid CFG_RunOnceExecutionPolicy values" {
        $CFG_RunOnceExecutionPolicy = "Nope"

        Set-RunOnce -name "CS2_Phase3" -scriptPath "C:\CS2_OPTIMIZE\PostReboot-Setup.ps1"

        Should -Invoke Write-Warn -Exactly 1 -ParameterFilter { $t -match 'invalid CFG_RunOnceExecutionPolicy' }
        Should -Invoke Set-ItemProperty -Exactly 0
    }

    It "rejects Undefined because client policy precedence can block RunOnce" {
        $CFG_RunOnceExecutionPolicy = "Undefined"

        Set-RunOnce -name "CS2_Phase3" -scriptPath "C:\CS2_OPTIMIZE\PostReboot-Setup.ps1"

        Should -Invoke Write-Warn -Exactly 1 -ParameterFilter { $t -match 'unsupported on client systems' }
        Should -Invoke Set-ItemProperty -Exactly 0
    }

    It "rejects Unrestricted to keep the RunOnce trust surface narrow" {
        $CFG_RunOnceExecutionPolicy = "Unrestricted"

        Set-RunOnce -name "CS2_Phase3" -scriptPath "C:\CS2_OPTIMIZE\PostReboot-Setup.ps1"

        Should -Invoke Write-Warn -Exactly 1 -ParameterFilter { $t -match 'invalid CFG_RunOnceExecutionPolicy' }
        Should -Invoke Set-ItemProperty -Exactly 0
    }
}
