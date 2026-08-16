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

        Should -Invoke Set-SecureAcl -Exactly 1 -ParameterFilter { $Path -eq $CFG_BackupFile -and $Required }
    }

    It "fails closed for an existing user-owned exact-DACL backup before ACL repair or byte use" {
        Set-Content -LiteralPath $CFG_BackupFile -Value '{"entries":[]}' -Encoding UTF8
        Mock Assert-TrustedExistingControlFile { throw 'Trusted control file ACL validation failed: owner is not BUILTIN\\Administrators or SYSTEM.' }

        { Initialize-Backup } | Should -Throw '*owner is not BUILTIN\\Administrators or SYSTEM*'

        Should -Invoke Assert-TrustedExistingControlFile -Exactly 1 -ParameterFilter { $Path -eq $CFG_BackupFile }
        Should -Invoke Set-SecureAcl -Exactly 0 -ParameterFilter { $Path -eq $CFG_BackupFile }
    }

    It "fails closed for a reparse backup before ACL repair or byte use" {
        Set-Content -LiteralPath $CFG_BackupFile -Value '{"entries":[]}' -Encoding UTF8
        Mock Assert-TrustedExistingControlFile { throw 'Trusted control file is not a regular non-reparse filesystem file.' }

        { Initialize-Backup } | Should -Throw '*regular non-reparse*'

        Should -Invoke Set-SecureAcl -Exactly 0 -ParameterFilter { $Path -eq $CFG_BackupFile }
    }

    It "fails closed for a backup DACL with an unsafe ACE before ACL repair or byte use" {
        Set-Content -LiteralPath $CFG_BackupFile -Value '{"entries":[]}' -Encoding UTF8
        Mock Assert-TrustedExistingControlFile { throw 'Trusted control file ACL validation failed: untrusted identity has write access.' }

        { Initialize-Backup } | Should -Throw '*untrusted identity has write access*'

        Should -Invoke Set-SecureAcl -Exactly 0 -ParameterFilter { $Path -eq $CFG_BackupFile }
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

    It "leaves the only corrupted backup untouched when preservation fails" {
        $corruptContent = "not json and must survive"
        Set-Content $CFG_BackupFile -Value $corruptContent -Encoding UTF8
        Mock Copy-Item { throw "disk full" }
        Mock Write-Warn {}

        { Get-BackupDataRaw } | Should -Throw '*Refusing to reset corrupted backup*'

        Test-Path -LiteralPath $CFG_BackupFile | Should -Be $true
        (Get-Content -LiteralPath $CFG_BackupFile -Raw).Trim() | Should -BeExactly $corruptContent
        @(Get-ChildItem $SCRIPT:TestTempRoot -Filter "backup.corrupt.*.json").Count | Should -Be 0
    }

    It "does not consume or reset an existing backup when integrity validation rejects it" {
        Set-Content -LiteralPath $CFG_BackupFile -Value 'untrusted bytes' -Encoding UTF8
        Mock Assert-TrustedExistingControlFile { throw 'unsafe ACE' }
        Mock Get-Content { throw 'backup bytes must not be read' }
        Mock Remove-Item {}

        { Get-BackupDataRaw } | Should -Throw '*unsafe ACE*'

        Should -Invoke Get-Content -Exactly 0 -ParameterFilter { $Path -eq $CFG_BackupFile }
        Should -Invoke Remove-Item -Exactly 0 -ParameterFilter { $Path -eq $CFG_BackupFile }
    }
}

