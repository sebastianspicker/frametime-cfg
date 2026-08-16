# ==============================================================================
#  tests/integration/backup-restore-roundtrip.Tests.ps1
#  End-to-end roundtrip for backup/restore entry types.
# ==============================================================================
#
#  Core backup types: registry, service, bootconfig, powerplan, drs, scheduledtask
#  Extended backup types: nic_adapter, qos_uro, defender, pagefile, dns
#  Each test: write -> backup.json captures previous -> restore writes back

BeforeAll {
    . "$PSScriptRoot/_IntegrationInit.ps1"

    # Pester cannot expose a synthetic CmdArgs parameter when it mocks the
    # native Windows powercfg.exe application. Shadow it with a test-only
    # function so mock argument inspection is identical on every platform.
    function global:powercfg {
        param([Parameter(ValueFromRemainingArguments)][string[]]$CmdArgs)
        $null = $CmdArgs
    }
    if (-not (Get-Command New-NetQosPolicy -ErrorAction SilentlyContinue)) {
        function global:New-NetQosPolicy {
            param($Name, [Parameter(ValueFromRemainingArguments)]$RemainingArgs)
            $null = $Name
        }
    }
}

AfterAll {
    Remove-Item Function:\global:powercfg -Force -ErrorAction SilentlyContinue
    Remove-Item Function:\global:New-NetQosPolicy -Force -ErrorAction SilentlyContinue
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Registry backup/restore roundtrip ────────────────────────────────────────
Describe "Registry backup and restore roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        $SCRIPT:CurrentStepTitle = "Registry Test Step"
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

        # Initialize backup file
        New-TestBackupFile -Entries @()

        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
    }

    It "Backup-RegistryValue captures existing value and Restore writes it back" {
        # Mock reading the existing value
        Mock Test-Path { $true } -ParameterFilter { $Path -eq "HKCU:\System\GameConfigStore" }
        Mock Get-ItemProperty {
            return [PSCustomObject]@{ GameDVR_Enabled = 42 }
        } -ParameterFilter { $Name -eq "GameDVR_Enabled" }
        Mock Get-Item {
            $mock = New-Object PSObject
            $mock | Add-Member -MemberType ScriptMethod -Name GetValueKind -Value { "DWord" }
            return $mock
        } -ParameterFilter { $Path -eq "HKCU:\System\GameConfigStore" }

        # Perform backup
        Backup-RegistryValue -Path "HKCU:\System\GameConfigStore" -Name "GameDVR_Enabled" -StepTitle "Registry Test Step"

        $SCRIPT:_backupPending.Count | Should -Be 1
        $entry = $SCRIPT:_backupPending[0]
        $entry.type | Should -Be "registry"
        $entry.path | Should -Be "HKCU:\System\GameConfigStore"
        $entry.name | Should -Be "GameDVR_Enabled"
        $entry.originalValue | Should -Be 42
        $entry.existed | Should -Be $true
        $entry.originalType | Should -Be "DWord"

        # Flush to disk
        Flush-BackupBuffer

        # Verify backup.json has the entry
        $backup = Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json
        $backup.entries.Count | Should -Be 1

        # Now mock the restore path
        Mock Set-ItemProperty {} -Verifiable

        # Perform restore
        Restore-StepChanges -StepTitle "Registry Test Step"

        Should -InvokeVerifiable
    }

    It "Backup-RegistryValue captures non-existent key and Restore removes it" {
        # Mock: key does not exist
        $videoSettingsPath = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\VideoSettings"
        Mock Test-Path { $false } -ParameterFilter { $Path -eq $videoSettingsPath }

        Backup-RegistryValue -Path $videoSettingsPath -Name "AutoHDREnabled" -StepTitle "Registry Test Step"

        $SCRIPT:_backupPending.Count | Should -Be 1
        $entry = $SCRIPT:_backupPending[0]
        $entry.existed | Should -Be $false
        $entry.originalValue | Should -BeNullOrEmpty

        # Flush to disk
        Flush-BackupBuffer

        # Restore should call Remove-ItemProperty for non-existent originals
        Mock Test-Path { $true } -ParameterFilter { $Path -eq $videoSettingsPath }
        Mock Get-ItemProperty { [PSCustomObject]@{ AutoHDREnabled = "some_value" } } -ParameterFilter { $Path -eq $videoSettingsPath }
        Mock Remove-ItemProperty {} -Verifiable

        Restore-StepChanges -StepTitle "Registry Test Step"

        Should -InvokeVerifiable
    }

    It "Multiple registry entries are backed up and restored as a group" {
        Mock Test-Path { $true } -ParameterFilter { $Path -eq "HKCU:\System\GameConfigStore" }
        Mock Get-ItemProperty {
            if ($Name -eq "GameDVR_Enabled") { return [PSCustomObject]@{ GameDVR_Enabled = 10 } }
            if ($Name -eq "GameDVR_FSEBehavior") { return [PSCustomObject]@{ GameDVR_FSEBehavior = 20 } }
        }
        Mock Get-Item {
            $mock = New-Object PSObject
            $mock | Add-Member -MemberType ScriptMethod -Name GetValueKind -Value { "DWord" }
            return $mock
        } -ParameterFilter { $Path -eq "HKCU:\System\GameConfigStore" }

        Backup-RegistryValue -Path "HKCU:\System\GameConfigStore" -Name "GameDVR_Enabled" -StepTitle "Multi Step"
        Backup-RegistryValue -Path "HKCU:\System\GameConfigStore" -Name "GameDVR_FSEBehavior" -StepTitle "Multi Step"

        $SCRIPT:_backupPending.Count | Should -Be 2

        Flush-BackupBuffer

        $backup = Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json
        @($backup.entries).Count | Should -Be 2

        # Restore both
        Mock Set-ItemProperty {}
        Restore-StepChanges -StepTitle "Multi Step"

        Should -Invoke Set-ItemProperty -Times 2
    }
}

