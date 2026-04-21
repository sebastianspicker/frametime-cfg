# ==============================================================================
#  tests/integration/state-persistence.Tests.ps1
#  Verify state survives simulated reboot cycles (save/load roundtrip).
# ==============================================================================
#
#  Tests that state.json and progress.json persist all fields correctly
#  and handle corruption/missing files gracefully.

BeforeAll {
    . "$PSScriptRoot/_IntegrationInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── state.json roundtrip ────────────────────────────────────────────────────
Describe "state.json persistence roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        Mock Write-DebugLog {}
    }

    It "Save-JsonAtomic / Load-State roundtrip preserves profile" {
        $state = [PSCustomObject]@{
            profile  = "COMPETITIVE"
            mode     = "CONTROL"
            logLevel = "VERBOSE"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        $loaded = Load-State $CFG_StateFile

        $loaded.profile | Should -Be "COMPETITIVE"
        $SCRIPT:Profile  | Should -Be "COMPETITIVE"
    }

    It "Save-JsonAtomic / Load-State roundtrip preserves mode" {
        $state = [PSCustomObject]@{
            profile  = "RECOMMENDED"
            mode     = "DRY-RUN"
            logLevel = "NORMAL"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        $loaded = Load-State $CFG_StateFile

        $loaded.mode    | Should -Be "DRY-RUN"
        $SCRIPT:Mode    | Should -Be "DRY-RUN"
        $SCRIPT:DryRun  | Should -Be $true
    }

    It "Save-JsonAtomic / Load-State roundtrip preserves logLevel" {
        $state = [PSCustomObject]@{
            profile  = "SAFE"
            mode     = "CONTROL"
            logLevel = "VERBOSE"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        $loaded = Load-State $CFG_StateFile

        $loaded.logLevel    | Should -Be "VERBOSE"
        $SCRIPT:LogLevel    | Should -Be "VERBOSE"
    }

    It "Load-State sets DryRun = false for non-DRY-RUN mode" {
        $state = [PSCustomObject]@{
            profile  = "RECOMMENDED"
            mode     = "CONTROL"
            logLevel = "NORMAL"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Load-State $CFG_StateFile

        $SCRIPT:DryRun | Should -Be $false
    }

    It "Load-State throws when state.json is missing" {
        Remove-Item $CFG_StateFile -Force -ErrorAction SilentlyContinue

        { Load-State $CFG_StateFile } | Should -Throw "*Settings file not found*"
    }

    It "state.json is valid JSON after save" {
        $state = [PSCustomObject]@{
            profile  = "CUSTOM"
            mode     = "CONTROL"
            logLevel = "MINIMAL"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        $raw = Get-Content $CFG_StateFile -Raw
        { $raw | ConvertFrom-Json } | Should -Not -Throw
    }

    It "state.json preserves custom properties (driver path, etc.)" {
        $state = [PSCustomObject]@{
            profile    = "RECOMMENDED"
            mode       = "CONTROL"
            logLevel   = "NORMAL"
            driverPath = "C:\NVIDIA\driver.exe"
            nicName    = "Realtek RTL8125"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        $loaded = Load-State $CFG_StateFile

        $loaded.driverPath | Should -Be "C:\NVIDIA\driver.exe"
        $loaded.nicName    | Should -Be "Realtek RTL8125"
    }

    It "Save-AppliedSteps persists step keys into state.json" {
        New-TestStateFile -TestProfile "RECOMMENDED"
        $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()
        $SCRIPT:AppliedSteps.Add("Clear Shader Cache")
        $SCRIPT:AppliedSteps.Add("Fullscreen Optimizations")
        $SCRIPT:AppliedSteps.Add("FPS Cap")

        Save-AppliedSteps | Should -Be $true

        $raw = Get-Content $CFG_StateFile -Raw | ConvertFrom-Json
        $raw.appliedSteps | Should -Contain "Clear Shader Cache"
        $raw.appliedSteps | Should -Contain "Fullscreen Optimizations"
        $raw.appliedSteps | Should -Contain "FPS Cap"
        @($raw.appliedSteps).Count | Should -Be 3
    }
}

# ── Initialize-ScriptDefaults (soft state load) ─────────────────────────────
Describe "Initialize-ScriptDefaults soft state loader" {

    BeforeEach {
        Reset-IntegrationState
        Mock Write-DebugLog {}
    }

    It "loads profile from existing state.json" {
        $state = [PSCustomObject]@{
            profile = "COMPETITIVE"; mode = "CONTROL"; logLevel = "VERBOSE"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Initialize-ScriptDefaults

        $SCRIPT:Profile  | Should -Be "COMPETITIVE"
        $SCRIPT:Mode     | Should -Be "CONTROL"
        $SCRIPT:LogLevel | Should -Be "VERBOSE"
    }

    It "sets safe defaults when state.json is missing" {
        Remove-Item $CFG_StateFile -Force -ErrorAction SilentlyContinue

        Initialize-ScriptDefaults

        $SCRIPT:Mode     | Should -Be "CONTROL"
        $SCRIPT:LogLevel | Should -Be "NORMAL"
        $SCRIPT:Profile  | Should -Be "RECOMMENDED"
        $SCRIPT:DryRun   | Should -Be $false
    }

    It "sets safe defaults when state.json is corrupted" {
        Set-Content $CFG_StateFile -Value "NOT VALID JSON {{{" -Encoding UTF8

        Initialize-ScriptDefaults

        $SCRIPT:Mode     | Should -Be "CONTROL"
        $SCRIPT:LogLevel | Should -Be "NORMAL"
        $SCRIPT:Profile  | Should -Be "RECOMMENDED"
        $SCRIPT:DryRun   | Should -Be $false
    }

    It "sets DryRun from DRY-RUN mode" {
        $state = [PSCustomObject]@{
            profile = "RECOMMENDED"; mode = "DRY-RUN"; logLevel = "NORMAL"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Initialize-ScriptDefaults

        $SCRIPT:DryRun | Should -Be $true
    }

    It "defaults logLevel to NORMAL when not present in state" {
        $state = [PSCustomObject]@{
            profile = "SAFE"; mode = "CONTROL"
            # No logLevel property
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Initialize-ScriptDefaults

        $SCRIPT:LogLevel | Should -Be "NORMAL"
    }

    It "defaults profile to RECOMMENDED when not present in state" {
        $state = [PSCustomObject]@{
            mode = "CONTROL"; logLevel = "NORMAL"
            # No profile property
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Initialize-ScriptDefaults

        $SCRIPT:Profile | Should -Be "RECOMMENDED"
    }
}

# ── progress.json roundtrip ──────────────────────────────────────────────────
Describe "progress.json persistence roundtrip" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        Mock Write-DebugLog {}
    }

    It "Complete-Step creates and persists progress.json" {
        Complete-Step -phase 1 -stepNum 5 -stepName "Power Plan"

        Test-Path $CFG_ProgressFile | Should -Be $true
        $prog = Get-Content $CFG_ProgressFile -Raw | ConvertFrom-Json
        $prog.phase | Should -Be 1
        $prog.lastCompletedStep | Should -Be 5
        $prog.completedSteps | Should -Contain "P1:5"
    }

    It "Multiple Complete-Step calls accumulate correctly" {
        Complete-Step -phase 1 -stepNum 1 -stepName "Step 1"
        Complete-Step -phase 1 -stepNum 2 -stepName "Step 2"
        Complete-Step -phase 1 -stepNum 3 -stepName "Step 3"

        $prog = Load-Progress
        $prog.lastCompletedStep | Should -Be 3
        @($prog.completedSteps) | Should -Contain "P1:1"
        @($prog.completedSteps) | Should -Contain "P1:2"
        @($prog.completedSteps) | Should -Contain "P1:3"
    }

    It "Skip-Step tracks skipped steps separately" {
        Complete-Step -phase 1 -stepNum 1 -stepName "Step 1"
        Skip-Step -phase 1 -stepNum 2 -stepName "Step 2"
        Complete-Step -phase 1 -stepNum 3 -stepName "Step 3"

        $prog = Load-Progress
        @($prog.completedSteps) | Should -Contain "P1:1"
        @($prog.completedSteps) | Should -Not -Contain "P1:2"
        @($prog.skippedSteps) | Should -Contain "P1:2"
        @($prog.completedSteps) | Should -Contain "P1:3"
    }

    It "Test-StepDone returns true for completed steps" {
        Complete-Step -phase 1 -stepNum 10 -stepName "Test Step"

        Test-StepDone -phase 1 -stepNum 10 | Should -Be $true
    }

    It "Test-StepDone returns true for skipped steps (for resume)" {
        Skip-Step -phase 1 -stepNum 7 -stepName "Skipped Step"

        Test-StepDone -phase 1 -stepNum 7 | Should -Be $true
    }

    It "Test-StepDone returns false for unknown steps" {
        Complete-Step -phase 1 -stepNum 1 -stepName "Step 1"

        Test-StepDone -phase 1 -stepNum 99 | Should -Be $false
    }

    It "Test-StepDone returns false when no progress file exists" {
        Remove-Item $CFG_ProgressFile -Force -ErrorAction SilentlyContinue

        Test-StepDone -phase 1 -stepNum 1 | Should -Be $false
    }

    It "Phase-scoped keys prevent cross-phase collisions" {
        Complete-Step -phase 1 -stepNum 5 -stepName "Phase 1 Step 5"
        # Phase 3 step 5 should not be marked done
        Test-StepDone -phase 3 -stepNum 5 | Should -Be $false
    }

    It "progress.json timestamps are recorded" {
        Complete-Step -phase 1 -stepNum 1 -stepName "Timestamped Step"

        $prog = Load-Progress
        $prog.timestamps."1-1" | Should -Not -BeNullOrEmpty
    }
}

# ── Corrupted progress.json recovery ────────────────────────────────────────
Describe "Corrupted progress.json recovery" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        Mock Write-DebugLog {}
        Mock Write-Warn {}
    }

    It "Load-Progress returns null for corrupted JSON" {
        Set-Content $CFG_ProgressFile -Value "INVALID JSON !!!" -Encoding UTF8

        $result = Load-Progress

        $result | Should -BeNullOrEmpty
    }

    It "Load-Progress preserves corrupted file as .corrupt" {
        Set-Content $CFG_ProgressFile -Value "BAD JSON" -Encoding UTF8

        Load-Progress

        $corruptFile = "$CFG_ProgressFile.corrupt"
        Test-Path $corruptFile | Should -Be $true
    }

    It "Load-Progress returns null for missing file" {
        Remove-Item $CFG_ProgressFile -Force -ErrorAction SilentlyContinue

        $result = Load-Progress

        $result | Should -BeNullOrEmpty
    }
}

# ── Clear-Progress ──────────────────────────────────────────────────────────
Describe "Clear-Progress" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        Mock Write-DebugLog {}
        Mock Write-Warn {}
    }

    It "resets progress for matching phase" {
        Complete-Step -phase 1 -stepNum 5 -stepName "Step 5"
        Complete-Step -phase 1 -stepNum 6 -stepName "Step 6"

        Clear-Progress 1

        $prog = Load-Progress
        $prog.lastCompletedStep | Should -Be 0
        @($prog.completedSteps).Count | Should -Be 0
    }

    It "resets progress even for non-matching phase (cross-phase re-run)" {
        New-TestProgressFile -Phase 1 -LastStep 5 -CompletedSteps @("P1:5")

        Clear-Progress 3

        $prog = Load-Progress
        $prog.lastCompletedStep | Should -Be 0
    }

    It "does nothing when progress file does not exist" {
        Remove-Item $CFG_ProgressFile -Force -ErrorAction SilentlyContinue

        { Clear-Progress 1 } | Should -Not -Throw
    }
}

# ── Save-JsonAtomic reliability ─────────────────────────────────────────────
Describe "Save-JsonAtomic atomicity" {

    BeforeEach {
        Reset-IntegrationState
    }

    It "writes valid JSON to the target path" {
        $data = @{ key = "value"; number = 42; nested = @{ inner = "data" } }
        $path = Join-Path $SCRIPT:TestTempRoot "atomic-test.json"

        Save-JsonAtomic -Data $data -Path $path

        Test-Path $path | Should -Be $true
        $loaded = Get-Content $path -Raw | ConvertFrom-Json
        $loaded.key | Should -Be "value"
        $loaded.number | Should -Be 42
        $loaded.nested.inner | Should -Be "data"
    }

    It "does not leave .tmp file on success" {
        $data = @{ clean = $true }
        $path = Join-Path $SCRIPT:TestTempRoot "notmp-test.json"

        Save-JsonAtomic -Data $data -Path $path

        Test-Path "$path.tmp" | Should -Be $false
    }

    It "creates parent directory if missing" {
        $data = @{ auto = "mkdir" }
        $path = Join-Path $SCRIPT:TestTempRoot "newdir/subdir/deep.json"

        Save-JsonAtomic -Data $data -Path $path

        Test-Path $path | Should -Be $true
    }

    It "overwrites existing file atomically" {
        $path = Join-Path $SCRIPT:TestTempRoot "overwrite-test.json"
        Save-JsonAtomic -Data @{ version = 1 } -Path $path
        Save-JsonAtomic -Data @{ version = 2 } -Path $path

        $loaded = Get-Content $path -Raw | ConvertFrom-Json
        $loaded.version | Should -Be 2
    }
}

# ── Full simulated reboot cycle ─────────────────────────────────────────────
Describe "Simulated reboot cycle (Phase 1 save -> Phase 3 load)" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false
        Mock Write-DebugLog {}
    }

    It "Phase 1 state survives simulated reboot and loads correctly in Phase 3" {
        # Phase 1: Setup profile and save state
        $SCRIPT:Profile = "COMPETITIVE"
        $SCRIPT:Mode = "CONTROL"
        $SCRIPT:LogLevel = "VERBOSE"

        $state = [PSCustomObject]@{
            profile    = $SCRIPT:Profile
            mode       = $SCRIPT:Mode
            logLevel   = $SCRIPT:LogLevel
            driverPath = "C:\NVIDIA\561.00-desktop-win10-win11-64bit.exe"
            nicName    = "Realtek Gaming 2.5GbE"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        # Phase 1: Complete some steps
        Complete-Step -phase 1 -stepNum 1 -stepName "Power Plan"
        Complete-Step -phase 1 -stepNum 2 -stepName "Shader Cache"
        Skip-Step -phase 1 -stepNum 3 -stepName "Optional Step"

        # Phase 1: Track applied steps
        $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()
        $SCRIPT:AppliedSteps.Add("Clear Shader Cache")
        $SCRIPT:AppliedSteps.Add("Power Plan")
        Save-AppliedSteps

        # --- SIMULATE REBOOT: Reset all in-memory state ---
        $SCRIPT:Profile = $null
        $SCRIPT:Mode = $null
        $SCRIPT:LogLevel = $null
        $SCRIPT:DryRun = $false
        $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()

        # --- Phase 3: Load everything back ---
        $loaded = Load-State $CFG_StateFile
        Load-AppliedSteps

        # Verify state restored
        $SCRIPT:Profile  | Should -Be "COMPETITIVE"
        $SCRIPT:Mode     | Should -Be "CONTROL"
        $SCRIPT:LogLevel | Should -Be "VERBOSE"
        $SCRIPT:DryRun   | Should -Be $false
        $loaded.driverPath | Should -Be "C:\NVIDIA\561.00-desktop-win10-win11-64bit.exe"
        $loaded.nicName    | Should -Be "Realtek Gaming 2.5GbE"

        # Verify progress restored
        Test-StepDone -phase 1 -stepNum 1 | Should -Be $true
        Test-StepDone -phase 1 -stepNum 2 | Should -Be $true
        Test-StepDone -phase 1 -stepNum 3 | Should -Be $true  # Skipped counts as done for resume

        # Verify applied steps restored
        $SCRIPT:AppliedSteps | Should -Contain "Clear Shader Cache"
        $SCRIPT:AppliedSteps | Should -Contain "Power Plan"
        $SCRIPT:AppliedSteps.Count | Should -Be 2
    }
}
