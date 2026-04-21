# ==============================================================================
#  tests/helpers/system-utils.Tests.ps1  --  JSON I/O, registry, boot config
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Save-JsonAtomic ──────────────────────────────────────────────────────────
Describe "Save-JsonAtomic" {

    BeforeEach { Reset-TestState }

    It "writes valid JSON to a new file" {
        $path = "$SCRIPT:TestTempRoot\test-atomic.json"
        $data = @{ foo = "bar"; count = 42 }

        Save-JsonAtomic -Data $data -Path $path

        Test-Path $path | Should -Be $true
        $loaded = Get-Content $path -Raw | ConvertFrom-Json
        $loaded.foo   | Should -Be "bar"
        $loaded.count | Should -Be 42
    }

    It "overwrites an existing file" {
        $path = "$SCRIPT:TestTempRoot\test-overwrite.json"
        @{ old = "data" } | ConvertTo-Json | Set-Content $path -Encoding UTF8

        Save-JsonAtomic -Data @{ new = "data" } -Path $path

        $loaded = Get-Content $path -Raw | ConvertFrom-Json
        $loaded.new | Should -Be "data"
        # Old key should not be present
        $loaded.PSObject.Properties.Name | Should -Not -Contain "old"
    }

    It "creates parent directory if it does not exist" {
        $path = "$SCRIPT:TestTempRoot\deep\nested\dir\test.json"

        Save-JsonAtomic -Data @{ ok = $true } -Path $path

        Test-Path $path | Should -Be $true
    }

    It "cleans up .tmp file on failure and preserves existing target" {
        $path = "$SCRIPT:TestTempRoot\test-fail.json"
        $tmpPath = "$path.tmp"

        # Pre-create target file to verify it survives a failed write
        @{ original = "preserved" } | ConvertTo-Json | Set-Content $path -Encoding UTF8

        # Mock Move-Item to simulate failure after .tmp is written
        Mock Move-Item { throw "Simulated disk full" }

        { Save-JsonAtomic -Data @{ x = 1 } -Path $path } | Should -Throw "*Save-JsonAtomic*"

        # .tmp file should be cleaned up
        Test-Path $tmpPath | Should -Be $false
        # Original file should be preserved (atomic guarantee — no partial write)
        Test-Path $path | Should -Be $true
        $preserved = Get-Content $path -Raw | ConvertFrom-Json
        $preserved.original | Should -Be "preserved"
    }

    It "preserves nested objects with default depth" {
        $path = "$SCRIPT:TestTempRoot\test-nested.json"
        $data = @{
            level1 = @{
                level2 = @{
                    level3 = @{
                        value = "deep"
                    }
                }
            }
        }

        Save-JsonAtomic -Data $data -Path $path -Depth 10

        $loaded = Get-Content $path -Raw | ConvertFrom-Json
        $loaded.level1.level2.level3.value | Should -Be "deep"
    }

    It "writes valid UTF-8 encoded content" {
        $path = Join-Path $SCRIPT:TestTempRoot "test-encoding.json"
        $data = @{ name = "test" }

        Save-JsonAtomic -Data $data -Path $path

        # Verify content is readable as UTF-8
        $content = [System.IO.File]::ReadAllText($path, [System.Text.Encoding]::UTF8)
        $content | Should -Match '"name"'
    }
}