# ── Service backup/restore roundtrip ─────────────────────────────────────────
Describe "Service backup and restore roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        $SCRIPT:CurrentStepTitle = "Service Test Step"
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

        New-TestBackupFile -Entries @()

        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
    }

    It "Backup-ServiceState captures startup type and Restore re-enables" {
        # Mock service query
        Mock Get-Service {
            return [PSCustomObject]@{
                Name = "DiagTrack"
                Status = "Running"
                StartType = "Automatic"
            }
        } -ParameterFilter { $Name -eq "DiagTrack" }

        Mock Get-CimInstance {
            return [PSCustomObject]@{ StartMode = "Auto" }
        } -ParameterFilter { $ClassName -eq "Win32_Service" }

        Mock Get-ItemProperty {
            return [PSCustomObject]@{ DelayedAutostart = 0 }
        } -ParameterFilter { $Path -like "*\Services\*" }

        Backup-ServiceState -ServiceName "DiagTrack" -StepTitle "Service Test Step"

        $SCRIPT:_backupPending.Count | Should -Be 1
        $entry = $SCRIPT:_backupPending[0]
        $entry.type | Should -Be "service"
        $entry.name | Should -Be "DiagTrack"
        $entry.originalStartType | Should -Be "Auto"
        $entry.originalStatus | Should -Be "Running"

        Flush-BackupBuffer

        # Restore
        Mock Set-Service {}
        Mock Start-Service {}

        Restore-StepChanges -StepTitle "Service Test Step"

        Should -Invoke Set-Service -Times 1
        Should -Invoke Start-Service -Times 1  # Because original status was Running
    }

    It "Backup-ServiceState captures delayed auto start flag" {
        Mock Get-Service {
            return [PSCustomObject]@{ Name = "DelayedSvc"; Status = "Running"; StartType = "Automatic" }
        }
        Mock Get-CimInstance {
            return [PSCustomObject]@{ StartMode = "Auto" }
        }
        Mock Get-ItemProperty {
            return [PSCustomObject]@{ DelayedAutostart = 1 }
        }

        Backup-ServiceState -ServiceName "DelayedSvc" -StepTitle "Service Test Step"

        $entry = $SCRIPT:_backupPending[0]
        $entry.delayedAutoStart | Should -Be $true
    }
}

# ── BootConfig backup/restore roundtrip ──────────────────────────────────────
Describe "BootConfig backup and restore roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        $SCRIPT:CurrentStepTitle = "Boot Test Step"
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

        New-TestBackupFile -Entries @()

        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
    }

    It "Backup-BootConfig captures existing bcdedit value" {
        # Backup-BootConfig uses bcdedit /enum /v which outputs hex element IDs.
        # "disabledynamictick" maps to 0x26000060 in the bcdElementMap.
        Mock bcdedit {
            return @(
                "Windows Boot Loader",
                "identifier              {current}",
                "0x26000060              Yes"
            )
        }

        Backup-BootConfig -Key "disabledynamictick" -StepTitle "Boot Test Step"

        $SCRIPT:_backupPending.Count | Should -Be 1
        $entry = $SCRIPT:_backupPending[0]
        $entry.type | Should -Be "bootconfig"
        $entry.key | Should -Be "disabledynamictick"
        $entry.originalValue | Should -Be "Yes"
        $entry.existed | Should -Be $true
    }

    It "Backup-BootConfig captures non-existent bcdedit value" {
        Mock bcdedit {
            return @(
                "Windows Boot Loader",
                "identifier              {current}"
            )
        }

        Backup-BootConfig -Key "nonexistentkey" -StepTitle "Boot Test Step"

        $entry = $SCRIPT:_backupPending[0]
        $entry.existed | Should -Be $false
        $entry.originalValue | Should -BeNullOrEmpty
    }

    It "Backup-BootConfig reports a failed BCD inventory" {
        Mock bcdedit {
            $global:LASTEXITCODE = 1
            "access denied"
        }

        $capture = Backup-BootConfig -Key "disabledynamictick" -StepTitle "Boot Test Step" -PassThru

        $capture.Captured | Should -BeFalse
        $capture.Message | Should -Match "exit code 1"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "BootConfig restore calls bcdedit /set for existing values" {
        Mock bcdedit {
            return @("identifier  {current}", "0x26000060  No")
        }

        Mock Invoke-BootConfigRestoreCommand {
            $SCRIPT:MockTracker.Bcdedit.Add(@{ Args = @($Arguments) })
            $global:LASTEXITCODE = 0
            return "The operation completed successfully."
        }

        Backup-BootConfig -Key "disabledynamictick" -StepTitle "Boot Test Step"
        Flush-BackupBuffer
        Restore-StepChanges -StepTitle "Boot Test Step"

        $SCRIPT:MockTracker.Bcdedit.Count | Should -BeGreaterThan 0
        # Verify it used /set (restoring an existing value)
        $setCall = $SCRIPT:MockTracker.Bcdedit | Where-Object { ($_.Args -join " ") -match "/set" }
        $setCall | Should -Not -BeNullOrEmpty
    }

    It "BootConfig restore calls bcdedit /deletevalue for non-existent values" {
        Mock bcdedit {
            return @("identifier  {current}")
        }

        Mock Invoke-BootConfigRestoreCommand {
            $SCRIPT:MockTracker.Bcdedit.Add(@{ Args = @($Arguments) })
            $global:LASTEXITCODE = 0
            return "The operation completed successfully."
        }

        Backup-BootConfig -Key "useplatformtick" -StepTitle "Boot Test Step"
        Flush-BackupBuffer
        Restore-StepChanges -StepTitle "Boot Test Step"

        $SCRIPT:MockTracker.Bcdedit.Count | Should -BeGreaterThan 0
        # Verify it used /deletevalue (removing a value that didn't originally exist)
        $delCall = $SCRIPT:MockTracker.Bcdedit | Where-Object { ($_.Args -join " ") -match "/deletevalue" }
        $delCall | Should -Not -BeNullOrEmpty
    }
}

