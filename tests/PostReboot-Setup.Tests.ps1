# ==============================================================================
#  tests/PostReboot-Setup.Tests.ps1  --  direct shipped-entrypoint contract tests
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/helpers/_TestInit.ps1"
    $script:ProjectRoot = (Resolve-Path "$PSScriptRoot/..").Path
    $script:TargetScript = Join-Path $script:ProjectRoot "PostReboot-Setup.ps1"
    if (-not (Get-Command Apply-NvidiaCS2Profile -ErrorAction SilentlyContinue)) {
        function global:Apply-NvidiaCS2Profile {}
    }
    if (-not (Get-Command Install-NvidiaDriverClean -ErrorAction SilentlyContinue)) {
        function global:Install-NvidiaDriverClean { param([string]$DriverExe) }
    }
    if (-not (Get-Command Enable-DeviceMSI -ErrorAction SilentlyContinue)) {
        function global:Enable-DeviceMSI {}
    }
    if (-not (Get-Command Set-NicInterruptAffinity -ErrorAction SilentlyContinue)) {
        function global:Set-NicInterruptAffinity {}
    }
    if (-not (Get-Command Set-CS2ProcessPriority -ErrorAction SilentlyContinue)) {
        function global:Set-CS2ProcessPriority {}
    }
    if (-not (Get-Command Show-CS2SettingsGuide -ErrorAction SilentlyContinue)) {
        function global:Show-CS2SettingsGuide { param([int]$fpsCap, [int]$avgFps, [string]$gpuInput) }
    }
    if (-not (Get-Command Invoke-BenchmarkCapture -ErrorAction SilentlyContinue)) {
        function global:Invoke-BenchmarkCapture { param([string]$Label) }
    }
    if (-not (Get-Command Get-BenchmarkHistory -ErrorAction SilentlyContinue)) {
        function global:Get-BenchmarkHistory { @() }
    }
    if (-not (Get-Command Get-AppxPackage -ErrorAction SilentlyContinue)) {
        function global:Get-AppxPackage { param([switch]$AllUsers, $ErrorAction) @() }
    }
    if (-not (Get-Command Remove-AppxPackage -ErrorAction SilentlyContinue)) {
        function global:Remove-AppxPackage { param($Package, [switch]$AllUsers, $ErrorAction) }
    }
    if (-not (Get-Command Get-AppxProvisionedPackage -ErrorAction SilentlyContinue)) {
        function global:Get-AppxProvisionedPackage { param([switch]$Online, $ErrorAction) @() }
    }
    if (-not (Get-Command Remove-AppxProvisionedPackage -ErrorAction SilentlyContinue)) {
        function global:Remove-AppxProvisionedPackage {
            param([string]$PackageName, [switch]$Online, $ErrorAction)
            $PackageName
        }
    }
    function global:shutdown { param([Parameter(ValueFromRemainingArguments)]$CmdArgs) }
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "PostReboot-Setup.ps1 shipped smoke contract" {

    It "supports -SmokeTest as a clean short-circuit" -Skip:(-not $IsWindows) {
        $records = & powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File $script:TargetScript -SmokeTest 2>&1
        $exitCode = $LASTEXITCODE
        $output = ($records | ForEach-Object { $_.ToString() }) -join "`n"
        $errorRecords = @($records | Where-Object { $_ -is [System.Management.Automation.ErrorRecord] })

        $exitCode | Should -Be 0
        $errorRecords | Should -BeNullOrEmpty
        $output | Should -Match 'SMOKE TEST OK'
    }
}

Describe "PostReboot-Setup.ps1 entrypoint wrapper" {

    BeforeAll {
        . $script:TargetScript
    }

    BeforeEach {
        Mock Test-PublishedRuntimePayloadBootstrap { [PSCustomObject]@{ Valid = $true; Message = "verified" } }
    }

    It "bypasses administrator validation before orchestration for smoke tests" {
        Mock Assert-PostRebootSetupAdministrator { throw "Administrator check should not run" }
        Mock Invoke-PostRebootSetup { throw "Orchestration should not run" }
        Mock Write-Host {}

        { Invoke-PostRebootSetupEntryPoint -SmokeTest } | Should -Not -Throw
        Should -Invoke Assert-PostRebootSetupAdministrator -Exactly 0
        Should -Invoke Invoke-PostRebootSetup -Exactly 0
    }

    It "blocks orchestration when administrator validation fails" {
        Mock Assert-PostRebootSetupAdministrator { throw "Not elevated" }
        Mock Invoke-PostRebootSetup {}

        { Invoke-PostRebootSetupEntryPoint } | Should -Throw "Not elevated"
        Should -Invoke Assert-PostRebootSetupAdministrator -Exactly 1
        Should -Invoke Invoke-PostRebootSetup -Exactly 0
    }

    It "runs administrator validation before orchestration" {
        $script:CallOrder = [System.Collections.Generic.List[string]]::new()
        Mock Assert-PostRebootSetupAdministrator { $script:CallOrder.Add("assert") }
        Mock Invoke-PostRebootSetup { $script:CallOrder.Add("orchestrate") }

        Invoke-PostRebootSetupEntryPoint

        $script:CallOrder | Should -Be @("assert", "orchestrate")
    }

    It "fails closed before administrator validation when the published payload is invalid" {
        Mock Test-PublishedRuntimePayloadBootstrap { [PSCustomObject]@{ Valid = $false; Message = "extra runtime file" } }
        Mock Assert-PostRebootSetupAdministrator { throw "must not validate elevation" }
        Mock Invoke-PostRebootSetup { throw "must not orchestrate" }
        Mock Write-Host {}

        { Invoke-PostRebootSetupEntryPoint } | Should -Not -Throw
        Should -Invoke Assert-PostRebootSetupAdministrator -Exactly 0
        Should -Invoke Invoke-PostRebootSetup -Exactly 0
        Should -Invoke Write-Host -ParameterFilter { $Object -match "CRITICAL.*extra runtime file" }
    }

}

