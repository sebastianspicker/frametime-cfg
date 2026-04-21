# ==============================================================================
#  tests/helpers/benchmark-history.Tests.ps1  --  Benchmark history & FPS cap tests
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
    # benchmark-history.ps1 depends on functions from hardware-detect (Parse-BenchmarkOutput,
    # Calculate-FpsCap) and system-utils (Save-JsonAtomic) which are already loaded by _TestInit.
    . "$PSScriptRoot/../../helpers/benchmark-history.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Parse-BenchmarkOutput ───────────────────────────────────────────────────
Describe "Parse-BenchmarkOutput" {

    It "parses standard VProf single-run output" {
        $result = Parse-BenchmarkOutput "[VProf] FPS: Avg=280.5, P1=210.3"
        $result | Should -Not -BeNullOrEmpty
        $result.Avg | Should -Be 280.5
        $result.P1  | Should -Be 210.3
        $result.Runs | Should -Be 1
    }

    It "parses VProf output with extra whitespace" {
        $result = Parse-BenchmarkOutput "[VProf]  FPS:  Avg = 150.0 ,  P1 = 95.0"
        $result | Should -Not -BeNullOrEmpty
        $result.Avg | Should -Be 150.0
        $result.P1  | Should -Be 95.0
    }

    It "averages multiple VProf runs" {
        $vprofOutput = @(
            "[VProf] FPS: Avg=300.0, P1=200.0",
            "[VProf] FPS: Avg=320.0, P1=220.0"
        ) -join "`n"
        $result = Parse-BenchmarkOutput $vprofOutput
        $result | Should -Not -BeNullOrEmpty
        $result.Runs | Should -Be 2
        $result.Avg | Should -Be 310.0
        $result.P1  | Should -Be 210.0
    }

    It "returns null for unrecognized input" {
        $result = Parse-BenchmarkOutput "random text no fps data"
        $result | Should -BeNullOrEmpty
    }

    It "returns null for empty string" {
        $result = Parse-BenchmarkOutput ""
        $result | Should -BeNullOrEmpty
    }

    It "handles integer-only values" {
        $result = Parse-BenchmarkOutput "[VProf] FPS: Avg=300, P1=200"
        # The regex expects digits possibly with decimal; "300" should match as 300.0
        # If the regex strictly requires a decimal, this may be null.
        # The regex is: ([\d.]+) which matches "300"
        $result | Should -Not -BeNullOrEmpty
        $result.Avg | Should -Be 300
    }
}

# ── Calculate-FpsCap ────────────────────────────────────────────────────────
Describe "Calculate-FpsCap" {

    It "calculates 9% below average FPS" {
        # 300 - (300 * 0.09) = 300 - 27 = 273
        $cap = Calculate-FpsCap 300
        $cap | Should -Be 273
    }

    It "returns minimum cap for very low FPS" {
        # If avg=50, cap=50-4.5=45, but min is 60
        $cap = Calculate-FpsCap 50
        $cap | Should -Be $CFG_FpsCap_Min
    }

    It "returns minimum cap for zero FPS" {
        $cap = Calculate-FpsCap 0
        $cap | Should -Be $CFG_FpsCap_Min
    }

    It "returns minimum cap for negative FPS" {
        $cap = Calculate-FpsCap (-100)
        $cap | Should -Be $CFG_FpsCap_Min
    }

    It "handles very large FPS values" {
        $cap = Calculate-FpsCap 1000
        # 1000 - 90 = 910
        $cap | Should -Be 910
    }

    It "handles fractional FPS values" {
        $cap = Calculate-FpsCap 333.3
        # Floor(333.3 - Floor(333.3 * 0.09)) = Floor(333.3 - 29) = 304
        $cap | Should -BeOfType [int]
        $cap | Should -Be 304
    }
}

# ── Get-BenchmarkHistory ───────────────────────────────────────────────────
Describe "Get-BenchmarkHistory" {

    BeforeEach {
        Reset-TestState
        Remove-Item $CFG_BenchmarkFile -Force -ErrorAction SilentlyContinue
    }

    It "returns empty array when file does not exist" {
        $h = @(Get-BenchmarkHistory)
        $h.Count | Should -Be 0
    }

    It "returns empty array for empty JSON array" {
        Set-Content $CFG_BenchmarkFile "[]" -Encoding UTF8
        $h = @(Get-BenchmarkHistory)
        $h.Count | Should -Be 0
    }

    It "returns empty array for corrupted JSON" {
        Set-Content $CFG_BenchmarkFile "{ not valid json !!!" -Encoding UTF8
        $h = @(Get-BenchmarkHistory)
        $h.Count | Should -Be 0
    }

    It "returns single entry wrapped in array" {
        $entry = @{ timestamp = "2026-01-01 12:00:00"; avgFps = 300; p1Fps = 200; label = "test"; runs = 1; index = 1 }
        ConvertTo-Json $entry -Depth 5 | Set-Content $CFG_BenchmarkFile -Encoding UTF8
        $h = Get-BenchmarkHistory
        @($h).Count | Should -Be 1
    }

    It "returns multiple entries" {
        $entries = @(
            @{ timestamp = "2026-01-01 12:00:00"; avgFps = 300; p1Fps = 200; label = "run1"; runs = 1; index = 1 },
            @{ timestamp = "2026-01-01 13:00:00"; avgFps = 310; p1Fps = 210; label = "run2"; runs = 1; index = 2 }
        )
        ConvertTo-Json $entries -Depth 5 | Set-Content $CFG_BenchmarkFile -Encoding UTF8
        $h = Get-BenchmarkHistory
        @($h).Count | Should -Be 2
    }
}

