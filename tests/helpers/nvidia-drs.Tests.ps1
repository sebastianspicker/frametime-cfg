# ==============================================================================
#  tests/helpers/nvidia-drs.Tests.ps1  --  NvApiDrs C# interop layer
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
    . "$PSScriptRoot/../../helpers/nvidia-drs.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── NvApiDrsCode (C# source) ──────────────────────────────────────────────
Describe "NvApiDrsCode C# source" {

    It "contains the NvApiDrs class definition" {
        $NvApiDrsCode | Should -Match 'public class NvApiDrs'
    }

    It "contains DllImport for nvapi64.dll" {
        $NvApiDrsCode | Should -Match 'DllImport\("nvapi64\.dll"'
    }

    It "defines all 12 NVAPI function IDs" {
        $NvApiDrsCode | Should -Match 'ID_Initialize'
        $NvApiDrsCode | Should -Match 'ID_DRS_CreateSession'
        $NvApiDrsCode | Should -Match 'ID_DRS_DestroySession'
        $NvApiDrsCode | Should -Match 'ID_DRS_LoadSettings'
        $NvApiDrsCode | Should -Match 'ID_DRS_SaveSettings'
        $NvApiDrsCode | Should -Match 'ID_DRS_FindProfileByName'
        $NvApiDrsCode | Should -Match 'ID_DRS_CreateProfile'
        $NvApiDrsCode | Should -Match 'ID_DRS_DeleteProfile'
        $NvApiDrsCode | Should -Match 'ID_DRS_CreateApplication'
        $NvApiDrsCode | Should -Match 'ID_DRS_SetSetting'
        $NvApiDrsCode | Should -Match 'ID_DRS_GetSetting'
        $NvApiDrsCode | Should -Match 'ID_DRS_FindAppByName'
    }

    It "defines correct SETTING_SIZE constant (12320 bytes)" {
        $NvApiDrsCode | Should -Match 'SETTING_SIZE\s*=\s*12320'
    }

    It "defines correct PROFILE_SIZE constant (4116 bytes)" {
        $NvApiDrsCode | Should -Match 'PROFILE_SIZE\s*=\s*4116'
    }

    It "defines correct APP_V1_SIZE constant (4104 bytes)" {
        $NvApiDrsCode | Should -Match 'APP_V1_SIZE\s*=\s*4104'
    }

    It "defines correct APP_V3_SIZE constant (16392 bytes)" {
        $NvApiDrsCode | Should -Match 'APP_V3_SIZE\s*=\s*16392'
    }

    It "defines UNICODE_STR_BYTES as 4096" {
        $NvApiDrsCode | Should -Match 'UNICODE_STR_BYTES\s*=\s*4096'
    }

    It "defines version constants for all struct types" {
        $NvApiDrsCode | Should -Match 'SETTING_VER1\s*=\s*0x00013020'
        $NvApiDrsCode | Should -Match 'PROFILE_VER1\s*=\s*0x00011014'
        $NvApiDrsCode | Should -Match 'APP_VER1\s*=\s*0x00011008'
        $NvApiDrsCode | Should -Match 'APP_VER3\s*=\s*0x00034008'
    }

    It "defines NVAPI status codes" {
        $NvApiDrsCode | Should -Match 'public const int OK\s*=\s*0'
        $NvApiDrsCode | Should -Match 'INCOMPATIBLE_STRUCT_VERSION\s*=\s*-9'
        $NvApiDrsCode | Should -Match 'SETTING_NOT_FOUND\s*=\s*-174'
        $NvApiDrsCode | Should -Match 'PROFILE_NOT_FOUND\s*=\s*-175'
        $NvApiDrsCode | Should -Match 'EXECUTABLE_ALREADY_IN_USE\s*=\s*-179'
    }

    It "uses GCHandle.Alloc(Pinned) for struct marshaling" {
        $NvApiDrsCode | Should -Match 'GCHandle\.Alloc\([^)]+GCHandleType\.Pinned\)'
    }

    It "does not use unsafe code" {
        $NvApiDrsCode | Should -Not -Match '\bunsafe\b'
    }

    It "frees GCHandle in finally blocks" {
        $NvApiDrsCode | Should -Match 'finally\s*\{[^}]*\.Free\(\)'
    }
}

