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
        if (-not (Get-Command Get-AppxPackage -ErrorAction SilentlyContinue)) {
            function global:Get-AppxPackage { @() }
        }
        if (-not (Get-Command Get-AppxProvisionedPackage -ErrorAction SilentlyContinue)) {
            function global:Get-AppxProvisionedPackage { @() }
        }
        if (-not (Get-Command Remove-AppxPackage -ErrorAction SilentlyContinue)) {
            function global:Remove-AppxPackage { param($Package, [switch]$AllUsers, $ErrorAction) }
        }
        if (-not (Get-Command Remove-AppxProvisionedPackage -ErrorAction SilentlyContinue)) {
            function global:Remove-AppxProvisionedPackage {
                param([string]$PackageName, [switch]$Online, $ErrorAction)
                $PackageName
            }
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

    BeforeEach {
        Reset-TestState
        Mock Invoke-GpuPnpUtilDelete {
            [PSCustomObject]@{ ExitCode = 0; Output = @('deleted') }
        }
    }

    Context "DRY-RUN mode" {

        It "skips all operations in DRY-RUN mode" {
            $SCRIPT:DryRun = $true
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-ConsoleLine {}
            Mock Get-Service { $null }
            Mock Get-CimInstance { $null }

            { Remove-GpuDriverClean -GpuVendor "NVIDIA" } | Should -Not -Throw
        }

        It "does not call the native pnputil seam in DRY-RUN mode" {
            $SCRIPT:DryRun = $true
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-ConsoleLine {}

            Remove-GpuDriverClean -GpuVendor "NVIDIA"

            Should -Invoke Invoke-GpuPnpUtilDelete -Times 0
        }

        It "does not delete registry entries in DRY-RUN mode" {
            $SCRIPT:DryRun = $true
            Mock Remove-Item {}
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-ConsoleLine {}

            Remove-GpuDriverClean -GpuVendor "NVIDIA"

            Should -Invoke Remove-Item -Times 0
        }

        It "returns a non-completing dry-run result when requested" {
            $SCRIPT:DryRun = $true
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-ConsoleLine {}

            $result = Remove-GpuDriverClean -GpuVendor "NVIDIA" -PassThru

            $result.Status | Should -Be "DryRun"
            $result.Applied | Should -BeFalse
            $result.CanCompleteStep | Should -BeFalse
        }
    }

    Context "Structured result contract" {

        BeforeEach {
            $SCRIPT:DryRun = $false
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-ConsoleLine {}
            Mock Write-OK {}
            Mock Write-Warn {}
            Mock Write-DebugLog {}
            Mock Write-Blank {}
            Mock Backup-ServiceState {}
            Mock Get-Service { @() }
            Mock Stop-Service {}
            Mock Set-Service {}
            Mock Get-ScheduledTask { @() }
            Mock Test-Path { $false }
            Mock Remove-Item {}
            Mock Get-Command { $null } -ParameterFilter { $Name -contains "Get-AppxPackage" }
        }

        It "returns success only when driver package removal succeeds" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                if ($script:cimCalls -eq 1) {
                    [PSCustomObject]@{
                        ClassGuid = $CFG_GUID_Display
                        DriverProviderName = "NVIDIA"
                        InfName = "oem12.inf"
                    }
                } else {
                    @()
                }
            }
            Mock Get-Command { [PSCustomObject]@{ Name = 'Get-AppxPackage' } } -ParameterFilter { $Name -eq 'Get-AppxPackage' }
            Mock Get-AppxPackage { @() }
            Mock Get-AppxProvisionedPackage { @() }
            $result = Remove-GpuDriverClean -GpuVendor "NVIDIA" -PassThru

            $result.Status | Should -Be "Success"
            $result.Applied | Should -BeTrue
            $result.CanCompleteStep | Should -BeTrue
            $result.FoundDriverPackages | Should -Be 1
            $result.RemovedDriverPackages | Should -Be 1
            $result.FailedDriverPackages | Should -Be 0
            $result.DriverRemovalVerified | Should -BeTrue
            $result.CleanupFailures | Should -Be 0
            $result.CleanupSkipped | Should -Be 0
        }

        It "returns already absent only when locale-independent CIM enumeration proves no matching package" {
            Mock Get-CimInstance { @() }
            $result = Remove-GpuDriverClean -GpuVendor "NVIDIA" -PassThru

            $result.Status | Should -Be "AlreadyAbsent"
            $result.Applied | Should -BeFalse
            $result.CanCompleteStep | Should -BeTrue
            $result.AlreadyAbsent | Should -BeTrue
            $result.FoundDriverPackages | Should -Be 0
        }

        It "discovers the standard Advanced Micro Devices provider name" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                if ($script:cimCalls -eq 1) {
                    [PSCustomObject]@{
                        ClassGuid = $CFG_GUID_Display
                        DriverProviderName = "Advanced Micro Devices, Inc."
                        InfName = "oem42.inf"
                    }
                } else {
                    @()
                }
            }
            $result = Remove-GpuDriverClean -GpuVendor "AMD" -PassThru

            $result.Status | Should -Be "Success"
            $result.FoundDriverPackages | Should -Be 1
            $result.RemovedDriverPackages | Should -Be 1
        }

        It "deduplicates a valid OEM INF referenced by multiple matching device rows" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                if ($script:cimCalls -eq 1) {
                    @(
                        [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'NVIDIA'; InfName = 'oem12.inf' }
                        [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'NVIDIA'; InfName = 'oem12.inf' }
                    )
                } else { @() }
            }
            $result = Remove-GpuDriverClean -GpuVendor NVIDIA -PassThru

            $result.Status | Should -Be 'Success'
            $result.FoundDriverPackages | Should -Be 1
            $result.RemovedDriverPackages | Should -Be 1
        }

        It "does not complete when no package is found after untrusted enumeration" {
            Mock Get-CimInstance { throw "CIM unavailable" }
            $result = Remove-GpuDriverClean -GpuVendor "NVIDIA" -PassThru

            $result.Status | Should -Be "Failed"
            $result.Applied | Should -BeFalse
            $result.CanCompleteStep | Should -BeFalse
            $result.AlreadyAbsent | Should -BeFalse
            $result.FoundDriverPackages | Should -Be 0
            Should -Invoke Remove-Item -Times 0
            Should -Invoke Stop-Service -Times 0
            Should -Invoke Set-Service -Times 0
        }

        It "fails closed on matching display rows with unusable INF names" -TestCases @(
            @{ InfName = $null; Label = 'missing' }
            @{ InfName = 'nv_dispi.inf'; Label = 'non-OEM' }
            @{ InfName = 'oem12.inf;evil'; Label = 'malformed' }
        ) {
            param($InfName, $Label)
            Mock Get-CimInstance {
                [PSCustomObject]@{
                    ClassGuid = $CFG_GUID_Display
                    DriverProviderName = 'NVIDIA'
                    InfName = $InfName
                }
            }
            Mock Invoke-GpuPnpUtilDelete {}

            $result = Remove-GpuDriverClean -GpuVendor NVIDIA -PassThru

            $result.Status | Should -Be 'Failed' -Because "$Label INF ownership is not safe to delete"
            $result.CanCompleteStep | Should -BeFalse
            $result.FoundDriverPackages | Should -Be 1
            $result.FailedDriverPackages | Should -Be 1
            $result.UnknownDriverPackages | Should -Be 0
            Should -Invoke Invoke-GpuPnpUtilDelete -Exactly 0
            Should -Invoke Remove-Item -Exactly 0
        }

        It "does not complete when all driver package removals fail" {
            Mock Get-CimInstance {
                [PSCustomObject]@{
                    ClassGuid = $CFG_GUID_Display
                    DriverProviderName = "NVIDIA"
                    InfName = "oem12.inf"
                }
            }
            Mock Invoke-GpuPnpUtilDelete {
                [PSCustomObject]@{ ExitCode = 5; Output = @('access denied') }
            }

            $result = Remove-GpuDriverClean -GpuVendor "NVIDIA" -PassThru

            $result.Status | Should -Be "Failed"
            $result.Applied | Should -BeFalse
            $result.CanCompleteStep | Should -BeFalse
            $result.FoundDriverPackages | Should -Be 1
            $result.RemovedDriverPackages | Should -Be 0
            $result.FailedDriverPackages | Should -Be 1
            Should -Invoke Remove-Item -Times 0
            Should -Invoke Stop-Service -Times 0
            Should -Invoke Set-Service -Times 0
        }

        It "classifies native successes as unknown when the authoritative post-query fails" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                if ($script:cimCalls -eq 1) {
                    [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'NVIDIA'; InfName = 'oem12.inf' }
                } else {
                    throw 'CIM post-query unavailable'
                }
            }
            $result = Remove-GpuDriverClean -GpuVendor NVIDIA -PassThru

            $result.Status | Should -Be 'Failed'
            $result.Applied | Should -BeFalse
            $result.FoundDriverPackages | Should -Be 1
            $result.RemovedDriverPackages | Should -Be 0
            $result.FailedDriverPackages | Should -Be 0
            $result.UnknownDriverPackages | Should -Be 1
            ($result.RemovedDriverPackages + $result.FailedDriverPackages + $result.UnknownDriverPackages) |
                Should -Be $result.FoundDriverPackages
        }

        It "does not complete when only part of driver package removal succeeds" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                if ($script:cimCalls -eq 1) {
                    @(
                        [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'NVIDIA'; InfName = 'oem12.inf' }
                        [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'NVIDIA'; InfName = 'oem13.inf' }
                    )
                } else {
                    [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'NVIDIA'; InfName = 'oem13.inf' }
                }
            }
            Mock Invoke-GpuPnpUtilDelete {
                if ($InfName -eq 'oem12.inf') {
                    return [PSCustomObject]@{ ExitCode = 0; Output = @('deleted') }
                }
                [PSCustomObject]@{ ExitCode = 5; Output = @('access denied') }
            }

            $result = Remove-GpuDriverClean -GpuVendor "NVIDIA" -PassThru

            $result.Status | Should -Be "Partial"
            $result.Applied | Should -BeTrue
            $result.CanCompleteStep | Should -BeFalse
            $result.FoundDriverPackages | Should -Be 2
            $result.RemovedDriverPackages | Should -Be 1
            $result.FailedDriverPackages | Should -Be 1
            $result.UnknownDriverPackages | Should -Be 0
            Should -Invoke Remove-Item -Times 0
            Should -Invoke Stop-Service -Times 0
            Should -Invoke Set-Service -Times 0
        }
    }

    Context "truthful cleanup outcomes" {

        BeforeEach {
            $SCRIPT:DryRun = $false
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-ConsoleLine {}
            Mock Write-OK {}
            Mock Write-Warn {}
            Mock Write-DebugLog {}
            Mock Write-Blank {}
            Mock Backup-ServiceState {}
            Mock Get-Service { @() }
            Mock Stop-Service {}
            Mock Set-Service {}
            Mock Get-ScheduledTask { @() }
            Mock Test-Path { $false }
            Mock Remove-Item {}
            Mock Get-GpuVendorApplicationCleanupTargets { @() }
        }

        It "fails closed when pnputil reports success but post-removal CIM still finds the INF" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                [PSCustomObject]@{
                    ClassGuid = $CFG_GUID_Display
                    DriverProviderName = 'Intel'
                    InfName = 'oem12.inf'
                }
            }
            $result = Remove-GpuDriverClean -GpuVendor Intel -PassThru

            $result.Status | Should -Be 'Failed'
            $result.DriverRemovalVerified | Should -BeFalse
            $result.RemovedDriverPackages | Should -Be 0
            $result.FailedDriverPackages | Should -Be 1
            $result.CanCompleteStep | Should -BeFalse
            Should -Invoke Stop-Service -Times 0
        }

        It "fails closed when a different matching vendor INF appears after removal" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                if ($script:cimCalls -eq 1) {
                    [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'NVIDIA'; InfName = 'oem12.inf' }
                } else {
                    [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'NVIDIA'; InfName = 'oem99.inf' }
                }
            }

            $result = Remove-GpuDriverClean -GpuVendor NVIDIA -PassThru

            $result.Status | Should -Be 'Partial'
            $result.DriverRemovalVerified | Should -BeFalse
            $result.RemovedDriverPackages | Should -Be 1
            $result.CanCompleteStep | Should -BeFalse
            Should -Invoke Stop-Service -Exactly 0
            Should -Invoke Remove-Item -Exactly 0
        }

        It "marks a discovered service stop or disable failure as partial" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                if ($script:cimCalls -eq 1) {
                    [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'Intel'; InfName = 'oem12.inf' }
                } else { @() }
            }
            Mock Get-Service { [PSCustomObject]@{ Name = 'igfxCUIService' } }
            Mock Stop-Service { throw 'access denied' }
            $result = Remove-GpuDriverClean -GpuVendor Intel -PassThru

            $result.Status | Should -Be 'Partial'
            $result.CleanupFailures | Should -BeGreaterThan 0
            $result.CanCompleteStep | Should -BeFalse
        }

        It "unregisters a discovered NVIDIA task from its exact task path" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                if ($script:cimCalls -eq 1) {
                    [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'NVIDIA'; InfName = 'oem12.inf' }
                } else { @() }
            }
            Mock Get-ScheduledTask {
                [PSCustomObject]@{ TaskName = 'NvTelemetryDaily'; TaskPath = '\VendorFolder\' }
            }
            Mock Unregister-ScheduledTask {}
            $result = Remove-GpuDriverClean -GpuVendor NVIDIA -PassThru

            $result.Status | Should -Be 'Success'
            Should -Invoke Unregister-ScheduledTask -Exactly 1 -ParameterFilter {
                $TaskName -eq 'NvTelemetryDaily' -and $TaskPath -eq '\VendorFolder\'
            }
        }

        It "does not invoke AMD-wide application-directory cleanup" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                if ($script:cimCalls -eq 1) {
                    [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'AMD'; InfName = 'oem12.inf' }
                } else { @() }
            }
            Mock Get-GpuVendorApplicationCleanupTargets { throw 'AMD-wide cleanup must not be enumerated' }
            $result = Remove-GpuDriverClean -GpuVendor AMD -PassThru

            $result.Status | Should -Be 'Success'
            $result.SoftwareRemoved | Should -Be 0
            $result.CleanupFailures | Should -Be 0
            $result.CanCompleteStep | Should -BeTrue
            Should -Invoke Get-GpuVendorApplicationCleanupTargets -Exactly 0
            Should -Invoke Remove-Item -Exactly 0 -ParameterFilter { $LiteralPath -eq 'C:\Program Files\AMD' }
        }

        It "does not delete AMD-wide registry paths" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                if ($script:cimCalls -eq 1) {
                    [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'AMD'; InfName = 'oem12.inf' }
                } else { @() }
            }
            Mock Remove-Item { throw 'registry denied' } -ParameterFilter { $Path -eq 'HKLM:\SOFTWARE\AMD' }
            $result = Remove-GpuDriverClean -GpuVendor AMD -PassThru

            $result.Status | Should -Be 'Success'
            $result.CleanupFailures | Should -Be 0
            $result.CanCompleteStep | Should -BeTrue
            Should -Invoke Remove-Item -Exactly 0 -ParameterFilter { $Path -eq 'HKLM:\SOFTWARE\AMD' }
        }

        It "defers NVIDIA AppX cleanup to Normal Mode without blocking the handoff" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                if ($script:cimCalls -eq 1) {
                    [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'NVIDIA'; InfName = 'oem12.inf' }
                } else { @() }
            }
            Mock Get-AppxPackage { throw 'Safe Mode must not enumerate AppX' }
            Mock Get-AppxProvisionedPackage { throw 'Safe Mode must not enumerate provisioned AppX' }
            $result = Remove-GpuDriverClean -GpuVendor NVIDIA -PassThru

            $result.Status | Should -Be 'Success'
            $result.CleanupSkipped | Should -Be 0
            $result.CleanupDeferred | Should -BeGreaterThan 0
            $result.CanCompleteStep | Should -BeTrue
            Should -Invoke Get-AppxPackage -Exactly 0
            Should -Invoke Get-AppxProvisionedPackage -Exactly 0
        }

        It "does not perform any NVIDIA AppX mutation in Safe Mode" {
            $script:cimCalls = 0
            Mock Get-CimInstance {
                $script:cimCalls++
                if ($script:cimCalls -eq 1) {
                    [PSCustomObject]@{ ClassGuid = $CFG_GUID_Display; DriverProviderName = 'NVIDIA'; InfName = 'oem12.inf' }
                } else { @() }
            }
            Mock Remove-AppxPackage {}
            Mock Remove-AppxProvisionedPackage {}
            $result = Remove-GpuDriverClean -GpuVendor NVIDIA -PassThru

            $result.Status | Should -Be 'Success'
            $result.CleanupDeferred | Should -BeGreaterThan 0
            Should -Invoke Remove-AppxPackage -Exactly 0
            Should -Invoke Remove-AppxProvisionedPackage -Exactly 0
        }
    }

    Context "Vendor parameter validation" {

        It "accepts NVIDIA as vendor" {
            $SCRIPT:DryRun = $true
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-ConsoleLine {}

            { Remove-GpuDriverClean -GpuVendor "NVIDIA" } | Should -Not -Throw
        }

        It "accepts AMD as vendor" {
            $SCRIPT:DryRun = $true
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-ConsoleLine {}

            { Remove-GpuDriverClean -GpuVendor "AMD" } | Should -Not -Throw
        }

        It "accepts Intel as vendor" {
            $SCRIPT:DryRun = $true
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-ConsoleLine {}

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
            $source | Should -Not -Match 'AMDRyzenMasterDriver'
        }

        It "uses Intel service patterns for Intel vendor" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Match 'igfxCUIService'
        }
    }

    Context "DriverStore safety" {

        It "does not manually delete FileRepository folders" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Not -Match 'Remove-Item \$f\.FullName'
            $source | Should -Not -Match 'Get-ChildItem \$driverStore'
        }
    }

    Context "CIM vs pnputil enumeration" {

        It "uses ClassGuid for locale-independent CIM query" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Match 'ClassGuid'
        }

        It "reports pnputil as a manual investigation command when CIM is unavailable" {
            $source = Get-Content "$PSScriptRoot/../../helpers/gpu-driver-clean.ps1" -Raw
            $source | Should -Match 'pnputil /enum-drivers'
        }

        It "resolves the inbox pnputil executable without PATH lookup on Windows" {
            $source = (Get-Command Get-GpuPnpUtilPath).ScriptBlock.ToString()

            $source | Should -Match 'SystemDirectory'
            $source | Should -Match "Join-Path.*pnputil\.exe"
            $source | Should -Match 'DriveType.*Fixed'
            (Get-Command Remove-GpuDriverClean).ScriptBlock.ToString() |
                Should -Not -Match '(?m)^\s*pnputil\s+/delete-driver'
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

        It "never selects an entire vendor data root for recursive deletion" {
            $targets = @(Get-GpuCacheCleanupTargets -GpuVendor NVIDIA)

            $targets.Path | Should -Not -Contain (Join-Path $targets[0].Root "NVIDIA Corporation")
            $targets.Path | Should -Not -Contain (Join-Path $targets[0].Root "NVIDIA")
            @($targets.Path | Where-Object { $_ -match '(DXCache|GLCache|NV_Cache|D3DSCache)$' }).Count |
                Should -Be $targets.Count
        }

        It "uses known-folder APIs instead of mutable environment variables for cache targets" {
            $source = (Get-Command Get-GpuCacheCleanupTargets).ScriptBlock.ToString()

            $source | Should -Match 'GetFolderPath'
            $source | Should -Not -Match '\$env:'
        }

        It "uses known folders for vendor application cleanup despite mutable process environment" {
            $originalProgramFiles = $env:ProgramFiles
            $env:ProgramFiles = (Join-Path $SCRIPT:TestTempRoot 'attacker-controlled-program-files')
            try {
                $targets = @(Get-GpuVendorApplicationCleanupTargets -GpuVendor NVIDIA)
            } finally {
                $env:ProgramFiles = $originalProgramFiles
            }

            $targets.Path | Should -Not -Match 'attacker-controlled-program-files'
            $source = (Get-Command Get-GpuVendorApplicationCleanupTargets).ScriptBlock.ToString()
            $source | Should -Match 'GetFolderPath'
            $source | Should -Not -Match '\$env:'
        }

        It "does not select AMD-wide application roots for recursive deletion" {
            @(Get-GpuVendorApplicationCleanupTargets -GpuVendor AMD).Count | Should -Be 0
        }

        It "rejects wildcard cleanup paths before any item lookup" {
            $root = Join-Path $SCRIPT:TestTempRoot 'cache-root'
            $wildcardTarget = Join-Path $root 'NVIDIA*'
            Mock Get-Item { throw 'Get-Item should not be reached for a wildcard path' }

            Test-GpuCacheCleanupTarget -Root $root -Path $wildcardTarget | Should -BeFalse
            Should -Invoke Get-Item -Exactly 0
        }

        It "rejects a reparse-point known-folder root" {
            $root = Join-Path $SCRIPT:TestTempRoot 'cache-root'
            $path = Join-Path $root 'NVIDIA/DXCache'
            $rootItem = [PSCustomObject]@{
                FullName = $root
                Parent = $null
                Attributes = ([IO.FileAttributes]::Directory -bor [IO.FileAttributes]::ReparsePoint)
            }
            Mock Get-Item { $rootItem }

            Test-GpuCacheCleanupTarget -Root $root -Path $path | Should -BeFalse
        }

        It "rejects a cache directory that is a reparse point" {
            $root = Join-Path $SCRIPT:TestTempRoot "cache-root"
            $path = Join-Path $root "NVIDIA/DXCache"
            $rootItem = [PSCustomObject]@{ FullName = $root; Parent = $null; Attributes = [IO.FileAttributes]::Directory }
            $vendorItem = [PSCustomObject]@{ FullName = (Split-Path $path -Parent); Parent = $rootItem; Attributes = [IO.FileAttributes]::Directory }
            $cacheItem = [PSCustomObject]@{
                FullName = $path
                Parent = $vendorItem
                Attributes = ([IO.FileAttributes]::Directory -bor [IO.FileAttributes]::ReparsePoint)
            }
            Mock Get-Item {
                if ($LiteralPath -eq $root) { return $rootItem }
                return $cacheItem
            }

            Test-GpuCacheCleanupTarget -Root $root -Path $path | Should -BeFalse
        }

        It "rejects a UNC cleanup root before item lookup" {
            Mock Get-Item { throw 'UNC paths must be rejected before lookup' }

            Test-GpuCacheCleanupTarget -Root '\\server\profile' -Path '\\server\profile\NVIDIA\DXCache' |
                Should -BeFalse
            Should -Invoke Get-Item -Exactly 0
        }

        It "rejects an intermediate directory reparse point" {
            $root = Join-Path $SCRIPT:TestTempRoot 'intermediate-root'
            $vendor = Join-Path $root 'NVIDIA'
            $path = Join-Path $vendor 'DXCache'
            $rootItem = [PSCustomObject]@{ FullName = $root; Parent = $null; PSIsContainer = $true; Attributes = [IO.FileAttributes]::Directory }
            $vendorItem = [PSCustomObject]@{
                FullName = $vendor; Parent = $rootItem; PSIsContainer = $true
                Attributes = ([IO.FileAttributes]::Directory -bor [IO.FileAttributes]::ReparsePoint)
            }
            $cacheItem = [PSCustomObject]@{ FullName = $path; Parent = $vendorItem; PSIsContainer = $true; Attributes = [IO.FileAttributes]::Directory }
            Mock Get-Item {
                if ($LiteralPath -eq $root) { $rootItem } else { $cacheItem }
            }

            Test-GpuCacheCleanupTarget -Root $root -Path $path | Should -BeFalse
        }

        It "rejects a nested descendant reparse point" {
            $root = Join-Path $SCRIPT:TestTempRoot 'nested-root'
            $path = Join-Path $root 'DXCache'
            $rootItem = [PSCustomObject]@{ FullName = $root; Parent = $null; PSIsContainer = $true; Attributes = [IO.FileAttributes]::Directory }
            $cacheItem = [PSCustomObject]@{ FullName = $path; Parent = $rootItem; PSIsContainer = $true; Attributes = [IO.FileAttributes]::Directory }
            $nested = [PSCustomObject]@{
                FullName = (Join-Path $path 'redirect'); PSIsContainer = $true
                Attributes = ([IO.FileAttributes]::Directory -bor [IO.FileAttributes]::ReparsePoint)
            }
            Mock Get-Item { if ($LiteralPath -eq $root) { $rootItem } else { $cacheItem } }
            Mock Get-ChildItem { @($nested) }

            Test-GpuCacheCleanupTarget -Root $root -Path $path | Should -BeFalse
        }

        It "accepts a physical local descendant tree" {
            $root = Join-Path $SCRIPT:TestTempRoot 'physical-root'
            $path = Join-Path $root 'NVIDIA/DXCache'
            New-Item -ItemType Directory -Path $path -Force | Out-Null
            New-Item -ItemType File -Path (Join-Path $path 'cache.bin') -Force | Out-Null

            Test-GpuCacheCleanupTarget -Root $root -Path $path | Should -BeTrue
        }

        It "anchors elevated LocalApplicationData cleanup to the machine ProfileList path on Windows" {
            $source = (Get-Command Test-GpuCacheCleanupTarget).ScriptBlock.ToString()

            $source | Should -Match 'ProfileList'
            $source | Should -Match 'WindowsIdentity'
            $source | Should -Match 'DriveType.*Fixed'
        }
    }
}
