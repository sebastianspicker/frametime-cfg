# ==============================================================================
#  tests/helpers/process-boundaries.Tests.ps1 -- elevated external-process edges
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "elevated protocol boundary" {
    It "defers Steam and web links instead of shell-executing user protocol handlers" {
        $cleanup = Get-Content -LiteralPath (Join-Path $PSScriptRoot "../../Cleanup.ps1") -Raw
        $optimizer = Get-Content -LiteralPath (Join-Path $PSScriptRoot "../../Optimize-GameConfig.ps1") -Raw
        $gui = Get-Content -LiteralPath (Join-Path $PSScriptRoot "../../helpers/gui-panels.ps1") -Raw

        $cleanup | Should -Match "steam://validate/730.*Set-ClipboardSafe"
        $gui | Should -Match "steam://rungameid/730.*Set-ClipboardSafe"
        $cleanup | Should -Not -Match 'Start-Process\s+["'']steam://'
        $gui | Should -Not -Match 'Start-Process\s+["'']steam://'
        $optimizer | Should -Not -Match 'Start-Process\s+\$url'
    }
}

Describe "GUI source script boundary" {
    BeforeEach {
        Reset-TestState
        $script:GuiRoot = Join-Path $SCRIPT:TestTempRoot "gui-root"
        New-Item -ItemType Directory -Path $script:GuiRoot -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $script:GuiRoot "Run-Optimize.ps1") -Value "# fixture" -Encoding ASCII
    }

    It "returns an absolute regular descendant for an allowlisted filename" {
        $resolved = Get-TrustedDescendantRegularFilePath -Root $script:GuiRoot -RelativePath "Run-Optimize.ps1"

        $resolved | Should -Be ([IO.Path]::GetFullPath((Join-Path $script:GuiRoot "Run-Optimize.ps1")))
    }

    It "rejects traversal and quote-bearing script paths" {
        { Get-TrustedDescendantRegularFilePath -Root $script:GuiRoot -RelativePath "..\outside.ps1" } | Should -Throw
        { Get-TrustedDescendantRegularFilePath -Root $script:GuiRoot -RelativePath 'Run-Optimize.ps1" -Command evil' } | Should -Throw
    }

    It "keeps Launch-Terminal to the two reviewed script names without a free-form argument channel" {
        $source = Get-Content -LiteralPath (Join-Path $PSScriptRoot "../../helpers/gui-panels.ps1") -Raw
        $launchTerminal = [regex]::Match($source, 'function Launch-Terminal \{(?s:.*?)\n\}').Value

        $launchTerminal | Should -Match 'ValidateSet\("Run-Optimize\.ps1", "Boot-SafeMode\.ps1"\)'
        $launchTerminal | Should -Not -Match 'ScriptArgs'
        $launchTerminal | Should -Match 'Get-TrustedDescendantRegularFilePath'
    }
}

Describe "normal-boot elevation handoff boundary" {
    It "persists only the fixed protected bootstrap as a PowerShell -File command" {
        Reset-TestState
        $SCRIPT:DryRun = $false
        Mock Write-Warn {}
        Mock Test-Path { $true }
        Mock Get-Item {
            [PSCustomObject]@{
                PSProvider = [PSCustomObject]@{ Name = 'FileSystem' }
                PSIsContainer = $false
                Attributes = [IO.FileAttributes]::Normal
            }
        }
        Mock Get-TrustedWindowsToolPath { "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" }
        Mock Test-PhaseRuntimePayload { [PSCustomObject]@{ Valid = $true; Message = "verified" } }
        Mock Set-SecureAcl {}
        Mock Set-ItemProperty {}
        Mock New-Item {}

        $result = Set-RunOnce -name "FRAMETIME_Phase3" -scriptPath "C:\FRAMETIME_CFG\runtime-generations\0123456789abcdef0123456789abcdef\PostReboot-Setup.ps1" -PassThru

        $result.Status | Should -Be "Success"
        Should -Invoke Set-ItemProperty -Exactly 1 -ParameterFilter {
            $Value -match 'PhaseRuntime-ElevationBootstrap\.ps1' -and
            $Value -match '-File' -and
            $Value -notmatch '-Command' -and
            $Value -notmatch '-Verb RunAs'
        }
    }
}
