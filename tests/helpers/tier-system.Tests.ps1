# ==============================================================================
#  tests/helpers/tier-system.Tests.ps1  --  Profile, tier, and risk system tests
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Test-RiskAllowed ──────────────────────────────────────────────────────────
Describe "Test-RiskAllowed" {

    BeforeEach { Reset-TestState }

    Context "SAFE profile (max risk = SAFE)" {
        BeforeEach { $SCRIPT:Profile = "SAFE" }

        It "allows SAFE risk" {
            Test-RiskAllowed -StepRisk "SAFE" | Should -Be $true
        }
        It "blocks MODERATE risk" {
            Test-RiskAllowed -StepRisk "MODERATE" | Should -Be $false
        }
        It "blocks AGGRESSIVE risk" {
            Test-RiskAllowed -StepRisk "AGGRESSIVE" | Should -Be $false
        }
        It "blocks CRITICAL risk" {
            Test-RiskAllowed -StepRisk "CRITICAL" | Should -Be $false
        }
    }

    Context "RECOMMENDED profile (max risk = MODERATE)" {
        BeforeEach { $SCRIPT:Profile = "RECOMMENDED" }

        It "allows SAFE risk" {
            Test-RiskAllowed -StepRisk "SAFE" | Should -Be $true
        }
        It "allows MODERATE risk" {
            Test-RiskAllowed -StepRisk "MODERATE" | Should -Be $true
        }
        It "blocks AGGRESSIVE risk" {
            Test-RiskAllowed -StepRisk "AGGRESSIVE" | Should -Be $false
        }
        It "blocks CRITICAL risk" {
            Test-RiskAllowed -StepRisk "CRITICAL" | Should -Be $false
        }
    }

    Context "COMPETITIVE profile (max risk = AGGRESSIVE)" {
        BeforeEach { $SCRIPT:Profile = "COMPETITIVE" }

        It "allows SAFE risk" {
            Test-RiskAllowed -StepRisk "SAFE" | Should -Be $true
        }
        It "allows MODERATE risk" {
            Test-RiskAllowed -StepRisk "MODERATE" | Should -Be $true
        }
        It "allows AGGRESSIVE risk" {
            Test-RiskAllowed -StepRisk "AGGRESSIVE" | Should -Be $true
        }
        It "blocks CRITICAL risk" {
            Test-RiskAllowed -StepRisk "CRITICAL" | Should -Be $false
        }
    }

    Context "YOLO profile (max risk = AGGRESSIVE)" {
        BeforeEach { $SCRIPT:Profile = "YOLO" }

        It "allows SAFE risk" {
            Test-RiskAllowed -StepRisk "SAFE" | Should -Be $true
        }
        It "allows MODERATE risk" {
            Test-RiskAllowed -StepRisk "MODERATE" | Should -Be $true
        }
        It "allows AGGRESSIVE risk" {
            Test-RiskAllowed -StepRisk "AGGRESSIVE" | Should -Be $true
        }
        It "blocks CRITICAL risk" {
            Test-RiskAllowed -StepRisk "CRITICAL" | Should -Be $false
        }
    }

    Context "CUSTOM profile (max risk = CRITICAL)" {
        BeforeEach { $SCRIPT:Profile = "CUSTOM" }

        It "allows SAFE risk" {
            Test-RiskAllowed -StepRisk "SAFE" | Should -Be $true
        }
        It "allows MODERATE risk" {
            Test-RiskAllowed -StepRisk "MODERATE" | Should -Be $true
        }
        It "allows AGGRESSIVE risk" {
            Test-RiskAllowed -StepRisk "AGGRESSIVE" | Should -Be $true
        }
        It "allows CRITICAL risk" {
            Test-RiskAllowed -StepRisk "CRITICAL" | Should -Be $true
        }
    }

    Context "Edge cases" {
        It "allows empty risk (no risk specified)" {
            $SCRIPT:Profile = "SAFE"
            Test-RiskAllowed -StepRisk "" | Should -Be $true
        }

        It "blocks unknown risk level for safety" {
            $SCRIPT:Profile = "CUSTOM"
            # Unknown risk should be blocked even in CUSTOM profile
            Mock Write-Warn {}
            Test-RiskAllowed -StepRisk "UNKNOWN_LEVEL" | Should -Be $false
            Should -Invoke Write-Warn -Times 1
        }

        It "is case-insensitive for profile names" {
            $SCRIPT:Profile = "safe"
            Test-RiskAllowed -StepRisk "SAFE" | Should -Be $true
        }
    }
}

