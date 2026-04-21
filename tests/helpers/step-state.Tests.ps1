# ==============================================================================
#  tests/helpers/step-state.Tests.ps1  --  Step progress & resume system tests
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Complete-Step / Test-StepDone round-trip ──────────────────────────────────
Describe "Complete-Step / Test-StepDone round-trip" {

    BeforeEach { Reset-TestState }

    It "marks a step as done and Test-StepDone confirms it" {
        Complete-Step -phase 1 -stepNum 5 -stepName "Test Step 5"

        Test-StepDone -phase 1 -stepNum 5 | Should -Be $true
    }

    It "uses composite key format P1:5" {
        Complete-Step -phase 1 -stepNum 5 -stepName "Step 5"

        $prog = Load-Progress
        $prog.completedSteps | Should -Contain "P1:5"
    }

    It "does not mark step done in DRY-RUN mode" {
        $SCRIPT:DryRun = $true
        Mock Write-DebugLog {}

        Complete-Step -phase 1 -stepNum 3 -stepName "DryRun Step"

        Test-StepDone -phase 1 -stepNum 3 | Should -Be $false
    }

    It "handles multiple steps in sequence" {
        Complete-Step -phase 1 -stepNum 1 -stepName "Step 1"
        Complete-Step -phase 1 -stepNum 2 -stepName "Step 2"
        Complete-Step -phase 1 -stepNum 3 -stepName "Step 3"

        Test-StepDone -phase 1 -stepNum 1 | Should -Be $true
        Test-StepDone -phase 1 -stepNum 2 | Should -Be $true
        Test-StepDone -phase 1 -stepNum 3 | Should -Be $true
        Test-StepDone -phase 1 -stepNum 4 | Should -Be $false
    }

    It "updates lastCompletedStep to latest" {
        Complete-Step -phase 1 -stepNum 5 -stepName "Step 5"
        Complete-Step -phase 1 -stepNum 10 -stepName "Step 10"

        $prog = Load-Progress
        $prog.lastCompletedStep | Should -Be 10
    }

    It "does not duplicate steps when called twice" {
        Complete-Step -phase 1 -stepNum 5 -stepName "Step 5"
        Complete-Step -phase 1 -stepNum 5 -stepName "Step 5"

        $prog = Load-Progress
        @($prog.completedSteps | Where-Object { $_ -eq "P1:5" }).Count | Should -Be 1
    }

    It "records timestamp for the step" {
        Complete-Step -phase 1 -stepNum 7 -stepName "Timed Step"

        $prog = Load-Progress
        # Timestamp key is "{phase}-{step}"
        $ts = $prog.timestamps."1-7"
        $ts | Should -Not -BeNullOrEmpty
        # Should be a valid date string
        { [datetime]::ParseExact($ts, "yyyy-MM-dd HH:mm:ss", [System.Globalization.CultureInfo]::InvariantCulture) } | Should -Not -Throw
    }
}

# ── Phase key format collision avoidance ─────────────────────────────────────
Describe "Phase key format (P{phase}:{step})" {

    BeforeEach { Reset-TestState }

    It "Phase 1 Step 5 and Phase 3 Step 5 do not collide" {
        Complete-Step -phase 1 -stepNum 5 -stepName "Phase 1 Step 5"
        Complete-Step -phase 3 -stepNum 5 -stepName "Phase 3 Step 5"

        $prog = Load-Progress
        $prog.completedSteps | Should -Contain "P1:5"
        $prog.completedSteps | Should -Contain "P3:5"

        # Each is independently queryable
        Test-StepDone -phase 1 -stepNum 5 | Should -Be $true
        Test-StepDone -phase 3 -stepNum 5 | Should -Be $true

        # Phase 2 Step 5 should NOT be done
        Test-StepDone -phase 2 -stepNum 5 | Should -Be $false
    }

    It "does not match legacy bare step numbers" {
        # Simulate a legacy progress file with bare step numbers (no prefix)
        $legacy = [PSCustomObject]@{
            phase = 1; lastCompletedStep = 5;
            completedSteps = @("1","2","3","4","5");
            skippedSteps = @();
            timestamps = @{}
        }
        $legacy | ConvertTo-Json -Depth 5 | Set-Content $CFG_ProgressFile -Encoding UTF8

        # Test-StepDone should NOT find bare "5" when looking for "P1:5"
        Test-StepDone -phase 1 -stepNum 5 | Should -Be $false
    }
}