# ── Set-RegistryValue DRY-RUN ────────────────────────────────────────────────
Describe "Set-RegistryValue DRY-RUN" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $true
        $SCRIPT:CurrentStepTitle = "Test Step"
        # Mock all functions that Set-RegistryValue might call
        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Backup-RegistryValue {}
    }

    It "does not write to registry in DRY-RUN mode" {
        Mock Set-ItemProperty {}
        Mock New-Item {}

        Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 1 "DWord" "Test reason"

        # Set-ItemProperty should NOT be called
        Should -Invoke Set-ItemProperty -Exactly 0
        Should -Invoke New-Item -Exactly 0
    }

    It "outputs DRY-RUN message with value details" {
        Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 42 "DWord" "Test reason"

        Should -Invoke Write-Host -ParameterFilter {
            $Object -match "DRY-RUN" -and $Object -match "TestValue" -and $Object -match "42"
        }
    }

    It "still delegates to Backup-RegistryValue in DRY-RUN mode so the helper can self-guard" {
        Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestValue" 1 "DWord" "Test reason"

        Should -Invoke Backup-RegistryValue -Exactly 1 -ParameterFilter {
            $Path -eq "HKLM:\SOFTWARE\Test" -and
            $Name -eq "TestValue" -and
            $StepTitle -eq "Test Step"
        }
    }
}

# ── Set-BootConfig DRY-RUN ───────────────────────────────────────────────────
Describe "Set-BootConfig DRY-RUN" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $true
        $SCRIPT:CurrentStepTitle = "Test Step"
        Mock Write-Host {}
        Mock Backup-BootConfig {}
    }

    It "does not execute bcdedit in DRY-RUN mode" {
        # We cannot easily mock bcdedit (external exe), but in DRY-RUN mode
        # the function returns before reaching the bcdedit call
        Mock Write-Step {}

        Set-BootConfig "disabledynamictick" "yes" "Test boot config"

        # Write-Step is called only in the non-DRY-RUN path
        Should -Invoke Write-Step -Exactly 0
    }

    It "outputs DRY-RUN message with key and value" {
        Set-BootConfig "disabledynamictick" "yes" "Disable dynamic tick"

        Should -Invoke Write-Host -ParameterFilter {
            $Object -match "DRY-RUN" -and $Object -match "disabledynamictick" -and $Object -match "yes"
        }
    }

    It "still delegates to Backup-BootConfig in DRY-RUN mode so the helper can self-guard" {
        Set-BootConfig "disabledynamictick" "yes" "Test"

        Should -Invoke Backup-BootConfig -Exactly 1 -ParameterFilter {
            $Key -eq "disabledynamictick" -and
            $StepTitle -eq "Test Step"
        }
    }
}

# ── Initialize-VerifyCounters / Get-VerifyCounters ───────────────────────────
Describe "Initialize-VerifyCounters / Get-VerifyCounters" {

    BeforeEach { Reset-TestState }

    It "initializes all counters to zero" {
        Initialize-VerifyCounters

        $c = Get-VerifyCounters
        $c.okCount      | Should -Be 0
        $c.changedCount | Should -Be 0
        $c.missingCount | Should -Be 0
    }

    It "returns hashtable with correct keys" {
        Initialize-VerifyCounters

        $c = Get-VerifyCounters
        $c.Keys | Should -Contain "okCount"
        $c.Keys | Should -Contain "changedCount"
        $c.Keys | Should -Contain "missingCount"
    }

    It "resets counters when called again" {
        Initialize-VerifyCounters
        $Script:_verifyOkCount = 5
        $Script:_verifyChangedCount = 3

        Initialize-VerifyCounters
        $c = Get-VerifyCounters
        $c.okCount      | Should -Be 0
        $c.changedCount | Should -Be 0
    }
}