# ── Get-ProfileMaxRisk ────────────────────────────────────────────────────────
Describe "Get-ProfileMaxRisk" {

    BeforeEach { Reset-TestState }

    It "SAFE -> SAFE" {
        $SCRIPT:Profile = "SAFE"
        Get-ProfileMaxRisk | Should -Be "SAFE"
    }

    It "RECOMMENDED -> MODERATE" {
        $SCRIPT:Profile = "RECOMMENDED"
        Get-ProfileMaxRisk | Should -Be "MODERATE"
    }

    It "COMPETITIVE -> AGGRESSIVE" {
        $SCRIPT:Profile = "COMPETITIVE"
        Get-ProfileMaxRisk | Should -Be "AGGRESSIVE"
    }

    It "YOLO -> AGGRESSIVE" {
        $SCRIPT:Profile = "YOLO"
        Get-ProfileMaxRisk | Should -Be "AGGRESSIVE"
    }

    It "CUSTOM -> CRITICAL" {
        $SCRIPT:Profile = "CUSTOM"
        Get-ProfileMaxRisk | Should -Be "CRITICAL"
    }

    It "unknown profile defaults to MODERATE" {
        $SCRIPT:Profile = "NONEXISTENT"
        Get-ProfileMaxRisk | Should -Be "MODERATE"
    }

    It "null profile defaults to MODERATE" {
        $SCRIPT:Profile = $null
        Get-ProfileMaxRisk | Should -Be "MODERATE"
    }
}

