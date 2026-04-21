# ==============================================================================
#  helpers/process-priority.ps1  —  Native Process Priority & CCD Affinity
# ==============================================================================
#
#  Replaces Process Lasso with native Windows mechanisms:
#    1. IFEO PerfOptions — persistent CPU priority via registry (kernel-level)
#    2. Scheduled task — CCD affinity for dual-CCD Ryzen X3D (if applicable)
#
#  IFEO (Image File Execution Options) PerfOptions:
#    Windows kernel reads CpuPriorityClass at process creation — zero overhead.
#    No background service, no polling. The kernel applies it before the process
#    entry point runs.
#
#  CpuPriorityClass values (PROCESS_PRIORITY_CLASS kernel enum):
#    1=Idle  2=Normal  3=High  4=Realtime  5=BelowNormal  6=AboveNormal

$CS2_AffinityTaskName  = "CS2_Optimize_CCD_Affinity"
$CS2_AffinityScriptPath = "$CFG_WorkDir\cs2_affinity.ps1"

function Get-X3DCcdInfo {
    <#
    .SYNOPSIS  Detects Ryzen X3D CPU and returns CCD topology info.
    .DESCRIPTION
        Single-CCD X3D (5700X3D, 5800X3D, 7800X3D, 9800X3D): no pinning needed.
        Dual-CCD X3D (7900X3D, 7950X3D, 9900X3D, 9950X3D): returns affinity
        mask for CCD0 (V-Cache CCD).

        AMD standard logical processor numbering (SMT enabled):
          LP 0..(totalCores-1)      = first thread of each core (CCD0 then CCD1)
          LP totalCores..(totalLP-1) = second thread (SMT partner)
        CCD0 always contains cores 0..(ccd0Cores-1).
    #>
    try {
        $cpu = Get-CachedCpuInfo
    } catch { return $null }

    if ($cpu.Name -notmatch "X3D") { return $null }

    # Single CCD X3D — all cores on V-Cache, no pinning needed
    if ($cpu.Name -match "(5700X3D|5800X3D|7800X3D|9700X3D|9800X3D)") {
        return @{
            IsX3D   = $true
            DualCCD = $false
            CpuName = $cpu.Name.Trim()
            Reason  = "Single CCD — all cores have V-Cache, no pinning needed"
        }
    }

    # Dual CCD X3D — CCD0 has V-Cache
    if ($cpu.Name -match "(7900X3D|7950X3D|9900X3D|9950X3D)") {
        $totalCores = $cpu.NumberOfCores
        $totalLP    = $cpu.NumberOfLogicalProcessors
        $smtEnabled = ($totalLP -gt $totalCores)
        # Per-model CCD0 core count for known asymmetric X3D CPUs.
        # 9900X3D = 8+4, 9950X3D/7950X3D = 8+8, 7900X3D = 6+6.
        $ccd0Cores = if ($cpu.Name -match "9900X3D") { 8 }
            elseif ($cpu.Name -match "7950X3D|9950X3D") { 8 }
            elseif ($cpu.Name -match "7900X3D") { 6 }
            else { [math]::Floor($totalCores / 2) }

        # Build affinity mask for CCD0 (V-Cache CCD)
        # Use [uint64] to avoid sign overflow on 64-core systems
        [uint64]$mask = 0
        for ($i = 0; $i -lt $ccd0Cores; $i++) {
            $mask = $mask -bor ([uint64]1 -shl $i)                     # First thread
            if ($smtEnabled -and ($totalCores + $i) -lt 64) {
                $mask = $mask -bor ([uint64]1 -shl ($totalCores + $i))  # SMT partner
            }
        }

        return @{
            IsX3D        = $true
            DualCCD      = $true
            CpuName      = $cpu.Name.Trim()
            Ccd0Cores    = $ccd0Cores
            TotalCores   = $totalCores
            SmtEnabled   = $smtEnabled
            AffinityMask = $mask
            AffinityHex  = "0x" + $mask.ToString("X")
            Reason       = "Dual CCD — CCD0 (cores 0-$($ccd0Cores - 1)) has V-Cache"
        }
    }

    # Unknown X3D variant — inform but don't auto-pin
    return @{
        IsX3D   = $true
        DualCCD = $null
        CpuName = $cpu.Name.Trim()
        Reason  = "Unknown X3D model — manual CCD identification recommended"
    }
}

