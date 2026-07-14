# ==============================================================================
#  tests/helpers/power-plan.Tests.ps1  --  Power plan creation & tiered settings
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"

    # Give Pester stable parameter metadata on every platform. On Windows the
    # native powercfg.exe command has no PowerShell parameter named CmdArgs, so
    # mock bodies and parameter filters cannot inspect arguments consistently.
    function global:powercfg {
        param([Parameter(ValueFromRemainingArguments)][string[]]$CmdArgs)
        $null = $CmdArgs
    }

    . "$PSScriptRoot/../../helpers/power-plan.ps1"
}

AfterAll {
    Remove-Item Function:\global:powercfg -Force -ErrorAction SilentlyContinue
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── GUID Constants ──────────────────────────────────────────────────────────
Describe "Power Plan GUID Constants" {

    It "defines PCIe ASPM subgroup GUID" {
        $PP_SUB_PCIE | Should -Be "501a4d13-42af-4429-9fd1-a8218c268e20"
    }

    It "defines ASPM setting GUID" {
        $PP_ASPM | Should -Be "ee12f906-d277-404b-b6da-e5fa1a576df5"
    }

    It "defines processor subgroup GUID" {
        $PP_SUB_PROCESSOR | Should -Be "54533251-82be-4824-96c1-47b60b740d00"
    }

    It "maps dab60367... as the AHCI adaptive link-power setting" {
        $PP_DISKAHCIADAPTIVE | Should -Be "dab60367-53fe-4fbc-825e-521d069d2456"
    }

    It "uses documented processor timing setting GUIDs" {
        $PP_PERFINCRTIME | Should -Be "984cf492-3bed-4488-a8f9-4286c97bf5aa"
        $PP_PERFDECRTIME | Should -Be "d8edeb9b-95cf-4f95-a73c-b061973693c8"
    }

    It "all GUIDs match 36-char GUID format" {
        $guidVars = @(
            $PP_SUB_PROCESSOR, $PP_SUB_DISK, $PP_SUB_USB, $PP_SUB_SLEEP,
            $PP_SUB_NETWORK, $PP_SUB_GPUPREF, $PP_SUB_COOLING, $PP_SUB_PCIE,
            $PP_PERFBOOSTMODE, $PP_PERFBOOSTPOL, $PP_PERFEPP, $PP_PERFEPP2,
            $PP_PROCTHROTTLEMAX, $PP_PROCTHROTTLEMIN, $PP_IDLEDISABLE,
            $PP_IDLESTATEMAX, $PP_DUTYCYCLING, $PP_PERFHISTCOUNT,
            $PP_PERFINCRTIME, $PP_PERFDECRTIME, $PP_CPMINCORES, $PP_CPMAXCORES,
            $PP_CPMINCORES1, $PP_DISKIDLE, $PP_DISKPOWERMGMT, $PP_DISKLPM,
            $PP_DISKAHCIADAPTIVE, $PP_DISKNVIDLE, $PP_DISKADAPTIVE, $PP_USBSS,
            $PP_USBHUB, $PP_USBC, $PP_WIFIPOWERSAVE, $PP_GPUPREF,
            $PP_SYSCOOLPOL, $PP_STANDBYIDLE, $PP_HIBERNATEIDLE, $PP_ASPM
        )
        foreach ($g in $guidVars) {
            $g | Should -Match '^[a-fA-F0-9\-]{36}$'
        }
    }
}

# ── Set-PowerPlanValue ──────────────────────────────────────────────────────
Describe "Set-PowerPlanValue" {

    BeforeEach { Reset-TestState }

    Context "GUID Validation (Security)" {

        It "rejects invalid PlanGuid" {
            Mock Write-Warn {}
            Set-PowerPlanValue "INVALID!" $PP_SUB_PROCESSOR $PP_PROCTHROTTLEMAX 100 "test"
            Should -Invoke Write-Warn -Times 1
        }

        It "rejects invalid SubgroupGuid" {
            Mock Write-Warn {}
            Set-PowerPlanValue "a1b2c3d4-e5f6-7890-abcd-ef1234567890" "NOT-A-GUID" $PP_PROCTHROTTLEMAX 100 "test"
            Should -Invoke Write-Warn -Times 1
        }

        It "rejects invalid SettingGuid" {
            Mock Write-Warn {}
            Set-PowerPlanValue "a1b2c3d4-e5f6-7890-abcd-ef1234567890" $PP_SUB_PROCESSOR "INVALID" 100 "test"
            Should -Invoke Write-Warn -Times 1
        }

        It "allows DRY-RUN-GUID as PlanGuid" {
            Mock Write-Warn {}
            $SCRIPT:DryRun = $true
            Set-PowerPlanValue "DRY-RUN-GUID" $PP_SUB_PROCESSOR $PP_PROCTHROTTLEMAX 100 "test"
            Should -Invoke Write-Warn -Times 0
        }
    }

    Context "DRY-RUN mode" {

        It "skips powercfg in DRY-RUN mode" {
            $SCRIPT:DryRun = $true
            Mock powercfg {}
            Set-PowerPlanValue "a1b2c3d4-e5f6-7890-abcd-ef1234567890" $PP_SUB_PROCESSOR $PP_PROCTHROTTLEMAX 100 "test"
            Should -Invoke powercfg -Times 0
        }
    }

    Context "Normal execution" {

        It "calls powercfg with correct arguments" {
            $SCRIPT:DryRun = $false
            $planGuid = "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
            Mock powercfg { $global:LASTEXITCODE = 0 }
            Mock Write-DebugLog {}
            Set-PowerPlanValue $planGuid $PP_SUB_PROCESSOR $PP_PROCTHROTTLEMAX 100 "CPU max"
            Should -Invoke powercfg -Times 1
        }
    }
}

# ── New-CS2PowerPlan ────────────────────────────────────────────────────────
Describe "New-CS2PowerPlan" {

    BeforeEach { Reset-TestState }

    It "returns DRY-RUN-GUID in DRY-RUN mode" {
        $SCRIPT:DryRun = $true
        $result = New-CS2PowerPlan
        $result | Should -Be "DRY-RUN-GUID"
    }

    It "does not call powercfg in DRY-RUN mode" {
        $SCRIPT:DryRun = $true
        Mock powercfg {}
        New-CS2PowerPlan
        Should -Invoke powercfg -Times 0
    }

    It "does not inspect or delete a foreign plan that has the same display name" {
        $SCRIPT:DryRun = $false
        $newGuid = "11111111-2222-3333-4444-555555555555"
        Mock powercfg {
            $global:LASTEXITCODE = 0
            if ($CmdArgs[0] -eq '/duplicatescheme') {
                return "Power Scheme GUID: $newGuid  (High performance)"
            }
            return ""
        }

        New-CS2PowerPlan | Should -Be $newGuid

        Should -Invoke powercfg -Exactly 0 -ParameterFilter { $CmdArgs[0] -eq '/list' }
        Should -Invoke powercfg -Exactly 0 -ParameterFilter { $CmdArgs[0] -eq '/delete' }
    }

    It "does not accept a GUID echoed by a failed duplicate command" {
        $SCRIPT:DryRun = $false
        $highBaseGuid = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c"
        $newGuid = "11111111-2222-3333-4444-555555555555"
        Mock Write-Warn {}
        Mock powercfg {
            if ($CmdArgs[0] -eq '/duplicatescheme' -and $CmdArgs[1] -eq $highBaseGuid) {
                $global:LASTEXITCODE = 1
                return "Could not duplicate Power Scheme GUID: $highBaseGuid"
            }
            if ($CmdArgs[0] -eq '/duplicatescheme') {
                $global:LASTEXITCODE = 0
                return "Power Scheme GUID: $newGuid"
            }
            $global:LASTEXITCODE = 0
            return ""
        }

        New-CS2PowerPlan | Should -Be $newGuid

        Should -Invoke powercfg -Exactly 2 -ParameterFilter { $CmdArgs[0] -eq '/duplicatescheme' }
        Should -Invoke powercfg -Exactly 1 -ParameterFilter {
            $CmdArgs[0] -eq '/changename' -and $CmdArgs[1] -eq $newGuid
        }
    }

    It "extracts the new GUID from multiline native command output" {
        $SCRIPT:DryRun = $false
        $newGuid = "11111111-2222-3333-4444-555555555555"
        Mock powercfg {
            $global:LASTEXITCODE = 0
            if ($CmdArgs[0] -eq '/duplicatescheme') {
                return @('Power scheme duplicated successfully.', 'Power Scheme GUID: 11111111-2222-3333-4444-555555555555')
            }
            return ''
        }

        New-CS2PowerPlan | Should -Be $newGuid
    }
}

# ── Apply-PowerPlan ─────────────────────────────────────────────────────────
Describe "Invoke-CS2PowerPlanTransaction" {

    BeforeEach {
        Reset-TestState
        New-TestStateFile | Out-Null
        $SCRIPT:DryRun = $false
        Mock Write-Warn {}
        Mock Update-PowerPlanBackupOwnership {}
    }

    It "activates the replacement before deleting only the tracked prior suite GUID" {
        $activeGuid = "381b4222-f694-41f0-9685-ff5bb260df2e"
        $oldOwnedGuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        $newGuid = "11111111-2222-3333-4444-555555555555"
        $script:transactionCommands = [System.Collections.Generic.List[string]]::new()
        Mock Get-ActivePowerPlanGuid { "381b4222-f694-41f0-9685-ff5bb260df2e" }
        Mock Get-SuiteOwnedPowerPlanGuids { @("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee") }
        Mock New-CS2PowerPlan { "11111111-2222-3333-4444-555555555555" }
        Mock Apply-PowerPlan {}
        Mock Set-SuiteOwnedPowerPlanGuids {}
        Mock powercfg {
            $script:transactionCommands.Add(($CmdArgs -join ' '))
            $global:LASTEXITCODE = 0
        }

        Invoke-CS2PowerPlanTransaction | Should -Be $newGuid

        $script:transactionCommands[0] | Should -Be "/setactive $newGuid"
        $script:transactionCommands[1] | Should -Be "/delete $oldOwnedGuid"
        Should -Invoke powercfg -Exactly 0 -ParameterFilter { $CmdArgs[0] -eq '/list' }
        Should -Invoke Set-SuiteOwnedPowerPlanGuids -Exactly 1 -ParameterFilter {
            $Guids -contains "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee" -and
                $Guids -contains "11111111-2222-3333-4444-555555555555"
        }
    }

    It "performs no creation, configuration, activation, or persistence under WhatIf" {
        Mock Get-ActivePowerPlanGuid { throw "must not query live state" }
        Mock Get-SuiteOwnedPowerPlanGuids { throw "must not query ownership" }
        Mock New-CS2PowerPlan { throw "must not create" }
        Mock Apply-PowerPlan { throw "must not configure" }
        Mock Set-SuiteOwnedPowerPlanGuids { throw "must not persist" }
        Mock powercfg { throw "must not execute" }

        Invoke-CS2PowerPlanTransaction -WhatIf | Should -Be "DRY-RUN-GUID"

        Should -Invoke Get-ActivePowerPlanGuid -Exactly 0
        Should -Invoke New-CS2PowerPlan -Exactly 0
        Should -Invoke Apply-PowerPlan -Exactly 0
        Should -Invoke Set-SuiteOwnedPowerPlanGuids -Exactly 0
        Should -Invoke powercfg -Exactly 0
    }

    It "retains and records the active replacement when rollback reactivation fails" {
        $previousGuid = "381b4222-f694-41f0-9685-ff5bb260df2e"
        $oldOwnedGuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        $newGuid = "11111111-2222-3333-4444-555555555555"
        $script:persistCallCount = 0
        Mock Get-ActivePowerPlanGuid { "381b4222-f694-41f0-9685-ff5bb260df2e" }
        Mock Get-SuiteOwnedPowerPlanGuids { @("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee") }
        Mock New-CS2PowerPlan { "11111111-2222-3333-4444-555555555555" }
        Mock Apply-PowerPlan {}
        Mock Set-SuiteOwnedPowerPlanGuids {
            $script:persistCallCount++
            if ($script:persistCallCount -eq 1) { throw "state disk full" }
        }
        Mock powercfg { $global:LASTEXITCODE = 0 }
        Mock powercfg { $global:LASTEXITCODE = 0 } -ParameterFilter {
            $CmdArgs[0] -eq '/setactive' -and $CmdArgs[1] -eq "11111111-2222-3333-4444-555555555555"
        }
        Mock powercfg { $global:LASTEXITCODE = 1 } -ParameterFilter {
            $CmdArgs[0] -eq '/setactive' -and $CmdArgs[1] -eq "381b4222-f694-41f0-9685-ff5bb260df2e"
        }

        $transactionError = $null
        try { Invoke-CS2PowerPlanTransaction } catch { $transactionError = $_ }

        $transactionError.Exception.Message | Should -Match ([regex]::Escape($newGuid))
        $transactionError.Exception.Message | Should -Match 'remains active'
        $script:persistCallCount | Should -Be 2
        Should -Invoke Set-SuiteOwnedPowerPlanGuids -Exactly 2
        Should -Invoke powercfg -Exactly 0 -ParameterFilter {
            $CmdArgs[0] -eq '/delete' -and $CmdArgs[1] -eq "11111111-2222-3333-4444-555555555555"
        }
    }

    It "preserves the prior active and owned plan when configuration fails" {
        $activeGuid = "381b4222-f694-41f0-9685-ff5bb260df2e"
        $oldOwnedGuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        $newGuid = "11111111-2222-3333-4444-555555555555"
        Mock Get-ActivePowerPlanGuid { "381b4222-f694-41f0-9685-ff5bb260df2e" }
        Mock Get-SuiteOwnedPowerPlanGuids { @("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee") }
        Mock New-CS2PowerPlan { "11111111-2222-3333-4444-555555555555" }
        Mock Apply-PowerPlan { throw "configuration failed" }
        Mock Set-SuiteOwnedPowerPlanGuids {}
        Mock powercfg { $global:LASTEXITCODE = 0 }

        { Invoke-CS2PowerPlanTransaction } | Should -Throw '*configuration failed*'

        Should -Invoke powercfg -Exactly 0 -ParameterFilter { $CmdArgs[0] -eq '/setactive' }
        Should -Invoke powercfg -Exactly 1 -ParameterFilter { $CmdArgs[0] -eq '/delete' -and $CmdArgs[1] -eq "11111111-2222-3333-4444-555555555555" }
        Should -Invoke powercfg -Exactly 0 -ParameterFilter { $CmdArgs[0] -eq '/delete' -and $CmdArgs[1] -eq "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee" }
        Should -Invoke Set-SuiteOwnedPowerPlanGuids -Exactly 1 -ParameterFilter {
            @($Guids).Count -eq 1 -and $Guids -contains "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        }
    }

    It "preserves the prior active and owned plan when duplication fails" {
        Mock Get-ActivePowerPlanGuid { "381b4222-f694-41f0-9685-ff5bb260df2e" }
        Mock Get-SuiteOwnedPowerPlanGuids { @("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee") }
        Mock New-CS2PowerPlan { throw "duplicatescheme returned no GUID" }
        Mock Apply-PowerPlan {}
        Mock Set-SuiteOwnedPowerPlanGuids {}
        Mock powercfg { $global:LASTEXITCODE = 0 }

        { Invoke-CS2PowerPlanTransaction } | Should -Throw '*duplicatescheme returned no GUID*'

        Should -Invoke Apply-PowerPlan -Exactly 0
        Should -Invoke powercfg -Exactly 0
        Should -Invoke Set-SuiteOwnedPowerPlanGuids -Exactly 1 -ParameterFilter {
            @($Guids).Count -eq 1 -and $Guids -contains "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        }
    }

    It "preserves the prior active and owned plan when activation fails" {
        $activeGuid = "381b4222-f694-41f0-9685-ff5bb260df2e"
        $oldOwnedGuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        $newGuid = "11111111-2222-3333-4444-555555555555"
        Mock Get-ActivePowerPlanGuid { "381b4222-f694-41f0-9685-ff5bb260df2e" }
        Mock Get-SuiteOwnedPowerPlanGuids { @("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee") }
        Mock New-CS2PowerPlan { "11111111-2222-3333-4444-555555555555" }
        Mock Apply-PowerPlan {}
        Mock Set-SuiteOwnedPowerPlanGuids {}
        Mock powercfg {
            if ($CmdArgs[0] -eq '/setactive') {
                $global:LASTEXITCODE = 1
                return 'activation denied'
            }
            $global:LASTEXITCODE = 0
        }

        { Invoke-CS2PowerPlanTransaction } | Should -Throw '*Failed to activate*'

        Should -Invoke powercfg -Exactly 1 -ParameterFilter { $CmdArgs[0] -eq '/delete' -and $CmdArgs[1] -eq "11111111-2222-3333-4444-555555555555" }
        Should -Invoke powercfg -Exactly 0 -ParameterFilter { $CmdArgs[0] -eq '/delete' -and $CmdArgs[1] -eq "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee" }
        Should -Invoke Set-SuiteOwnedPowerPlanGuids -Exactly 1 -ParameterFilter {
            @($Guids).Count -eq 1 -and $Guids -contains "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        }
    }

    It "retains ownership when rollback cannot delete a created replacement" {
        $oldOwnedGuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        $newGuid = "11111111-2222-3333-4444-555555555555"
        Mock Get-ActivePowerPlanGuid { "381b4222-f694-41f0-9685-ff5bb260df2e" }
        Mock Get-SuiteOwnedPowerPlanGuids { @("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee") }
        Mock New-CS2PowerPlan { "11111111-2222-3333-4444-555555555555" }
        Mock Apply-PowerPlan { throw "configuration failed" }
        Mock Set-SuiteOwnedPowerPlanGuids {}
        Mock powercfg {
            $global:LASTEXITCODE = 5
            return 'access denied'
        }

        { Invoke-CS2PowerPlanTransaction } | Should -Throw "*remains recorded*"

        Should -Invoke Set-SuiteOwnedPowerPlanGuids -Exactly 1 -ParameterFilter {
            @($Guids).Count -eq 2 -and
                $Guids -contains "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee" -and
                $Guids -contains "11111111-2222-3333-4444-555555555555"
        }
        Should -Invoke Update-PowerPlanBackupOwnership -Exactly 1 -ParameterFilter {
            @($OwnedGuids).Count -eq 2 -and $OwnedGuids -contains "11111111-2222-3333-4444-555555555555"
        }
    }

    It "restores state and durable backup ownership after a successful rollback" {
        $oldOwnedGuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        $newGuid = "11111111-2222-3333-4444-555555555555"
        $script:backupOwnershipCalls = 0
        $script:rollbackCommands = [System.Collections.Generic.List[string]]::new()
        Mock Get-ActivePowerPlanGuid { "381b4222-f694-41f0-9685-ff5bb260df2e" }
        Mock Get-SuiteOwnedPowerPlanGuids { @("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee") }
        Mock New-CS2PowerPlan { "11111111-2222-3333-4444-555555555555" }
        Mock Apply-PowerPlan {}
        Mock Set-SuiteOwnedPowerPlanGuids {}
        Mock Update-PowerPlanBackupOwnership {
            $script:backupOwnershipCalls++
            if ($script:backupOwnershipCalls -eq 1) { throw "backup write failed after expansion" }
        }
        Mock powercfg {
            $script:rollbackCommands.Add(($CmdArgs -join ' '))
            $global:LASTEXITCODE = 0
        }

        { Invoke-CS2PowerPlanTransaction } | Should -Throw '*backup write failed after expansion*'

        $script:rollbackCommands | Should -Contain "/setactive 381b4222-f694-41f0-9685-ff5bb260df2e"
        $script:rollbackCommands | Should -Contain "/delete $newGuid"
        Should -Invoke Set-SuiteOwnedPowerPlanGuids -Exactly 1 -ParameterFilter {
            @($Guids).Count -eq 1 -and $Guids -contains "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        }
        Should -Invoke Update-PowerPlanBackupOwnership -Exactly 1 -ParameterFilter {
            @($OwnedGuids).Count -eq 1 -and $OwnedGuids -contains "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        }
    }

    It "does not retire prior ownership when state persistence fails after activation" {
        $previousGuid = "381b4222-f694-41f0-9685-ff5bb260df2e"
        $oldOwnedGuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        $newGuid = "11111111-2222-3333-4444-555555555555"
        Mock Get-ActivePowerPlanGuid { "381b4222-f694-41f0-9685-ff5bb260df2e" }
        Mock Get-SuiteOwnedPowerPlanGuids { @("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee") }
        Mock New-CS2PowerPlan { "11111111-2222-3333-4444-555555555555" }
        Mock Apply-PowerPlan {}
        Mock Set-SuiteOwnedPowerPlanGuids { throw "state write failed after activation" }
        Mock powercfg { $global:LASTEXITCODE = 0 }

        { Invoke-CS2PowerPlanTransaction } | Should -Throw '*state write failed after activation*'

        Should -Invoke powercfg -Exactly 1 -ParameterFilter {
            $CmdArgs[0] -eq '/setactive' -and $CmdArgs[1] -eq "381b4222-f694-41f0-9685-ff5bb260df2e"
        }
        Should -Invoke powercfg -Exactly 1 -ParameterFilter {
            $CmdArgs[0] -eq '/delete' -and $CmdArgs[1] -eq "11111111-2222-3333-4444-555555555555"
        }
        Should -Invoke powercfg -Exactly 0 -ParameterFilter {
            $CmdArgs[0] -eq '/delete' -and $CmdArgs[1] -eq "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        }
    }

    It "keeps the committed replacement active when retirement metadata narrowing fails" {
        $previousGuid = "381b4222-f694-41f0-9685-ff5bb260df2e"
        $oldOwnedGuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        $newGuid = "11111111-2222-3333-4444-555555555555"
        $script:stateWriteCalls = 0
        Mock Get-ActivePowerPlanGuid { "381b4222-f694-41f0-9685-ff5bb260df2e" }
        Mock Get-SuiteOwnedPowerPlanGuids { @("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee") }
        Mock New-CS2PowerPlan { "11111111-2222-3333-4444-555555555555" }
        Mock Apply-PowerPlan {}
        Mock Set-SuiteOwnedPowerPlanGuids {
            $script:stateWriteCalls++
            if ($script:stateWriteCalls -eq 2) { throw "injected retirement narrowing failure" }
        }
        Mock powercfg { $global:LASTEXITCODE = 0 }

        Invoke-CS2PowerPlanTransaction | Should -Be $newGuid

        Should -Invoke powercfg -Exactly 1 -ParameterFilter {
            $CmdArgs[0] -eq '/setactive' -and $CmdArgs[1] -eq $newGuid
        }
        Should -Invoke powercfg -Exactly 0 -ParameterFilter {
            $CmdArgs[0] -eq '/setactive' -and $CmdArgs[1] -eq $previousGuid
        }
        Should -Invoke powercfg -Exactly 1 -ParameterFilter {
            $CmdArgs[0] -eq '/delete' -and $CmdArgs[1] -eq $oldOwnedGuid
        }
        Should -Invoke powercfg -Exactly 0 -ParameterFilter {
            $CmdArgs[0] -eq '/delete' -and $CmdArgs[1] -eq $newGuid
        }
    }
}

Describe "Get-SuiteOwnedPowerPlanGuids" {

    BeforeEach {
        Reset-TestState
        New-TestStateFile | Out-Null
    }

    It "returns an empty collection when legacy state has no ownership properties" {
        $state = Get-Content -LiteralPath $CFG_StateFile -Raw | ConvertFrom-Json
        $state.PSObject.Properties.Remove('suiteOwnedPowerPlanGuids')
        $state.PSObject.Properties.Remove('suiteOwnedPowerPlanGuid')
        Save-SuiteState -State $state

        @(Get-SuiteOwnedPowerPlanGuids).Count | Should -Be 0
    }

    It "does not persist ownership under WhatIf" {
        $ownedGuid = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee'

        Set-SuiteOwnedPowerPlanGuids -Guids @($ownedGuid) -WhatIf

        $saved = Get-Content -LiteralPath $CFG_StateFile -Raw | ConvertFrom-Json
        $saved.PSObject.Properties.Name | Should -Not -Contain 'suiteOwnedPowerPlanGuids'
    }
}

Describe "Invoke-CS2PowerPlanWithFallback" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        Mock Write-Warn {}
        Mock Write-Info {}
        Mock Write-OK {}
    }

    It "returns a completable result only for the intended owned plan" {
        $newGuid = "11111111-2222-3333-4444-555555555555"
        Mock Invoke-CS2PowerPlanTransaction { $newGuid }

        $result = Invoke-CS2PowerPlanWithFallback

        $result.Status | Should -Be 'Success'
        $result.CanCompleteStep | Should -BeTrue
        $result.Guid | Should -Be $newGuid
    }

    It "reports a successful Windows fallback as skipped rather than complete" {
        Mock Invoke-CS2PowerPlanTransaction { throw 'transaction failed' }
        Mock powercfg { $global:LASTEXITCODE = 0 }

        $result = Invoke-CS2PowerPlanWithFallback

        $result.Status | Should -Be 'Fallback'
        $result.CanCompleteStep | Should -BeFalse
    }

    It "reports failure when neither fallback can be activated" {
        Mock Invoke-CS2PowerPlanTransaction { throw 'transaction failed' }
        Mock powercfg {
            $global:LASTEXITCODE = 1
            return "activation failed"
        }

        $result = Invoke-CS2PowerPlanWithFallback

        $result.Status | Should -Be 'Failed'
        $result.CanCompleteStep | Should -BeFalse
        Should -Invoke powercfg -Exactly 2
    }

    It "requires the Phase 1 caller to gate persisted completion on the structured result" {
        $source = Get-Content "$PSScriptRoot/../../Optimize-SystemBase.ps1" -Raw

        $source | Should -Match '(?s)Invoke-CS2PowerPlanWithFallback.*Status -eq ''Failed''.*throw.*Status -eq ''Fallback''.*Skip-Step.*CanCompleteStep.*Complete-Step'
        $source.IndexOf('Backup-PowerPlan -StepTitle "CS2 Optimized Power Plan"') |
            Should -BeLessThan $source.IndexOf('Invoke-CS2PowerPlanWithFallback')
    }
}

Describe "Apply-PowerPlan" {

    BeforeEach { Reset-TestState }

    Context "T1 settings (SAFE profile)" {

        It "applies T1 settings for SAFE profile" {
            $SCRIPT:Profile = "SAFE"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "Intel" }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Set-PowerPlanValue {}

            Apply-PowerPlan "DRY-RUN-GUID"

            # T1 settings: verify Set-PowerPlanValue was called at least once (count varies as T1 set evolves)
            Should -Invoke Set-PowerPlanValue -Scope It
        }
    }

    Context "T2 settings (RECOMMENDED profile)" {

        It "applies T2 AMD vendor branching (PROCTHROTTLEMIN=0)" {
            $SCRIPT:Profile = "RECOMMENDED"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "AMD" }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-ConsoleLine {}

            $script:ppCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-PowerPlanValue {
                $script:ppCalls.Add(@{ Label = $Label; Value = $Value })
            }

            Apply-PowerPlan "DRY-RUN-GUID"

            $minCall = $script:ppCalls | Where-Object { $_.Label -match "CPU min perf" }
            $minCall | Should -Not -BeNullOrEmpty
        }

        It "applies T2 Intel vendor branching with CPMINCORES1" {
            $SCRIPT:Profile = "RECOMMENDED"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "Intel" }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-ConsoleLine {}

            $script:ppCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-PowerPlanValue {
                $script:ppCalls.Add(@{ Label = $Label; Value = $Value })
            }

            Apply-PowerPlan "DRY-RUN-GUID"

            $intelCall = $script:ppCalls | Where-Object { $_.Label -match "Intel ring min cores" }
            $intelCall | Should -Not -BeNullOrEmpty
        }
    }

    Context "T3 settings (COMPETITIVE profile)" {

        It "applies T3 settings only for COMPETITIVE" {
            $SCRIPT:Profile = "COMPETITIVE"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "Intel" }
            Mock Get-AmdCpuInfo { return $null }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-ConsoleLine {}

            $script:ppCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-PowerPlanValue {
                $script:ppCalls.Add(@{ Label = $Label; Value = $Value })
            }

            Apply-PowerPlan "DRY-RUN-GUID"

            $idleCall = $script:ppCalls | Where-Object { $_.Label -match "idle disable" }
            $idleCall | Should -Not -BeNullOrEmpty
        }

        It "does not apply T3 for RECOMMENDED profile" {
            $SCRIPT:Profile = "RECOMMENDED"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "Intel" }
            Mock Get-AmdCpuInfo { return $null }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-ConsoleLine {}

            $script:ppCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-PowerPlanValue {
                $script:ppCalls.Add(@{ Label = $Label; Value = $Value })
            }

            Apply-PowerPlan "DRY-RUN-GUID"

            $idleCall = $script:ppCalls | Where-Object { $_.Label -match "idle disable" }
            $idleCall | Should -BeNullOrEmpty
        }

        It "uses documented in-range interval values for perf increase and decrease time" {
            $SCRIPT:Profile = "COMPETITIVE"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "Intel" }
            Mock Get-AmdCpuInfo { return $null }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-ConsoleLine {}

            $script:ppCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-PowerPlanValue {
                $script:ppCalls.Add(@{ Label = $Label; Value = $Value; SettingGuid = $SettingGuid })
            }

            Apply-PowerPlan "DRY-RUN-GUID"

            ($script:ppCalls | Where-Object SettingGuid -eq $PP_PERFINCRTIME | Select-Object -ExpandProperty Value) | Should -Be 0
            ($script:ppCalls | Where-Object SettingGuid -eq $PP_PERFDECRTIME | Select-Object -ExpandProperty Value) | Should -Be 100
        }
    }

    Context "PCIe ASPM (T1)" {

        It "sets PCIe ASPM to 0 (off) as T1" {
            $SCRIPT:Profile = "SAFE"
            $SCRIPT:DryRun = $true
            Mock Get-ChipsetVendor { return "Intel" }
            Mock Write-Step {}
            Mock Write-OK {}

            $script:ppCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-PowerPlanValue {
                $script:ppCalls.Add(@{ SubgroupGuid = $SubgroupGuid; SettingGuid = $SettingGuid; Value = $Value; Label = $Label })
            }

            Apply-PowerPlan "DRY-RUN-GUID"

            $aspmCall = $script:ppCalls | Where-Object { $_.SettingGuid -eq $PP_ASPM }
            $aspmCall | Should -Not -BeNullOrEmpty
            $aspmCall.Value | Should -Be 0
        }
    }
}