Describe "PostReboot-Setup.ps1 source-tree trust boundary" {

    BeforeAll {
        . $script:TargetScript
    }

    It "does not treat the source-tree entrypoint as a published runtime" {
        $result = Test-PublishedRuntimePayloadBootstrap -RuntimeRoot $script:ProjectRoot

        $result.Valid | Should -Be $false
        $result.Message | Should -Match "runtime-manifest.json is missing"
    }
}

Describe "PostReboot-Setup.ps1 required GPU state" {

    BeforeEach {
        Reset-TestState
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue
        . $script:TargetScript

        Mock Write-Host {}
        Mock Initialize-Backup { throw "must not initialize backup for invalid phase state" }
        Mock Remove-GpuAppxPackages { throw "must not enter vendor-specific work" }
    }

    It "stops before Phase 3 changes when gpuInput is missing" {
        Mock Load-State {
            [PSCustomObject]@{ mode = "CONTROL"; logLevel = "NORMAL"; profile = "RECOMMENDED" }
        }

        { Invoke-PostRebootSetup } | Should -Throw "*gpuInput must be a scalar value from 1 through 4*"

        Should -Invoke Initialize-Backup -Exactly 0
        Should -Invoke Remove-GpuAppxPackages -Exactly 0
        Should -Invoke Write-Host -ParameterFilter { $Object -match "CRITICAL:.*gpuInput" }
    }

    It "stops before Phase 3 changes when gpuInput is invalid" {
        Mock Load-State {
            [PSCustomObject]@{
                gpuInput = @("1", "3"); mode = "CONTROL"; logLevel = "NORMAL"; profile = "RECOMMENDED"
            }
        }

        { Invoke-PostRebootSetup } | Should -Throw "*gpuInput must be a scalar value from 1 through 4*"

        Should -Invoke Initialize-Backup -Exactly 0
        Should -Invoke Remove-GpuAppxPackages -Exactly 0
    }

    It "reports a JSON null state as an invalid GPU selection" {
        Mock Load-State { $null }

        { Invoke-PostRebootSetup } | Should -Throw "*gpuInput must be a scalar value from 1 through 4*"

        Should -Invoke Initialize-Backup -Exactly 0
        Should -Invoke Remove-GpuAppxPackages -Exactly 0
        Should -Invoke Write-Host -ParameterFilter { $Object -match "CRITICAL:.*gpuInput" }
    }
}

Describe "PostReboot-Setup.ps1 GPU AppX dry-run boundary" {

    BeforeAll {
        . $script:TargetScript
    }

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $true
        Mock Write-Host {}
        Mock Get-AppxPackage { throw "dry-run must not enumerate AppX packages" }
        Mock Get-AppxProvisionedPackage { throw "dry-run must not enumerate provisioned packages" }
        Mock Remove-AppxPackage { throw "dry-run must not remove AppX packages" }
        Mock Remove-AppxProvisionedPackage { throw "dry-run must not remove provisioned packages" }
    }

    It "previews NVIDIA cleanup without enumerating or uninstalling packages" {
        $result = Remove-GpuAppxPackages -GpuInput "2"

        $result.Status | Should -Be 'DryRun'
        $result.CanCompleteStep | Should -BeFalse
        Should -Invoke Get-AppxPackage -Exactly 0
        Should -Invoke Get-AppxProvisionedPackage -Exactly 0
        Should -Invoke Remove-AppxPackage -Exactly 0
        Should -Invoke Remove-AppxProvisionedPackage -Exactly 0
        Should -Invoke Write-Host -Exactly 1 -ParameterFilter { $Object -match "DRY-RUN.*NVIDIA" }
    }
}

