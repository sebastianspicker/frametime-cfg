# ==============================================================================
#  tests/helpers/security-validation.Tests.ps1  --  Input validation guards
# ==============================================================================
#
#  Tests the security hardening added in SECURITY-R6:
#    - Set-RegistryValue: hive prefix validation, name character validation
#    - Set-BootConfig: key/value format validation
#    - Set-RunOnce: name validation, path containment
#    - Get-ActiveNicGuid: GUID format validation
#    - Test-SystemCompatibility: ARM64, Server, CLM detection

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Set-RegistryValue input validation ────────────────────────────────────────
Describe "Set-RegistryValue security validation" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $SCRIPT:CurrentStepTitle = "Security Test"
        Mock Write-Warn {}
        Mock Write-DebugLog {}
        Mock Write-OK {}
        Mock Backup-RegistryValue {}
        Mock Test-Path { $true }
        Mock New-Item {}
        Mock Set-ItemProperty {}
    }

    Context "rejects invalid registry hive prefixes" {

        It "rejects a bare filesystem path" {
            Set-RegistryValue "C:\Windows\System32" "TestName" 1 "DWord" "reason"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "valid registry hive" }
            Should -Invoke Set-ItemProperty -Exactly 0
        }

        It "rejects a UNC path" {
            Set-RegistryValue "\\server\share" "TestName" 1 "DWord" "reason"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "valid registry hive" }
            Should -Invoke Set-ItemProperty -Exactly 0
        }

        It "rejects empty path" {
            Set-RegistryValue "" "TestName" 1 "DWord" "reason"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "valid registry hive" }
            Should -Invoke Set-ItemProperty -Exactly 0
        }

        It "accepts HKLM: path" {
            Set-RegistryValue "HKLM:\SOFTWARE\Test" "TestName" 1 "DWord" "reason"

            Should -Invoke Set-ItemProperty -Exactly 1
        }

        It "accepts HKCU: path" {
            Set-RegistryValue "HKCU:\SOFTWARE\Test" "TestName" 1 "DWord" "reason"

            Should -Invoke Set-ItemProperty -Exactly 1
        }
    }

    Context "rejects invalid value names" {

        It "rejects name with backslash" {
            Set-RegistryValue "HKLM:\SOFTWARE\Test" "bad\name" 1 "DWord" "reason"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "invalid characters" }
            Should -Invoke Set-ItemProperty -Exactly 0
        }

        It "rejects name with forward slash" {
            Set-RegistryValue "HKLM:\SOFTWARE\Test" "bad/name" 1 "DWord" "reason"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "invalid characters" }
            Should -Invoke Set-ItemProperty -Exactly 0
        }

        It "rejects name with null byte" {
            Set-RegistryValue "HKLM:\SOFTWARE\Test" "bad`0name" 1 "DWord" "reason"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "invalid characters" }
            Should -Invoke Set-ItemProperty -Exactly 0
        }

        It "accepts normal registry value name" {
            Set-RegistryValue "HKLM:\SOFTWARE\Test" "NormalValueName" 1 "DWord" "reason"

            Should -Invoke Set-ItemProperty -Exactly 1
        }
    }
}

# ── Set-BootConfig input validation ──────────────────────────────────────────
Describe "Set-BootConfig security validation" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $true  # Use DRY-RUN to avoid calling bcdedit
        $SCRIPT:CurrentStepTitle = "Security Test"
        Mock Write-Warn {}
        Mock Write-Host {}
        Mock Backup-BootConfig {}
    }

    Context "rejects invalid key formats" {

        It "rejects key starting with digit" {
            Set-BootConfig "1badkey" "yes" "test"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "invalid key format" }
        }

        It "rejects key with spaces" {
            Set-BootConfig "bad key" "yes" "test"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "invalid key format" }
        }

        It "rejects key with semicolons (injection attempt)" {
            Set-BootConfig "key;evil" "yes" "test"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "invalid key format" }
        }

        It "accepts valid key like disabledynamictick" {
            Set-BootConfig "disabledynamictick" "yes" "test"

            Should -Invoke Write-Host -ParameterFilter { $Object -match "DRY-RUN" }
        }
    }

    Context "rejects invalid value formats" {

        It "rejects value with spaces" {
            Set-BootConfig "testkey" "val ue" "test"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "invalid value format" }
        }

        It "rejects value with pipe (injection attempt)" {
            Set-BootConfig "testkey" "yes|evil" "test"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "invalid value format" }
        }

        It "accepts valid value like yes" {
            Set-BootConfig "testkey" "yes" "test"

            Should -Invoke Write-Host -ParameterFilter { $Object -match "DRY-RUN" }
        }

        It "accepts braced value like {current}" {
            Set-BootConfig "testkey" "{current}" "test"

            Should -Invoke Write-Host -ParameterFilter { $Object -match "DRY-RUN" }
        }
    }
}