# ── Test-RegistryCheck ────────────────────────────────────────────────────────
Describe "Test-RegistryCheck" {

    BeforeEach {
        Reset-TestState
        Initialize-VerifyCounters
        Mock Write-Host {}
    }

    Context "with -Quiet switch (returns structured result)" {

        It "returns OK when value matches expected" {
            # Use a real temp registry-like path via mocking
            Mock Test-Path { $true } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }
            Mock Get-ItemProperty {
                [PSCustomObject]@{ TestName = 1 }
            } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" -and $Name -eq "TestName" }

            $result = Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test" -Quiet

            $result.Status | Should -Be "OK"
            $result.Value  | Should -Be 1
        }

        It "returns CHANGED when value differs from expected" {
            Mock Test-Path { $true } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }
            Mock Get-ItemProperty {
                [PSCustomObject]@{ TestName = 0 }
            } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" -and $Name -eq "TestName" }

            $result = Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test" -Quiet

            $result.Status | Should -Be "CHANGED"
            $result.Value  | Should -Be 0
        }

        It "returns MISSING when key does not exist" {
            Mock Test-Path { $false } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }

            $result = Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test" -Quiet

            $result.Status | Should -Be "MISSING"
        }

        It "returns MISSING when value read throws" {
            Mock Test-Path { $true } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }
            Mock Get-ItemProperty { throw "Access denied" } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }

            $result = Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test" -Quiet

            $result.Status | Should -Be "MISSING"
        }
    }

    Context "without -Quiet (updates global counters)" {

        It "increments okCount on match" {
            Mock Test-Path { $true } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }
            Mock Get-ItemProperty {
                [PSCustomObject]@{ TestName = 1 }
            } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" -and $Name -eq "TestName" }

            Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test"

            $c = Get-VerifyCounters
            $c.okCount | Should -Be 1
        }

        It "increments changedCount on mismatch" {
            Mock Test-Path { $true } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }
            Mock Get-ItemProperty {
                [PSCustomObject]@{ TestName = 99 }
            } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" -and $Name -eq "TestName" }

            Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test"

            $c = Get-VerifyCounters
            $c.changedCount | Should -Be 1
        }

        It "increments missingCount when key absent" {
            Mock Test-Path { $false } -ParameterFilter { $Path -eq "HKLM:\SOFTWARE\TestKey" }

            Test-RegistryCheck -Path "HKLM:\SOFTWARE\TestKey" -Name "TestName" -Expected 1 -Label "Test"

            $c = Get-VerifyCounters
            $c.missingCount | Should -Be 1
        }
    }
}