Describe "PostReboot-Setup.ps1 injected full DRY-RUN boundary" {

    BeforeEach {
        Reset-TestState
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue
        . $script:TargetScript

        $script:PreviewState = [PSCustomObject]@{
            gpuInput = "2"; mode = "DRY-RUN"; logLevel = "VERBOSE"; profile = "CUSTOM"
            fpsCap = 0; avgFps = 0; rollbackDriver = $null; nvidiaDriverPath = $null
            baselineAvg = $null; baselineP1 = $null
        }

        Mock Load-State { throw "Injected preview must not load persisted state" }
        Mock Save-SuiteState { throw "Injected preview must not persist state" }
        Mock Initialize-Backup { throw "Injected preview must not initialize backups" }
        Mock Initialize-Log { throw "Injected preview must not initialize logs" }
        Mock Ensure-Dir { throw "Injected preview must not create directories" }
        Mock Remove-BackupLock { throw "Injected preview must not remove locks" }
        Mock Clear-SafeBootVerified { throw "Injected preview must not change BCD" }
        Mock Set-RunOnce { throw "Injected preview must not register a handoff" }
        Mock Remove-PhaseHandoff { throw "Injected preview must not remove a handoff" }
        Mock Restart-Computer { throw "Injected preview must not reboot" }
        Mock shutdown { throw "Injected preview must not reboot" }
        Mock Read-Host { throw "Injected preview must not prompt" }

        Mock Initialize-PhaseCounters {}
        Mock Show-ResumePrompt { 1 }
        Mock Test-StepCompleted { $false }
        Mock Write-Banner {}
        Mock Write-Host {}
        Mock Write-Info {}
        Mock Write-Warn {}
        Mock Write-Err {}
        Mock Write-OK {}
        Mock Write-Step {}
        Mock Write-Section {}
        Mock Write-TierBadge {}
        Mock Write-Blank {}
        Mock Write-DebugLog {}
        Mock Write-PhaseSummary {}
        Mock Complete-Step {}
        Mock Skip-Step {}

        Mock Invoke-TieredStep { & $Action; return $true }
        Mock Remove-GpuAppxPackages {
            [PSCustomObject]@{ Status = "DryRun"; CanCompleteStep = $false; Message = "previewed" }
        }
        Mock Install-NvidiaDriverClean { $true }
        Mock Enable-DeviceMSI {
            [PSCustomObject]@{ Status = "DryRun"; CanCompleteStep = $true; Message = "previewed" }
        }
        Mock Set-NicInterruptAffinity {
            [PSCustomObject]@{ Status = "DryRun"; CanCompleteStep = $true; Message = "previewed" }
        }
        Mock Apply-NvidiaCS2Profile {
            [PSCustomObject]@{ Status = "DryRun"; CanCompleteStep = $false; Message = "previewed" }
        }
        Mock Show-CS2SettingsGuide {}
        Mock Set-RegistryValue {
            [PSCustomObject]@{ Status = "DryRun"; Applied = $false; Message = "previewed" }
        }
        Mock Get-NetAdapter {
            @([PSCustomObject]@{
                Name = "Preview Ethernet"; InterfaceDescription = "Physical Ethernet"
                Status = "Up"; ifIndex = 1; InterfaceIndex = 1
            })
        }
        Mock Set-VerifiedDnsProfileForAdapter { throw "DRY-RUN must not write DNS" }
        Mock Set-CS2ProcessPriority {
            [PSCustomObject]@{ Status = "DryRun"; CanCompleteStep = $false; Message = "previewed" }
        }
        Mock Get-AmdCpuInfo { $null }
        Mock Invoke-BenchmarkCapture { $null }
        Mock Get-BenchmarkHistory { @() }
    }

    It "exercises Phase 3 planners without persistence, live writes, prompts, or reboot" {
        Invoke-PostRebootSetup -PreviewState $script:PreviewState -SimulateNormalBoot

        Should -Invoke Load-State -Exactly 0
        Should -Invoke Initialize-Backup -Exactly 0
        Should -Invoke Initialize-Log -Exactly 0
        Should -Invoke Ensure-Dir -Exactly 0
        Should -Invoke Save-SuiteState -Exactly 0
        Should -Invoke Clear-SafeBootVerified -Exactly 0
        Should -Invoke Set-RunOnce -Exactly 0
        Should -Invoke Remove-PhaseHandoff -Exactly 0
        Should -Invoke Read-Host -Exactly 0
        Should -Invoke Restart-Computer -Exactly 0
        Should -Invoke shutdown -Exactly 0
        Should -Invoke Remove-BackupLock -Exactly 0

        Should -Invoke Remove-GpuAppxPackages -Exactly 1 -ParameterFilter { $GpuInput -eq "2" }
        Should -Invoke Install-NvidiaDriverClean -Exactly 1 -ParameterFilter { $DriverExe -match 'nvidia_driver-preview\.exe$' }
        Should -Invoke Enable-DeviceMSI -Exactly 1
        Should -Invoke Set-NicInterruptAffinity -Exactly 1
        Should -Invoke Apply-NvidiaCS2Profile -Exactly 1
        Should -Invoke Show-CS2SettingsGuide -Exactly 1
        Should -Invoke Set-RegistryValue -Exactly 1 -ParameterFilter { $name -eq "Enabled" -and $value -eq 0 }
        Should -Invoke Set-VerifiedDnsProfileForAdapter -Exactly 0
        Should -Invoke Set-CS2ProcessPriority -Exactly 1
        Should -Invoke Invoke-BenchmarkCapture -Exactly 1
        Should -Invoke Write-PhaseSummary -Exactly 1 -ParameterFilter { $PhaseLabel -eq "PHASE 3" -and $DryRun }
    }
}