Describe "Load-Progress control-data validation" {

    BeforeEach {
        Reset-TestState
        Mock Write-Warn {}
        Mock Write-DebugLog {}
    }

    It "does not consume or preserve a progress file when integrity validation rejects it" {
        Set-Content -LiteralPath $CFG_ProgressFile -Value '{"phase":1}' -Encoding UTF8
        Mock Assert-TrustedExistingControlFile { throw 'unsafe ACE' }
        Mock Get-Content { throw 'progress bytes must not be read' }
        Mock Copy-Item {}

        { Load-Progress } | Should -Throw '*unsafe ACE*'

        Should -Invoke Get-Content -Exactly 0 -ParameterFilter { $Path -eq $CFG_ProgressFile }
        Should -Invoke Copy-Item -Exactly 0 -ParameterFilter { $Path -eq $CFG_ProgressFile }
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

    It "Save-SuiteState reapplies the secure ACL to state.json" {
        $state = [PSCustomObject]@{
            mode = "CONTROL"
            profile = "RECOMMENDED"
        }

        Save-SuiteState -State $state

        Should -Invoke Set-SecureAcl -Exactly 1 -ParameterFilter { $Path -eq $CFG_StateFile -and $Required }
    }

    It "persists an explicitly authorized GUI preview selection into an absent directory" {
        $originalStateFile = $CFG_StateFile
        $previewRoot = Join-Path $SCRIPT:TestTempRoot "gui-preview-state"
        $CFG_StateFile = Join-Path $previewRoot "state.json"
        $SCRIPT:DryRun = $true
        try {
            Save-SuiteState -State ([PSCustomObject]@{
                mode = "DRY-RUN"
                profile = "SAFE"
            }) -AllowDryRunPersistence

            Test-Path -LiteralPath $CFG_StateFile | Should -BeTrue
            (Get-Content -LiteralPath $CFG_StateFile -Raw | ConvertFrom-Json).mode | Should -Be "DRY-RUN"
        } finally {
            $CFG_StateFile = $originalStateFile
            Remove-Item -LiteralPath $previewRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

}

Describe "Critical ACL failures" {

    BeforeEach {
        Reset-TestState
        Mock Test-HostIsWindows { $true }
        Mock Get-Item {
            [PSCustomObject]@{
                Attributes = [System.IO.FileAttributes]::Directory
                PSIsContainer = $true
            }
        }
        Mock Set-SecureAcl { throw "acl failed" }
    }

    It "fails closed when the work directory cannot be secured" {
        { Ensure-SecureWorkDir -Path $CFG_WorkDir } | Should -Throw
    }
}

Describe "Set-RunOnce configurable ExecutionPolicy" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $script:RuntimeGeneration = "C:\FRAMETIME_CFG\runtime-generations\0123456789abcdef0123456789abcdef"
        $script:Phase2Script = "$script:RuntimeGeneration\SafeMode-DriverClean.ps1"
        $script:Phase3Script = "$script:RuntimeGeneration\PostReboot-Setup.ps1"
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Err {}
        Mock Test-Path { $true }
        Mock Test-HostIsWindows { $true }
        Mock Get-PhaseRuntimePublisherSid { "S-1-5-21-1000-1000-1000-1001" }
        Mock Ensure-SecureWorkDir {}
        Mock Set-PhaseRuntimePayloadAcl {}
        Mock Test-PhaseRuntimePayload { [PSCustomObject]@{ Valid = $true; Message = "verified" } }
        Mock Set-SecureAcl {}
        Mock Set-ItemProperty {}
        Mock New-Item {}
        Mock Get-TrustedWindowsToolPath { "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" }
    }

    It "uses CFG_RunOnceExecutionPolicy in the RunOnce command line" {
        $CFG_RunOnceExecutionPolicy = "AllSigned"

        Set-RunOnce -name "FRAMETIME_Phase2" -scriptPath $script:Phase2Script -SafeMode

        Should -Invoke Set-ItemProperty -Exactly 1 -ParameterFilter {
            $Path -eq "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce" -and
            $Name -eq "*!FRAMETIME_Phase2" -and
            $Value -match "-ExecutionPolicy AllSigned" -and
            $Value -match "-File" -and
            $Value -notmatch "-Command"
        }
        Should -Invoke Test-PhaseRuntimePayload -Exactly 1
        Should -Invoke Set-PhaseRuntimePayloadAcl -Exactly 1 -ParameterFilter {
            $Path -eq $CFG_WorkDir -and $PublisherSid -eq "S-1-5-21-1000-1000-1000-1001" -and $NoInheritance
        }
    }

    It "keeps Safe Mode execution semantics for the Phase 2 handoff" {
        Set-RunOnce -name "FRAMETIME_Phase2" -scriptPath $script:Phase2Script -SafeMode

        Should -Invoke Set-ItemProperty -Exactly 1 -ParameterFilter {
            $Name -eq "*!FRAMETIME_Phase2" -and
            $Value -match "-ExecutionPolicy Bypass"
        }
        Should -Invoke Test-PhaseRuntimePayload -Exactly 1
    }

    It "rejects invalid CFG_RunOnceExecutionPolicy values" {
        $CFG_RunOnceExecutionPolicy = "Nope"

        Set-RunOnce -name "FRAMETIME_Phase3" -scriptPath $script:Phase3Script

        Should -Invoke Write-Warn -Exactly 1 -ParameterFilter { $t -match 'invalid CFG_RunOnceExecutionPolicy' }
        Should -Invoke Set-ItemProperty -Exactly 0
        Should -Invoke Test-PhaseRuntimePayload -Exactly 0
        Should -Invoke Ensure-SecureWorkDir -Exactly 0
        Should -Invoke Set-PhaseRuntimePayloadAcl -Exactly 0
    }

    It "rejects Undefined because client policy precedence can block RunOnce" {
        $CFG_RunOnceExecutionPolicy = "Undefined"

        Set-RunOnce -name "FRAMETIME_Phase3" -scriptPath $script:Phase3Script

        Should -Invoke Write-Warn -Exactly 1 -ParameterFilter { $t -match 'unsupported on client systems' }
        Should -Invoke Set-ItemProperty -Exactly 0
        Should -Invoke Test-PhaseRuntimePayload -Exactly 0
        Should -Invoke Ensure-SecureWorkDir -Exactly 0
        Should -Invoke Set-PhaseRuntimePayloadAcl -Exactly 0
    }

    It "rejects Unrestricted to keep the RunOnce trust surface narrow" {
        $CFG_RunOnceExecutionPolicy = "Unrestricted"

        Set-RunOnce -name "FRAMETIME_Phase3" -scriptPath $script:Phase3Script

        Should -Invoke Write-Warn -Exactly 1 -ParameterFilter { $t -match 'invalid CFG_RunOnceExecutionPolicy' }
        Should -Invoke Set-ItemProperty -Exactly 0
        Should -Invoke Test-PhaseRuntimePayload -Exactly 0
        Should -Invoke Ensure-SecureWorkDir -Exactly 0
        Should -Invoke Set-PhaseRuntimePayloadAcl -Exactly 0
    }
}