# ── Skip-Step ────────────────────────────────────────────────────────────────
Describe "Skip-Step" {

    BeforeEach { Reset-TestState }

    It "records step as skipped" {
        Skip-Step -phase 1 -stepNum 4 -stepName "Skipped Step 4"

        $prog = Load-Progress
        $prog.skippedSteps | Should -Contain "P1:4"
    }

    It "skipped steps count as 'done' for resume purposes" {
        Skip-Step -phase 1 -stepNum 4 -stepName "Skipped"

        Test-StepDone -phase 1 -stepNum 4 | Should -Be $true
    }

    It "does not add skipped step to completedSteps" {
        Skip-Step -phase 1 -stepNum 4 -stepName "Skipped"

        $prog = Load-Progress
        $prog.completedSteps | Should -Not -Contain "P1:4"
    }

    It "tracks lastSkippedStep separately from lastCompletedStep" {
        Complete-Step -phase 1 -stepNum 3 -stepName "Done"
        Skip-Step -phase 1 -stepNum 4 -stepName "Skipped"

        $prog = Load-Progress
        # Skip-Step no longer advances lastCompletedStep — it tracks lastSkippedStep separately
        $prog.lastCompletedStep | Should -Be 3
        $prog.lastSkippedStep | Should -Be 4
    }

    It "does not record in DRY-RUN mode" {
        $SCRIPT:DryRun = $true
        Mock Write-DebugLog {}

        Skip-Step -phase 1 -stepNum 4 -stepName "DryRun Skip"

        Test-StepDone -phase 1 -stepNum 4 | Should -Be $false
    }

    It "does not duplicate when called twice" {
        Skip-Step -phase 1 -stepNum 4 -stepName "Skip"
        Skip-Step -phase 1 -stepNum 4 -stepName "Skip"

        $prog = Load-Progress
        @($prog.skippedSteps | Where-Object { $_ -eq "P1:4" }).Count | Should -Be 1
    }
}

# ── Clear-Progress ───────────────────────────────────────────────────────────
Describe "Clear-Progress" {

    BeforeEach { Reset-TestState }

    It "resets progress for matching phase" {
        Complete-Step -phase 1 -stepNum 5 -stepName "Step 5"
        Complete-Step -phase 1 -stepNum 10 -stepName "Step 10"

        Clear-Progress 1

        $prog = Load-Progress
        if ($prog) {
            $prog.completedSteps.Count | Should -Be 0
            $prog.lastCompletedStep    | Should -Be 0
        } else {
            # File was deleted — verify it no longer exists
            Test-Path $CFG_ProgressFile | Should -Be $false
        }
    }

    It "resets progress even for a different phase (cross-phase re-run)" {
        Complete-Step -phase 1 -stepNum 5 -stepName "P1 Step 5"

        Clear-Progress 3

        # Cross-phase reset: progress should be cleared so the new phase starts fresh
        Test-StepDone -phase 1 -stepNum 5 | Should -Be $false
    }

    It "resets all progress when phase is null" {
        Complete-Step -phase 1 -stepNum 5 -stepName "Step 5"

        Clear-Progress

        $prog = Load-Progress
        if ($prog) {
            $prog.completedSteps.Count | Should -Be 0
        } else {
            Test-Path $CFG_ProgressFile | Should -Be $false
        }
    }

    It "is safe to call when no progress file exists" {
        { Clear-Progress 1 } | Should -Not -Throw
    }
}

# ── Load-Progress with corrupted JSON ────────────────────────────────────────
Describe "Load-Progress with corrupted JSON" {

    BeforeEach { Reset-TestState }

    It "returns null for corrupted JSON" {
        "this is {{{ not valid json !!!" | Set-Content $CFG_ProgressFile -Encoding UTF8

        Mock Write-Warn {}

        $result = Load-Progress
        $result | Should -BeNullOrEmpty
    }

    It "preserves corrupted file as .corrupt" {
        "corrupted data here" | Set-Content $CFG_ProgressFile -Encoding UTF8

        Mock Write-Warn {}
        Mock Write-DebugLog {}

        Load-Progress | Out-Null

        Test-Path "$CFG_ProgressFile.corrupt" | Should -Be $true
    }

    It "returns null when file does not exist" {
        $result = Load-Progress
        $result | Should -BeNullOrEmpty
    }

    It "handles empty file gracefully" {
        "" | Set-Content $CFG_ProgressFile -Encoding UTF8

        Mock Write-Warn {}
        Mock Write-DebugLog {}

        # Empty content will fail ConvertFrom-Json
        $result = Load-Progress
        $result | Should -BeNullOrEmpty
    }
}

# ── Integration: Complete + Skip + Resume ────────────────────────────────────
Describe "Integration: mixed complete and skip steps" {

    BeforeEach { Reset-TestState }

    It "both completed and skipped steps are 'done' for resume" {
        Complete-Step -phase 1 -stepNum 1 -stepName "Done 1"
        Complete-Step -phase 1 -stepNum 2 -stepName "Done 2"
        Skip-Step     -phase 1 -stepNum 3 -stepName "Skipped 3"
        Complete-Step -phase 1 -stepNum 4 -stepName "Done 4"

        Test-StepDone -phase 1 -stepNum 1 | Should -Be $true
        Test-StepDone -phase 1 -stepNum 2 | Should -Be $true
        Test-StepDone -phase 1 -stepNum 3 | Should -Be $true
        Test-StepDone -phase 1 -stepNum 4 | Should -Be $true
        Test-StepDone -phase 1 -stepNum 5 | Should -Be $false

        $prog = Load-Progress
        $prog.completedSteps | Should -Contain "P1:1"
        $prog.completedSteps | Should -Contain "P1:2"
        $prog.completedSteps | Should -Not -Contain "P1:3"
        $prog.skippedSteps   | Should -Contain "P1:3"
        $prog.completedSteps | Should -Contain "P1:4"
    }
}