Describe "PostReboot-Setup.ps1 GPU AppX cleanup" {

    BeforeAll {
        . $script:TargetScript
    }

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        Mock Write-OK {}
        Mock Write-Debug {}
        Mock Remove-AppxPackage {}
        Mock Remove-AppxProvisionedPackage {}
        Mock Get-Command { [PSCustomObject]@{ Name = $Name } } -ParameterFilter {
            $Name -in @('Get-AppxPackage', 'Get-AppxProvisionedPackage')
        }
    }

    It "preserves NVIDIA Control Panel while removing other installed and provisioned NVIDIA packages" {
        $installedControlPanel = [PSCustomObject]@{
            Name = "NVIDIACorp.NVIDIAControlPanel"
            PackageFullName = "NVIDIACorp.NVIDIAControlPanel_8.1.0_x64__56jybvy8sckqj"
        }
        $installedCompanion = [PSCustomObject]@{
            Name = "NVIDIACorp.NVIDIAApp"
            PackageFullName = "NVIDIACorp.NVIDIAApp_1.0.0_x64__56jybvy8sckqj"
        }
        $provisionedControlPanel = [PSCustomObject]@{
            DisplayName = "NVIDIACorp.NVIDIAControlPanel"
            PackageName = "NVIDIACorp.NVIDIAControlPanel_8.1.0_neutral_~_56jybvy8sckqj"
        }
        $provisionedCompanion = [PSCustomObject]@{
            DisplayName = "NVIDIACorp.NVIDIAApp"
            PackageName = "NVIDIACorp.NVIDIAApp_1.0.0_neutral_~_56jybvy8sckqj"
        }
        $script:installedQueries = 0
        $script:provisionedQueries = 0
        Mock Get-AppxPackage {
            $script:installedQueries++
            if ($script:installedQueries -eq 1) { @($installedControlPanel, $installedCompanion) }
            else { @($installedControlPanel) }
        }
        Mock Get-AppxProvisionedPackage {
            $script:provisionedQueries++
            if ($script:provisionedQueries -eq 1) { @($provisionedControlPanel, $provisionedCompanion) }
            else { @($provisionedControlPanel) }
        }
        Mock Remove-AppxPackage {}
        $result = Remove-GpuAppxPackages -GpuInput "2"

        $result.Status | Should -Be 'Success' -Because $result.Message
        $result.CanCompleteStep | Should -BeTrue
        $result.RemovedCount | Should -Be 2
        Should -Invoke Remove-AppxPackage -Exactly 1 -ParameterFilter {
            $Package -eq $installedCompanion.PackageFullName
        }
        Should -Invoke Remove-AppxPackage -Exactly 0 -ParameterFilter {
            $Package -eq $installedControlPanel.PackageFullName
        }
        Should -Invoke Remove-AppxProvisionedPackage -Exactly 1
        (Get-Command Remove-GpuAppxPackages).ScriptBlock.ToString() |
            Should -Match 'Remove-AppxProvisionedPackage\s+-Online\s+-PackageName\s+\$pkg\.PackageName'
    }

    It "fails closed when AppX inventory is unavailable" {
        Mock Get-AppxPackage { throw 'AppXSVC unavailable' }
        Mock Get-AppxProvisionedPackage { @() }

        $result = Remove-GpuAppxPackages -GpuInput '2'

        $result.Status | Should -Be 'Failed'
        $result.CanCompleteStep | Should -BeFalse
        $result.Message | Should -Match 'inventory failed'
        Should -Invoke Remove-AppxPackage -Exactly 0
    }

    It "fails closed when a removed package remains in the verified post-state" {
        $package = [PSCustomObject]@{
            Name = 'NVIDIACorp.NVIDIAApp'
            PackageFullName = 'NVIDIACorp.NVIDIAApp_1.0.0_x64__56jybvy8sckqj'
        }
        Mock Get-AppxPackage { @($package) }
        Mock Get-AppxProvisionedPackage { @() }
        Mock Remove-AppxPackage {}

        $result = Remove-GpuAppxPackages -GpuInput '2'

        $result.Status | Should -Be 'Failed'
        $result.CanCompleteStep | Should -BeFalse
        $result.Message | Should -Match 'remains'
    }

    It "exposes a fail-closed Phase 3 caller gate" {
        $source = (Get-Command Invoke-PostRebootSetup).ScriptBlock.ToString()

        $source | Should -Match 'gpuAppxCleanup\.CanCompleteStep'
        $source | Should -Match 'Normal-Mode GPU AppX cleanup did not complete'
    }
}

