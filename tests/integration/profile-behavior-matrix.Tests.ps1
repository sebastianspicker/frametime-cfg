# ==============================================================================
#  tests/integration/profile-behavior-matrix.Tests.ps1
#  Test all profile x risk x tier combinations for correct behavior.
# ==============================================================================
#
#  The behavior matrix from tier-system.ps1:
#  ┌──────────────┬──────────┬──────────────────────────────┬──────────────────┐
#  │ Profile      │ T1       │ T2                           │ T3               │
#  ├──────────────┼──────────┼──────────────────────────────┼──────────────────┤
#  │ SAFE         │ auto     │ SAFE->auto, MODERATE+->skip  │ skip             │
#  │ RECOMMENDED  │ auto     │ <=MODERATE->prompted,else skip│ skip            │
#  │ COMPETITIVE  │ auto     │ <=AGGRESSIVE->prompted       │ <=AGGRESSIVE->ask│
#  │ CUSTOM       │ prompted │ prompted (full card)         │ prompted         │
#  └──────────────┴──────────┴──────────────────────────────┴──────────────────┘
#
#  Expected behaviors:
#    APPLY   = step runs automatically (no prompt)
#    SKIP    = step is filtered out (not executed, no prompt)
#    PROMPT  = user is asked (we mock Read-Host to test both yes/no)

BeforeAll {
    . "$PSScriptRoot/_IntegrationInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── SAFE profile ─────────────────────────────────────────────────────────────
Describe "SAFE profile behavior matrix" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:Profile = "SAFE"
        $SCRIPT:DryRun = $false

        Mock Write-Blank {}
        Mock Write-TierBadge {}
        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Write-Info {}
        Mock Show-StepInfoCard {}
        Mock Flush-BackupBuffer {}
        Mock Read-Host { "n" }
    }

    # T1: always auto-apply regardless of risk
    It "T1/SAFE risk -> APPLY (auto)" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 1 -Title "T1 SAFE" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
        Should -Not -Invoke Read-Host
    }

    It "T1/MODERATE risk -> APPLY (auto, T1 ignores risk filter)" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 1 -Title "T1 MOD" -Why "Test" `
            -Risk "MODERATE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T1/AGGRESSIVE risk -> APPLY (auto, T1 ignores risk filter)" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 1 -Title "T1 AGG" -Why "Test" `
            -Risk "AGGRESSIVE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T1/CRITICAL risk -> APPLY (auto, T1 ignores risk filter)" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 1 -Title "T1 CRIT" -Why "Test" `
            -Risk "CRITICAL" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    # T2: SAFE risk auto, MODERATE+ skip
    It "T2/SAFE risk -> APPLY (auto)" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 SAFE" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
        Should -Not -Invoke Read-Host
    }

    It "T2/MODERATE risk -> SKIP" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 MOD" -Why "Test" `
            -Risk "MODERATE" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "T2/AGGRESSIVE risk -> SKIP" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 AGG" -Why "Test" `
            -Risk "AGGRESSIVE" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "T2/CRITICAL risk -> SKIP" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 CRIT" -Why "Test" `
            -Risk "CRITICAL" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    # T3: always skip in SAFE
    It "T3/SAFE risk -> SKIP" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 3 -Title "T3 SAFE" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "T3/MODERATE risk -> SKIP" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 3 -Title "T3 MOD" -Why "Test" `
            -Risk "MODERATE" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "T3/AGGRESSIVE risk -> SKIP" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 3 -Title "T3 AGG" -Why "Test" `
            -Risk "AGGRESSIVE" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "T3/CRITICAL risk -> SKIP" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 3 -Title "T3 CRIT" -Why "Test" `
            -Risk "CRITICAL" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }
}

# ── RECOMMENDED profile ──────────────────────────────────────────────────────
Describe "RECOMMENDED profile behavior matrix" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:Profile = "RECOMMENDED"
        $SCRIPT:DryRun = $false

        Mock Write-Blank {}
        Mock Write-TierBadge {}
        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Write-Info {}
        Mock Show-StepInfoCard {}
        Mock Flush-BackupBuffer {}
    }

    # T1: always auto
    It "T1/SAFE -> APPLY (auto)" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 1 -Title "T1 SAFE" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T1/CRITICAL -> APPLY (auto, T1 bypasses risk)" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 1 -Title "T1 CRIT" -Why "Test" `
            -Risk "CRITICAL" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    # T2: <= MODERATE prompted, AGGRESSIVE+ skip
    It "T2/SAFE -> PROMPT (user says yes)" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 SAFE" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T2/SAFE -> PROMPT (user says no)" {
        $state = @{ executed = $false }
        Mock Read-Host { "n" }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 SAFE no" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "T2/MODERATE -> PROMPT (user says yes)" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 MOD" -Why "Test" `
            -Risk "MODERATE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T2/AGGRESSIVE -> SKIP (exceeds RECOMMENDED threshold)" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 AGG" -Why "Test" `
            -Risk "AGGRESSIVE" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "T2/CRITICAL -> SKIP" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 CRIT" -Why "Test" `
            -Risk "CRITICAL" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    # T3: always skip in RECOMMENDED
    It "T3/SAFE -> SKIP" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 3 -Title "T3 SAFE" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "T3/MODERATE -> SKIP" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 3 -Title "T3 MOD" -Why "Test" `
            -Risk "MODERATE" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }
}

# ── COMPETITIVE profile ──────────────────────────────────────────────────────
Describe "COMPETITIVE profile behavior matrix" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:Profile = "COMPETITIVE"
        $SCRIPT:DryRun = $false

        Mock Write-Blank {}
        Mock Write-TierBadge {}
        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Write-Info {}
        Mock Show-StepInfoCard {}
        Mock Flush-BackupBuffer {}
    }

    # T1: always auto
    It "T1/SAFE -> APPLY (auto)" {
        $state = @{ executed = $false }
        $result = Invoke-TieredStep -Tier 1 -Title "T1 SAFE" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    # T2: <= AGGRESSIVE prompted
    It "T2/MODERATE -> PROMPT (yes)" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 MOD" -Why "Test" `
            -Risk "MODERATE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T2/AGGRESSIVE -> PROMPT (yes)" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 AGG" -Why "Test" `
            -Risk "AGGRESSIVE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T2/CRITICAL -> SKIP (exceeds COMPETITIVE threshold)" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 CRIT" -Why "Test" `
            -Risk "CRITICAL" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    # T3: <= AGGRESSIVE prompted
    It "T3/SAFE -> PROMPT (yes)" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 3 -Title "T3 SAFE" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T3/MODERATE -> PROMPT (yes)" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 3 -Title "T3 MOD" -Why "Test" `
            -Risk "MODERATE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T3/AGGRESSIVE -> PROMPT (no)" {
        $state = @{ executed = $false }
        Mock Read-Host { "n" }
        $result = Invoke-TieredStep -Tier 3 -Title "T3 AGG no" -Why "Test" `
            -Risk "AGGRESSIVE" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "T3/CRITICAL -> SKIP (exceeds COMPETITIVE threshold)" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 3 -Title "T3 CRIT" -Why "Test" `
            -Risk "CRITICAL" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }
}