# ── Invoke-TieredStep ────────────────────────────────────────────────────────
Describe "Invoke-TieredStep" {

    BeforeEach {
        Reset-TestState
        # Mock all console output functions to keep test output clean
        Mock Write-Blank {}
        Mock Write-TierBadge {}
        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Write-Info {}
        Mock Show-StepInfoCard {}
        Mock Flush-BackupBuffer {}
    }

    # NOTE: Pester 5 runs each It block in its own scope. The $script: modifier
    # inside an -Action scriptblock resolves to the helpers module scope, NOT the
    # test scope.  Use a hashtable (reference type) to track execution state.

    Context "T1 steps auto-run in all profiles" {
        It "T1 auto-runs in SAFE profile" {
            $SCRIPT:Profile = "SAFE"
            $state = @{ executed = $false }

            $result = Invoke-TieredStep -Tier 1 -Title "Test T1 Step" -Why "Testing" `
                -Risk "SAFE" -Action { $state.executed = $true }

            $result          | Should -Be $true
            $state.executed  | Should -Be $true
        }

        It "T1 auto-runs in RECOMMENDED profile" {
            $SCRIPT:Profile = "RECOMMENDED"
            $state = @{ executed = $false }

            $result = Invoke-TieredStep -Tier 1 -Title "Test T1 Step" -Why "Testing" `
                -Risk "SAFE" -Action { $state.executed = $true }

            $result          | Should -Be $true
            $state.executed  | Should -Be $true
        }
    }

    Context "T2 behavior varies by profile" {
        It "T2 SAFE-risk auto-runs in SAFE profile" {
            $SCRIPT:Profile = "SAFE"
            $state = @{ executed = $false }

            $result = Invoke-TieredStep -Tier 2 -Title "Test T2 Safe" -Why "Testing" `
                -Risk "SAFE" -Action { $state.executed = $true }

            $result          | Should -Be $true
            $state.executed  | Should -Be $true
        }

        It "T2 MODERATE-risk is skipped in SAFE profile" {
            $SCRIPT:Profile = "SAFE"
            $state = @{ executed = $false }

            $result = Invoke-TieredStep -Tier 2 -Title "Test T2 Moderate" -Why "Testing" `
                -Risk "MODERATE" -Action { $state.executed = $true }

            $result          | Should -Be $false
            $state.executed  | Should -Be $false
        }

        It "T2 MODERATE-risk is prompted in RECOMMENDED profile (user says yes)" {
            $SCRIPT:Profile = "RECOMMENDED"
            $state = @{ executed = $false }
            Mock Read-Host { "y" }

            $result = Invoke-TieredStep -Tier 2 -Title "Test T2 Moderate" -Why "Testing" `
                -Risk "MODERATE" -Action { $state.executed = $true }

            $result          | Should -Be $true
            $state.executed  | Should -Be $true
        }

        It "T2 MODERATE-risk is prompted in RECOMMENDED profile (user says no)" {
            $SCRIPT:Profile = "RECOMMENDED"
            $state = @{ executed = $false }
            Mock Read-Host { "n" }

            $result = Invoke-TieredStep -Tier 2 -Title "Test T2 Moderate" -Why "Testing" `
                -Risk "MODERATE" -Action { $state.executed = $true }

            $result          | Should -Be $false
            $state.executed  | Should -Be $false
        }
    }

    Context "T3 steps" {
        It "T3 is skipped in SAFE profile via risk filter" {
            $SCRIPT:Profile = "SAFE"
            $state = @{ executed = $false }

            $result = Invoke-TieredStep -Tier 3 -Title "Test T3" -Why "Testing" `
                -Risk "MODERATE" -Action { $state.executed = $true }

            $result          | Should -Be $false
            $state.executed  | Should -Be $false
        }

        It "T3 is prompted in COMPETITIVE profile (user says yes)" {
            $SCRIPT:Profile = "COMPETITIVE"
            $state = @{ executed = $false }
            Mock Read-Host { "y" }

            $result = Invoke-TieredStep -Tier 3 -Title "Test T3" -Why "Testing" `
                -Risk "MODERATE" -Action { $state.executed = $true }

            $result          | Should -Be $true
            $state.executed  | Should -Be $true
        }
    }

    Context "DRY-RUN modifier" {
        It "DRY-RUN runs action but returns false (preview only)" {
            $SCRIPT:Profile = "RECOMMENDED"
            $SCRIPT:DryRun = $true
            $state = @{ executed = $false }

            $result = Invoke-TieredStep -Tier 1 -Title "Test DryRun" -Why "Testing" `
                -Risk "SAFE" -Action { $state.executed = $true }

            $result          | Should -Be $false
            $state.executed  | Should -Be $true
        }

        It "DRY-RUN does not record EstimateKey" {
            $SCRIPT:Profile = "RECOMMENDED"
            $SCRIPT:DryRun = $true
            $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()

            Invoke-TieredStep -Tier 1 -Title "Test DryRun" -Why "Testing" `
                -Risk "SAFE" -EstimateKey "Test Key" -Action { }

            $SCRIPT:AppliedSteps | Should -Not -Contain "Test Key"
        }

        It "DRY-RUN skips steps filtered by SAFE profile" {
            $SCRIPT:Profile = "SAFE"
            $SCRIPT:DryRun = $true
            $state = @{ executed = $false }

            $result = Invoke-TieredStep -Tier 3 -Title "Test DryRun Skip" -Why "Testing" `
                -Risk "MODERATE" -Action { $state.executed = $true }

            $result          | Should -Be $false
            $state.executed  | Should -Be $false
        }
    }

    Context "EstimateKey tracking" {
        It "records EstimateKey on successful execution" {
            $SCRIPT:Profile = "SAFE"
            $SCRIPT:DryRun = $false
            $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()

            Invoke-TieredStep -Tier 1 -Title "Test Step" -Why "Testing" `
                -Risk "SAFE" -EstimateKey "Clear Shader Cache" -Action { }

            $SCRIPT:AppliedSteps | Should -Contain "Clear Shader Cache"
        }

        It "does not record EstimateKey when step fails" {
            $SCRIPT:Profile = "SAFE"
            $SCRIPT:DryRun = $false
            $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()
            Mock Write-Warn {}

            Invoke-TieredStep -Tier 1 -Title "Failing Step" -Why "Testing" `
                -Risk "SAFE" -EstimateKey "Should Not Record" -Action { throw "Intentional failure" }

            $SCRIPT:AppliedSteps | Should -Not -Contain "Should Not Record"
        }
    }

    Context "CUSTOM profile" {
        It "prompts for T1 in CUSTOM profile (default yes, user presses enter)" {
            $SCRIPT:Profile = "CUSTOM"
            $state = @{ executed = $false }
            Mock Read-Host { "" }  # empty = default yes for T1

            $result = Invoke-TieredStep -Tier 1 -Title "Test Custom T1" -Why "Testing" `
                -Risk "SAFE" -Action { $state.executed = $true }

            $result          | Should -Be $true
            $state.executed  | Should -Be $true
        }

        It "prompts for T1 in CUSTOM profile (user says no)" {
            $SCRIPT:Profile = "CUSTOM"
            $state = @{ executed = $false }
            Mock Read-Host { "n" }

            $result = Invoke-TieredStep -Tier 1 -Title "Test Custom T1" -Why "Testing" `
                -Risk "SAFE" -Action { $state.executed = $true }

            $result          | Should -Be $false
            $state.executed  | Should -Be $false
        }
    }

    Context "YOLO profile (auto-execute all, no prompts)" {
        It "T1 auto-runs in YOLO profile without Read-Host" {
            $SCRIPT:Profile = "YOLO"
            $state = @{ executed = $false }
            Mock Read-Host { throw "Read-Host should not be called in YOLO" }

            $result = Invoke-TieredStep -Tier 1 -Title "Test YOLO T1" -Why "Testing" `
                -Risk "SAFE" -Action { $state.executed = $true }

            $result          | Should -Be $true
            $state.executed  | Should -Be $true
            Should -Not -Invoke Read-Host
        }

        It "T2 MODERATE auto-runs in YOLO profile without Read-Host" {
            $SCRIPT:Profile = "YOLO"
            $state = @{ executed = $false }
            Mock Read-Host { throw "Read-Host should not be called in YOLO" }

            $result = Invoke-TieredStep -Tier 2 -Title "Test YOLO T2" -Why "Testing" `
                -Risk "MODERATE" -Action { $state.executed = $true }

            $result          | Should -Be $true
            $state.executed  | Should -Be $true
            Should -Not -Invoke Read-Host
        }

        It "T3 AGGRESSIVE auto-runs in YOLO profile without Read-Host" {
            $SCRIPT:Profile = "YOLO"
            $state = @{ executed = $false }
            Mock Read-Host { throw "Read-Host should not be called in YOLO" }

            $result = Invoke-TieredStep -Tier 3 -Title "Test YOLO T3" -Why "Testing" `
                -Risk "AGGRESSIVE" -Action { $state.executed = $true }

            $result          | Should -Be $true
            $state.executed  | Should -Be $true
            Should -Not -Invoke Read-Host
        }

        It "T2 CRITICAL is skipped in YOLO profile (exceeds AGGRESSIVE ceiling)" {
            $SCRIPT:Profile = "YOLO"
            $state = @{ executed = $false }

            $result = Invoke-TieredStep -Tier 2 -Title "Test YOLO Critical" -Why "Testing" `
                -Risk "CRITICAL" -Action { $state.executed = $true }

            $result          | Should -Be $false
            $state.executed  | Should -Be $false
        }
    }

    Context "SkipAction callback" {
        It "calls SkipAction when step is skipped" {
            $SCRIPT:Profile = "SAFE"
            $state = @{ skipCalled = $false }

            Invoke-TieredStep -Tier 2 -Title "Skipped Step" -Why "Testing" `
                -Risk "MODERATE" `
                -Action { } `
                -SkipAction { $state.skipCalled = $true }

            $state.skipCalled | Should -Be $true
        }
    }
}