# ── Set-RunOnce input validation ─────────────────────────────────────────────
Describe "Set-RunOnce security validation" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $true  # Use DRY-RUN to avoid real registry writes
        Mock Write-Warn {}
        Mock Write-Host {}
    }

    Context "rejects invalid names" {

        It "rejects name with spaces" {
            Set-RunOnce "bad name" "C:\CS2_OPTIMIZE\test.ps1"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "invalid name" }
        }

        It "rejects name with special characters" {
            Set-RunOnce "bad;name" "C:\CS2_OPTIMIZE\test.ps1"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "invalid name" }
        }

        It "accepts valid name with underscore" {
            Set-RunOnce "CS2_Phase3" "C:\CS2_OPTIMIZE\test.ps1"

            # Should proceed past name validation (may warn about path, that's ok)
            Should -Invoke Write-Warn -ParameterFilter { $t -match "invalid name" } -Exactly 0
        }
    }

    Context "rejects paths outside C:\\CS2_OPTIMIZE\\" {

        It "rejects path to Windows directory" {
            Set-RunOnce "CS2_Test" "C:\Windows\evil.ps1"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "must be under" }
        }

        It "rejects path traversal via .." {
            Set-RunOnce "CS2_Test" "C:\CS2_OPTIMIZE\..\Windows\evil.ps1"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "must be under" }
        }

        It "rejects non-.ps1 extension" {
            Set-RunOnce "CS2_Test" "C:\CS2_OPTIMIZE\evil.exe"

            Should -Invoke Write-Warn -ParameterFilter { $t -match "must be under" }
        }
    }
}

Describe "Test-TrustedSuiteScriptPath" {

    It "accepts suite-owned PowerShell paths" {
        Test-TrustedSuiteScriptPath -Path "C:\CS2_OPTIMIZE\PostReboot-Setup.ps1" | Should -Be $true
    }

    It "rejects paths outside the suite workspace" {
        Test-TrustedSuiteScriptPath -Path "C:\Windows\System32\evil.ps1" | Should -Be $false
    }

    It "rejects path traversal and non-PowerShell targets" {
        Test-TrustedSuiteScriptPath -Path "C:\CS2_OPTIMIZE\..\Windows\evil.ps1" | Should -Be $false
        Test-TrustedSuiteScriptPath -Path "C:\CS2_OPTIMIZE\tool.cmd" | Should -Be $false
    }
}

# ── Get-ActiveNicGuid GUID validation ────────────────────────────────────────
Describe "Get-ActiveNicGuid GUID validation" {

    BeforeEach {
        Reset-TestState
        Mock Write-Warn {}
        Mock Write-DebugLog {}
    }

    It "returns null for malformed GUID with injection characters" {
        Mock Get-ActiveNicAdapter {
            [PSCustomObject]@{ InterfaceGuid = '{evil;drop table--}' }
        }

        $result = Get-ActiveNicGuid

        $result | Should -BeNullOrEmpty
        Should -Invoke Write-Warn -ParameterFilter { $t -match "GUID failed format validation" }
    }

    It "returns null for GUID without braces" {
        Mock Get-ActiveNicAdapter {
            [PSCustomObject]@{ InterfaceGuid = '12345678-1234-1234-1234-123456789abc' }
        }

        $result = Get-ActiveNicGuid

        $result | Should -BeNullOrEmpty
    }

    It "returns valid GUID when format is correct" {
        Mock Get-ActiveNicAdapter {
            [PSCustomObject]@{ InterfaceGuid = '{12345678-1234-1234-1234-123456789abc}' }
        }

        $result = Get-ActiveNicGuid

        $result | Should -Be '{12345678-1234-1234-1234-123456789abc}'
    }

    It "returns null when no NIC adapter found" {
        Mock Get-ActiveNicAdapter { $null }

        $result = Get-ActiveNicGuid

        $result | Should -BeNullOrEmpty
    }
}

# ── Test-SystemCompatibility ─────────────────────────────────────────────────
Describe "Test-SystemCompatibility" {

    BeforeEach {
        Reset-TestState
        Mock Write-Warn {}
        Mock Write-Info {}
        Mock Write-DebugLog {}
        Mock Get-CimInstance { [PSCustomObject]@{ ProductType = 1 } }
        Mock Get-Command { $true }
    }

    It "warns on ARM64 architecture" {
        $savedArch = $env:PROCESSOR_ARCHITECTURE
        try {
            $env:PROCESSOR_ARCHITECTURE = "ARM64"
            Test-SystemCompatibility
            Should -Invoke Write-Warn -ParameterFilter { $t -match "ARM64" }
        } finally {
            $env:PROCESSOR_ARCHITECTURE = $savedArch
        }
    }

    It "warns on Windows Server (ProductType 3)" {
        Mock Get-CimInstance { [PSCustomObject]@{ ProductType = 3 } }

        Test-SystemCompatibility

        Should -Invoke Write-Warn -ParameterFilter { $t -match "Server" }
    }

    It "does not warn about Server on desktop workstation (ProductType 1)" {
        Mock Get-CimInstance { [PSCustomObject]@{ ProductType = 1 } }
        $savedArch = $env:PROCESSOR_ARCHITECTURE
        try {
            $env:PROCESSOR_ARCHITECTURE = "AMD64"
            Test-SystemCompatibility
            Should -Invoke Write-Warn -ParameterFilter { $t -match "Server" } -Exactly 0
        } finally {
            $env:PROCESSOR_ARCHITECTURE = $savedArch
        }
    }
}
