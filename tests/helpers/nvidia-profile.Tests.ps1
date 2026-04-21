# ==============================================================================
#  tests/helpers/nvidia-profile.Tests.ps1  --  NVIDIA DRS settings table & profile
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"

    # Stub DRS functions that nvidia-profile.ps1 references
    if (-not (Get-Command Initialize-NvApiDrs -ErrorAction SilentlyContinue)) {
        function global:Initialize-NvApiDrs { return $false }
    }
    if (-not (Get-Command Invoke-DrsSession -ErrorAction SilentlyContinue)) {
        function global:Invoke-DrsSession { param($Action, [switch]$NoSave) }
    }
    if (-not (Get-Command Backup-DrsSettings -ErrorAction SilentlyContinue)) {
        function global:Backup-DrsSettings { param($Session, $DrsProfile, $SettingIds, $StepTitle, $ProfileName, $ProfileCreated) }
    }

    . "$PSScriptRoot/../../helpers/nvidia-profile.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── DRS Settings Table ─────────────────────────────────────────────────────
Describe "NV_DRS_SETTINGS table" {

    It "contains at least 52 entries" {
        $NV_DRS_SETTINGS.Count | Should -BeGreaterOrEqual 52
    }

    It "every entry has an Id field" {
        foreach ($s in $NV_DRS_SETTINGS) {
            $s.Id | Should -Not -BeNullOrEmpty -Because "entry '$($s.Name)' must have an Id"
        }
    }

    It "every entry has a numeric Id" {
        foreach ($s in $NV_DRS_SETTINGS) {
            { [uint32]$s.Id } | Should -Not -Throw -Because "Id for '$($s.Name)' must be convertible to uint32"
        }
    }

    It "every entry has a Value field" {
        foreach ($s in $NV_DRS_SETTINGS) {
            $s.ContainsKey('Value') | Should -Be $true -Because "entry '$($s.Name)' must have a Value"
        }
    }

    It "every entry has a numeric Value" {
        foreach ($s in $NV_DRS_SETTINGS) {
            { [uint32]$s.Value } | Should -Not -Throw -Because "Value for '$($s.Name)' must be convertible to uint32"
        }
    }

    It "every entry has a Name field" {
        foreach ($s in $NV_DRS_SETTINGS) {
            $s.Name | Should -Not -BeNullOrEmpty -Because "every DRS entry must have a descriptive Name"
        }
    }

    It "has no duplicate setting IDs" {
        $ids = $NV_DRS_SETTINGS | ForEach-Object { $_.Id }
        $uniqueIds = $ids | Select-Object -Unique
        $uniqueIds.Count | Should -Be $ids.Count
    }

    It "includes Power Management Mode (Id=274197361)" {
        $pm = $NV_DRS_SETTINGS | Where-Object { $_.Id -eq 274197361 }
        $pm | Should -Not -BeNullOrEmpty
        $pm.Value | Should -Be 1
    }

    It "includes VSync Force OFF (Id=11041231)" {
        $vsync = $NV_DRS_SETTINGS | Where-Object { $_.Id -eq 11041231 }
        $vsync | Should -Not -BeNullOrEmpty
    }

    It "includes Shader Cache 10GB (Id=11306135)" {
        $cache = $NV_DRS_SETTINGS | Where-Object { $_.Id -eq 11306135 }
        $cache | Should -Not -BeNullOrEmpty
        $cache.Value | Should -Be 10240
    }

    It "includes FRL NVCPL 500 FPS cap (Id=277041162)" {
        $frl = $NV_DRS_SETTINGS | Where-Object { $_.Id -eq 277041162 }
        $frl | Should -Not -BeNullOrEmpty
        $frl.Value | Should -Be 500
    }

    It "includes Ultra Low Latency CPL State (Id=390467)" {
        $ull = $NV_DRS_SETTINGS | Where-Object { $_.Id -eq 390467 }
        $ull | Should -Not -BeNullOrEmpty
        $ull.Value | Should -Be 1
    }
}

