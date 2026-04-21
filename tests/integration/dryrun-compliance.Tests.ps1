# ==============================================================================
#  tests/integration/dryrun-compliance.Tests.ps1
#  Verify DRY-RUN mode produces ZERO writes to system state.
# ==============================================================================
#
#  DRY-RUN is a critical safety feature: it allows users to preview changes
#  without modifying the system. These tests ensure that Set-RegistryValue,
#  Set-BootConfig, and all external tool invocations are fully intercepted
#  when $SCRIPT:DryRun = $true.

BeforeAll {
    . "$PSScriptRoot/_IntegrationInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Set-RegistryValue DRY-RUN interception ───────────────────────────────────
Describe "Set-RegistryValue DRY-RUN compliance" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $true
        $SCRIPT:Profile = "RECOMMENDED"
        $SCRIPT:CurrentStepTitle = "DRY-RUN Test Step"

        # Mock Write-Host to capture DRY-RUN messages
        Mock Write-Host {}
        Mock Write-DebugLog {}

        # Track any actual write attempts — these should NEVER be called
        Mock Set-ItemProperty {
            $SCRIPT:MockTracker.SetItemProperty.Add(@{
                Path = $Path; Name = $Name; Value = $Value; Type = $Type
            })
        }
        Mock New-Item {
            $SCRIPT:MockTracker.NewItem.Add(@{ Path = $Path })
            # Return a mock object so callers don't fail on null
            return [PSCustomObject]@{ PSPath = $Path }
        } -ParameterFilter { $Path -like "HK*" }
    }

    It "Set-RegistryValue does NOT call Set-ItemProperty in DRY-RUN" {
        Set-RegistryValue -path "HKLM:\SOFTWARE\TestKey" -name "TestValue" `
            -value 1 -type "DWord" -why "Test"

        $SCRIPT:MockTracker.SetItemProperty.Count | Should -Be 0
    }

    It "Set-RegistryValue does NOT create registry keys in DRY-RUN" {
        Set-RegistryValue -path "HKLM:\SOFTWARE\TestKey\NewSubKey" -name "TestValue" `
            -value 1 -type "DWord" -why "Test"

        $SCRIPT:MockTracker.NewItem.Count | Should -Be 0
    }

    It "Set-RegistryValue prints DRY-RUN message" {
        Set-RegistryValue -path "HKLM:\SOFTWARE\TestKey" -name "TestValue" `
            -value 1 -type "DWord" -why "Test"

        Should -Invoke Write-Host -ParameterFilter {
            $Object -like "*DRY-RUN*" -or $Object -like "*Would set*"
        } -Times 1 -Scope It
    }

    It "Set-RegistryValue does NOT call Backup-RegistryValue in DRY-RUN" {
        # Backup should be skipped when DryRun is true
        $backupCountBefore = $SCRIPT:_backupPending.Count
        Set-RegistryValue -path "HKLM:\SOFTWARE\TestKey" -name "TestValue" `
            -value 1 -type "DWord" -why "Test"

        $SCRIPT:_backupPending.Count | Should -Be $backupCountBefore
    }

    It "Multiple Set-RegistryValue calls produce zero writes" {
        $paths = @(
            @{ path = "HKLM:\SYSTEM\CurrentControlSet\Control"; name = "SvcHostSplitThresholdInKB" },
            @{ path = "HKCU:\SOFTWARE\Microsoft\GameBar"; name = "AllowAutoGameMode" },
            @{ path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; name = "PerfLevelSrc" }
        )

        foreach ($p in $paths) {
            Set-RegistryValue -path $p.path -name $p.name -value 1 -type "DWord" -why "Test"
        }

        $SCRIPT:MockTracker.SetItemProperty.Count | Should -Be 0
        $SCRIPT:MockTracker.NewItem.Count | Should -Be 0
    }
}

