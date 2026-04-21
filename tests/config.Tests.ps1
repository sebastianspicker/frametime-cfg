# ==============================================================================
#  tests/config.Tests.ps1  --  Configuration integrity & cross-reference tests
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"

    # Resolve project root for file scanning
    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── EstimateKey cross-reference ──────────────────────────────────────────────
Describe "EstimateKey cross-reference" {

    It "every EstimateKey used in phase scripts exists in CFG_ImprovementEstimates" {
        # Scan all .ps1 files for -EstimateKey "..." patterns
        $phaseScripts = @(
            "$script:ProjectRoot/Optimize-SystemBase.ps1",
            "$script:ProjectRoot/Optimize-RegistryTweaks.ps1",
            "$script:ProjectRoot/Optimize-Hardware.ps1",
            "$script:ProjectRoot/Optimize-GameConfig.ps1",
            "$script:ProjectRoot/PostReboot-Setup.ps1"
        )

        $usedKeys = @()
        foreach ($file in $phaseScripts) {
            if (-not (Test-Path $file)) { continue }
            $content = Get-Content $file -Raw
            $regexHits = [regex]::Matches($content, '-EstimateKey\s+"([^"]+)"')
            foreach ($m in $regexHits) {
                $usedKeys += $m.Groups[1].Value
            }
        }

        $usedKeys.Count | Should -BeGreaterThan 0 -Because "at least one EstimateKey should be used in phase scripts"

        $missingKeys = @()
        foreach ($key in $usedKeys) {
            if (-not $CFG_ImprovementEstimates.ContainsKey($key)) {
                $missingKeys += $key
            }
        }

        $missingKeys | Should -BeNullOrEmpty -Because "all EstimateKey values in phase scripts must have matching entries in `$CFG_ImprovementEstimates. Missing: $($missingKeys -join ', ')"
    }

    It "CFG_ImprovementEstimates entries have required fields" {
        foreach ($key in $CFG_ImprovementEstimates.Keys) {
            $est = $CFG_ImprovementEstimates[$key]
            $est.Keys | Should -Contain "P1LowMin" -Because "$key must have P1LowMin"
            $est.Keys | Should -Contain "P1LowMax" -Because "$key must have P1LowMax"
            $est.Keys | Should -Contain "AvgMin" -Because "$key must have AvgMin"
            $est.Keys | Should -Contain "AvgMax" -Because "$key must have AvgMax"
            $est.Keys | Should -Contain "Confidence" -Because "$key must have Confidence"
        }
    }

    It "P1LowMin <= P1LowMax for all estimates" {
        foreach ($key in $CFG_ImprovementEstimates.Keys) {
            $est = $CFG_ImprovementEstimates[$key]
            $est.P1LowMin | Should -BeLessOrEqual $est.P1LowMax -Because "${key}: P1LowMin ($($est.P1LowMin)) should be <= P1LowMax ($($est.P1LowMax))"
        }
    }

    It "Confidence values are valid (HIGH, MEDIUM, LOW)" {
        $validConfidence = @("HIGH", "MEDIUM", "LOW")
        foreach ($key in $CFG_ImprovementEstimates.Keys) {
            $est = $CFG_ImprovementEstimates[$key]
            $est.Confidence | Should -BeIn $validConfidence -Because "$key has invalid Confidence: $($est.Confidence)"
        }
    }
}

# ── CS2 Autoexec duplicate key check ────────────────────────────────────────
Describe "CS2 Autoexec configuration" {

    It "has no duplicate CVar keys" {
        # $CFG_CS2_Autoexec is an [ordered] hashtable, so duplicates would be
        # caught at parse time. But we verify no silent overwrite occurred.
        $keys = @($CFG_CS2_Autoexec.Keys)
        $uniqueKeys = @($keys | Select-Object -Unique)

        $keys.Count | Should -Be $uniqueKeys.Count -Because "duplicate CVar keys would silently overwrite earlier values"
    }

    It "has at least 50 CVars defined" {
        # The project documents 73 CVars; this is a lower-bound sanity check
        $CFG_CS2_Autoexec.Count | Should -BeGreaterOrEqual 50 -Because "suite documents 73 CVars across 10 categories"
    }

    It "all CVar values are strings (autoexec format requirement)" {
        foreach ($key in $CFG_CS2_Autoexec.Keys) {
            $val = $CFG_CS2_Autoexec[$key]
            $val | Should -BeOfType [string] -Because "CVar '$key' value must be a string for autoexec.cfg output"
        }
    }

    It "rate value is 1000000 (CS2 actual max)" {
        $CFG_CS2_Autoexec["rate"] | Should -Be "1000000"
    }

    It "fps_max is 0 (uncapped default, user sets via FPS Cap calculator)" {
        $CFG_CS2_Autoexec["fps_max"] | Should -Be "0"
    }

    It "m_rawinput is 1 (forced raw input)" {
        $CFG_CS2_Autoexec["m_rawinput"] | Should -Be "1"
    }

    It "does not emit the removed snd_use_hrtf CVar" {
        $CFG_CS2_Autoexec.Keys | Should -Not -Contain "snd_use_hrtf"
    }

    It "Optimize-GameConfig treats m_rawinput as a no-op documentation stub" {
        $content = Get-Content "$script:ProjectRoot/Optimize-GameConfig.ps1" -Raw

        $content | Should -Match "m_rawinput 1 kept as a harmless documentation/forward-compatibility stub"
        $content | Should -Match "m_rawinput 1 is retained only as a no-op documentation stub"
        $content | Should -Not -Match "m_rawinput 1: reads from HID device"
        $content | Should -Not -Match "snd_use_hrtf 1"
        $content | Should -Not -Match "requires fps_max cap to function"
    }

    It "speaker_config is 1 (headphone-mode suite baseline)" {
        $CFG_CS2_Autoexec["speaker_config"] | Should -Be "1"
    }
}