Describe "PostReboot-Setup.ps1 Safe Mode recovery" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $env:SAFEBOOT_OPTION = "MINIMAL"
        Mock Load-State {
            [PSCustomObject]@{
                gpuInput = "2"
                mode = "CONTROL"
                logLevel = "NORMAL"
                profile = "RECOMMENDED"
                fpsCap = 0
                avgFps = 0
            }
        }
        Mock Write-Host {}
        Mock Write-DebugLog {}
        Mock Read-Host { "" }
        Mock Test-YoloProfile { $false }
        Mock Start-Sleep {}
        Mock shutdown {}
        Mock Complete-Step {}
        Mock Clear-SafeBootVerified {
            [PSCustomObject]@{
                Status = "Success"
                Verified = $true
                Applied = $true
                DeleteExitCode = 0
                EnumExitCode = 0
                Message = "Safe Mode disabled and verified."
            }
        }
        Mock Set-RunOnce {
            [PSCustomObject]@{
                Status = "Success"
                Applied = $true
                Message = "RunOnce set"
            }
        }
    }

    AfterEach {
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue
    }

    It "keeps the session running when SafeBoot verification fails" {
        Mock Clear-SafeBootVerified {
            [PSCustomObject]@{
                Status = "Failed"
                Verified = $false
                Applied = $true
                DeleteExitCode = 0
                EnumExitCode = 5
                Message = "enum failed"
            }
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        Should -Invoke Set-RunOnce -Exactly 0
        Should -Invoke shutdown -Exactly 0
        Should -Invoke Complete-Step -Exactly 0
        Should -Invoke Read-Host -Exactly 1
    }

    It "keeps the session running when Phase 3 per-user handoff registration fails" {
        Mock Set-RunOnce {
            [PSCustomObject]@{
                Status = "Failed"
                Applied = $false
                Message = "registry write failed"
            }
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        Should -Invoke Set-RunOnce -Exactly 1 -ParameterFilter { $PassThru }
        Should -Invoke shutdown -Exactly 0
        Should -Invoke Start-Sleep -Exactly 0
        Should -Invoke Complete-Step -Exactly 0
        Should -Invoke Read-Host -Exactly 1
    }

    It "restarts only after verified SafeBoot removal and an applied Phase 3 handoff" {
        . $script:TargetScript
        Invoke-PostRebootSetup

        Should -Invoke Clear-SafeBootVerified -Exactly 1
        Should -Invoke Set-RunOnce -Exactly 1 -ParameterFilter { $PassThru }
        Should -Invoke Start-Sleep -Exactly 1
        Should -Invoke shutdown -Exactly 1
        Should -Invoke Complete-Step -Exactly 0
        Should -Invoke Read-Host -Exactly 0
    }

    It "does not remove another process lock when backup initialization fails" {
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue
        Mock Initialize-Backup { throw "backup lock is already held" }
        Mock Remove-BackupLock {}

        . $script:TargetScript
        Invoke-PostRebootSetup

        Should -Invoke Remove-BackupLock -Exactly 0
        Should -Invoke shutdown -Exactly 0
        Should -Invoke Complete-Step -Exactly 0
    }
}

Describe "PostReboot-Setup.ps1 NVIDIA profile Step 4" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $SCRIPT:Profile = "YOLO"
        $SCRIPT:LogLevel = "NORMAL"
        $script:ActionError = $null
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue

        Mock Load-State {
            [PSCustomObject]@{
                gpuInput = "2"
                mode = "CONTROL"
                logLevel = "NORMAL"
                profile = "YOLO"
                fpsCap = 0
                avgFps = 0
                rollbackDriver = $null
                nvidiaDriverPath = $null
                baselineAvg = $null
                baselineP1 = $null
            }
        }
        Mock Save-SuiteState {}
        Mock Initialize-Backup {}
        Mock Initialize-PhaseCounters {}
        Mock Ensure-Dir {}
        Mock Initialize-Log {}
        Mock Write-Banner {}
        Mock Write-Section {}
        Mock Write-TierBadge {}
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-Info {}
        Mock Write-Warn {}
        Mock Write-Err {}
        Mock Write-DebugLog {}
        Mock Read-Host { "n" }
        Mock Remove-BackupLock {}
        Mock Complete-Step {}
        Mock Skip-Step {}
        Mock Test-StepCompleted {
            param($phase, $stepNum)
            $phase -eq 3 -and $stepNum -eq 1
        }
        Mock Show-ResumePrompt { 4 }
        Mock Invoke-TieredStep {
            param(
                [int]$Tier,
                [string]$Title,
                [string]$Why,
                [string]$Evidence,
                [string]$Caveat,
                [string]$Risk,
                [string]$Depth,
                [string]$Improvement,
                [string]$SideEffects,
                [string]$Undo,
                [scriptblock]$Action,
                [scriptblock]$SkipAction
            )

            if ($Title -match "NVIDIA CS2 profile") {
                try {
                    & $Action
                } catch {
                    $script:ActionError = $_
                }
                throw "StopAfterNvidiaProfileTest"
            }
        }
    }

    It "does not complete Step 4 when the NVIDIA profile result is partial" {
        Mock Apply-NvidiaCS2Profile {
            [PSCustomObject]@{
                Status = "Partial"
                CanCompleteStep = $false
                Message = "Only 41 of 42 DRS settings applied."
            }
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "NVIDIA CS2 profile did not complete"
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 4 -and $stepName -eq "NVProfile"
        }
    }

    It "completes Step 4 when the NVIDIA profile result can complete" {
        Mock Apply-NvidiaCS2Profile {
            [PSCustomObject]@{
                Status = "Success"
                CanCompleteStep = $true
                Message = "NVIDIA DRS profile and required registry locks applied."
            }
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 4 -and $stepName -eq "NVProfile"
        }
    }
}