# ── CUSTOM profile ───────────────────────────────────────────────────────────
Describe "CUSTOM profile behavior matrix" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:Profile = "CUSTOM"
        $SCRIPT:DryRun = $false

        Mock Write-Blank {}
        Mock Write-TierBadge {}
        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Write-Info {}
        Mock Show-StepInfoCard {}
        Mock Flush-BackupBuffer {}
    }

    # CUSTOM: everything prompted, T1 defaults to yes

    It "T1/SAFE -> PROMPT (default yes, enter)" {
        $state = @{ executed = $false }
        Mock Read-Host { "" }  # empty = default yes for T1
        $result = Invoke-TieredStep -Tier 1 -Title "T1 SAFE" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T1/SAFE -> PROMPT (user says no)" {
        $state = @{ executed = $false }
        Mock Read-Host { "n" }
        $result = Invoke-TieredStep -Tier 1 -Title "T1 SAFE no" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "T1/CRITICAL -> PROMPT (everything allowed in CUSTOM)" {
        $state = @{ executed = $false }
        Mock Read-Host { "" }
        $result = Invoke-TieredStep -Tier 1 -Title "T1 CRIT" -Why "Test" `
            -Risk "CRITICAL" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T2/AGGRESSIVE -> PROMPT (yes)" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 AGG" -Why "Test" `
            -Risk "AGGRESSIVE" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T2/CRITICAL -> PROMPT (yes, CUSTOM allows everything)" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 2 -Title "T2 CRIT" -Why "Test" `
            -Risk "CRITICAL" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T3/CRITICAL -> PROMPT (yes, CUSTOM allows everything)" {
        $state = @{ executed = $false }
        Mock Read-Host { "y" }
        $result = Invoke-TieredStep -Tier 3 -Title "T3 CRIT" -Why "Test" `
            -Risk "CRITICAL" -Action { $state.executed = $true }
        $result | Should -Be $true
        $state.executed | Should -Be $true
    }

    It "T3/SAFE -> PROMPT (no)" {
        $state = @{ executed = $false }
        Mock Read-Host { "n" }
        $result = Invoke-TieredStep -Tier 3 -Title "T3 SAFE no" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $result | Should -Be $false
        $state.executed | Should -Be $false
    }

    It "CUSTOM always shows full info card for steps with metadata" {
        Mock Read-Host { "y" }
        Mock Show-StepInfoCard {} -Verifiable

        Invoke-TieredStep -Tier 1 -Title "Card Test" -Why "Testing" `
            -Risk "SAFE" -Improvement "+5% 1% lows" -Action { }

        Should -InvokeVerifiable
    }
}