# ── Test-YoloProfile ─────────────────────────────────────────────────────────
Describe "Test-YoloProfile" {
    BeforeEach { Reset-TestState }

    It "returns true when profile is YOLO" {
        $SCRIPT:Profile = "YOLO"
        Test-YoloProfile | Should -Be $true
    }

    It "returns false for other profiles" {
        $SCRIPT:Profile = "RECOMMENDED"
        Test-YoloProfile | Should -Be $false
    }
}

# ── Get-ImprovementEstimate ──────────────────────────────────────────────────
Describe "Get-ImprovementEstimate" {

    BeforeEach {
        Reset-TestState
    }

    It "returns zeros with no applied steps" {
        $result = Get-ImprovementEstimate
        $result.P1LowMin | Should -Be 0
        $result.P1LowMax | Should -Be 0
        $result.Count     | Should -Be 0
    }

    It "sums estimates for multiple applied steps" {
        $SCRIPT:AppliedSteps.Add("Clear Shader Cache")
        $SCRIPT:AppliedSteps.Add("Fullscreen Optimizations")

        $result = Get-ImprovementEstimate
        $result.Count    | Should -Be 2
        # Clear Shader Cache: P1 0-5, Fullscreen: P1 1-5
        $result.P1LowMin | Should -Be 1
        $result.P1LowMax | Should -Be 10
    }

    It "ignores unknown estimate keys gracefully" {
        $SCRIPT:AppliedSteps.Add("NonexistentKey")
        $SCRIPT:AppliedSteps.Add("Clear Shader Cache")

        $result = Get-ImprovementEstimate
        $result.Count | Should -Be 1  # Only the valid key counts
    }

    It "handles negative AvgMin values (FPS Cap reduces avg)" {
        $SCRIPT:AppliedSteps.Add("FPS Cap")

        $result = Get-ImprovementEstimate
        $result.AvgMin | Should -BeLessThan 0
    }
}