Describe "PostReboot-Setup.ps1 DNS Step 9" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $false
        $SCRIPT:Profile = "YOLO"
        $SCRIPT:LogLevel = "NORMAL"
        $script:ActionError = $null
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue

        Mock Load-State {
            [PSCustomObject]@{
                gpuInput = "2"
                mode = "CONTROL"
                logLevel = "NORMAL"
                profile = "YOLO"
                fpsCap = 0
                avgFps = 0
                rollbackDriver = $null
                nvidiaDriverPath = $null
                baselineAvg = $null
                baselineP1 = $null
            }
        }
        Mock Save-SuiteState {}
        Mock Initialize-Backup {}
        Mock Initialize-PhaseCounters {}
        Mock Ensure-Dir {}
        Mock Initialize-Log {}
        Mock Write-Banner {}
        Mock Write-Section {}
        Mock Write-TierBadge {}
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-Info {}
        Mock Write-Warn {}
        Mock Write-Err {}
        Mock Write-DebugLog {}
        Mock Read-Host { "n" }
        Mock Remove-BackupLock {}
        Mock Complete-Step {}
        Mock Skip-Step {}
        Mock Test-StepCompleted {
            param($phase, $stepNum)
            $phase -eq 3 -and $stepNum -eq 1
        }
        Mock Show-ResumePrompt { 9 }
        Mock Get-DnsClientServerAddress {
            [PSCustomObject]@{ ServerAddresses = @("8.8.8.8") }
        }
        Mock Set-VerifiedDnsProfileForAdapter {
            [PSCustomObject]@{
                Changed = $true
                AdapterName = $AdapterName
                Provider = $Provider
            }
        }
        Mock Invoke-TieredStep {
            param(
                [int]$Tier,
                [string]$Title,
                [string]$Why,
                [string]$Evidence,
                [string]$Caveat,
                [string]$Risk,
                [string]$Depth,
                [string]$Improvement,
                [string]$SideEffects,
                [string]$Undo,
                [scriptblock]$Action,
                [scriptblock]$SkipAction
            )

            if ($Title -match "DNS server") {
                try {
                    & $Action
                } catch {
                    $script:ActionError = $_
                }
                throw "StopAfterDnsStepTest"
            }
        }
    }

    It "does not complete Step 9 when the only selected adapter fails" {
        Mock Get-NetAdapter {
            [PSCustomObject]@{
                Name = "Ethernet"
                Status = "Up"
                InterfaceDescription = "Intel Ethernet"
                ifIndex = 7
            }
        }
        Mock Set-VerifiedDnsProfileForAdapter { throw "DNS post-check failed" }

        . $script:TargetScript
        Invoke-PostRebootSetup

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "DNS post-check failed"
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 9 -and $stepName -eq "DNS"
        }
    }

    It "does not complete Step 9 when the second selected adapter fails" {
        Mock Get-NetAdapter {
            @(
                [PSCustomObject]@{
                    Name = "Ethernet"
                    Status = "Up"
                    InterfaceDescription = "Intel Ethernet"
                    ifIndex = 7
                }
                [PSCustomObject]@{
                    Name = "Wi-Fi"
                    Status = "Up"
                    InterfaceDescription = "Intel Wi-Fi"
                    ifIndex = 8
                }
            )
        }
        Mock Set-VerifiedDnsProfileForAdapter {
            if ($AdapterName -eq "Wi-Fi") { throw "DNS post-check failed" }
            [PSCustomObject]@{ Changed = $true; AdapterName = $AdapterName; Provider = $Provider }
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "DNS post-check failed"
        Should -Invoke Set-VerifiedDnsProfileForAdapter -Exactly 2
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 9 -and $stepName -eq "DNS"
        }
    }

    It "does not complete Step 9 when no active physical adapter is available" {
        Mock Get-NetAdapter { @() }

        . $script:TargetScript
        Invoke-PostRebootSetup

        $script:ActionError | Should -Not -BeNullOrEmpty
        $script:ActionError.Exception.Message | Should -Match "No active network adapter"
        Should -Invoke Set-VerifiedDnsProfileForAdapter -Exactly 0
        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 9 -and $stepName -eq "DNS"
        }
    }

    It "completes Step 9 after all selected adapters verify" {
        Mock Get-NetAdapter {
            @(
                [PSCustomObject]@{
                    Name = "Ethernet"
                    Status = "Up"
                    InterfaceDescription = "Intel Ethernet"
                    ifIndex = 7
                }
                [PSCustomObject]@{
                    Name = "Wi-Fi"
                    Status = "Up"
                    InterfaceDescription = "Intel Wi-Fi"
                    ifIndex = 8
                }
            )
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        $script:ActionError | Should -BeNullOrEmpty
        Should -Invoke Set-VerifiedDnsProfileForAdapter -Exactly 2
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 9 -and $stepName -eq "DNS"
        }
    }

    It "does not write DNS during dry-run" {
        $SCRIPT:DryRun = $true
        $SCRIPT:Profile = "YOLO"
        Mock Get-NetAdapter {
            [PSCustomObject]@{
                Name = "Ethernet"
                Status = "Up"
                InterfaceDescription = "Intel Ethernet"
                ifIndex = 7
            }
        }

        . $script:TargetScript
        Invoke-PostRebootSetup

        Should -Invoke Set-VerifiedDnsProfileForAdapter -Exactly 0
        Should -Invoke Complete-Step -Exactly 1 -ParameterFilter {
            $phase -eq 3 -and $stepNum -eq 9 -and $stepName -eq "DNS"
        }
    }
}

Describe "PostReboot-Setup.ps1 truthful MSI and NIC completion contract" {

    It "routes skipped MSI/NIC results to Skip-Step and only completes completable results" {
        $source = Get-Content -LiteralPath $script:TargetScript -Raw

        $source | Should -Match '(?s)\$msiResult\s*=\s*Enable-DeviceMSI.*?if\s*\(\$msiResult\.Status\s+-eq\s+"Skipped"\)\s*\{\s*Skip-Step\s+\$PHASE\s+2\s+"MSI"\s*\}.*?elseif\s*\(-not\s+\$msiResult\.CanCompleteStep\)\s*\{\s*throw.*?\}.*?else\s*\{\s*Complete-Step\s+\$PHASE\s+2\s+"MSI"'
        $source | Should -Match '(?s)\$affinityResult\s*=\s*Set-NicInterruptAffinity.*?if\s*\(\$affinityResult\.Status\s+-eq\s+"Skipped"\)\s*\{\s*Skip-Step\s+\$PHASE\s+3\s+"NicAffinity"\s*\}.*?elseif\s*\(-not\s+\$affinityResult\.CanCompleteStep\)\s*\{\s*throw.*?\}.*?else\s*\{\s*Complete-Step\s+\$PHASE\s+3\s+"NicAffinity"'
    }
}