# ── PowerPlan backup/restore roundtrip ───────────────────────────────────────
Describe "PowerPlan backup and restore roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        $SCRIPT:CurrentStepTitle = "PowerPlan Test Step"
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

        New-TestBackupFile -Entries @()

        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
    }

    It "Backup-PowerPlan durably captures and verifies the active plan GUID" {
        Mock powercfg {
            $global:LASTEXITCODE = 0
            return "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)"
        }

        Backup-PowerPlan -StepTitle "PowerPlan Test Step"

        $SCRIPT:_backupPending.Count | Should -Be 0
        $backup = Get-Content -LiteralPath $CFG_BackupFile -Raw | ConvertFrom-Json
        @($backup.entries).Count | Should -Be 1
        $entry = @($backup.entries)[0]
        $entry.type | Should -Be "powerplan"
        $entry.originalGuid | Should -Be "381b4222-f694-41f0-9685-ff5bb260df2e"
        $entry.originalName | Should -Be "Balanced"
    }

    It "aborts power-plan backup when durable persistence fails" {
        Mock powercfg {
            $global:LASTEXITCODE = 0
            return "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)"
        }
        Mock Save-BackupData { throw "disk full" }

        { Backup-PowerPlan -StepTitle "PowerPlan Test Step" } | Should -Throw '*disk full*'

        $SCRIPT:_backupPending.Count | Should -Be 1
    }

    It "aborts when the active power plan cannot be identified" {
        Mock powercfg {
            $global:LASTEXITCODE = 1
            return "access denied"
        }

        { Backup-PowerPlan -StepTitle "PowerPlan Test Step" } | Should -Throw '*durable power-plan restore point*'

        $SCRIPT:_backupPending.Count | Should -Be 0
        $backup = Get-Content -LiteralPath $CFG_BackupFile -Raw | ConvertFrom-Json
        @($backup.entries).Count | Should -Be 0
    }

    It "reuses a durable original restore point when the active plan is suite-owned" {
        $ownedGuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        New-TestBackupFile -Entries @([ordered]@{
            type = "powerplan"
            originalGuid = "381b4222-f694-41f0-9685-ff5bb260df2e"
            originalName = "Balanced"
            suiteOwnedGuids = @($ownedGuid)
            step = "PowerPlan Test Step"
            timestamp = "2026-01-01"
        })
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = "RECOMMENDED"
            suiteOwnedPowerPlanGuids = @($ownedGuid)
        }) -Path $CFG_StateFile
        Mock powercfg {
            $global:LASTEXITCODE = 0
            return "Power Scheme GUID: aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee  (frametime.cfg)"
        }

        { Backup-PowerPlan -StepTitle "frametime.cfg Power Plan" } | Should -Not -Throw

        $SCRIPT:_backupPending.Count | Should -Be 0
        $backup = Get-Content -LiteralPath $CFG_BackupFile -Raw | ConvertFrom-Json
        @($backup.entries).Count | Should -Be 1
        @($backup.entries)[0].originalGuid | Should -Be "381b4222-f694-41f0-9685-ff5bb260df2e"
    }

    It "Backup-PowerPlan skips in DRY-RUN" {
        $SCRIPT:DryRun = $true

        Backup-PowerPlan -StepTitle "PowerPlan DRY Test"

        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "does not update backup ownership metadata under WhatIf" {
        $oldGuid = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee'
        $newGuid = '11111111-2222-3333-4444-555555555555'
        $pendingEntry = [ordered]@{
            type = 'powerplan'; originalGuid = '381b4222-f694-41f0-9685-ff5bb260df2e'
            suiteOwnedGuids = @($oldGuid); step = 'PowerPlan Test Step'
        }
        $SCRIPT:_backupPending.Add($pendingEntry)
        New-TestBackupFile -Entries @([ordered]@{
            type = 'powerplan'; originalGuid = '381b4222-f694-41f0-9685-ff5bb260df2e'
            suiteOwnedGuids = @($oldGuid); step = 'PowerPlan Test Step'
        })

        Update-PowerPlanBackupOwnership -OwnedGuids @($newGuid) -WhatIf

        @($SCRIPT:_backupPending[0].suiteOwnedGuids) | Should -Be @($oldGuid)
        $saved = Get-Content -LiteralPath $CFG_BackupFile -Raw | ConvertFrom-Json
        @(@($saved.entries)[0].suiteOwnedGuids) | Should -Be @($oldGuid)
    }

    It "does not update recorded ownership state under WhatIf" {
        $oldGuid = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee'
        $newGuid = '11111111-2222-3333-4444-555555555555'
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = 'RECOMMENDED'; suiteOwnedPowerPlanGuids = @($oldGuid)
        }) -Path $CFG_StateFile

        Set-RecordedSuiteOwnedPowerPlanGuids -OwnedGuids @($newGuid) -WhatIf

        $saved = Get-Content -LiteralPath $CFG_StateFile -Raw | ConvertFrom-Json
        @($saved.suiteOwnedPowerPlanGuids) | Should -Be @($oldGuid)
    }

    It "restore deletes only recorded suite-owned GUIDs and leaves foreign same-name plans alone" {
        $originalGuid = "381b4222-f694-41f0-9685-ff5bb260df2e"
        $ownedGuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        $foreignSameNameGuid = "11111111-2222-3333-4444-555555555555"
        New-TestBackupFile -Entries @([ordered]@{
            type = "powerplan"
            originalGuid = $originalGuid
            originalName = "Balanced"
            suiteOwnedGuids = @($ownedGuid)
            step = "PowerPlan Test Step"
            timestamp = "2026-01-01"
        })
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = "RECOMMENDED"
            suiteOwnedPowerPlanGuids = @($ownedGuid)
        }) -Path $CFG_StateFile
        $script:restorePowerCommands = [System.Collections.Generic.List[string]]::new()
        Mock powercfg {
            $script:restorePowerCommands.Add(($CmdArgs -join ' '))
            $global:LASTEXITCODE = 0
            if ($CmdArgs[0] -eq '/list') {
                return @(
                    "Power Scheme GUID: $ownedGuid  (frametime.cfg)",
                    "Power Scheme GUID: $foreignSameNameGuid  (frametime.cfg)"
                )
            }
        }

        Restore-StepChanges -StepTitle "PowerPlan Test Step" | Should -Be $true

        $script:restorePowerCommands | Should -Contain "/setactive $originalGuid"
        $script:restorePowerCommands | Should -Contain "/delete $ownedGuid"
        $script:restorePowerCommands | Should -Not -Contain "/delete $foreignSameNameGuid"
        Should -Invoke powercfg -Exactly 0 -ParameterFilter { $CmdArgs[0] -eq '/list' }
    }

    It "refuses a backup-only forged power-plan GUID and retains the restore entry" {
        $originalGuid = '381b4222-f694-41f0-9685-ff5bb260df2e'
        $forgedGuid = '11111111-2222-3333-4444-555555555555'
        $stateOwnedGuid = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee'
        New-TestBackupFile -Entries @([ordered]@{
            type = 'powerplan'; originalGuid = $originalGuid; originalName = 'Balanced'
            suiteOwnedGuids = @($forgedGuid); step = 'PowerPlan Test Step'; timestamp = '2026-01-01'
        })
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = 'RECOMMENDED'; suiteOwnedPowerPlanGuids = @($stateOwnedGuid)
        }) -Path $CFG_StateFile
        Mock powercfg { $global:LASTEXITCODE = 0 }

        Restore-StepChanges -StepTitle 'PowerPlan Test Step' | Should -BeFalse

        Should -Invoke powercfg -Exactly 1 -ParameterFilter { $CmdArgs[0] -eq '/setactive' -and $CmdArgs[1] -eq $originalGuid }
        Should -Not -Invoke powercfg -ParameterFilter { $CmdArgs[0] -eq '/delete' }
        @((Get-Content -LiteralPath $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 1
        @((Get-Content -LiteralPath $CFG_StateFile -Raw | ConvertFrom-Json).suiteOwnedPowerPlanGuids) | Should -Be @($stateOwnedGuid)
    }

    It "retains only failed suite-owned deletions and retries them" {
        $originalGuid = "381b4222-f694-41f0-9685-ff5bb260df2e"
        $deletedGuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        $retryGuid = "11111111-2222-3333-4444-555555555555"
        New-TestBackupFile -Entries @([ordered]@{
            type = "powerplan"
            originalGuid = $originalGuid
            originalName = "Balanced"
            suiteOwnedGuids = @($deletedGuid, $retryGuid)
            step = "PowerPlan Test Step"
            timestamp = "2026-01-01"
        })
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = "RECOMMENDED"
            suiteOwnedPowerPlanGuids = @($deletedGuid, $retryGuid)
        }) -Path $CFG_StateFile
        $script:failRetryDeletion = $true
        Mock powercfg {
            if ($CmdArgs[0] -eq '/delete' -and $CmdArgs[1] -eq "11111111-2222-3333-4444-555555555555" -and $script:failRetryDeletion) {
                $global:LASTEXITCODE = 1
                return 'access denied'
            }
            if ($CmdArgs[0] -eq '/list') {
                $global:LASTEXITCODE = 0
                if ($script:failRetryDeletion) {
                    return "Power Scheme GUID: 11111111-2222-3333-4444-555555555555  (frametime.cfg)"
                }
                return "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)"
            }
            $global:LASTEXITCODE = 0
        }

        Restore-StepChanges -StepTitle "PowerPlan Test Step" | Should -Be $false

        $retainedBackup = Get-Content -LiteralPath $CFG_BackupFile -Raw | ConvertFrom-Json
        @($retainedBackup.entries).Count | Should -Be 1
        @(@($retainedBackup.entries)[0].suiteOwnedGuids) | Should -Be @($retryGuid)
        $retainedState = Get-Content -LiteralPath $CFG_StateFile -Raw | ConvertFrom-Json
        @($retainedState.suiteOwnedPowerPlanGuids) | Should -Be @($retryGuid)

        $script:failRetryDeletion = $false
        Restore-StepChanges -StepTitle "PowerPlan Test Step" | Should -Be $true

        @((Get-Content -LiteralPath $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 0
        $restoredState = Get-Content -LiteralPath $CFG_StateFile -Raw | ConvertFrom-Json
        @($restoredState.suiteOwnedPowerPlanGuids).Count | Should -Be 0
        Should -Invoke powercfg -Exactly 1 -ParameterFilter {
            $CmdArgs[0] -eq '/delete' -and $CmdArgs[1] -eq "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        }
        Should -Invoke powercfg -Exactly 2 -ParameterFilter {
            $CmdArgs[0] -eq '/delete' -and $CmdArgs[1] -eq "11111111-2222-3333-4444-555555555555"
        }
    }

    It "converges when state persistence fails after deletion and the stale GUID is already absent on retry" {
        $originalGuid = "381b4222-f694-41f0-9685-ff5bb260df2e"
        $ownedGuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        New-TestBackupFile -Entries @([ordered]@{
            type = "powerplan"
            originalGuid = $originalGuid
            originalName = "Balanced"
            suiteOwnedGuids = @($ownedGuid)
            step = "PowerPlan Test Step"
            timestamp = "2026-01-01"
        })
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = "RECOMMENDED"
            suiteOwnedPowerPlanGuids = @($ownedGuid)
        }) -Path $CFG_StateFile
        $script:deleteCalls = 0
        Mock powercfg {
            if ($CmdArgs[0] -eq '/delete') {
                $script:deleteCalls++
                $global:LASTEXITCODE = if ($script:deleteCalls -eq 1) { 0 } else { 1 }
                return $(if ($global:LASTEXITCODE -eq 0) { '' } else { 'scheme does not exist' })
            }
            if ($CmdArgs[0] -eq '/list') {
                $global:LASTEXITCODE = 0
                return "Power Scheme GUID: $originalGuid  (Balanced)"
            }
            $global:LASTEXITCODE = 0
        }
        $script:StateWriter = ${function:Set-RecordedSuiteOwnedPowerPlanGuids}
        $script:stateWriteCalls = 0
        Mock Set-RecordedSuiteOwnedPowerPlanGuids {
            param($OwnedGuids)
            $script:stateWriteCalls++
            if ($script:stateWriteCalls -eq 1) { throw "injected state persistence failure" }
            & $script:StateWriter -OwnedGuids $OwnedGuids
        }

        Restore-StepChanges -StepTitle "PowerPlan Test Step" | Should -Be $false

        $retained = Get-Content -LiteralPath $CFG_BackupFile -Raw | ConvertFrom-Json
        @($retained.entries).Count | Should -Be 1
        @(@($retained.entries)[0].suiteOwnedGuids).Count | Should -Be 0

        Restore-StepChanges -StepTitle "PowerPlan Test Step" | Should -Be $true

        @((Get-Content -LiteralPath $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 0
        @((Get-Content -LiteralPath $CFG_StateFile -Raw | ConvertFrom-Json).suiteOwnedPowerPlanGuids).Count | Should -Be 0
        # The retry inventories the state-only identity and clears stale
        # bookkeeping without issuing another delete for an already absent plan.
        $script:deleteCalls | Should -Be 1
        Should -Invoke powercfg -Exactly 1 -ParameterFilter { $CmdArgs[0] -eq '/list' }
    }
}

# ── ScheduledTask backup/restore roundtrip ───────────────────────────────────
Describe "ScheduledTask backup and restore roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        $SCRIPT:CurrentStepTitle = "Task Test Step"
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

        New-TestBackupFile -Entries @()

        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
    }

    It "Backup-ScheduledTask captures existing enabled task" {
        $script:TaskQueryCount = 0
        Mock Get-ScheduledTask {
            $script:TaskQueryCount++
            if ($script:TaskQueryCount -eq 1) {
                return [PSCustomObject]@{ TaskName = "CS2_Optimize_CCD_Affinity"; TaskPath = "\"; State = "Ready" }
            }
        }

        Backup-ScheduledTask -TaskName "CS2_Optimize_CCD_Affinity" -StepTitle "Task Test Step"

        $SCRIPT:_backupPending.Count | Should -Be 1
        $entry = $SCRIPT:_backupPending[0]
        $entry.type | Should -Be "scheduledtask"
        $entry.taskName | Should -Be "CS2_Optimize_CCD_Affinity"
        $entry.existed | Should -Be $true
        $entry.wasEnabled | Should -Be $true
    }

    It "Backup-ScheduledTask captures non-existent task" {
        Mock Get-ScheduledTask { $null }

        $capture = Backup-ScheduledTask -TaskName "CS2_Optimize_CCD_Affinity" -StepTitle "Task Test Step" -ScriptPath "C:\test.ps1" -PassThru

        $capture.Captured | Should -BeTrue -Because $capture.Message
        $entry = $SCRIPT:_backupPending[0]
        $entry.existed | Should -Be $false
        $entry.wasEnabled | Should -Be $false
        $entry.scriptPath | Should -Be "C:\test.ps1"
    }

    It "Backup-ScheduledTask reports an authoritative query failure" {
        Mock Get-ScheduledTask { throw "provider unavailable" }

        $capture = Backup-ScheduledTask -TaskName "CS2_Optimize_CCD_Affinity" -StepTitle "Task Test Step" -PassThru

        $capture.Captured | Should -BeFalse
        $capture.Message | Should -Match "provider unavailable"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Restore removes task that did not exist before" {
        Mock Get-ScheduledTask { $null }
        Backup-ScheduledTask -TaskName "CS2_Optimize_CCD_Affinity" -StepTitle "Task Test Step" -ScriptPath "C:\fake.ps1"
        Flush-BackupBuffer

        # Mock for restore: task now exists and should be removed
        Mock Get-ScheduledTask {
            return [PSCustomObject]@{ TaskName = "CS2_Optimize_CCD_Affinity"; State = "Ready" }
        }
        Mock Unregister-ScheduledTask {}
        Mock Test-Path { $false } -ParameterFilter { $Path -eq "C:\fake.ps1" }

        Restore-StepChanges -StepTitle "Task Test Step"

        Should -Invoke Unregister-ScheduledTask -Times 1
    }

    It "Restore re-enables task that was enabled before" {
        Mock Get-ScheduledTask {
            return [PSCustomObject]@{ TaskName = "CS2_Optimize_CCD_Affinity"; State = "Ready" }
        }
        Backup-ScheduledTask -TaskName "CS2_Optimize_CCD_Affinity" -StepTitle "Task Test Step"
        Flush-BackupBuffer

        # Mock for restore: task is now disabled
        Mock Get-ScheduledTask {
            return [PSCustomObject]@{ TaskName = "CS2_Optimize_CCD_Affinity"; State = "Disabled" }
        }
        Mock Enable-ScheduledTask {}

        Restore-StepChanges -StepTitle "Task Test Step"

        Should -Invoke Enable-ScheduledTask -Times 1
    }
}