# ── Set-BootConfig DRY-RUN interception ──────────────────────────────────────
Describe "Set-BootConfig DRY-RUN compliance" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $true
        $SCRIPT:Profile = "RECOMMENDED"
        $SCRIPT:CurrentStepTitle = "DRY-RUN BootConfig Test"

        Mock Write-Host {}
        Mock Write-DebugLog {}

        Mock bcdedit {
            $SCRIPT:MockTracker.Bcdedit.Add(@{ Args = $args })
            return "The operation completed successfully."
        }
    }

    It "Set-BootConfig does NOT call bcdedit in DRY-RUN" {
        Set-BootConfig -key "disabledynamictick" -val "yes" -why "Test"

        $SCRIPT:MockTracker.Bcdedit.Count | Should -Be 0
    }

    It "Set-BootConfig prints DRY-RUN message" {
        Set-BootConfig -key "disabledynamictick" -val "yes" -why "Test"

        Should -Invoke Write-Host -ParameterFilter {
            $Object -like "*DRY-RUN*"
        } -Times 1 -Scope It
    }

    It "Set-BootConfig does NOT backup in DRY-RUN" {
        $backupCountBefore = $SCRIPT:_backupPending.Count
        Set-BootConfig -key "disabledynamictick" -val "yes" -why "Test"

        $SCRIPT:_backupPending.Count | Should -Be $backupCountBefore
    }

    It "Multiple Set-BootConfig calls produce zero bcdedit invocations" {
        Set-BootConfig -key "disabledynamictick" -val "yes" -why "Timer resolution"
        Set-BootConfig -key "useplatformtick" -val "yes" -why "Timer resolution"

        $SCRIPT:MockTracker.Bcdedit.Count | Should -Be 0
    }
}

# ── Invoke-TieredStep DRY-RUN integration ────────────────────────────────────
Describe "Invoke-TieredStep DRY-RUN integration with write functions" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $true
        $SCRIPT:Profile = "RECOMMENDED"

        Mock Write-Blank {}
        Mock Write-TierBadge {}
        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Write-Info {}
        Mock Write-OK {}
        Mock Write-Step {}
        Mock Show-StepInfoCard {}
        Mock Flush-BackupBuffer {}

        Mock Set-ItemProperty {
            $SCRIPT:MockTracker.SetItemProperty.Add(@{
                Path = $Path; Name = $Name; Value = $Value
            })
        }
        Mock bcdedit {
            $SCRIPT:MockTracker.Bcdedit.Add(@{ Args = $args })
        }
    }

    It "T1 step with Set-RegistryValue action produces zero writes in DRY-RUN" {
        Invoke-TieredStep -Tier 1 -Title "Test Registry Step" -Why "Testing" `
            -Risk "SAFE" -Depth "REGISTRY" -Action {
                Set-RegistryValue -path "HKLM:\SOFTWARE\Test" -name "Val" `
                    -value 1 -type "DWord" -why "Test"
            }

        $SCRIPT:MockTracker.SetItemProperty.Count | Should -Be 0
    }

    It "T1 step with Set-BootConfig action produces zero bcdedit calls in DRY-RUN" {
        Invoke-TieredStep -Tier 1 -Title "Test Boot Step" -Why "Testing" `
            -Risk "SAFE" -Depth "BOOT" -Action {
                Set-BootConfig -key "disabledynamictick" -val "yes" -why "Test"
            }

        $SCRIPT:MockTracker.Bcdedit.Count | Should -Be 0
    }

    It "T1 step with mixed actions produces zero total writes in DRY-RUN" {
        Invoke-TieredStep -Tier 1 -Title "Mixed Step" -Why "Testing" `
            -Risk "SAFE" -Action {
                Set-RegistryValue -path "HKLM:\SOFTWARE\Test" -name "A" `
                    -value 1 -type "DWord" -why "Test"
                Set-RegistryValue -path "HKCU:\SOFTWARE\Test" -name "B" `
                    -value 0 -type "DWord" -why "Test"
                Set-BootConfig -key "disabledynamictick" -val "yes" -why "Test"
            }

        $totalWrites = $SCRIPT:MockTracker.SetItemProperty.Count +
                       $SCRIPT:MockTracker.Bcdedit.Count
        $totalWrites | Should -Be 0
    }

    It "DRY-RUN does not record step in progress.json" {
        New-TestProgressFile -Phase 1 -LastStep 0

        Invoke-TieredStep -Tier 1 -Title "DRY-RUN No Progress" -Why "Testing" `
            -Risk "SAFE" -EstimateKey "DryRunTestKey" -Action { }

        # EstimateKey should not be tracked in DRY-RUN
        $SCRIPT:AppliedSteps | Should -Not -Contain "DryRunTestKey"
        # progress.json should remain at lastStep 0 (not updated)
        $prog = Get-Content $CFG_ProgressFile -Raw | ConvertFrom-Json
        $prog.lastCompletedStep | Should -Be 0
    }

    It "DRY-RUN does not track EstimateKey" {
        $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()

        Invoke-TieredStep -Tier 1 -Title "DRY Estimate" -Why "Testing" `
            -Risk "SAFE" -EstimateKey "TestEstimate" -Action { }

        $SCRIPT:AppliedSteps.Count | Should -Be 0
    }
}

