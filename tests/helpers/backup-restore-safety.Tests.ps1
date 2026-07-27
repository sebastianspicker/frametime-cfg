# ==============================================================================
#  tests/helpers/backup-restore-safety.Tests.ps1
#  Focused safety contracts for persisted rollback data.
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Registry restore coverage for active mutations" {
    It "accepts every active registry mutation target" {
        $cs2ValueName = "C:\Games\Steam\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\cs2.exe"
        $displayClassPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000"
        $nicEnumPath = "HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\VEN_1234&DEV_5678\0001\Device Parameters\Interrupt Management"
        $nicDriverPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}\0001"

        $targets = @(
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power"; Name = "HiberbootEnabled" }
            @{ Path = "HKLM:\SOFTWARE\Microsoft\Windows\Dwm"; Name = "OverlayTestMode" }
            @{ Path = "HKCU:\SOFTWARE\Microsoft\GameBar"; Name = "AllowAutoGameMode" }
            @{ Path = "HKCU:\SOFTWARE\Microsoft\GameBar"; Name = "AutoGameModeEnabled" }
            @{ Path = "HKLM:\SOFTWARE\Policies\Microsoft\Windows\CloudContent"; Name = "DisableWindowsConsumerFeatures" }
            @{ Path = "HKLM:\SOFTWARE\Policies\Microsoft\Windows\CloudContent"; Name = "DisableSoftLanding" }
            @{ Path = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo"; Name = "Enabled" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\QoS"; Name = "Do not use NLA" }
            @{ Path = "HKCU:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers"; Name = $cs2ValueName }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers"; Name = "HwSchMode" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity"; Name = "Enabled" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{11111111-2222-3333-4444-555555555555}"; Name = "TcpNoDelay" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{11111111-2222-3333-4444-555555555555}"; Name = "TcpAckFrequency" }
            @{ Path = "HKCU:\System\GameConfigStore"; Name = "GameDVR_DXGIHonorFSEWindowsCompatible" }
            @{ Path = "HKCU:\System\GameConfigStore"; Name = "GameDVR_FSEBehavior" }
            @{ Path = "HKCU:\System\GameConfigStore"; Name = "GameDVR_FSEBehaviorMode" }
            @{ Path = "HKCU:\System\GameConfigStore"; Name = "GameDVR_HonorUserFSEBehaviorMode" }
            @{ Path = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile"; Name = "SystemResponsiveness" }
            @{ Path = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile"; Name = "NoLazyMode" }
            @{ Path = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games"; Name = "Priority" }
            @{ Path = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games"; Name = "Scheduling Category" }
            @{ Path = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games"; Name = "GPU Priority" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\PriorityControl"; Name = "Win32PrioritySeparation" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management"; Name = "DisablePagingExecutive" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\Power\PowerThrottling"; Name = "PowerThrottlingOff" }
            @{ Path = "HKLM:\SOFTWARE\Microsoft\FTH"; Name = "Enabled" }
            @{ Path = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Device Installer"; Name = "DisableCoInstallers" }
            @{ Path = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\Maintenance"; Name = "MaintenanceDisabled" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem"; Name = "NtfsDisableLastAccessUpdate" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem"; Name = "NtfsDisable8dot3NameCreation" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\kernel"; Name = "GlobalTimerResolutionRequests" }
            @{ Path = "HKCU:\Control Panel\Mouse"; Name = "MouseSpeed" }
            @{ Path = "HKCU:\Control Panel\Mouse"; Name = "MouseThreshold1" }
            @{ Path = "HKCU:\Control Panel\Mouse"; Name = "MouseThreshold2" }
            @{ Path = "HKCU:\Control Panel\Mouse"; Name = "SmoothMouseXCurve" }
            @{ Path = "HKCU:\Control Panel\Mouse"; Name = "SmoothMouseYCurve" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Services\mouclass\Parameters"; Name = "MouseDataQueueSize" }
            @{ Path = "HKCU:\SOFTWARE\Microsoft\DirectX\UserGpuPreferences"; Name = $cs2ValueName }
            @{ Path = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\GameDVR"; Name = "AppCaptureEnabled" }
            @{ Path = "HKCU:\SOFTWARE\Microsoft\GameBar"; Name = "UseNexusForGameBarEnabled" }
            @{ Path = "HKLM:\SOFTWARE\Policies\Microsoft\Windows\GameDVR"; Name = "AllowGameDVR" }
            @{ Path = "HKCU:\System\GameConfigStore"; Name = "GameDVR_Enabled" }
            @{ Path = "HKCU:\Software\Valve\Steam"; Name = "GameOverlayDisabled" }
            @{ Path = "HKCU:\Software\Microsoft\Multimedia\Audio"; Name = "UserDuckingPreference" }
            @{ Path = "$nicEnumPath\MessageSignaledInterruptProperties"; Name = "MSISupported" }
            @{ Path = "$nicEnumPath\MessageSignaledInterruptProperties"; Name = "MessageNumberLimit" }
            @{ Path = $nicDriverPath; Name = "*RSS" }
            @{ Path = $nicDriverPath; Name = "*RSSProfile" }
            @{ Path = $nicDriverPath; Name = "*RssBaseProcNumber" }
            @{ Path = $nicDriverPath; Name = "*MaxRssProcessors" }
            @{ Path = $nicDriverPath; Name = "*NumRssQueues" }
            @{ Path = "$nicEnumPath\Affinity Policy"; Name = "DevicePolicy" }
            @{ Path = "$nicEnumPath\Affinity Policy"; Name = "AssignmentSetOverride" }
            @{ Path = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\cs2.exe\PerfOptions"; Name = "CpuPriorityClass" }
            @{ Path = $displayClassPath; Name = "RMHdcpKeyglobZero" }
            @{ Path = $displayClassPath; Name = "PerfLevelSrc" }
            @{ Path = $displayClassPath; Name = "DisableDynamicPstate" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers"; Name = "EnableWriteCombining" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\NvControlPanel2\Client"; Name = "OptInOrOutPreference" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\FTS"; Name = "EnableRID44231" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\FTS"; Name = "EnableRID64640" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\FTS"; Name = "EnableRID66610" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\NVTweak"; Name = "Gestalt" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "OGL_THREAD_CONTROL_DEFAULT" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "OGL_QUALITY_ENHANCEMENTS_DEFAULT" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "OGL_QUALITY_ENHANCEMENTS" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "OGL_FXAA_DEF" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "OGL_GAMMA_CORRECT_DEF" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "AA_MODE_SELECTOR" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "AA_LINE_GAMMA" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "LOD_BIAS_ADJUST" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "PS_TEXFILTER_BILINEAR_QUAL" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "PS_TEXFILTER_ANISO_OPTS2" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "PS_TEXFILTER_ANISO_OPTS" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "PS_TEXFILTER_LOD_BIAS" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "ANISO_SETTING" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "ANISO_MODE_SELECTOR" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "MAX_PRERENDERED_FRAMES" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "VSYNC_MODE" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "PRERENDERLIMIT_OPTION" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "ANSEL_ENABLE" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "FRL_VALUE" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "FRL_LOW_LATENCY" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "PS_FRAMERATE_LIMITER" }
            @{ Path = "HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d"; Name = "AFR_CONTROL" }
            @{ Path = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects"; Name = "VisualFXSetting" }
            @{ Path = "HKCU:\Control Panel\Desktop"; Name = "UserPreferencesMask" }
            @{ Path = "HKCU:\Control Panel\Desktop"; Name = "FontSmoothing" }
            @{ Path = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\VideoSettings"; Name = "AutoHDREnabled" }
        )

        foreach ($hive in @("HKCU:", "HKLM:")) {
            foreach ($name in $CFG_Autostart_Remove) {
                $targets += @{
                    Path = "$hive\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"
                    Name = $name
                }
            }
        }

        $rejected = @($targets | Where-Object {
            -not (Test-RegistryRestoreAllowed -Path $_.Path -Name $_.Name)
        } | ForEach-Object { "$($_.Path) :: $($_.Name)" })

        ($rejected -join "`n") | Should -BeNullOrEmpty
    }

    It "allows only configured names on the autostart Run keys" {
        foreach ($hive in @("HKCU:", "HKLM:")) {
            $runPath = "$hive\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"
            foreach ($name in $CFG_Autostart_Remove) {
                Test-RegistryRestoreAllowed -Path $runPath -Name $name | Should -BeTrue
            }
            Test-RegistryRestoreAllowed -Path $runPath -Name "ArbitraryStartup" | Should -BeFalse
        }
    }

    It "rejects unrelated values below formerly broad restore prefixes" {
        $rejectedTargets = @(
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; Name = "ComSpec" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\VEN_1234&DEV_5678\0001"; Name = "ConfigFlags" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000"; Name = "UpperFilters" }
            @{ Path = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{11111111-2222-3333-4444-555555555555}"; Name = "NameServer" }
            @{ Path = "HKCU:\Control Panel\Desktop"; Name = "Wallpaper" }
        )

        foreach ($target in $rejectedTargets) {
            Test-RegistryRestoreAllowed -Path $target.Path -Name $target.Name | Should -BeFalse
        }
    }
}

Describe "Selected and ordered restore safety" {
    BeforeEach {
        Reset-TestState
        Mock Write-ConsoleLine {}
        Mock Write-Step {}
        Mock Write-OK {}
        Mock Write-Warn {}
        Mock Write-DebugLog {}
        Mock Write-Info {}
    }

    It "restores only the selected zero-based global entry index" {
        $entries = @(
            [ordered]@{ type = "registry"; path = "HKCU:\System\GameConfigStore"; name = "GameDVR_Enabled"; originalValue = 1; originalType = "DWord"; existed = $true; step = "Repeated"; timestamp = "2026-01-01" },
            [ordered]@{ type = "registry"; path = "HKCU:\System\GameConfigStore"; name = "GameDVR_FSEBehavior"; originalValue = 2; originalType = "DWord"; existed = $true; step = "Repeated"; timestamp = "2026-01-02" },
            [ordered]@{ type = "registry"; path = "HKCU:\System\GameConfigStore"; name = "GameDVR_FSEBehaviorMode"; originalValue = 3; originalType = "DWord"; existed = $true; step = "Other"; timestamp = "2026-01-03" }
        )
        New-TestBackupFile -Entries $entries
        Mock Test-Path { $true }
        Mock Set-ItemProperty {}

        $result = Restore-StepChanges -EntryIndex 1

        $result | Should -BeTrue
        Should -Invoke Set-ItemProperty -Exactly 1 -ParameterFilter { $Name -eq "GameDVR_FSEBehavior" -and $Value -eq 2 }
        $remaining = @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries)
        $remaining.Count | Should -Be 2
        @($remaining.name) | Should -Be @("GameDVR_Enabled", "GameDVR_FSEBehaviorMode")
    }

    It "restores all entries in strict reverse global capture order" {
        $entries = @(
            [ordered]@{ type = "registry"; path = "HKCU:\System\GameConfigStore"; name = "GameDVR_Enabled"; originalValue = 1; originalType = "DWord"; existed = $true; step = "Step Z"; timestamp = "2026-01-01" },
            [ordered]@{ type = "registry"; path = "HKCU:\System\GameConfigStore"; name = "GameDVR_FSEBehavior"; originalValue = 2; originalType = "DWord"; existed = $true; step = "Step A"; timestamp = "2026-01-02" },
            [ordered]@{ type = "registry"; path = "HKCU:\System\GameConfigStore"; name = "GameDVR_FSEBehaviorMode"; originalValue = 3; originalType = "DWord"; existed = $true; step = "Step Z"; timestamp = "2026-01-03" }
        )
        New-TestBackupFile -Entries $entries
        $script:RestoreOrder = [System.Collections.Generic.List[string]]::new()
        Mock Test-Path { $true }
        Mock Set-ItemProperty { $script:RestoreOrder.Add([string]$Name) | Out-Null }

        $result = Restore-AllChanges

        $result.Succeeded | Should -BeTrue
        $result.Attempted | Should -Be 3
        $result.Failed | Should -Be 0
        $result.Skipped | Should -Be 0
        ($script:RestoreOrder -join ",") | Should -Be "GameDVR_FSEBehaviorMode,GameDVR_FSEBehavior,GameDVR_Enabled"
        @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries).Count | Should -Be 0
    }

    It "keeps reverse capture order when Restore All is filtered by step" {
        $entries = @(
            [ordered]@{ type = "registry"; path = "HKCU:\System\GameConfigStore"; name = "GameDVR_Enabled"; originalValue = 1; originalType = "DWord"; existed = $true; step = "Included"; timestamp = "2026-01-01" },
            [ordered]@{ type = "registry"; path = "HKCU:\System\GameConfigStore"; name = "GameDVR_FSEBehavior"; originalValue = 2; originalType = "DWord"; existed = $true; step = "Excluded"; timestamp = "2026-01-02" },
            [ordered]@{ type = "registry"; path = "HKCU:\System\GameConfigStore"; name = "GameDVR_FSEBehaviorMode"; originalValue = 3; originalType = "DWord"; existed = $true; step = "Included"; timestamp = "2026-01-03" }
        )
        New-TestBackupFile -Entries $entries
        $script:RestoreOrder = [System.Collections.Generic.List[string]]::new()
        Mock Test-Path { $true }
        Mock Set-ItemProperty { $script:RestoreOrder.Add([string]$Name) | Out-Null }

        $result = Restore-AllChanges -IncludeStep @("Included")

        $result.Succeeded | Should -BeTrue
        $result.Attempted | Should -Be 2
        $result.Failed | Should -Be 0
        $result.Skipped | Should -Be 1
        ($script:RestoreOrder -join ",") | Should -Be "GameDVR_FSEBehaviorMode,GameDVR_Enabled"
        $remaining = @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries)
        $remaining.Count | Should -Be 1
        $remaining[0].name | Should -Be "GameDVR_FSEBehavior"
    }

    It "retains an allowlist-rejected entry for inspection and retry" {
        $entries = @(
            [ordered]@{
                type = "registry"; path = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
                name = "ArbitraryStartup"; originalValue = "untrusted.exe"; originalType = "String";
                existed = $true; step = "Rejected"; timestamp = "2026-01-01"
            }
        )
        New-TestBackupFile -Entries $entries
        Mock Set-ItemProperty {}

        $result = Restore-StepChanges -EntryIndex 0

        $result | Should -BeFalse
        Should -Invoke Set-ItemProperty -Exactly 0
        $remaining = @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries)
        $remaining.Count | Should -Be 1
        $remaining[0].name | Should -Be "ArbitraryStartup"
    }

    It "retains an entry when its approved restore operation fails" {
        $entries = @(
            [ordered]@{
                type = "registry"; path = "HKCU:\System\GameConfigStore";
                name = "RestoreFailure"; originalValue = 1; originalType = "DWord";
                existed = $true; step = "Failed"; timestamp = "2026-01-01"
            }
        )
        New-TestBackupFile -Entries $entries
        Mock Test-Path { $true }
        Mock Set-ItemProperty { throw "access denied" }

        $result = Restore-StepChanges -EntryIndex 0

        $result | Should -BeFalse
        $remaining = @((Get-Content $CFG_BackupFile -Raw | ConvertFrom-Json).entries)
        $remaining.Count | Should -Be 1
        $remaining[0].name | Should -Be "RestoreFailure"
    }
}

Describe "Checked service backup results" {
    BeforeEach {
        Reset-TestState
        Mock Write-DebugLog {}
    }

    It "returns a successful checked result after queuing the service entry" {
        Mock Get-Service { [PSCustomObject]@{ Name = "DiagTrack"; Status = "Running" } }
        Mock Get-CimInstance { [PSCustomObject]@{ StartMode = "Auto" } }
        Mock Get-ItemProperty { [PSCustomObject]@{ DelayedAutostart = 0 } }

        $result = Backup-ServiceState -ServiceName "DiagTrack" -StepTitle "Service safety" -PassThru

        $result.Captured | Should -BeTrue
        $result.Entry.name | Should -Be "DiagTrack"
        $SCRIPT:_backupPending.Count | Should -Be 1
    }

    It "returns a failed checked result without queuing an entry" {
        Mock Get-Service { throw "service query failed" }

        $result = Backup-ServiceState -ServiceName "DiagTrack" -StepTitle "Service safety" -PassThru

        $result.Captured | Should -BeFalse
        $result.Message | Should -Match "DiagTrack"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }
}

Describe "GUI restore orchestration contract" {
    It "routes Restore All through the reverse-order helper" {
        $source = Get-Content -LiteralPath "$PSScriptRoot/../../helpers/gui-panels.ps1" -Raw
        $restoreAllBlock = [regex]::Match(
            $source,
            '(?s)\(El "BtnRestoreAll"\)\.Add_Click\(\{.*?(?=\(El "BtnRestoreStep"\)\.Add_Click)'
        ).Value

        $restoreAllBlock | Should -Not -BeNullOrEmpty
        $restoreAllBlock | Should -Match '\bRestore-AllChanges\b'
        $restoreAllBlock | Should -Not -Match '\bRestore-StepChanges\b'
    }
}