# ── Load-State ────────────────────────────────────────────────────────────────
Describe "Load-State" {

    BeforeEach { Reset-TestState }

    It "round-trips state through file" {
        $state = [PSCustomObject]@{
            mode     = "DRY-RUN"
            logLevel = "VERBOSE"
            profile  = "COMPETITIVE"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        $loaded = Load-State $CFG_StateFile
        $SCRIPT:Mode     | Should -Be "DRY-RUN"
        $SCRIPT:Profile  | Should -Be "COMPETITIVE"
        $SCRIPT:LogLevel | Should -Be "VERBOSE"
        $SCRIPT:DryRun   | Should -Be $true
    }

    It "throws when state file is missing" {
        { Load-State "$SCRIPT:TestTempRoot\nonexistent.json" } | Should -Throw "*Settings file not found*"
    }

    It "sets DryRun to false for non-DRY-RUN mode" {
        $state = [PSCustomObject]@{ mode = "CONTROL"; profile = "SAFE" }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Load-State $CFG_StateFile | Out-Null
        $SCRIPT:DryRun | Should -Be $false
    }

    It "defaults logLevel to NORMAL when absent" {
        $state = [PSCustomObject]@{ mode = "CONTROL"; profile = "SAFE" }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Load-State $CFG_StateFile | Out-Null
        $SCRIPT:LogLevel | Should -Be "NORMAL"
    }
}

# ── Initialize-ScriptDefaults ────────────────────────────────────────────────
Describe "Initialize-ScriptDefaults" {

    BeforeEach { Reset-TestState }

    It "loads from state file when present" {
        $state = [PSCustomObject]@{
            mode     = "DRY-RUN"
            logLevel = "VERBOSE"
            profile  = "COMPETITIVE"
        }
        Save-JsonAtomic -Data $state -Path $CFG_StateFile

        Initialize-ScriptDefaults

        $SCRIPT:Mode     | Should -Be "DRY-RUN"
        $SCRIPT:Profile  | Should -Be "COMPETITIVE"
        $SCRIPT:DryRun   | Should -Be $true
    }

    It "sets safe defaults when state file is absent" {
        Initialize-ScriptDefaults

        $SCRIPT:Mode     | Should -Be "CONTROL"
        $SCRIPT:Profile  | Should -Be "RECOMMENDED"
        $SCRIPT:DryRun   | Should -Be $false
    }

    It "sets safe defaults when state file is corrupted" {
        "this is not json" | Set-Content $CFG_StateFile -Encoding UTF8

        Initialize-ScriptDefaults

        $SCRIPT:Mode     | Should -Be "CONTROL"
        $SCRIPT:Profile  | Should -Be "RECOMMENDED"
    }
}

Describe "Copy-PhaseRuntimePayload" {

    BeforeEach {
        Reset-TestState
        $script:PayloadSource = Join-Path $SCRIPT:TestTempRoot "payload-src"
        $script:PayloadDest = Join-Path $SCRIPT:TestTempRoot "payload-dest"
        New-Item -ItemType Directory -Path (Join-Path $script:PayloadSource "helpers") -Force | Out-Null
        New-Item -ItemType Directory -Path (Join-Path $script:PayloadSource "cfgs") -Force | Out-Null
        New-Item -ItemType Directory -Path (Join-Path $script:PayloadSource "docs") -Force | Out-Null
        foreach ($file in @(
            "SafeMode-DriverClean.ps1",
            "PostReboot-Setup.ps1",
            "Guide-VideoSettings.ps1",
            "helpers.ps1",
            "config.env.ps1"
        )) {
            Set-Content (Join-Path $script:PayloadSource $file) -Value "# $file" -Encoding UTF8
        }
        Set-Content (Join-Path $script:PayloadSource "helpers/helper.ps1") -Value "# helper" -Encoding UTF8
        Set-Content (Join-Path $script:PayloadSource "cfgs/net_highping.cfg") -Value "cfg" -Encoding UTF8
        Set-Content (Join-Path $script:PayloadSource "docs/video.txt") -Value "video" -Encoding UTF8
        Set-Content (Join-Path $script:PayloadSource "docs/nvidia-drs-settings.md") -Value "drs" -Encoding UTF8
        Mock Write-OK {}
    }

    It "copies the shared runtime payload, cfgs, and repo-local docs in one pass" {
        Copy-PhaseRuntimePayload -SourceRoot $script:PayloadSource -DestinationRoot $script:PayloadDest

        Test-Path (Join-Path $script:PayloadDest "SafeMode-DriverClean.ps1") | Should -Be $true
        Test-Path (Join-Path $script:PayloadDest "helpers/helper.ps1") | Should -Be $true
        Test-Path (Join-Path $script:PayloadDest "cfgs/net_highping.cfg") | Should -Be $true
        Test-Path (Join-Path $script:PayloadDest "docs/video.txt") | Should -Be $true
        Test-Path (Join-Path $script:PayloadDest "docs/nvidia-drs-settings.md") | Should -Be $true
    }
}

Describe "Phase 1 Safe Mode readiness marker" {

    BeforeEach {
        Reset-TestState
        Mock Set-SecureAcl {}
    }

    It "persists the Safe Mode readiness flag into state.json" {
        Save-JsonAtomic -Data ([PSCustomObject]@{
            profile = "RECOMMENDED"
            mode = "AUTO"
        }) -Path $CFG_StateFile

        Set-Phase1SafeModeReadyFlag -Path $CFG_StateFile | Out-Null

        $saved = Get-Content $CFG_StateFile -Raw | ConvertFrom-Json
        $saved.phase1SafeModeReady | Should -Be $true
    }

    It "detects the readiness marker only when explicitly set" {
        Test-Phase1SafeModeReady -State ([PSCustomObject]@{ profile = "RECOMMENDED" }) | Should -Be $false
        Test-Phase1SafeModeReady -State ([PSCustomObject]@{ phase1SafeModeReady = $true }) | Should -Be $true
    }
}