Describe "PostReboot-Setup.ps1 truthful Phase 3 completion contracts" {

    It "requires a successful structured HVCI registry write before completing VBS" {
        $source = Get-Content -LiteralPath $script:TargetScript -Raw

        $source | Should -Match '(?s)\$hvciWriteResult\s*=\s*Set-RegistryValue.*?HypervisorEnforcedCodeIntegrity.*?-PassThru'
        $source | Should -Match '(?s)if\s*\(\$hvciWriteResult\.Status\s+-eq\s+"DryRun"\).*?no state was changed'
        $source | Should -Match '(?s)if\s*\(\$hvciWriteResult\.Status\s+-ne\s+"Success"\)\s*\{\s*throw\s+"VBS/HVCI disable did not complete'
        $source | Should -Match '(?s)\$hvciWriteResult\.Status\s+-ne\s+"Success".*?Complete-Step\s+\$PHASE\s+7\s+"VBS"'
    }

    It "treats a null DeviceGuard query as a structured VBS detection failure without completion" {
        $source = Get-Content -LiteralPath $script:TargetScript -Raw
        $vbsBlock = [regex]::Match($source, '(?s)# STEP 7.*?(?=# STEP 8)').Value

        $vbsBlock | Should -Match '\$vbsDetection\s*=\s*Get-VbsDetectionResult'
        $vbsBlock | Should -Match 'if\s*\(\$vbsDetection\.Status\s+-ne\s+"Success"\)\s*\{\s*throw\s+"VBS detection did not complete'
    }

    It "treats a DeviceGuard query error as a structured VBS detection failure without completion" {
        $source = Get-Content -LiteralPath $script:TargetScript -Raw
        $vbsBlock = [regex]::Match($source, '(?s)# STEP 7.*?(?=# STEP 8)').Value

        $source | Should -Match '(?s)function\s+Get-VbsDetectionResult.*?Get-CimInstance\s+-ClassName\s+Win32_DeviceGuard.*?-ErrorAction\s+Stop'
        $source | Should -Match '(?s)function\s+Get-VbsDetectionResult.*?catch\s*\{\s*return\s+\[PSCustomObject\]@\{\s*Status\s*=\s*"Failed".*?VBS detection query failed:'
        $vbsBlock | Should -Match '(?s)\$vbsDetection\.Status\s+-ne\s+"Success".*?throw\s+"VBS detection did not complete.*?Complete-Step\s+\$PHASE\s+7\s+"VBS"'
    }

    It "only completes explicit, successful inactive VBS detection" {
        $source = Get-Content -LiteralPath $script:TargetScript -Raw
        $vbsBlock = [regex]::Match($source, '(?s)# STEP 7.*?(?=# STEP 8)').Value

        $source | Should -Match '(?s)function\s+Get-VbsDetectionResult.*?Status\s*=\s*"Success".*?IsActive\s*=\s*\(\[int\]\$dg\.VirtualizationBasedSecurityStatus\s+-ge\s+2\)'
        $vbsBlock | Should -Match '(?s)if\s*\(-not\s+\$vbsDetection\.IsActive\)\s*\{\s*Write-OK\s+"VBS/Core Isolation: not active.*?Complete-Step\s+\$PHASE\s+7\s+"VBS"'
    }

    It "gates process-priority completion on the structured persistent-operation result" {
        $source = Get-Content -LiteralPath $script:TargetScript -Raw

        $source | Should -Match '(?s)\$priorityResult\s*=\s*Set-CS2ProcessPriority.*?if\s*\(\$priorityResult\.Status\s+-eq\s+"DryRun"\).*?no state was changed.*?elseif\s*\(\$priorityResult\.Status\s+-eq\s+"Skipped"\).*?Skip-Step\s+\$PHASE\s+10\s+"ProcessPriority".*?elseif\s*\(-not\s+\$priorityResult\.CanCompleteStep\).*?throw.*?else\s*\{\s*Complete-Step\s+\$PHASE\s+10\s+"ProcessPriority"'
    }

    It "keeps manual AMD or Intel driver work pending in YOLO mode" {
        $source = Get-Content -LiteralPath $script:TargetScript -Raw

        $amdIntelDriverBlock = [regex]::Match(
            $source,
            '(?s)if\s*\(Test-YoloProfile\)\s*\{\s*Write-Warn\s+"Manual driver installation is required for AMD/Intel.*?\}\s+elseif\s*\(\$SCRIPT:DryRun\).*?else\s*\{\s*Read-Host.*?Complete-Step\s+\$PHASE\s+1\s+"Driver"'
        )

        $amdIntelDriverBlock.Success | Should -BeTrue
        $amdIntelDriverBlock.Value | Should -Not -Match 'Skip-Step\s+\$PHASE\s+1'
        $source | Should -Match '(?s)if\s*\(Test-YoloProfile\)\s*\{\s*Write-Warn\s+"Manual AMD Adrenalin settings are required.*?Skip-Step\s+\$PHASE\s+8\s+"AMDSettings \(manual action required\)".*?\}\s+elseif\s*\(\$SCRIPT:DryRun\).*?else\s*\{\s*Read-Host.*?Complete-Step\s+\$PHASE\s+8\s+"AMDSettings"'
    }
}

Describe "Get-VbsDetectionResult" {

    BeforeAll {
        . $script:TargetScript
    }

    It "returns a non-completable failure when DeviceGuard returns null" {
        Mock Get-CimInstance { $null }

        $result = Get-VbsDetectionResult

        $result.Status | Should -Be "Failed"
        $result.CanCompleteStep | Should -Be $false
        $result.IsActive | Should -BeNullOrEmpty
        $result.Message | Should -Match "no Win32_DeviceGuard instance"
    }

    It "returns a non-completable failure when the DeviceGuard query errors" {
        Mock Get-CimInstance { throw "provider unavailable" }

        $result = Get-VbsDetectionResult

        $result.Status | Should -Be "Failed"
        $result.CanCompleteStep | Should -Be $false
        $result.Message | Should -Match "provider unavailable"
    }

    It "reports an explicit inactive DeviceGuard status as successful" {
        Mock Get-CimInstance { [PSCustomObject]@{ VirtualizationBasedSecurityStatus = 0 } }

        $result = Get-VbsDetectionResult

        $result.Status | Should -Be "Success"
        $result.CanCompleteStep | Should -Be $true
        $result.IsActive | Should -Be $false
    }

    It "reports an explicit active DeviceGuard status as successful" {
        Mock Get-CimInstance { [PSCustomObject]@{ VirtualizationBasedSecurityStatus = 2 } }

        $result = Get-VbsDetectionResult

        $result.Status | Should -Be "Success"
        $result.CanCompleteStep | Should -Be $true
        $result.IsActive | Should -Be $true
    }
}