# ── Flush-BackupBuffer integration ──────────────────────────────────────────
Describe "Flush-BackupBuffer integration" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

        New-TestBackupFile -Entries @()

        Mock Write-DebugLog {}
    }

    It "Flush writes pending entries to backup.json and clears buffer" {
        $SCRIPT:_backupPending.Add([ordered]@{
            type = "registry"; path = "HKLM:\Test"; name = "X";
            originalValue = 1; originalType = "DWord"; existed = $true;
            step = "Flush Test"; timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        })
        $SCRIPT:_backupPending.Add([ordered]@{
            type = "registry"; path = "HKLM:\Test"; name = "Y";
            originalValue = 2; originalType = "DWord"; existed = $true;
            step = "Flush Test"; timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        })

        $SCRIPT:_backupPending.Count | Should -Be 2

        Flush-BackupBuffer

        $SCRIPT:_backupPending.Count | Should -Be 0
        $backup = Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json
        @($backup.entries).Count | Should -Be 2
    }

    It "Flush is idempotent (no-op when buffer is empty)" {
        Flush-BackupBuffer
        Flush-BackupBuffer

        $backup = Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json
        @($backup.entries).Count | Should -Be 0
    }
}

# ── Corrupted backup.json handling ──────────────────────────────────────────
Describe "Corrupted backup.json recovery" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false

        Mock Write-DebugLog {}
        Mock Write-Warn {}
        Mock Write-ConsoleLine {}
    }

    It "Get-BackupDataRaw recovers from corrupted JSON" {
        # Write invalid JSON to backup file
        Set-Content $CFG_BackupFile -Value "{ this is not valid json !!!" -Encoding UTF8

        $result = Get-BackupDataRaw

        # Should return a fresh empty backup structure
        $result | Should -Not -BeNullOrEmpty
        @($result.entries).Count | Should -Be 0
    }

    It "Get-BackupDataRaw preserves corrupted file before resetting" {
        Set-Content $CFG_BackupFile -Value "corrupt data here" -Encoding UTF8

        Get-BackupDataRaw

        # A .corrupt.*.json file should have been created
        $corruptFiles = @(Get-ChildItem (Split-Path $CFG_BackupFile -Parent) -Filter "backup.corrupt.*.json")
        $corruptFiles.Count | Should -BeGreaterOrEqual 1
    }

    It "Get-BackupData flushes buffer before reading" {
        New-TestBackupFile -Entries @()
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()
        $SCRIPT:_backupPending.Add([ordered]@{
            type = "registry"; path = "HKLM:\Test"; name = "Buffered";
            originalValue = 99; originalType = "DWord"; existed = $true;
            step = "Buffer Test"; timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        })

        $result = Get-BackupData

        @($result.entries).Count | Should -Be 1
        $result.entries[0].name | Should -Be "Buffered"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }
}