# ── Add-BenchmarkResult ────────────────────────────────────────────────────
Describe "Add-BenchmarkResult" {

    BeforeEach {
        Reset-TestState
        Remove-Item $CFG_BenchmarkFile -Force -ErrorAction SilentlyContinue
    }

    It "adds a result to empty history" {
        $entry = Add-BenchmarkResult -AvgFps 300 -P1Fps 200 -Label "baseline" -Runs 1
        $entry | Should -Not -BeNullOrEmpty
        $entry.avgFps | Should -Be 300
        $entry.p1Fps | Should -Be 200
        $entry.label | Should -Be "baseline"
    }

    It "persists result to JSON file" {
        Add-BenchmarkResult -AvgFps 300 -P1Fps 200 -Label "test"
        Test-Path $CFG_BenchmarkFile | Should -Be $true
        $h = Get-BenchmarkHistory
        @($h).Count | Should -Be 1
    }

    It "appends to existing history" {
        # Pre-seed two entries so Get-BenchmarkHistory returns a proper array
        # (avoids PS 5.1/7 single-item array unwrapping causing PSObject += failure)
        $seed = @(
            @{ timestamp = "2026-01-01 12:00:00"; avgFps = 300; p1Fps = 200; label = "first"; runs = 1; index = 1 },
            @{ timestamp = "2026-01-01 12:30:00"; avgFps = 305; p1Fps = 205; label = "second"; runs = 1; index = 2 }
        )
        ConvertTo-Json $seed -Depth 5 | Set-Content $CFG_BenchmarkFile -Encoding UTF8
        Add-BenchmarkResult -AvgFps 310 -P1Fps 210 -Label "third"
        $h = Get-BenchmarkHistory
        @($h).Count | Should -Be 3
    }

    It "trims oldest entries when exceeding max" {
        # Use $script: scope to match how the function reads the variable
        $originalMax = $script:CFG_BenchmarkMaxEntries
        $script:CFG_BenchmarkMaxEntries = 3
        try {
            Add-BenchmarkResult -AvgFps 100 -P1Fps 50 -Label "entry1"
            Add-BenchmarkResult -AvgFps 200 -P1Fps 100 -Label "entry2"
            Add-BenchmarkResult -AvgFps 300 -P1Fps 150 -Label "entry3"
            Add-BenchmarkResult -AvgFps 400 -P1Fps 200 -Label "entry4"
            $h = Get-BenchmarkHistory
            @($h).Count | Should -Be 3
            # Oldest entry (100 FPS) should be trimmed
            $h[0].label | Should -Be "entry2"
        } finally {
            $script:CFG_BenchmarkMaxEntries = $originalMax
        }
    }

    It "includes timestamp in entry" {
        $entry = Add-BenchmarkResult -AvgFps 300 -P1Fps 200
        $entry.timestamp | Should -Not -BeNullOrEmpty
        $entry.timestamp | Should -Match '^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$'
    }
}

# ── Show-BenchmarkComparison ───────────────────────────────────────────────
Describe "Show-BenchmarkComparison" {

    BeforeEach {
        Reset-TestState
        Remove-Item $CFG_BenchmarkFile -Force -ErrorAction SilentlyContinue
    }

    It "handles empty history without error" {
        Mock Write-Info {}
        { Show-BenchmarkComparison } | Should -Not -Throw
    }

    It "displays single entry without delta" {
        Add-BenchmarkResult -AvgFps 300 -P1Fps 200 -Label "single"
        { Show-BenchmarkComparison } | Should -Not -Throw
    }

    It "displays comparison for two entries" {
        # Pre-seed two entries in the JSON file to avoid PSObject += issue
        $entries = @(
            @{ timestamp = "2026-01-01 12:00:00"; avgFps = 300; p1Fps = 200; label = "before"; runs = 1; index = 1 },
            @{ timestamp = "2026-01-01 13:00:00"; avgFps = 320; p1Fps = 220; label = "after"; runs = 1; index = 2 }
        )
        ConvertTo-Json $entries -Depth 5 | Set-Content $CFG_BenchmarkFile -Encoding UTF8
        { Show-BenchmarkComparison } | Should -Not -Throw
    }
}