# ── Save-AppliedSteps / Load-AppliedSteps round-trip ─────────────────────────
Describe "Save-AppliedSteps / Load-AppliedSteps" {

    BeforeEach {
        Reset-TestState
        Mock Write-DebugLog {}
        Mock Write-Warn {}
    }

    It "round-trips applied steps through state.json" {
        # Create a state file first (Save-AppliedSteps requires it to exist)
        New-TestStateFile -TestProfile "RECOMMENDED"

        $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()
        $SCRIPT:AppliedSteps.Add("Clear Shader Cache")
        $SCRIPT:AppliedSteps.Add("FPS Cap")
        Save-AppliedSteps | Should -Be $true

        # Reset and reload
        $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()
        Load-AppliedSteps

        $SCRIPT:AppliedSteps | Should -Contain "Clear Shader Cache"
        $SCRIPT:AppliedSteps | Should -Contain "FPS Cap"
        $SCRIPT:AppliedSteps.Count | Should -Be 2
    }

    It "does not duplicate steps on repeated loads" {
        New-TestStateFile -TestProfile "RECOMMENDED"

        $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()
        $SCRIPT:AppliedSteps.Add("Clear Shader Cache")
        Save-AppliedSteps | Should -Be $true

        # Load twice
        $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()
        Load-AppliedSteps
        Load-AppliedSteps

        $SCRIPT:AppliedSteps.Count | Should -Be 1
    }

    It "does nothing when state file does not exist" {
        $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()
        # No state file created
        Save-AppliedSteps | Should -Be $true
        { Load-AppliedSteps } | Should -Not -Throw
        $SCRIPT:AppliedSteps.Count | Should -Be 0
    }

    It "returns false and warns when persistence fails" {
        New-TestStateFile -TestProfile "RECOMMENDED" | Out-Null
        $SCRIPT:AppliedSteps = [System.Collections.Generic.List[string]]::new()
        $SCRIPT:AppliedSteps.Add("Clear Shader Cache")
        Mock Save-JsonAtomic { throw "disk full" }

        $result = Save-AppliedSteps

        $result | Should -Be $false
        Should -Invoke Write-Warn -Times 1
    }
}