# ── Security validation on restore ──────────────────────────────────────────
Describe "Restore security validation" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false

        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
    }

    It "Rejects registry restore with invalid path" {
        $malicious = @([ordered]@{
            type = "registry"; path = "C:\Windows\System32\evil";
            name = "payload"; originalValue = "pwned"; originalType = "DWord";
            existed = $true; step = "Tampered Step";
            timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        })
        New-TestBackupFile -Entries $malicious

        Mock Set-ItemProperty {}

        Restore-StepChanges -StepTitle "Tampered Step"

        # Set-ItemProperty should NOT be called for invalid paths
        Should -Not -Invoke Set-ItemProperty
    }

    It "Rejects registry restore with path traversal in name" {
        $malicious = @([ordered]@{
            type = "registry"; path = "HKLM:\SOFTWARE\Test";
            name = "..\..\Run\evil"; originalValue = "payload"; originalType = "String";
            existed = $true; step = "Traversal Step";
            timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        })
        New-TestBackupFile -Entries $malicious

        Mock Set-ItemProperty {}

        Restore-StepChanges -StepTitle "Traversal Step"

        Should -Not -Invoke Set-ItemProperty
    }

    It "Rejects bcdedit restore with invalid key format" {
        $malicious = @([ordered]@{
            type = "bootconfig"; key = "evil;shutdown /s";
            originalValue = "yes"; existed = $true; step = "BCD Tamper";
            timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        })
        New-TestBackupFile -Entries $malicious

        Mock Invoke-BootConfigRestoreCommand {}

        Restore-StepChanges -StepTitle "BCD Tamper"

        Should -Not -Invoke Invoke-BootConfigRestoreCommand
    }

    It "Rejects bcdedit restore with unsupported but syntactically valid key" {
        $malicious = @([ordered]@{
            type = "bootconfig"; key = "hypervisorlaunchtype";
            originalValue = "off"; existed = $true; step = "BCD Tamper";
            timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        })
        New-TestBackupFile -Entries $malicious

        Mock Invoke-BootConfigRestoreCommand {}

        Restore-StepChanges -StepTitle "BCD Tamper"

        Should -Not -Invoke Invoke-BootConfigRestoreCommand
    }

    It "Rejects bcdedit restore with unsupported value for an allowed key" {
        $malicious = @([ordered]@{
            type = "bootconfig"; key = "safeboot";
            originalValue = "debug"; existed = $true; step = "BCD Tamper";
            timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        })
        New-TestBackupFile -Entries $malicious

        Mock Invoke-BootConfigRestoreCommand {}

        Restore-StepChanges -StepTitle "BCD Tamper"

        Should -Not -Invoke Invoke-BootConfigRestoreCommand
    }

    It "Rejects powerplan restore with invalid GUID" {
        $malicious = @([ordered]@{
            type = "powerplan"; originalGuid = "not-a-real-guid!; powercfg /delete all";
            originalName = "Evil Plan"; step = "Power Tamper";
            timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        })
        New-TestBackupFile -Entries $malicious

        Mock powercfg {}

        Restore-StepChanges -StepTitle "Power Tamper"

        Should -Not -Invoke powercfg
    }
}

