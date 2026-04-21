# ==============================================================================
#  tests/helpers/backup-dryrun.Tests.ps1  --  Backup-* DRY-RUN consistency
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Backup-* DRY-RUN guards" {

    BeforeEach {
        Reset-TestState
        $SCRIPT:DryRun = $true
        Mock Write-DebugLog {}
    }

    It "Backup-RegistryValue skips buffering in DRY-RUN" {
        Backup-RegistryValue -Path "HKLM:\SOFTWARE\Test" -Name "Value" -StepTitle "Step"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Backup-ServiceState skips buffering in DRY-RUN" {
        Backup-ServiceState -ServiceName "Spooler" -StepTitle "Step"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Backup-PowerPlan skips buffering in DRY-RUN" {
        Backup-PowerPlan -StepTitle "Step"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Backup-BootConfig skips buffering in DRY-RUN" {
        Backup-BootConfig -Key "disabledynamictick" -StepTitle "Step"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Backup-ScheduledTask skips buffering in DRY-RUN" {
        Backup-ScheduledTask -TaskName "CS2Task" -StepTitle "Step"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Backup-NicAdapterProperty skips buffering in DRY-RUN" {
        Backup-NicAdapterProperty -AdapterName "Ethernet" -PropertyName "EEE" -OriginalValue "Disabled" -PropertyType "DisplayName" -StepTitle "Step"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Backup-QosAndUro skips buffering in DRY-RUN" {
        Backup-QosAndUro -PolicyNames @("CS2 UDP") -UroState "disabled" -StepTitle "Step"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Backup-DefenderExclusions skips buffering in DRY-RUN" {
        Backup-DefenderExclusions -ExclusionPaths @("C:\Games\CS2") -ExclusionProcesses @("cs2.exe") -StepTitle "Step"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Backup-PagefileConfig skips buffering in DRY-RUN" {
        Backup-PagefileConfig -AutomaticManaged $true -PagefilePath "C:\pagefile.sys" -InitialSize 0 -MaximumSize 0 -StepTitle "Step"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Backup-DnsConfig skips buffering in DRY-RUN" {
        Backup-DnsConfig -AdapterName "Ethernet" -InterfaceIndex 12 -OriginalDnsServers @("1.1.1.1") -StepTitle "Step"
        $SCRIPT:_backupPending.Count | Should -Be 0
    }

    It "Backup-DrsSettings skips buffering in DRY-RUN" {
        Backup-DrsSettings -Session ([IntPtr]::Zero) -DrsProfile ([IntPtr]::Zero) -SettingIds @([uint32]1) -StepTitle "Step" -ProfileName "CS2" -ProfileCreated $false
        $SCRIPT:_backupPending.Count | Should -Be 0
    }
}