# ── Complete-Step / Skip-Step DRY-RUN behavior ──────────────────────────────
Describe "Step progress DRY-RUN compliance" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $true

        Mock Write-DebugLog {}
    }

    It "Complete-Step does not write to progress.json in DRY-RUN" {
        Complete-Step -phase 1 -stepNum 5 -stepName "Test Step"

        Test-Path $CFG_ProgressFile | Should -Be $false
    }

    It "Skip-Step does not write to progress.json in DRY-RUN" {
        Skip-Step -phase 1 -stepNum 5 -stepName "Test Step"

        Test-Path $CFG_ProgressFile | Should -Be $false
    }
}

# ── Set-RunOnce DRY-RUN behavior ────────────────────────────────────────────
Describe "Set-RunOnce DRY-RUN compliance" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $true

        Mock Write-Host {}
        Mock Set-ItemProperty {
            $SCRIPT:MockTracker.SetItemProperty.Add(@{ Path = $Path; Name = $Name; Value = $Value })
        }
    }

    It "Set-RunOnce does NOT write to RunOnce registry in DRY-RUN" {
        Set-RunOnce -name "CS2_Phase3" -scriptPath "C:\CS2_OPTIMIZE\PostReboot-Setup.ps1"

        $SCRIPT:MockTracker.SetItemProperty.Count | Should -Be 0
    }

    It "Set-RunOnce prints DRY-RUN or security rejection message (no writes either way)" {
        Mock Write-Warn {}

        Set-RunOnce -name "CS2_Phase3" -scriptPath "C:\CS2_OPTIMIZE\PostReboot-Setup.ps1"

        # Path validation is now host-independent; DRY-RUN should always stop
        # before any registry write and emit the preview message.
        $SCRIPT:MockTracker.SetItemProperty.Count | Should -Be 0
        Should -Invoke Write-Host -ParameterFilter {
            $Object -like "*DRY-RUN*"
        } -Times 1 -Scope It
    }
}

# ── DRY-RUN with profile filtering ──────────────────────────────────────────
Describe "DRY-RUN respects profile filtering" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $true

        Mock Write-Blank {}
        Mock Write-TierBadge {}
        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Write-Info {}
        Mock Show-StepInfoCard {}
        Mock Flush-BackupBuffer {}
    }

    It "SAFE DRY-RUN skips T3 steps entirely (no preview)" {
        $SCRIPT:Profile = "SAFE"
        $state = @{ executed = $false }

        $result = Invoke-TieredStep -Tier 3 -Title "T3 SAFE DRY-RUN" -Why "Testing" `
            -Risk "MODERATE" -Action { $state.executed = $true }

        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "SAFE DRY-RUN skips T2 MODERATE steps (no preview)" {
        $SCRIPT:Profile = "SAFE"
        $state = @{ executed = $false }

        $result = Invoke-TieredStep -Tier 2 -Title "T2 Mod SAFE DRY-RUN" -Why "Testing" `
            -Risk "MODERATE" -Action { $state.executed = $true }

        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "COMPETITIVE DRY-RUN previews T3 MODERATE steps" {
        $SCRIPT:Profile = "COMPETITIVE"
        $state = @{ executed = $false }

        $result = Invoke-TieredStep -Tier 3 -Title "T3 COMP DRY-RUN" -Why "Testing" `
            -Risk "MODERATE" -Action { $state.executed = $true }

        $result | Should -Be $false
        $state.executed | Should -Be $true
    }

    It "RECOMMENDED DRY-RUN skips T3 steps" {
        $SCRIPT:Profile = "RECOMMENDED"
        $state = @{ executed = $false }

        $result = Invoke-TieredStep -Tier 3 -Title "T3 REC DRY-RUN" -Why "Testing" `
            -Risk "SAFE" -Action { $state.executed = $true }

        $result | Should -Be $false
        $state.executed | Should -Be $false
    }
}

# ── Backup buffer not populated in DRY-RUN ──────────────────────────────────
Describe "Backup buffer stays empty in DRY-RUN" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $true
        $SCRIPT:CurrentStepTitle = "DRY-RUN Backup Test"
        $SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

        Mock Write-Host {}
        Mock Write-DebugLog {}
    }

    It "Set-RegistryValue does not add backup entries in DRY-RUN" {
        Set-RegistryValue -path "HKLM:\SOFTWARE\Test" -name "Val" `
            -value 1 -type "DWord" -why "Test"

        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Set-BootConfig does not add backup entries in DRY-RUN" {
        Mock bcdedit {}

        Set-BootConfig -key "disabledynamictick" -val "yes" -why "Test"

        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Backup-PowerPlan returns early in DRY-RUN" {
        Backup-PowerPlan -StepTitle "Test"

        $SCRIPT:_backupPending.Count | Should -Be 0
    }
}
