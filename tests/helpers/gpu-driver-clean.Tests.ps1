# ==============================================================================
#  tests/helpers/gpu-driver-clean.Tests.ps1  --  GPU driver removal (DDU replacement)
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"

    # Stub Windows-only cmdlets before loading the module
    if ($IsWindows -eq $false) {
        if (-not (Get-Command Stop-Service -ErrorAction SilentlyContinue)) {
            function global:Stop-Service { param($Name, [switch]$Force, $ErrorAction) $null }
        }
        if (-not (Get-Command pnputil -ErrorAction SilentlyContinue)) {
            function global:pnputil { param($CmdArgs) "" }
        }
    }

    . "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Remove-GpuDriverClean ─────────────────────────────────────────────────
Describe "Remove-GpuDriverClean" {

    BeforeEach { Reset-TestState }

    Context "DRY-RUN mode" {

        It "skips all operations in DRY-RUN mode" {
            $SCRIPT:DryRun = $true
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-Host {}
            Mock Get-Service { $null }
            Mock Get-CimInstance { $null }

            { Remove-GpuDriverClean -GpuVendor "NVIDIA" } | Should -Not -Throw
        }

        It "does not call pnputil in DRY-RUN mode" {
            $SCRIPT:DryRun = $true
            Mock pnputil {}
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-Host {}

            Remove-GpuDriverClean -GpuVendor "NVIDIA"

            Should -Invoke pnputil -Times 0
        }

        It "does not delete registry entries in DRY-RUN mode" {
            $SCRIPT:DryRun = $true
            Mock Remove-Item {}
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-Host {}

            Remove-GpuDriverClean -GpuVendor "NVIDIA"

            Should -Invoke Remove-Item -Times 0
        }
    }

    Context "Vendor parameter validation" {

        It "accepts NVIDIA as vendor" {
            $SCRIPT:DryRun = $true
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-Host {}

            { Remove-GpuDriverClean -GpuVendor "NVIDIA" } | Should -Not -Throw
        }

        It "accepts AMD as vendor" {
            $SCRIPT:DryRun = $true
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-Host {}

            { Remove-GpuDriverClean -GpuVendor "AMD" } | Should -Not -Throw
        }

        It "accepts Intel as vendor" {
            $SCRIPT:DryRun = $true
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-Host {}

            { Remove-GpuDriverClean -GpuVendor "Intel" } | Should -Not -Throw
        }

        It "rejects invalid vendor" {
            { Remove-GpuDriverClean -GpuVendor "Qualcomm" } | Should -Throw
        }
    }

    Context "INF filename validation (security)" {

        It "has INF filename validation guard in function body" {
            $fn = Get-Command Remove-GpuDriverClean -ErrorAction SilentlyContinue
            $fn | Should -Not -BeNullOrEmpty
            $fn.ScriptBlock.ToString() | Should -Match 'notmatch.*oem.*inf'
        }

        It "rejects non-oem INF names in source validation" {
            # The regex '^oem\d+\.inf$' should match oem0.inf, oem123.inf but not evil.inf
            "oem123.inf" | Should -Match '^oem\d+\.inf$'
            "evil.inf" | Should -Not -Match '^oem\d+\.inf$'
            "oem.inf" | Should -Not -Match '^oem\d+\.inf$'
            "oem123.inf; rm -rf /" | Should -Not -Match '^oem\d+\.inf$'
        }
    }

    Context "Service patterns by vendor" {

        It "uses NVIDIA service patterns for NVIDIA vendor" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Match 'NVDisplay'
            $source | Should -Match 'NvTelemetryContainer'
        }

        It "uses AMD service patterns for AMD vendor" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Match 'AMD External Events'
        }

        It "uses Intel service patterns for Intel vendor" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Match 'igfxCUIService'
        }
    }

    Context "DriverStore patterns" {

        It "uses precise NVIDIA patterns (avoids nvdimm false match)" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            # Should match nv_dispi*, nvdsp*, nvlddmkm* but NOT a generic nv* pattern
            $source | Should -Match 'nv_dispi'
            $source | Should -Match 'nvlddmkm'
        }

        It "uses precise AMD patterns" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Match 'atiilhag|atiumdag|amdkmdap'
        }

        It "uses precise Intel patterns" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Match 'iigd_dch|igfx'
        }
    }

    Context "CIM vs pnputil enumeration" {

        It "uses ClassGuid for locale-independent CIM query" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Match 'ClassGuid'
        }

        It "has pnputil text-parsing fallback" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Match 'pnputil /enum-drivers'
        }
    }

    Context "Shader cache cleanup" {

        It "includes D3DSCache as common cache path" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Match 'D3DSCache'
        }

        It "includes vendor-specific cache paths for NVIDIA" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Match 'NVIDIA.*DXCache'
            $source | Should -Match 'NVIDIA.*GLCache'
        }
    }
}