Describe "PostReboot-Setup.ps1 final benchmark completion contract" {

    It "only completes the final benchmark inside the usable-capture branch" {
        $source = Get-Content -LiteralPath $script:TargetScript -Raw
        $captureBlock = [regex]::Match(
            $source,
            '(?s)\$bmResult\s*=\s*Invoke-BenchmarkCapture\s+-Label\s+"After all optimizations"\s*\n\s*if\s*\(\$bmResult\)\s*\{(?<usable>.*?)\}\s*elseif\s*\(-not\s+\$SCRIPT:DryRun\)\s*\{(?<missing>.*?)\}'
        )

        $captureBlock.Success | Should -BeTrue
        $captureBlock.Groups['usable'].Value | Should -Match 'Complete-Step\s+\$PHASE\s+13\s+"FinalBenchmark"'
        $captureBlock.Groups['missing'].Value | Should -Not -Match 'Complete-Step\s+\$PHASE\s+13\s+"FinalBenchmark"'
        $captureBlock.Groups['missing'].Value | Should -Match 'remains incomplete'
    }

    It "retains the Phase 3 handoff when required driver or benchmark work is incomplete" {
        $source = Get-Content -LiteralPath $script:TargetScript -Raw

        $source | Should -Match '(?s)if \(-not \(Test-StepCompleted \$PHASE 1\)\) \{(?<driverIncomplete>.*?)\}\s*elseif \(-not \(Test-StepCompleted \$PHASE 13\)\) \{(?<benchmarkIncomplete>.*?)\}\s*else \{\s*\$handoffRemoval = Remove-PhaseHandoff -Name "FRAMETIME_Phase3" -PassThru'
        $source | Should -Match 'required driver installation is completed'
        $source | Should -Match 'automatic handoff is retained until the final benchmark is saved'
    }

    It "does not complete Step 13 or remove the handoff after a null benchmark capture" {
        Reset-TestState
        Remove-Item Env:SAFEBOOT_OPTION -ErrorAction SilentlyContinue
        $SCRIPT:DryRun = $false
        $SCRIPT:Profile = "YOLO"
        $SCRIPT:Mode = "YOLO"
        Mock Load-State {
            [PSCustomObject]@{
                gpuInput = "2"; mode = "YOLO"; logLevel = "NORMAL"; profile = "YOLO"
                fpsCap = 0; avgFps = 0; rollbackDriver = $null; nvidiaDriverPath = $null
            }
        }
        Mock Initialize-Backup {}
        Mock Initialize-PhaseCounters {}
        Mock Ensure-Dir {}
        Mock Initialize-Log {}
        Mock Write-Banner {}
        Mock Show-ResumePrompt { 13 }
        Mock Write-Section {}
        Mock Write-TierBadge {}
        Mock Write-Blank {}
        Mock Write-Host {}
        Mock Write-Info {}
        Mock Write-Warn {}
        Mock Write-Err {}
        Mock Write-PhaseSummary {}
        Mock Invoke-BenchmarkCapture { $null }
        Mock Get-BenchmarkHistory { @() }
        Mock Complete-Step {}
        Mock Test-StepCompleted { $false }
        Mock Remove-PhaseHandoff { [PSCustomObject]@{ Applied = $true; Message = "removed" } }
        Mock Restart-Computer {}
        Mock Remove-BackupLock {}

        . $script:TargetScript
        Invoke-PostRebootSetup

        Should -Invoke Complete-Step -Exactly 0 -ParameterFilter { $phase -eq 3 -and $stepNum -eq 13 }
        Should -Invoke Remove-PhaseHandoff -Exactly 0
        Should -Invoke Restart-Computer -Exactly 0
    }

    It "forces a previously skipped final benchmark to rerun instead of removing the handoff" {
        $source = Get-Content -LiteralPath $script:TargetScript -Raw

        $source | Should -Match '(?s)\$startStep\s*=\s*Show-ResumePrompt.*?if\s*\(-not\s*\(Test-StepCompleted\s+\$PHASE\s+1\)\).*?\$startStep\s*=\s*1.*?elseif\s*\(\$startStep\s*-gt\s*13\s*-and\s*-not\s*\(Test-StepCompleted\s+\$PHASE\s+13\)\).*?\$startStep\s*=\s*13.*?elseif\s*\(\$startStep\s*-gt\s*13\)'
        $source | Should -Match '\$p2DriverDone\s*=\s*Test-StepCompleted\s+2\s+2'
    }

    It "does not record deferred NVIDIA driver work as skipped" {
        $source = Get-Content -LiteralPath $script:TargetScript -Raw

        $nvidiaDeclineBlock = [regex]::Match(
            $source,
            '(?s)\$r\s*=\s*if \(\$SCRIPT:DryRun\).*?if \(\$r\s+-notmatch.*?\}\s*else\s*\{(?<declined>.*?)\}\s*\}\s*else\s*\{\s*Write-Err "No valid driver file'
        )
        $missingDriverBlock = [regex]::Match(
            $source,
            '(?s)Write-Err "No valid driver file \(\.exe\) found\.".*?if \(\$skipConfirm -match "\^\[jJyY\]\$"\) \{(?<missing>.*?)\}\s*else'
        )

        $nvidiaDeclineBlock.Success | Should -BeTrue
        $nvidiaDeclineBlock.Groups['declined'].Value | Should -Match 'remains pending'
        $nvidiaDeclineBlock.Groups['declined'].Value | Should -Not -Match 'Skip-Step\s+\$PHASE\s+1'
        $missingDriverBlock.Success | Should -BeTrue
        $missingDriverBlock.Groups['missing'].Value | Should -Match 'remains pending'
        $missingDriverBlock.Groups['missing'].Value | Should -Not -Match 'Skip-Step\s+\$PHASE\s+1'
    }
}