# ── NIC Tweaks configuration ────────────────────────────────────────────────
Describe "NIC Tweaks configuration" {

    It "has at least 3 NIC tweak entries" {
        $CFG_NIC_Tweaks.Count | Should -BeGreaterOrEqual 3
    }

    It "all keys are valid adapter RegistryKeyword format (no spaces, no special chars)" {
        foreach ($key in $CFG_NIC_Tweaks.Keys) {
            # Valid RegistryKeyword names are alphanumeric, may start with *
            $key | Should -Match "^\*?[A-Za-z][A-Za-z0-9]*$" -Because "NIC RegistryKeyword '$key' must be a valid adapter property name"
        }
    }

    It "InterruptModeration is set to Medium (not Disabled)" {
        # Critical: djdallmann testing showed Disabled causes interrupt storms
        $CFG_NIC_Tweaks["InterruptModeration"] | Should -Be "Medium"
    }

    It "EEE is Disabled (Energy Efficient Ethernet adds latency)" {
        $CFG_NIC_Tweaks["EEE"] | Should -Be "Disabled"
    }

    It "FlowControl is Disabled (prevents flow control pauses)" {
        $CFG_NIC_Tweaks["FlowControl"] | Should -Be "Disabled"
    }
}

# ── Path format validation ───────────────────────────────────────────────────
Describe "Path format validation" {

    It "CFG_WorkDir uses backslash Windows path format" {
        $SCRIPT:_OriginalCfgWorkDir | Should -Not -Match "/" -Because "Windows paths should use backslashes"
        $SCRIPT:_OriginalCfgWorkDir | Should -Match "^[A-Z]:\\" -Because "WorkDir should be an absolute Windows path"
    }

    It "CFG_StateFile uses backslash path format" {
        $SCRIPT:_OriginalCfgStateFile | Should -Not -Match "[^:]/" -Because "Windows paths should use backslashes (excluding drive letter colon)"
    }

    It "CFG_ProgressFile uses backslash path format" {
        $SCRIPT:_OriginalCfgProgressFile | Should -Not -Match "[^:]/" -Because "Windows paths should use backslashes"
    }

    It "CFG_LatencyHistoryFile uses backslash path format" {
        $SCRIPT:_OriginalCfgLatencyHistoryFile | Should -Not -Match "[^:]/" -Because "Windows paths should use backslashes"
    }

    It "CFG_BackupMaxVersions defaults to 3" {
        $CFG_BackupMaxVersions | Should -Be 3
    }

    It "CFG_RunOnceExecutionPolicy defaults to Bypass" {
        $CFG_RunOnceExecutionPolicy | Should -Be "Bypass"
    }

    It "CFG_RunOnceExecutionPolicy only uses supported values" {
        $CFG_RunOnceExecutionPolicy | Should -BeIn @("Bypass", "RemoteSigned", "AllSigned")
    }

    It "Shader cache paths use backslash format" {
        foreach ($path in $CFG_ShaderCache_Paths) {
            $path | Should -Not -Match "[^:]/" -Because "Shader cache path '$path' should use backslashes"
        }
    }
}

# ── NVIDIA Profile Estimates ─────────────────────────────────────────────────
Describe "NVIDIA Profile Estimates configuration" {

    It "has at least 10 NVIDIA profile estimate entries" {
        $CFG_NvidiaProfileEstimates.Count | Should -BeGreaterOrEqual 10
    }

    It "all entries have required fields" {
        foreach ($key in $CFG_NvidiaProfileEstimates.Keys) {
            $est = $CFG_NvidiaProfileEstimates[$key]
            $est.Keys | Should -Contain "P1LowMin" -Because "$key must have P1LowMin"
            $est.Keys | Should -Contain "P1LowMax" -Because "$key must have P1LowMax"
            $est.Keys | Should -Contain "Confidence" -Because "$key must have Confidence"
        }
    }

    It "P1LowMin <= P1LowMax for all NVIDIA estimates" {
        foreach ($key in $CFG_NvidiaProfileEstimates.Keys) {
            $est = $CFG_NvidiaProfileEstimates[$key]
            $est.P1LowMin | Should -BeLessOrEqual $est.P1LowMax -Because "${key}: P1LowMin should be <= P1LowMax"
        }
    }
}

# ── DNS configuration ────────────────────────────────────────────────────────
Describe "DNS configuration" {

    It "Cloudflare DNS has exactly 2 entries" {
        $CFG_DNS_Cloudflare.Count | Should -Be 2
    }

    It "Google DNS has exactly 2 entries" {
        $CFG_DNS_Google.Count | Should -Be 2
    }

    It "DNS entries are valid IPv4 addresses" {
        $allDns = $CFG_DNS_Cloudflare + $CFG_DNS_Google
        foreach ($ip in $allDns) {
            $ip | Should -Match "^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$" -Because "'$ip' should be a valid IPv4 address"
        }
    }

    It "latency target definition file exists in cfgs" {
        Test-Path $CFG_LatencyTargetsFile | Should -Be $true
    }
}

# ── FPS Cap configuration ───────────────────────────────────────────────────
Describe "FPS Cap configuration" {

    It "CFG_FpsCap_Percent is between 0 and 1 (exclusive)" {
        $CFG_FpsCap_Percent | Should -BeGreaterThan 0
        $CFG_FpsCap_Percent | Should -BeLessThan 1
    }

    It "CFG_FpsCap_Min is a reasonable minimum (>= 30)" {
        $CFG_FpsCap_Min | Should -BeGreaterOrEqual 30
    }
}