# ── NIC adapter backup/restore roundtrip ─────────────────────────────────────
Describe "NIC adapter backup and restore roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        $SCRIPT:CurrentStepTitle = "NIC Test Step"
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

        New-TestBackupFile -Entries @()

        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
    }

    It "restores nic_adapter entries via Set-NetAdapterAdvancedProperty" {
        Mock Get-NetAdapter {
            [PSCustomObject]@{ Name = "Ethernet"; InterfaceDescription = "Intel NIC" }
        }

        Backup-NicAdapterProperty -AdapterName "Ethernet" -PropertyName "EEE" `
            -OriginalValue "Disabled" -PropertyType "DisplayName" -StepTitle "NIC Test Step"
        Flush-BackupBuffer

        Mock Set-NetAdapterAdvancedProperty {} -Verifiable

        $result = Restore-StepChanges -StepTitle "NIC Test Step"

        $result | Should -Be $true
        Should -Invoke Set-NetAdapterAdvancedProperty -Exactly 1 -ParameterFilter {
            $Name -eq "Ethernet" -and $DisplayName -eq "EEE" -and $DisplayValue -eq "Disabled"
        }
    }

    It "retains nic_adapter entries when the adapter identity changed" {
        Mock Get-NetAdapter {
            [PSCustomObject]@{ Name = "Ethernet"; InterfaceDescription = "Intel NIC" }
        }

        Backup-NicAdapterProperty -AdapterName "Ethernet" -PropertyName "EEE" `
            -OriginalValue "Disabled" -PropertyType "DisplayName" -StepTitle "NIC Test Step"
        Flush-BackupBuffer

        Mock Get-NetAdapter {
            [PSCustomObject]@{ Name = "Ethernet"; InterfaceDescription = "Replacement NIC" }
        }
        Mock Set-NetAdapterAdvancedProperty {}

        $result = Restore-StepChanges -StepTitle "NIC Test Step"

        $result | Should -Be $false
        Should -Not -Invoke Set-NetAdapterAdvancedProperty
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 1
    }
}