# ── Apply-NvidiaCS2Profile ──────────────────────────────────────────────────
Describe "Apply-NvidiaCS2Profile" {

    BeforeEach { Reset-TestState }

    It "returns early when NVIDIA GPU key not found" {
        Mock Test-Path { $false }
        Mock Get-ChildItem { @() }
        Mock Write-Step {}
        Mock Write-Warn {}

        { Apply-NvidiaCS2Profile } | Should -Not -Throw
        Should -Invoke Write-Warn -Times 1
    }
}

# ── Apply-NvidiaCS2ProfileDrs ──────────────────────────────────────────────
Describe "Apply-NvidiaCS2ProfileDrs" {

    BeforeEach { Reset-TestState }

    Context "DRY-RUN interceptor" {

        It "prints all settings in DRY-RUN mode without DRS write" {
            $SCRIPT:DryRun = $true
            Mock Write-Host {}
            Mock Write-DebugLog {}

            # The function tries Invoke-DrsSession which is mocked
            Mock Invoke-DrsSession {}

            { Apply-NvidiaCS2ProfileDrs } | Should -Not -Throw
        }
    }
}

# ── Apply-NvidiaCS2ProfileRegistry ─────────────────────────────────────────
Describe "Apply-NvidiaCS2ProfileRegistry" {

    BeforeEach { Reset-TestState }

    It "applies PerfLevelSrc via registry" {
        # Set-RegistryValue uses positional params: ($path, $name, $value, $type, $why)
        $script:capturedRegCalls = [System.Collections.Generic.List[hashtable]]::new()
        Mock Set-RegistryValue {
            $script:capturedRegCalls.Add(@{ Path = $path; Name = $name; Value = $value })
        }
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-Info {}

        Apply-NvidiaCS2ProfileRegistry -NvKeyPath "HKLM:\SYSTEM\Test\0000"

        $perfCall = $script:capturedRegCalls | Where-Object { $_.Name -eq "PerfLevelSrc" }
        $perfCall | Should -Not -BeNullOrEmpty
        $perfCall.Value | Should -Be 0x2222
    }

    It "applies at least 25 registry settings" {
        $script:capturedRegCalls = [System.Collections.Generic.List[hashtable]]::new()
        Mock Set-RegistryValue {
            $script:capturedRegCalls.Add(@{ Name = $name })
        }
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-Info {}

        Apply-NvidiaCS2ProfileRegistry -NvKeyPath "HKLM:\SYSTEM\Test\0000"

        $script:capturedRegCalls.Count | Should -BeGreaterOrEqual 25
    }

    It "uses custom FPS cap value when provided" {
        $script:capturedRegCalls = [System.Collections.Generic.List[hashtable]]::new()
        Mock Set-RegistryValue {
            $script:capturedRegCalls.Add(@{ Name = $name; Value = $value })
        }
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-Info {}

        Apply-NvidiaCS2ProfileRegistry -NvKeyPath "HKLM:\SYSTEM\Test\0000" -FrlValue 300

        $frlCall = $script:capturedRegCalls | Where-Object { $_.Name -eq "FRL_VALUE" }
        $frlCall | Should -Not -BeNullOrEmpty
        $frlCall.Value | Should -Be 300
    }

    It "includes DisableDynamicPstate in GPU class key" {
        $script:capturedRegCalls = [System.Collections.Generic.List[hashtable]]::new()
        Mock Set-RegistryValue {
            $script:capturedRegCalls.Add(@{ Name = $name; Value = $value })
        }
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-Info {}

        Apply-NvidiaCS2ProfileRegistry -NvKeyPath "HKLM:\SYSTEM\Test\0000"

        $dpCall = $script:capturedRegCalls | Where-Object { $_.Name -eq "DisableDynamicPstate" }
        $dpCall | Should -Not -BeNullOrEmpty
        $dpCall.Value | Should -Be 1
    }
}
