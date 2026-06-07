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

# ── CS2 Autoexec duplicate key check ────────────────────────────────────────
Describe "CS2 Autoexec configuration" {

    It "has no duplicate CVar keys" {
        # $CFG_CS2_Autoexec is an [ordered] hashtable, so duplicates would be
        # caught at parse time. But we verify no silent overwrite occurred.
        $keys = @($CFG_CS2_Autoexec.Keys)
        $uniqueKeys = @($keys | Select-Object -Unique)

        $keys.Count | Should -Be $uniqueKeys.Count -Because "duplicate CVar keys would silently overwrite earlier values"
    }

    It "has exactly the documented CVar count" {
        $CFG_CS2_Autoexec.Count | Should -Be 73 -Because "suite documents 73 CVars across 10 categories"
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

    It "matchmaking max ping defaults to the May 2026 EU baseline" {
        $CFG_CS2_Autoexec["mm_dedicated_search_maxping"] | Should -Be "40"
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

    It "does not force Source 2 thread pool selection by default" {
        $CFG_CS2_Autoexec.Keys | Should -Not -Contain "thread_pool_option"

        $content = Get-Content "$script:ProjectRoot/Optimize-GameConfig.ps1" -Raw
        $content | Should -Not -Match 'effectiveAutoexec\["thread_pool_option"\]'
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

    It "does not emit Steam Audio reverb CVars" {
        $CFG_CS2_Autoexec.Keys | Should -Not -Contain "snd_steamaudio_enable_reverb"
        $CFG_CS2_Autoexec.Keys | Should -Not -Contain "snd_steamaudio_reverb_level_db"
    }
}

# ── Optional manually executed CFG checks ───────────────────────────────────
Describe "Optional CS2 CFG files" {

    BeforeAll {
        $script:OptionalCfgFiles = @(
            "net_stable.cfg",
            "net_highping.cfg",
            "net_unstable.cfg",
            "net_bad.cfg",
            "debug_hud.cfg",
            "debug_hud_off.cfg",
            "audio_stable.cfg",
            "audio_lowlatency_025.cfg",
            "audio_lowlatency_001.cfg"
        )
        $script:AudioCfgExpectations = @{
            "audio_stable.cfg"         = "0.05"
            "audio_lowlatency_025.cfg" = "0.025"
            "audio_lowlatency_001.cfg" = "0.001"
        }
    }

    It "all optional CFG files exist" {
        foreach ($cfgFile in $script:OptionalCfgFiles) {
            Test-Path (Join-Path $script:ProjectRoot "cfgs/$cfgFile") | Should -Be $true
        }
    }

    It "contains no binds, persistence commands, developer mode, or personal UI preferences" {
        $forbiddenPatterns = @(
            '(?m)^\s*bind(toggle)?\b',
            '(?m)^\s*unbind(all)?\b',
            '(?m)^\s*host_writeconfig\b',
            '(?m)^\s*developer\s+"?1"?\b',
            '(?m)^\s*(cl_)?radar_',
            '(?m)^\s*cl_hud_radar_',
            '(?m)^\s*viewmodel_',
            '(?m)^\s*cl_crosshair',
            '(?m)^\s*(zoom_)?sensitivity\b',
            '(?m)^\s*safezone[xy]\b',
            '(?m)^\s*cl_hide_avatar_images\b',
            '(?m)^\s*cl_allow_animated_avatars\b',
            '(?m)^\s*cl_spec_',
            '(?m)^\s*cl_obs_'
        )

        foreach ($cfgFile in $script:OptionalCfgFiles) {
            $content = Get-Content (Join-Path $script:ProjectRoot "cfgs/$cfgFile") -Raw
            foreach ($pattern in $forbiddenPatterns) {
                $content | Should -Not -Match $pattern -Because "$cfgFile must remain optional/diagnostic, not personal or persistent"
            }
        }
    }

    It "audio CFGs only set autodetect latency and mixahead" {
        foreach ($cfgFile in $script:AudioCfgExpectations.Keys) {
            $lines = Get-Content (Join-Path $script:ProjectRoot "cfgs/$cfgFile")
            $commandLines = @($lines | Where-Object { $_ -notmatch '^\s*(//|$)' -and $_ -notmatch '^\s*echo\b' })
            $keys = @($commandLines | ForEach-Object { ($_ -split '\s+', 2)[0] })

            ($keys -join ",") | Should -Be "snd_autodetect_latency,snd_mixahead"
            $commandLines[0] | Should -Be 'snd_autodetect_latency "1"'
            $commandLines[1] | Should -Be "snd_mixahead `"$($script:AudioCfgExpectations[$cfgFile])`""

            ($lines -join "`n") | Should -Not -Match 'snd_steamaudio_(enable_reverb|reverb_level_db)'
        }
    }

    It "diagnostic CFGs use current telemetry names and diagnostic commands" {
        $debugHud = Get-Content (Join-Path $script:ProjectRoot "cfgs/debug_hud.cfg") -Raw
        $debugHudOff = Get-Content (Join-Path $script:ProjectRoot "cfgs/debug_hud_off.cfg") -Raw

        foreach ($content in @($debugHud, $debugHudOff)) {
            $content | Should -Match 'cl_hud_telemetry_frametime_show'
            $content | Should -Match 'cl_hud_telemetry_ping_show'
            $content | Should -Match 'cl_hud_telemetry_net_misdelivery_show'
            $content | Should -Match 'cl_hud_telemetry_net_quality_graph_show'
            $content | Should -Match 'cl_hud_telemetry_serverrecvmargin_graph_show'
            $content | Should -Not -Match 'cl_hud_telemetry_net_quality_graph\s'
            $content | Should -Not -Match 'cl_hud_telemetry_serverrecvmargin_graph\s'
        }

        $debugHud | Should -Match '(?m)^\s*net_print_sdr_ping_times\s*$'
        $debugHud | Should -Match '(?m)^\s*net_status\s*$'
        $debugHud | Should -Match '(?m)^\s*cl_ticktiming\s+print\s+detail\s*$'
        $debugHud | Should -Match '(?m)^\s*cl_hud_telemetry_net_detailed\s+"2"\s*$'
        $debugHudOff | Should -Match '(?m)^\s*cl_hud_telemetry_net_detailed\s+"0"\s*$'
    }

    It "Step 34 deploys all optional CFG files without auto-executing them" {
        $content = Get-Content "$script:ProjectRoot/Optimize-GameConfig.ps1" -Raw

        foreach ($cfgFile in $script:OptionalCfgFiles) {
            $content | Should -Match ([regex]::Escape("`"$cfgFile`""))
        }

        $content | Should -Match 'They are NOT exec''d automatically'
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