# ── QoS/URO backup/restore roundtrip ────────────────────────────────────────
Describe "QoS/URO backup and restore roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        $SCRIPT:CurrentStepTitle = "QoS Test Step"
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

        New-TestBackupFile -Entries @()

        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
    }

    BeforeEach {
        $script:SuiteQosPolicies = @(
            [PSCustomObject]@{
                Name = 'CS2_UDP_Ports'; IPProtocolMatchCondition = 'UDP'
                IPDstPortStartMatchCondition = 27015; IPDstPortEndMatchCondition = 27036
                DSCPAction = 46; NetworkProfile = 'All'
            },
            [PSCustomObject]@{
                Name = 'CS2_App'; AppPathNameMatchCondition = '*\cs2.exe'
                DSCPAction = 46; NetworkProfile = 'All'
            }
        )
    }

    It "replaces suite policies with their captured original definitions and restores URO state" {
        Backup-QosAndUro -Policies $script:SuiteQosPolicies -UroState "disabled" -StepTitle "QoS Test Step"
        Flush-BackupBuffer

        Mock Get-NetQosPolicy { @($script:SuiteQosPolicies | Where-Object Name -eq $Name) }
        Mock Remove-NetQosPolicy {}
        Mock New-NetQosPolicy {}
        Mock netsh {
            $global:LASTEXITCODE = 0
            "Ok."
        }

        $result = Restore-StepChanges -StepTitle "QoS Test Step"

        $result | Should -Be $true
        Should -Invoke Remove-NetQosPolicy -Exactly 1 -ParameterFilter { $Name -eq 'CS2_UDP_Ports' }
        Should -Invoke Remove-NetQosPolicy -Exactly 1 -ParameterFilter { $Name -eq 'CS2_App' }
        Should -Invoke New-NetQosPolicy -Exactly 1 -ParameterFilter { $Name -eq 'CS2_UDP_Ports' }
        Should -Invoke New-NetQosPolicy -Exactly 1 -ParameterFilter { $Name -eq 'CS2_App' }
    }

    It "retains qos_uro entries when policy removal fails" {
        New-TestBackupFile -Entries @()
        Backup-QosAndUro -Policies $script:SuiteQosPolicies -UroState "disabled" -StepTitle "QoS Test Step"
        Flush-BackupBuffer

        Mock Get-NetQosPolicy { @($script:SuiteQosPolicies | Where-Object Name -eq $Name) }
        Mock Remove-NetQosPolicy { throw "Permission denied" }
        Mock netsh {
            $global:LASTEXITCODE = 0
            "Ok."
        }

        $result = Restore-StepChanges -StepTitle "QoS Test Step"

        $result | Should -Be $false
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 1
    }

    It "rejects a tampered policy name without deleting or recreating policies" {
        New-TestBackupFile -Entries @([ordered]@{
            type = 'qos_uro'; contractVersion = 2; suiteManagedPolicies = @('CS2_UDP_Ports', 'CS2_App', 'Foreign')
            policyStates = @(); uroState = 'disabled'; step = 'QoS Test Step'; timestamp = '2026-01-01'
        })
        Mock Remove-NetQosPolicy {}
        Mock New-NetQosPolicy {}
        Mock netsh {}

        Restore-StepChanges -StepTitle 'QoS Test Step' | Should -BeFalse

        Should -Not -Invoke Remove-NetQosPolicy
        Should -Not -Invoke New-NetQosPolicy
        Should -Not -Invoke netsh
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 1
    }

    It "rejects a tampered policy definition without deleting or recreating policies" {
        $states = @(
            [PSCustomObject]@{ name = 'CS2_UDP_Ports'; originalExisted = $true; originalDefinition = [PSCustomObject]@{
                ipProtocolMatchCondition = 'TCP'; ipDstPortStartMatchCondition = 27015; ipDstPortEndMatchCondition = 27036; dscpAction = 46; networkProfile = 'All'
            } },
            [PSCustomObject]@{ name = 'CS2_App'; originalExisted = $false; originalDefinition = $null }
        )
        New-TestBackupFile -Entries @([ordered]@{
            type = 'qos_uro'; contractVersion = 2; suiteManagedPolicies = @('CS2_UDP_Ports', 'CS2_App')
            policyStates = $states; uroState = 'disabled'; step = 'QoS Test Step'; timestamp = '2026-01-01'
        })
        Mock Remove-NetQosPolicy {}
        Mock New-NetQosPolicy {}

        Restore-StepChanges -StepTitle 'QoS Test Step' | Should -BeFalse

        Should -Not -Invoke Remove-NetQosPolicy
        Should -Not -Invoke New-NetQosPolicy
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 1
    }

    It "rejects a non-enum URO state without calling netsh" {
        New-TestBackupFile -Entries @()
        Backup-QosAndUro -Policies @() -UroState 'disabled' -StepTitle 'QoS Test Step'
        Flush-BackupBuffer
        $backup = Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json
        $backup.entries[0].uroState = 'enabled & arbitrary'
        Save-JsonAtomic -Data $backup -Path $CFG_BackupFile
        Mock Get-NetQosPolicy { $null }
        Mock Remove-NetQosPolicy {}
        Mock netsh {}

        Restore-StepChanges -StepTitle 'QoS Test Step' | Should -BeFalse

        Should -Not -Invoke Remove-NetQosPolicy
        Should -Not -Invoke netsh
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 1
    }
}

# ── Defender backup/restore roundtrip ───────────────────────────────────────
Describe "Defender backup and restore roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        $SCRIPT:CurrentStepTitle = "Defender Test Step"
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

        New-TestBackupFile -Entries @()

        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
    }

    It "removes stored Defender exclusions during restore" {
        Backup-DefenderExclusions -ExclusionPaths @("C:\Games\CS2") -ExclusionProcesses @("cs2.exe") -StepTitle "Defender Test Step"
        Flush-BackupBuffer

        Mock Remove-MpPreference {}

        $result = Restore-StepChanges -StepTitle "Defender Test Step"

        $result | Should -Be $true
        Should -Invoke Remove-MpPreference -Exactly 2
    }

    It "retains defender entries when exclusion removal fails" {
        Backup-DefenderExclusions -ExclusionPaths @("C:\Games\CS2") -ExclusionProcesses @("cs2.exe") -StepTitle "Defender Test Step"
        Flush-BackupBuffer

        Mock Remove-MpPreference { throw "Tamper protection" }

        $result = Restore-StepChanges -StepTitle "Defender Test Step"

        $result | Should -Be $false
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 1
    }
}