# ── Cross-profile: SkipAction callback fires on SKIP ────────────────────────
Describe "SkipAction callback fires correctly across profiles" {

    BeforeEach {
        Reset-IntegrationState
        $SCRIPT:DryRun = $false

        Mock Write-Blank {}
        Mock Write-TierBadge {}
        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Write-Info {}
        Mock Show-StepInfoCard {}
        Mock Flush-BackupBuffer {}
    }

    It "SkipAction called for T2/MODERATE in SAFE profile" {
        $SCRIPT:Profile = "SAFE"
        $state = @{ skipCalled = $false }

        Invoke-TieredStep -Tier 2 -Title "Skip Test" -Why "Test" `
            -Risk "MODERATE" -Action { } -SkipAction { $state.skipCalled = $true }

        $state.skipCalled | Should -Be $true
    }

    It "SkipAction called for T3 in RECOMMENDED profile" {
        $SCRIPT:Profile = "RECOMMENDED"
        $state = @{ skipCalled = $false }
        Mock Read-Host { "n" }

        Invoke-TieredStep -Tier 3 -Title "Skip T3 Test" -Why "Test" `
            -Risk "SAFE" -Action { } -SkipAction { $state.skipCalled = $true }

        $state.skipCalled | Should -Be $true
    }

    It "SkipAction NOT called when step executes" {
        $SCRIPT:Profile = "SAFE"
        $state = @{ skipCalled = $false }

        Invoke-TieredStep -Tier 1 -Title "Run Test" -Why "Test" `
            -Risk "SAFE" -Action { } -SkipAction { $state.skipCalled = $true }

        $state.skipCalled | Should -Be $false
    }
}

# ── YOLO profile ────────────────────────────────────────────────────────────
Describe "YOLO profile behavior matrix" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:Profile = "YOLO"
        Mock Write-Blank {}
        Mock Write-TierBadge {}
        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Write-Info {}
        Mock Show-StepInfoCard {}
        Mock Flush-BackupBuffer {}
    }

    It "T1/SAFE auto-executes without prompt" {
        $state = @{ executed = $false }
        Mock Read-Host { throw "YOLO should not prompt" }
        Invoke-TieredStep -Tier 1 -Title "YOLO T1" -Why "Test" `
            -Risk "SAFE" -Action { $state.executed = $true }
        $state.executed | Should -Be $true
        Should -Not -Invoke Read-Host
    }

    It "T2/MODERATE auto-executes without prompt" {
        $state = @{ executed = $false }
        Mock Read-Host { throw "YOLO should not prompt" }
        Invoke-TieredStep -Tier 2 -Title "YOLO T2" -Why "Test" `
            -Risk "MODERATE" -Action { $state.executed = $true }
        $state.executed | Should -Be $true
        Should -Not -Invoke Read-Host
    }

    It "T2/AGGRESSIVE auto-executes without prompt" {
        $state = @{ executed = $false }
        Mock Read-Host { throw "YOLO should not prompt" }
        Invoke-TieredStep -Tier 2 -Title "YOLO T2 Agg" -Why "Test" `
            -Risk "AGGRESSIVE" -Action { $state.executed = $true }
        $state.executed | Should -Be $true
        Should -Not -Invoke Read-Host
    }

    It "T3/MODERATE auto-executes without prompt" {
        $state = @{ executed = $false }
        Mock Read-Host { throw "YOLO should not prompt" }
        Invoke-TieredStep -Tier 3 -Title "YOLO T3" -Why "Test" `
            -Risk "MODERATE" -Action { $state.executed = $true }
        $state.executed | Should -Be $true
        Should -Not -Invoke Read-Host
    }

    It "T2/CRITICAL is SKIPPED (exceeds AGGRESSIVE ceiling)" {
        $state = @{ executed = $false }
        Invoke-TieredStep -Tier 2 -Title "YOLO Critical" -Why "Test" `
            -Risk "CRITICAL" -Action { $state.executed = $true }
        $state.executed | Should -Be $false
    }

    It "T3/CRITICAL is SKIPPED (exceeds AGGRESSIVE ceiling)" {
        $state = @{ executed = $false }
        Invoke-TieredStep -Tier 3 -Title "YOLO T3 Critical" -Why "Test" `
            -Risk "CRITICAL" -Action { $state.executed = $true }
        $state.executed | Should -Be $false
    }
}