function Set-CS2ProcessPriority {
    <#
    .SYNOPSIS  Sets persistent High CPU priority for cs2.exe via IFEO PerfOptions.
    .DESCRIPTION
        Uses Windows IFEO to set CpuPriorityClass=3 (High) for cs2.exe.
        The kernel reads this at process creation — zero overhead, no service.

        On dual-CCD Ryzen X3D CPUs, also creates a scheduled task to pin cs2.exe
        affinity to the V-Cache CCD (CCD0).

        DRY-RUN: IFEO write intercepted by Set-RegistryValue. Task printed only.
    #>
    [CmdletBinding()]
    param()

    # ── 1. IFEO PerfOptions — persistent High priority ────────────────
    $ifeoPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\cs2.exe\PerfOptions"
    Set-RegistryValue $ifeoPath "CpuPriorityClass" 3 "DWord" `
        "Persistent High CPU priority for cs2.exe (IFEO kernel-level)"

    # ── 2. Apply to currently running cs2.exe (if any) ─────────────────
    $cs2 = Get-Process cs2 -ErrorAction SilentlyContinue
    if ($cs2) {
        if ($SCRIPT:DryRun) {
            Write-Host "  [DRY-RUN] Would set running cs2.exe priority to High" -ForegroundColor Magenta
        } else {
            try {
                $cs2 | ForEach-Object { $_.PriorityClass = 'High' }
                Write-OK "Applied High priority to running cs2.exe"
            } catch { Write-Warn "Could not set priority on running cs2.exe: $_" }
        }
    }

    # ── 3. X3D CCD Affinity (dual-CCD only) ──────────────────────────
    $x3d = Get-X3DCcdInfo
    if ($x3d -and $x3d.IsX3D) {
        Write-Blank
        if ($x3d.DualCCD) {
            Write-Host "  X3D DETECTED: $($x3d.CpuName)" -ForegroundColor Yellow
            Write-Host "  $($x3d.Reason)" -ForegroundColor White
            Write-Host "  V-Cache CCD affinity mask: $($x3d.AffinityHex)" -ForegroundColor Cyan

            # Set affinity on running cs2.exe if present
            if ($cs2) {
                if ($SCRIPT:DryRun) {
                    Write-Host "  [DRY-RUN] Would set cs2.exe affinity to $($x3d.AffinityHex)" -ForegroundColor Magenta
                } else {
                    try {
                        $cs2 | ForEach-Object { $_.ProcessorAffinity = [IntPtr]::new([long]$x3d.AffinityMask) }
                        Write-OK "Applied CCD0 affinity to running cs2.exe ($($x3d.AffinityHex))"
                    } catch { Write-Warn "Could not set affinity on running cs2.exe: $_" }
                }
            }

            # Create scheduled task for persistent CCD affinity
            Install-CS2AffinityTask -AffinityMask $x3d.AffinityMask -AffinityHex $x3d.AffinityHex

        } elseif ($x3d.DualCCD -eq $false) {
            Write-Info "X3D detected ($($x3d.CpuName)): $($x3d.Reason)"
        } else {
            Write-Warn "X3D detected ($($x3d.CpuName)): $($x3d.Reason) — verify CCD layout manually."
        }
    }

    Write-Blank
    Write-OK "CS2 process priority: High (persistent via IFEO PerfOptions)"
    Write-Host "  Alternative for advanced CPU management: bitsum.com/processlasso/" -ForegroundColor DarkGray
}

function Install-CS2AffinityTask {
    <#
    .SYNOPSIS  Creates a scheduled task that periodically pins cs2.exe to V-Cache CCD.
    .DESCRIPTION
        Installs a lightweight script (C:\CS2_OPTIMIZE\cs2_affinity.ps1) that checks
        if cs2.exe is running and sets its ProcessorAffinity to the V-Cache CCD.
        A scheduled task runs this script every 2 minutes after logon.
        Each execution takes ~50ms and only modifies affinity if needed.
    #>
    param([uint64]$AffinityMask, [string]$AffinityHex)

    if ($SCRIPT:DryRun) {
        Write-Host "  [DRY-RUN] Would create scheduled task '$CS2_AffinityTaskName'" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN] Would create affinity script: $CS2_AffinityScriptPath" -ForegroundColor DarkMagenta
        return
    }

    # Backup task existence before creation
    Backup-ScheduledTask -TaskName $CS2_AffinityTaskName -StepTitle $SCRIPT:CurrentStepTitle -ScriptPath $CS2_AffinityScriptPath

    # SECURITY: The affinity script is written to C:\CS2_OPTIMIZE\ and executed by a scheduled
    # task with HighestAvailable privilege. If a non-admin user can modify this file, they achieve
    # privilege escalation. C:\CS2_OPTIMIZE\ is created by an admin process, so default ACLs
    # inherit from C:\ — Administrators: Full, SYSTEM: Full, Users: Read+Execute.
    # Accepted risk: if a local admin has already compromised ACLs on C:\, the entire system is
    # already compromised. The task runs as InteractiveToken (current user), NOT SYSTEM,
    # limiting the blast radius to the logged-in user's privilege level.

    # Create the affinity setter script
    # Use [long] cast to prevent Int32 truncation on high-core-count CPUs (>32 logical processors)
    $scriptContent = @"
# CS2 CCD Affinity Setter — created by CS2 Optimization Suite
# Sets cs2.exe affinity to V-Cache CCD (mask: $AffinityHex)
# Runs every 2 minutes via scheduled task. Each run takes ~50ms.
`$global:_affinityErrors = 0
[long]`$mask = $AffinityMask
`$procs = Get-Process cs2 -ErrorAction SilentlyContinue
if (`$procs) {
    foreach (`$p in `$procs) {
        try {
            if (`$p.ProcessorAffinity -ne [IntPtr]`$mask) {
                `$p.ProcessorAffinity = [IntPtr]`$mask
            }
        } catch { `$global:_affinityErrors++ }
    }
}
"@
    Set-Content -Path $CS2_AffinityScriptPath -Value $scriptContent -Encoding UTF8 -Force

    # Register scheduled task via XML for reliable logon trigger + repetition
    $escapedPath = [System.Security.SecurityElement]::Escape($CS2_AffinityScriptPath)
    $taskXml = @"
