# ==============================================================================
#  tests/helpers/debloat.Tests.ps1  --  Bloatware removal & telemetry disable
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"

    # Stub Windows-only cmdlets before loading the module
    if ($IsWindows -eq $false) {
        if (-not (Get-Command Get-AppxPackage -ErrorAction SilentlyContinue)) {
            function global:Get-AppxPackage { param($Name, [switch]$AllUsers, $ErrorAction) $null }
        }
        if (-not (Get-Command Remove-AppxPackage -ErrorAction SilentlyContinue)) {
            function global:Remove-AppxPackage { param([Parameter(ValueFromPipeline)]$InputObject, [switch]$AllUsers, $ErrorAction) process {} }
        }
        if (-not (Get-Command Get-AppxProvisionedPackage -ErrorAction SilentlyContinue)) {
            function global:Get-AppxProvisionedPackage { param($Online, $ErrorAction) $null }
        }
        if (-not (Get-Command Remove-AppxProvisionedPackage -ErrorAction SilentlyContinue)) {
            function global:Remove-AppxProvisionedPackage { param([Parameter(ValueFromPipeline)]$InputObject, $Online, $ErrorAction) process {} }
        }
        if (-not (Get-Command Stop-Service -ErrorAction SilentlyContinue)) {
            function global:Stop-Service { param($Name, [switch]$Force, $ErrorAction) $null }
        }
    }

    . "$PSScriptRoot/../../helpers/debloat.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ── Invoke-GamingDebloat ───────────────────────────────────────────────────