# ── Initialize-NvApiDrs ───────────────────────────────────────────────────
Describe "Initialize-NvApiDrs" {

    BeforeEach {
        Reset-TestState
        # Reset cached state
        $SCRIPT:NvApiAvailable = $null
    }

    It "returns false on non-Windows (no nvapi64.dll)" {
        # $IsWindows is undefined (null) on Windows PowerShell 5.1; treat null as Windows.
        $onWindows = ($IsWindows -ne $false)
        if ($onWindows) {
            Set-ItResult -Skipped -Because "Test only applicable on non-Windows"
        } else {
            $result = Initialize-NvApiDrs
            $result | Should -Be $false
        }
    }

    It "caches successful result" {
        $SCRIPT:NvApiAvailable = $true
        $result = Initialize-NvApiDrs
        $result | Should -Be $true
    }

    It "caches failure result (no re-attempt)" {
        $SCRIPT:NvApiAvailable = $false
        $result = Initialize-NvApiDrs
        $result | Should -Be $false
    }

    It "has guard for 32-bit PowerShell" {
        # Cannot mock [IntPtr]::Size at runtime; verify guard exists in function body
        $fn = Get-Command Initialize-NvApiDrs -ErrorAction SilentlyContinue
        $fn | Should -Not -BeNullOrEmpty
        $fn.ScriptBlock.ToString() | Should -Match '\[IntPtr\]::Size\s*-ne\s*8'
    }

    It "has guard for ARM64 architecture" {
        $fn = Get-Command Initialize-NvApiDrs -ErrorAction SilentlyContinue
        $fn | Should -Not -BeNullOrEmpty
        $fn.ScriptBlock.ToString() | Should -Match 'PROCESSOR_ARCHITECTURE.*ARM64'
    }

    It "has guard for Constrained Language Mode" {
        $fn = Get-Command Initialize-NvApiDrs -ErrorAction SilentlyContinue
        $fn | Should -Not -BeNullOrEmpty
        $fn.ScriptBlock.ToString() | Should -Match 'ConstrainedLanguage'
    }
}

# ── Invoke-DrsSession ─────────────────────────────────────────────────────
Describe "Invoke-DrsSession" {

    It "marks -Action as mandatory" {
        $cmd = Get-Command Invoke-DrsSession
        $actionMetadata = $cmd.Parameters['Action']
        $parameterAttribute = $actionMetadata.Attributes | Where-Object { $_ -is [System.Management.Automation.ParameterAttribute] } | Select-Object -First 1

        $parameterAttribute | Should -Not -BeNullOrEmpty
        $parameterAttribute.Mandatory | Should -Be $true
    }

    It "has a -NoSave switch parameter" {
        $cmd = Get-Command Invoke-DrsSession
        $cmd.Parameters.ContainsKey('NoSave') | Should -Be $true
    }
}

# ── Struct Size Verification ───────────────────────────────────────────────
Describe "NvApiDrs Struct Size Calculations" {

    It "SETTING_SIZE = 4 + 4096 + 4 + 4 + 4 + 4 + 4 + 4100 + 4100 = 12320" {
        $calculated = 4 + 4096 + 4 + 4 + 4 + 4 + 4 + 4100 + 4100
        $calculated | Should -Be 12320
    }

    It "PROFILE_SIZE = 4 + 4096 + 4 + 4 + 4 + 4 = 4116" {
        $calculated = 4 + 4096 + 4 + 4 + 4 + 4
        $calculated | Should -Be 4116
    }

    It "APP_V1_SIZE = 4 + 4 + 4096 = 4104" {
        $calculated = 4 + 4 + 4096
        $calculated | Should -Be 4104
    }

    It "APP_V3_SIZE = 4 + 4 + 4096*4 = 16392" {
        $calculated = 4 + 4 + (4096 * 4)
        $calculated | Should -Be 16392
    }

    It "SETTING_VER1 encodes size 12320 with version 1" {
        # Version encoding: size | (version << 16) = 12320 | 0x10000 = 0x13020
        $encoded = 12320 -bor (1 -shl 16)
        "0x{0:X8}" -f $encoded | Should -Be "0x00013020"
    }

    It "PROFILE_VER1 encodes size 4116 with version 1" {
        $encoded = 4116 -bor (1 -shl 16)
        "0x{0:X8}" -f $encoded | Should -Be "0x00011014"
    }

    It "APP_VER1 encodes size 4104 with version 1" {
        $encoded = 4104 -bor (1 -shl 16)
        "0x{0:X8}" -f $encoded | Should -Be "0x00011008"
    }

    It "APP_VER3 encodes size 16392 with version 3" {
        $encoded = 16392 -bor (3 -shl 16)
        "0x{0:X8}" -f $encoded | Should -Be "0x00034008"
    }
}