<?xml version="1.0"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <Repetition>
        <Interval>PT2M</Interval>
        <Duration>PT0S</Duration>
        <StopAtDurationEnd>false</StopAtDurationEnd>
      </Repetition>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Actions Context="Author">
    <Exec>
      <Command>%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe</Command>
      <Arguments>-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "$escapedPath"</Arguments>
    </Exec>
  </Actions>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT1M</ExecutionTimeLimit>
    <Enabled>true</Enabled>
    <Hidden>true</Hidden>
  </Settings>
</Task>
"@

    try {
        Register-ScheduledTask -TaskName $CS2_AffinityTaskName -Xml $taskXml -Force | Out-Null
        Write-OK "Scheduled task '$CS2_AffinityTaskName' created (CCD affinity every 2 min)"
    } catch {
        Write-Warn "Could not create scheduled task: $_"
        Write-Info "Manual alternative: set cs2.exe affinity to $AffinityHex in Task Manager"
    }
}

function Remove-CS2ProcessPriority {
    <#
    .SYNOPSIS  Removes IFEO priority setting and CCD affinity scheduled task.
    .DESCRIPTION  Called during rollback via Restore-Interactive.
    #>

    if ($SCRIPT:DryRun) {
        Write-Host "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Would remove CS2 process priority settings" -ForegroundColor Magenta
        return
    }

    # Remove IFEO PerfOptions
    $ifeoPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\cs2.exe\PerfOptions"
    if (Test-Path $ifeoPath) {
        try {
            Remove-Item -Path $ifeoPath -Force -ErrorAction Stop
            Write-OK "Removed IFEO PerfOptions for cs2.exe"

            # Clean up empty parent key
            $parentPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\cs2.exe"
            $parent = Get-Item $parentPath -ErrorAction SilentlyContinue
            if ($parent -and $parent.SubKeyCount -eq 0 -and $parent.ValueCount -eq 0) {
                Remove-Item -Path $parentPath -Force -ErrorAction SilentlyContinue
            }
        } catch { Write-Warn "Could not remove IFEO PerfOptions: $_" }
    }

    # Remove scheduled task — stop it first if running to avoid Unregister failure
    try {
        $task = Get-ScheduledTask -TaskName $CS2_AffinityTaskName -ErrorAction SilentlyContinue
        if ($task) {
            if ($task.State -eq "Running") {
                Stop-ScheduledTask -TaskName $CS2_AffinityTaskName -ErrorAction SilentlyContinue
                Write-DebugLog "Stopped running task '$CS2_AffinityTaskName' before unregistering"
            }
            Unregister-ScheduledTask -TaskName $CS2_AffinityTaskName -Confirm:$false
            Write-OK "Removed scheduled task: $CS2_AffinityTaskName"
        }
    } catch { Write-DebugLog "Task removal: $_" }

    # Remove affinity script
    if (Test-Path $CS2_AffinityScriptPath) {
        Remove-Item $CS2_AffinityScriptPath -Force -ErrorAction SilentlyContinue
        Write-OK "Removed: $CS2_AffinityScriptPath"
    }
}
