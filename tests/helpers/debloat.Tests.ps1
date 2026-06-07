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

        It "includes Win11 25H2 candidate packages in the explicit allowlist" {
            $packages = Get-GamingDebloatPackageNames

            $packages | Should -Contain "Microsoft.OutlookForWindows"
            $packages | Should -Contain "Microsoft.Windows.DevHome"
            $packages | Should -Contain "MSTeams"
            $packages | Should -Contain "Microsoft.BingSearch"
            $packages | Should -Contain "Microsoft.PowerAutomateDesktop"
        }

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
            Mock Write-ConsoleLine {
                if ($Message) { $script:hostOutput.Add([string]$Message) }
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
            Mock Write-ConsoleLine {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}

            Invoke-GamingDebloat

            Should -Invoke Remove-AppxPackage -Times 0
        }

        It "removes provisioned-only allowlist packages" {
            $SCRIPT:DryRun = $false
            Mock Get-AppxPackage { $null }
            Mock Get-AppxProvisionedPackage {
                @([PSCustomObject]@{
                    DisplayName = "Microsoft.Windows.DevHome"
                    PackageName = "Microsoft.Windows.DevHome_1.0.0.0_neutral_8wekyb3d8bbwe"
                })
            }
            Mock Remove-AppxProvisionedPackage {}
            Mock Get-ScheduledTask { $null }
            Mock Backup-ServiceState {}
            Mock Stop-Service {}
            Mock Set-Service {}
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-Info {}
            Mock Write-DebugLog {}
            Mock Write-Warn {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}

            Invoke-GamingDebloat

            Should -Invoke Remove-AppxProvisionedPackage -Times 1
        }

        It "does not call Remove-AppxProvisionedPackage in DRY-RUN" {
            $SCRIPT:DryRun = $true
            Mock Get-AppxPackage { $null }
            Mock Get-AppxProvisionedPackage {
                @([PSCustomObject]@{
                    DisplayName = "Microsoft.Windows.DevHome"
                    PackageName = "Microsoft.Windows.DevHome_1.0.0.0_neutral_8wekyb3d8bbwe"
                })
            }
            Mock Remove-AppxProvisionedPackage {}
            Mock Get-ScheduledTask { $null }
            Mock Write-Step {}
            Mock Write-OK {}
            Mock Write-DebugLog {}
            Mock Write-ConsoleLine {}
            Mock Set-RegistryValue {}
            Mock Write-ActionOK {}

            Invoke-GamingDebloat

            Should -Invoke Remove-AppxProvisionedPackage -Times 0
        }
    }

    Context "Preflight inventory" {

        It "reports matched AppX, provisioned packages, services, and tasks" {
            Mock Get-AppxPackage {
                if ($Name -eq "Microsoft.BingSearch") {
                    [PSCustomObject]@{
                        Name = $Name
                        PackageFullName = "Microsoft.BingSearch_1.0.0.0_neutral_8wekyb3d8bbwe"
                    }
                }
            }
            Mock Get-AppxProvisionedPackage {
                @([PSCustomObject]@{
                    DisplayName = "Microsoft.Windows.DevHome"
                    PackageName = "Microsoft.Windows.DevHome_1.0.0.0_neutral_8wekyb3d8bbwe"
                })
            }
            Mock Get-Service {
                if ($Name -eq "DiagTrack") {
                    [PSCustomObject]@{ Name = $Name; StartType = "Automatic"; Status = "Running" }
                }
            }
            Mock Get-ScheduledTask {
                if ($TaskPath -eq "\Microsoft\Windows\Application Experience\") {
                    [PSCustomObject]@{
                        TaskName = "ProgramDataUpdater"
                        TaskPath = $TaskPath
                        State = "Ready"
                    }
                }
            }

            $inventory = Get-GamingDebloatInventory

            $inventory.AppxAvailable | Should -BeTrue
            @($inventory.InstalledPackages).Count | Should -Be 1
            $inventory.InstalledPackages[0].Name | Should -Be "Microsoft.BingSearch"
            @($inventory.ProvisionedPackages).Count | Should -Be 1
            $inventory.ProvisionedPackages[0].Name | Should -Be "Microsoft.Windows.DevHome"
            @($inventory.Services | Where-Object { $_.NeedsDisable }).Count | Should -Be 1
            @($inventory.Tasks | Where-Object { $_.NeedsDisable }).Count | Should -Be 1
        }

        It "prints a preflight summary before mutation" {
            $inventory = [PSCustomObject]@{
                AppxAvailable = $true
                InstalledPackages = @([PSCustomObject]@{ Name = "Microsoft.BingSearch" })
                ProvisionedPackages = @([PSCustomObject]@{ Name = "Microsoft.Windows.DevHome" })
                Services = @([PSCustomObject]@{ Name = "DiagTrack"; StartType = "Automatic"; NeedsDisable = $true })
                Tasks = @([PSCustomObject]@{ TaskName = "ProgramDataUpdater"; TaskPath = "\Microsoft\Windows\Application Experience\"; NeedsDisable = $true })
            }
            Mock Write-Step {}
            Mock Write-Info {}
            Mock Write-Sub {}

            Write-GamingDebloatInventorySummary -Inventory $inventory

            Should -Invoke Write-Step -ParameterFilter { $t -eq "Debloat preflight inventory..." } -Times 1
            Should -Invoke Write-Sub -ParameterFilter { $t -like "*Microsoft.BingSearch*" } -Times 1
            Should -Invoke Write-Sub -ParameterFilter { $t -like "*DiagTrack*" } -Times 1
            Should -Invoke Write-Sub -ParameterFilter { $t -like "*ProgramDataUpdater*" } -Times 1
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
            Mock Write-ConsoleLine {}
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
            Mock Write-ConsoleLine {}
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