Describe "Invoke-GamingDebloat" {

    BeforeEach { Reset-TestState }

    Context "AppX Package Removal" {

        It "handles no bloatware packages found (already clean)" {
            Mock Get-AppxPackage { $null }
            Mock Get-AppxProvisionedPackage { $null }
            Mock Get-ScheduledTask { $null }
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-OK {}
            Mock Write-DebugLog {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}
            Mock Backup-ServiceState {}
            Mock Stop-Service {}
            Mock Set-Service {}

            { Invoke-GamingDebloat } | Should -Not -Throw
        }

        It "reports packages that would be removed in DRY-RUN" {
            $SCRIPT:DryRun = $true
            Mock Get-AppxPackage {
                [PSCustomObject]@{ Name = "Microsoft.BingNews"; PackageFullName = "Microsoft.BingNews_1.0.0_x64" }
            }
            Mock Get-ScheduledTask { $null }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}
            Mock Write-DebugLog {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}

            $script:hostOutput = [System.Collections.Generic.List[string]]::new()
            Mock Write-Host {
                if ($Object) { $script:hostOutput.Add([string]$Object) }
            }

            Invoke-GamingDebloat

            $dryRunMsg = $script:hostOutput | Where-Object { $_ -match "DRY-RUN.*Would remove AppX" }
            $dryRunMsg | Should -Not -BeNullOrEmpty
        }

        It "does not call Remove-AppxPackage in DRY-RUN" {
            $SCRIPT:DryRun = $true
            Mock Get-AppxPackage {
                [PSCustomObject]@{ Name = "Microsoft.BingNews"; PackageFullName = "Microsoft.BingNews_1.0.0_x64" }
            }
            Mock Remove-AppxPackage {}
            Mock Get-ScheduledTask { $null }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-DebugLog {}
            Mock Write-Host {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}

            Invoke-GamingDebloat

            Should -Invoke Remove-AppxPackage -Times 0
        }
    }

    Context "Telemetry Services" {

        It "disables DiagTrack service" {
            $SCRIPT:DryRun = $false
            Mock Get-AppxPackage { $null }
            Mock Get-AppxProvisionedPackage { $null }
            Mock Get-ScheduledTask { $null }
            Mock Get-Service {
                [PSCustomObject]@{ Name = $Name; StartType = "Automatic"; Status = "Running" }
            }
            Mock Backup-ServiceState {}
            Mock Stop-Service {}
            Mock Set-Service {}
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}
            Mock Write-DebugLog {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}

            Invoke-GamingDebloat

            Should -Invoke Backup-ServiceState -ParameterFilter { $ServiceName -eq "DiagTrack" }
            Should -Invoke Set-Service -ParameterFilter { $Name -eq "DiagTrack" -and $StartupType -eq "Disabled" }
        }

        It "disables dmwappushservice" {
            $SCRIPT:DryRun = $false
            Mock Get-AppxPackage { $null }
            Mock Get-AppxProvisionedPackage { $null }
            Mock Get-ScheduledTask { $null }
            Mock Get-Service {
                [PSCustomObject]@{ Name = $Name; StartType = "Automatic"; Status = "Running" }
            }
            Mock Backup-ServiceState {}
            Mock Stop-Service {}
            Mock Set-Service {}
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}
            Mock Write-DebugLog {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}

            Invoke-GamingDebloat

            Should -Invoke Set-Service -ParameterFilter { $Name -eq "dmwappushservice" }
        }

        It "skips service disable in DRY-RUN" {
            $SCRIPT:DryRun = $true
            Mock Get-AppxPackage { $null }
            Mock Get-ScheduledTask { $null }
            Mock Set-Service {}
            Mock Stop-Service {}
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-DebugLog {}
            Mock Write-Host {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}

            Invoke-GamingDebloat

            Should -Invoke Set-Service -Times 0
        }

        It "handles service not found gracefully" {
            $SCRIPT:DryRun = $false
            Mock Get-AppxPackage { $null }
            Mock Get-AppxProvisionedPackage { $null }
            Mock Get-ScheduledTask { $null }
            Mock Get-Service {
                [PSCustomObject]@{ Name = $Name; StartType = "Automatic"; Status = "Running" }
            }
            Mock Backup-ServiceState { throw "Service not found" }
            Mock Stop-Service {}
            Mock Set-Service {}
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}
            Mock Write-DebugLog {}
            Mock Write-Warn {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}

            { Invoke-GamingDebloat } | Should -Not -Throw
        }
    }

    Context "Telemetry Scheduled Tasks" {

        It "disables telemetry tasks when found" {
            $SCRIPT:DryRun = $false
            Mock Get-AppxPackage { $null }
            Mock Get-AppxProvisionedPackage { $null }
            Mock Get-ScheduledTask {
                @([PSCustomObject]@{ TaskName = "TestTask"; TaskPath = "\Microsoft\Windows\Test\"; State = "Ready" })
            }
            Mock Backup-ScheduledTask {}
            Mock Disable-ScheduledTask { $null }
            Mock Backup-ServiceState {}
            Mock Stop-Service {}
            Mock Set-Service {}
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}
            Mock Write-DebugLog {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}

            Invoke-GamingDebloat

            # 2 task paths × 1 task each = 2 disables
            Should -Invoke Disable-ScheduledTask -Times 2
        }

        It "skips task disable in DRY-RUN" {
            $SCRIPT:DryRun = $true
            Mock Get-AppxPackage { $null }
            Mock Get-ScheduledTask {
                @([PSCustomObject]@{ TaskName = "ProgramDataUpdater"; TaskPath = "\Microsoft\Windows\Application Experience\" })
            }
            Mock Disable-ScheduledTask {}
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-DebugLog {}
            Mock Write-Host {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}

            Invoke-GamingDebloat

            Should -Invoke Disable-ScheduledTask -Times 0
        }
    }

    Context "Consumer Features & Advertising" {

        It "disables consumer features via registry" {
            Mock Get-AppxPackage { $null }
            Mock Get-AppxProvisionedPackage { $null }
            Mock Get-ScheduledTask { $null }
            Mock Backup-ServiceState {}
            Mock Stop-Service {}
            Mock Set-Service {}
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}
            Mock Write-DebugLog {}
            Mock Write-ActionOK {}

            $script:regCalls = [System.Collections.Generic.List[hashtable]]::new()
            Mock Set-RegistryValue {
                $script:regCalls.Add(@{ Path = $path; Name = $name; Value = $value })
            }

            Invoke-GamingDebloat

            $consumerCall = $script:regCalls | Where-Object { $_.Name -eq "DisableWindowsConsumerFeatures" }
            $consumerCall | Should -Not -BeNullOrEmpty
            $consumerCall.Value | Should -Be 1

            $adCall = $script:regCalls | Where-Object { $_.Name -eq "Enabled" -and $_.Path -match "AdvertisingInfo" }
            $adCall | Should -Not -BeNullOrEmpty
            $adCall.Value | Should -Be 0
        }
    }

    Context "AppX cmdlets unavailable (Server Core / LTSC)" {

        BeforeEach { Reset-TestState }

        It "skips AppX removal when cmdlets not available" {
            Mock Get-Command { $null } -ParameterFilter { $Name -eq "Get-AppxPackage" }
            # This test verifies the guard works — on platforms where
            # Get-AppxPackage doesn't exist, it should fall through to
            # telemetry services without error.
            Mock Get-ScheduledTask { $null }
            Mock Backup-ServiceState {}
            Mock Stop-Service {}
            Mock Set-Service {}
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}
            Mock Write-DebugLog {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}

            { Invoke-GamingDebloat } | Should -Not -Throw
        }
    }
}