# ── Pagefile backup/restore roundtrip ───────────────────────────────────────
Describe "Pagefile backup and restore roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        $SCRIPT:CurrentStepTitle = "Pagefile Test Step"
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

        New-TestBackupFile -Entries @()

        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
    }

    It "automates pagefile restore and logs that a reboot is required" {
        Backup-PagefileConfig -AutomaticManaged $false -PagefilePath "C:\pagefile.sys" `
            -InitialSize 4096 -MaximumSize 8192 -StepTitle "Pagefile Test Step"
        Flush-BackupBuffer

        $computerSystem = [PSCustomObject]@{ Name = "HOST" }
        $pagefileSetting = [PSCustomObject]@{ Name = "C:\\pagefile.sys" }
        Mock Get-CimInstance {
            if ($ClassName -eq "Win32_ComputerSystem") { return $computerSystem }
            if ($ClassName -eq "Win32_PageFileSetting") { return $pagefileSetting }
        }
        Mock Invoke-PagefileCimUpdate {}

        $result = Restore-StepChanges -StepTitle "Pagefile Test Step"

        $result | Should -Be $true
        Should -Invoke Invoke-PagefileCimUpdate -Exactly 1 -ParameterFilter {
            $InputObject -eq $pagefileSetting -and $Property.InitialSize -eq 4096 -and $Property.MaximumSize -eq 8192
        }
        Should -Invoke Write-OK -ParameterFilter { $t -match "automated restore completed" }
        Should -Invoke Write-Info -ParameterFilter { $t -match "reboot is required" }
    }

    It "falls back to manual instructions and retains the pagefile entry when automation fails" {
        Backup-PagefileConfig -AutomaticManaged $true -PagefilePath "C:\pagefile.sys" `
            -InitialSize 0 -MaximumSize 0 -StepTitle "Pagefile Test Step"
        Flush-BackupBuffer

        Mock Get-CimInstance { throw "CIM unavailable" }
        Mock Write-Info {}

        $result = Restore-StepChanges -StepTitle "Pagefile Test Step"

        $result | Should -Be $false
        Should -Invoke Write-Info -ParameterFilter { $t -match "Manual restore: System Properties" }
        Should -Invoke Write-Warn -ParameterFilter { $t -match "partial success" }
        Should -Invoke Write-Info -ParameterFilter { $t -match "reboot is required" }
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 1
    }
}

# ── DNS backup/restore roundtrip ────────────────────────────────────────────
Describe "DNS backup and restore roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        $SCRIPT:CurrentStepTitle = "DNS Test Step"
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

        New-TestBackupFile -Entries @()

        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
    }

    It "restores DNS using the current adapter interface index" {
        Backup-DnsConfig -AdapterName "Ethernet" -InterfaceIndex 12 -OriginalDnsServers @("1.1.1.1", "1.0.0.1") -StepTitle "DNS Test Step"
        Flush-BackupBuffer

        Mock Get-NetAdapter {
            [PSCustomObject]@{ Name = "Ethernet"; InterfaceIndex = 99 }
        }
        Mock Set-DnsClientServerAddress {}

        $result = Restore-StepChanges -StepTitle "DNS Test Step"

        $result | Should -Be $true
        Should -Invoke Set-DnsClientServerAddress -Exactly 1 -ParameterFilter {
            $InterfaceIndex -eq 99 -and @($ServerAddresses).Count -eq 2
        }
    }

    It "retains dns entries when restore fails" {
        Backup-DnsConfig -AdapterName "Ethernet" -InterfaceIndex 12 -OriginalDnsServers @("1.1.1.1") -StepTitle "DNS Test Step"
        Flush-BackupBuffer

        Mock Get-NetAdapter {
            [PSCustomObject]@{ Name = "Ethernet"; InterfaceIndex = 12 }
        }
        Mock Set-DnsClientServerAddress { throw "Access denied" }

        $result = Restore-StepChanges -StepTitle "DNS Test Step"

        $result | Should -Be $false
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 1
    }

    It "fails closed when the backed-up adapter name no longer resolves" {
        Backup-DnsConfig -AdapterName "Ethernet" -InterfaceIndex 12 -OriginalDnsServers @("1.1.1.1") -StepTitle "DNS Test Step"
        Flush-BackupBuffer

        Mock Get-NetAdapter { $null }
        Mock Set-DnsClientServerAddress {}

        $result = Restore-StepChanges -StepTitle "DNS Test Step"

        $result | Should -Be $false
        Should -Invoke Set-DnsClientServerAddress -Exactly 0
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 1
    }

    It "fails closed when the backup lacks an adapter name" {
        $entries = @(
            [ordered]@{
                type = "dns"; adapterName = ""; interfaceIndex = 12;
                originalDnsServers = @("1.1.1.1"); step = "DNS Test Step";
                timestamp = "2026-01-01"
            }
        )
        New-TestBackupFile -Entries $entries

        Mock Get-NetAdapter {}
        Mock Set-DnsClientServerAddress {}

        $result = Restore-StepChanges -StepTitle "DNS Test Step"

        $result | Should -Be $false
        Should -Invoke Get-NetAdapter -Exactly 0
        Should -Invoke Set-DnsClientServerAddress -Exactly 0
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 1
    }
}

# ── DRS backup/restore roundtrip ────────────────────────────────────────────
Describe "DRS backup and restore roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false

        Mock Write-ConsoleLine {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-Step {}
        Mock Write-Info {}
    }

    It "delegates drs restore entries to Restore-DrsSettings" {
        New-TestBackupFile -Entries @(
            [ordered]@{
                type = "drs"
                step = "DRS Test Step"
                profile = "CS2"
                profileCreated = $false
                settings = @([ordered]@{ id = 1; previousValue = 1; existed = $true })
                timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
            }
        )

        Mock Restore-DrsSettings { $true }

        $result = Restore-StepChanges -StepTitle "DRS Test Step"

        $result | Should -Be $true
        Should -Invoke Restore-DrsSettings -Exactly 1
    }

    It "retains drs entries when Restore-DrsSettings reports failure" {
        New-TestBackupFile -Entries @(
            [ordered]@{
                type = "drs"
                step = "DRS Test Step"
                profile = "CS2"
                profileCreated = $false
                settings = @([ordered]@{ id = 1; previousValue = 1; existed = $true })
                timestamp = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
            }
        )

        Mock Restore-DrsSettings { $false }

        $result = Restore-StepChanges -StepTitle "DRS Test Step"

        $result | Should -Be $false
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 1
    }
}
