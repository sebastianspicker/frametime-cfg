# ==============================================================================
#  helpers/backup-restore.ps1  —  Setting Backup & Restore System
# ==============================================================================
#
#  Automatically captures registry, service, and boot config state BEFORE
#  modifications. Enables per-step or full rollback if something goes wrong.
#
#  Integration:
#    Set-RegistryValue / Set-BootConfig auto-backup via $SCRIPT:CurrentStepTitle
#    Backup-DrsSettings / Restore-DrsSettings for NVIDIA DRS profile settings
#    Manual: Backup-ServiceState, Restore-StepChanges, Restore-AllChanges

$CFG_BackupFile = "$CFG_WorkDir\backup.json"
$CFG_BackupLockFile = "$CFG_WorkDir\backup.lock"
# Sentinel used when DRS profile was found via app registration rather than by name.
# Must match between Backup-DrsSettings (write) and Restore-DrsSettings (read).
$SCRIPT:DRS_FOUND_VIA_APP = "(found via cs2.exe)"

# ── In-memory batch buffer ─────────────────────────────────────────────────
# Backup entries are accumulated in $SCRIPT:_backupPending during a step, then
# flushed to disk once via Flush-BackupBuffer.  This avoids O(n^2) I/O from
# reading+writing backup.json on every single Set-RegistryValue call (~60+
# calls per full Phase 1 run).  Flush is called automatically by
# Invoke-TieredStep after each step's action completes, and also by any
# function that reads backup data (Get-BackupData) to ensure consistency.
#
# DRY-RUN guard pattern:
#   Every Backup-* function owns its own `if ($SCRIPT:DryRun) { return }` guard
#   as the first statement. Callers should invoke backup capture unconditionally
#   when they have enough context; the backup function itself decides whether the
#   current mode allows persisting an entry.
$SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

function New-BackupDataObject {
    return [PSCustomObject]@{
        entries = @()
        created = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
}

function Get-BackupVersionFiles {
    $backupDir = Split-Path $CFG_BackupFile -Parent
    $backupName = Split-Path $CFG_BackupFile -Leaf
    $backupStem = if ($backupName -match '^(.*)\.json$') { $Matches[1] } else { $backupName }
    if (-not $backupDir -or -not (Test-Path $backupDir)) { return @() }
    return @(
        Get-ChildItem $backupDir -Filter "$backupStem.*.json" -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match "^$([regex]::Escape($backupStem))\.\d{8}-\d{6}\d{0,3}\.json$" } |
            Sort-Object Name
    )
}

function Prune-BackupVersions {
    $maxVersions = [Math]::Max(1, [int]$CFG_BackupMaxVersions)
    $versionFiles = @(Get-BackupVersionFiles)
    if ($versionFiles.Count -le $maxVersions) { return }

    $versionFiles |
        Select-Object -First ($versionFiles.Count - $maxVersions) |
        Remove-Item -Force -ErrorAction SilentlyContinue
}

function New-BackupFile {
    Save-JsonAtomic -Data (New-BackupDataObject) -Path $CFG_BackupFile
    Set-SecureAcl -Path $CFG_BackupFile
}

function Initialize-Backup {
    # Acquire lock before rotating or pruning so we never steal another live backup file.
    if (Test-BackupLock) {
        Write-Warn "Another CS2 Optimization window appears to be open already."
        Write-Host "  $([char]0x2139) What to do: Close the other window first, then try again." -ForegroundColor Cyan
        Write-Host "    If no other window is open, this will clear itself automatically." -ForegroundColor DarkGray
        throw "Backup lock is already held by another active CS2 Optimization process."
    }
    Set-BackupLock

    if (Test-Path $CFG_BackupFile) {
        $backupDir = Split-Path $CFG_BackupFile -Parent
        $backupName = Split-Path $CFG_BackupFile -Leaf
        $backupStem = if ($backupName -match '^(.*)\.json$') { $Matches[1] } else { $backupName }
        $stamp = (Get-Date).ToString("yyyyMMdd-HHmmssfff")
        $versionPath = Join-Path $backupDir "$backupStem.$stamp.json"
        Move-Item $CFG_BackupFile $versionPath -Force
        Prune-BackupVersions
    }
    if (-not (Test-Path $CFG_BackupFile)) { New-BackupFile } else { Set-SecureAcl -Path $CFG_BackupFile }
}

function Test-BackupLock {
    <#  Checks if another process is actively modifying backup.json.
        Returns $true if the lock is held (and the holding process is still alive).
        Stale locks are automatically cleaned up in three cases:
          1. The locking process is no longer running (crashed/exited).
          2. The PID was reused by a non-PowerShell process.
          3. The lock is older than 4 hours (handles hung/stalled processes).
        Mitigates PID reuse: verifies the process is PowerShell, not an unrelated
        process that inherited the recycled PID.  #>
    if (-not (Test-Path $CFG_BackupLockFile)) { return $false }
    try {
        $lockData = Get-Content $CFG_BackupLockFile -Raw -ErrorAction Stop | ConvertFrom-Json
        # Auto-expire: no optimization run should take more than 4 hours.
        # Handles the case where a process is alive but hung/stalled indefinitely.
        if ($lockData.started) {
            try {
                $parsedDate = [datetime]::Parse([string]$lockData.started)
            } catch {
                Write-DebugLog "Backup lock has unparseable timestamp '$($lockData.started)' — removing stale lock."
                Remove-Item $CFG_BackupLockFile -Force -ErrorAction SilentlyContinue
                return $false
            }
            $lockAge = (Get-Date) - $parsedDate
            if ($lockAge.TotalHours -gt 4) {
                Remove-Item $CFG_BackupLockFile -Force -ErrorAction SilentlyContinue
                Write-DebugLog "Removed expired backup lock (age: $([math]::Round($lockAge.TotalHours, 1))h, PID $($lockData.pid))."
                return $false
            }
        }
        $proc = Get-Process -Id $lockData.pid -ErrorAction SilentlyContinue
        if ($proc) {
            # Mitigate PID reuse: Windows recycles PIDs, so a live process with the
            # same PID may be entirely unrelated. Verify it's a PowerShell instance.
            $isPowerShell = $proc.ProcessName -match '^(?:powershell|pwsh|powershell_ise)$'
            if ($isPowerShell) { return $true }
            # PID was reused by a non-PowerShell process — stale lock
            Remove-Item $CFG_BackupLockFile -Force -ErrorAction SilentlyContinue
            Write-DebugLog "Removed stale backup lock (PID $($lockData.pid) reused by '$($proc.ProcessName)')."
            return $false
        }
        # Process is dead — stale lock; remove it
        Remove-Item $CFG_BackupLockFile -Force -ErrorAction SilentlyContinue
        Write-DebugLog "Removed stale backup lock (PID $($lockData.pid) no longer running)."
    } catch {
        # Corrupted lock file — remove it
        Remove-Item $CFG_BackupLockFile -Force -ErrorAction SilentlyContinue
    }
    return $false
}

function Set-BackupLock {
    <#  Called at the start of optimization and restore operations.  #>
    $lockData = @{ pid = $PID; started = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss") }
    Save-JsonAtomic -Data $lockData -Path $CFG_BackupLockFile
}

function Remove-BackupLock {
    Remove-Item $CFG_BackupLockFile -Force -ErrorAction SilentlyContinue
}

function Flush-BackupBuffer {
    <#  Writes any pending in-memory backup entries to backup.json in a single I/O pass.
        Safe to call multiple times — no-op when the buffer is empty.
        On failure: entries stay in memory (Clear runs AFTER Save) for the next flush attempt.
        If the process crashes before any flush, that step's backups are lost — acceptable
        tradeoff vs. O(n^2) I/O from flushing on every Set-RegistryValue call.  #>
    if ($SCRIPT:_backupPending.Count -eq 0) { return }
    $backup = Get-BackupDataRaw
    $entries = [System.Collections.ArrayList]@($backup.entries)
    foreach ($e in $SCRIPT:_backupPending) {
        # Deduplicate: skip if an entry for the same key already exists (prevents
        # duplicate backups on re-run — the first backup holds the true original value)
        $isDupe = $false
        foreach ($existing in $entries) {
            if ($existing.step -eq $e.step -and $existing.type -eq $e.type) {
                switch ($e.type) {
                    "registry"      { $isDupe = ($existing.PSObject.Properties['path'] -and $e.Contains('path') -and $existing.path -eq $e.path -and $existing.name -eq $e.name) }
                    "service"       { $isDupe = ($existing.name -eq $e.name) }
                    "scheduledtask" { $isDupe = ($existing.PSObject.Properties['taskName'] -and $e.Contains('taskName') -and $existing.taskName -eq $e.taskName) }
                    "bootconfig"    { $isDupe = ($existing.PSObject.Properties['key'] -and $e.Contains('key') -and $existing.key -eq $e.key) }
                    "powerplan"     { $isDupe = ($existing.PSObject.Properties['originalGuid'] -and $e.Contains('originalGuid') -and $existing.originalGuid -eq $e.originalGuid) }
                    "nic_adapter"   { $isDupe = ($existing.PSObject.Properties['adapterName'] -and $e.Contains('adapterName') -and $existing.adapterName -eq $e.adapterName -and $existing.propertyName -eq $e.propertyName) }
                    "dns"           { $isDupe = ($existing.PSObject.Properties['adapterName'] -and $e.Contains('adapterName') -and $existing.adapterName -eq $e.adapterName) }
                    default         { $isDupe = $false }
                }
                if ($isDupe) { break }
            }
        }
        if (-not $isDupe) { $entries.Add($e) | Out-Null }
    }
    $backup.entries = @($entries)
    Save-BackupData $backup
    $SCRIPT:_backupPending.Clear()
}

function Get-BackupDataRaw {
    <#  Reads backup.json from disk without flushing the pending buffer.
        Internal use only — callers outside this module should use Get-BackupData.  #>
    if (-not (Test-Path $CFG_BackupFile)) { Initialize-Backup }
    try {
        $raw = Get-Content $CFG_BackupFile -Raw -ErrorAction Stop | ConvertFrom-Json
        if ($null -eq $raw.entries) { $raw | Add-Member -NotePropertyName "entries" -NotePropertyValue @() -Force }
        # Force entries to array — PS 5.1 ConvertFrom-Json unwraps single-element arrays to scalars
        $raw.entries = @($raw.entries)
        return $raw
    } catch {
        # Preserve corrupted file for recovery before overwriting
        $ts = (Get-Date).ToString("yyyyMMdd-HHmmss")
        $backupDir = Split-Path $CFG_BackupFile -Parent
        $backupName = Split-Path $CFG_BackupFile -Leaf
        $backupStem = if ($backupName -match '^(.*)\.json$') { $Matches[1] } else { $backupName }
        $corruptPath = Join-Path $backupDir "$backupStem.corrupt.$ts.json"
        try { Copy-Item $CFG_BackupFile $corruptPath -Force -ErrorAction Stop } catch { Write-DebugLog "Could not preserve corrupted backup file — original may already be gone." }
        Write-Warn "backup.json was corrupted — saved copy to $corruptPath before resetting."
        Write-Warn "Backup history reset — previous entries preserved in $corruptPath"
        Remove-Item $CFG_BackupFile -Force -ErrorAction SilentlyContinue
        New-BackupFile
        return (New-BackupDataObject)
    }
}

function Get-BackupData {
    <#  Returns all backup data including any pending (unflushed) entries.
        Flushes the buffer first to ensure disk and memory are consistent.  #>
    Flush-BackupBuffer
    return Get-BackupDataRaw
}

function Save-BackupData($data) {
    Save-JsonAtomic -Data $data -Path $CFG_BackupFile -Depth 10
    Set-SecureAcl -Path $CFG_BackupFile
}

function Backup-RegistryValue {
    <#  Records the current value of a registry key before modification.
        Entries are buffered in memory and flushed to disk at step boundaries
        (via Flush-BackupBuffer) to avoid O(n^2) I/O.  #>
    [CmdletBinding()]
    param([string]$Path, [string]$Name, [string]$StepTitle)
    if ($SCRIPT:DryRun) { return }
    $existing = $null
    $regType  = $null
    try {
        if (Test-Path $Path) {
            $prop = Get-ItemProperty -Path $Path -Name $Name -ErrorAction Stop
            $existing = $prop.$Name
            try {
                $regType = (Get-Item $Path).GetValueKind($Name).ToString()
            } catch {
                # Fallback: autostart \Run keys store command-line strings, not DWords.
                # Default to "String" for Run paths, "DWord" otherwise.
                $regType = if ($Path -match '\\Run$' -or $Path -match '\\Run\\') { "String" } else { "DWord" }
            }
        }
    } catch { Write-DebugLog "Backup-RegistryValue: could not read '$Name' from '$Path' — treating as non-existent." }

    $entry = [ordered]@{
        type          = "registry"
        path          = $Path
        name          = $Name
        originalValue = $existing
        originalType  = $regType
        existed       = ($null -ne $existing)
        step          = $StepTitle
        timestamp     = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
    $SCRIPT:_backupPending.Add($entry)
}

function Backup-ServiceState {
    <#  Records current service start type, delayed-start flag, and status before modification.
        Entries are buffered in memory and flushed at step boundaries.  #>
    param([string]$ServiceName, [string]$StepTitle)
    if ($SCRIPT:DryRun) { return }
    try {
        $svc = Get-Service -Name $ServiceName -ErrorAction Stop
        $escapedName = $ServiceName -replace "'", "''"
        $startType = (Get-CimInstance Win32_Service -Filter "Name='$escapedName'" -ErrorAction Stop).StartMode
        # Capture DelayedAutoStart flag — services with "Automatic (Delayed Start)" show StartMode=Auto
        # but have a separate registry flag. Without this, restore loses the "Delayed" qualifier.
        $delayedStart = $false
        try {
            $regPath = "HKLM:\SYSTEM\CurrentControlSet\Services\$ServiceName"
            $delayReg = Get-ItemProperty -Path $regPath -Name "DelayedAutostart" -ErrorAction SilentlyContinue
            $delayedStart = ($delayReg.DelayedAutostart -eq 1)
        } catch { Write-DebugLog "Backup-ServiceState: could not read DelayedAutostart for '$ServiceName' — defaulting to false." }
        $entry = [ordered]@{
            type              = "service"
            name              = $ServiceName
            originalStartType = $startType
            delayedAutoStart  = $delayedStart
            originalStatus    = $svc.Status.ToString()
            step              = $StepTitle
            timestamp         = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        }
        $SCRIPT:_backupPending.Add($entry)
    } catch { Write-DebugLog "Backup-ServiceState: $ServiceName not found" }
}

function Backup-PowerPlan {
    <#  Records the currently active power plan GUID before switching.
        Entries are buffered in memory and flushed at step boundaries.  #>
    param([string]$StepTitle)
    if ($SCRIPT:DryRun) { return }
    $originalGuid = $null
    $originalName = $null
    try {
        $activeOutput = powercfg /getactivescheme 2>&1
        if ($activeOutput -match "([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})") {
            $originalGuid = $Matches[1]
            if ($activeOutput -match "\((.+)\)\s*$") {
                $originalName = $Matches[1]
            }
        }
    } catch { Write-DebugLog "Backup-PowerPlan: powercfg query failed — active plan GUID not captured." }

    if ($originalGuid) {
        # If the active plan is already "CS2 Optimized", skip backup — we don't want
        # to record the CS2 plan as the rollback target on re-runs. The original
        # user plan is already captured from the first run.
        if ($originalName -and $originalName -match "CS2 Optimized") {
            Write-DebugLog "Backup-PowerPlan: active plan is '$originalName' — skipping backup (re-run detected)"
            return
        }
        $entry = [ordered]@{
            type          = "powerplan"
            originalGuid  = $originalGuid
            originalName  = $originalName
            step          = $StepTitle
            timestamp     = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        }
        $SCRIPT:_backupPending.Add($entry)
        Write-DebugLog "Backup-PowerPlan: saved $originalGuid ($originalName)"
    }
}

function Backup-BootConfig {
    <#  Records current bcdedit value before modification.
        Entries are buffered in memory and flushed at step boundaries.
        Uses bcdedit /v to get raw BCD element names (hex IDs), which are locale-independent.
        Without /v, key names like "safeboot" are localized (e.g., German: "Abgesicherter Start")
        and the English key name match would fail on non-English Windows.  #>
    [CmdletBinding()]
    param([string]$Key, [string]$StepTitle)
    if ($SCRIPT:DryRun) { return }

    # Map well-known bcdedit key names to their raw BCD element hex IDs.
    # bcdedit /enum /v outputs hex IDs instead of localized names.
    # Reference: Microsoft BCD WMI Provider documentation.
    $bcdElementMap = @{
        "safeboot"           = "0x26000081"
        "disabledynamictick" = "0x26000060"  # BcdOSLoaderBoolean_DisableDynamicTick
        "useplatformtick"    = "0x26000092"  # BcdOSLoaderBoolean_UsePlatformTick
        "useplatformclock"   = "0x26000091"  # BcdOSLoaderBoolean_UseLegacyApicTimer
    }
    $hexId = if ($bcdElementMap.ContainsKey($Key)) { $bcdElementMap[$Key] } else { $null }

    $existing = $null
    try {
        $bcdOutput = bcdedit /enum "{current}" /v 2>&1
        foreach ($line in $bcdOutput) {
            # Try hex ID match first (locale-independent), fall back to key name
            if ($hexId -and $line -match "^\s*$hexId\s+(.+)$") {
                $existing = $Matches[1].Trim()
                break
            } elseif (-not $hexId -and $line -match "^\s*$([regex]::Escape($Key))\s+(.+)$") {
                $existing = $Matches[1].Trim()
                break
            }
        }
    } catch { Write-DebugLog "Backup-BootConfig: bcdedit enum failed for key '$Key' — treating as non-existent." }

    $entry = [ordered]@{
        type          = "bootconfig"
        key           = $Key
        originalValue = $existing
        existed       = ($null -ne $existing)
        step          = $StepTitle
        timestamp     = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
    $SCRIPT:_backupPending.Add($entry)
}

function Invoke-BootConfigRestoreCommand {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string[]]$Arguments
    )

    & bcdedit @Arguments 2>&1
}

function Backup-ScheduledTask {
    <#  Records whether a scheduled task existed and its enabled state before we modify it.
        Entries are buffered in memory and flushed at step boundaries.  #>
    param([string]$TaskName, [string]$StepTitle, [string]$ScriptPath = "")
    if ($SCRIPT:DryRun) { return }
    $existed = $false
    $wasEnabled = $false
    try {
        $task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
        $existed = ($null -ne $task)
        if ($existed) {
            $wasEnabled = ($task.State -ne "Disabled")
        }
    } catch { Write-DebugLog "Backup-ScheduledTask: could not query task '$TaskName' — assuming it does not exist." }

    $entry = [ordered]@{
        type       = "scheduledtask"
        taskName   = $TaskName
        existed    = $existed
        wasEnabled = $wasEnabled
        scriptPath = $ScriptPath
        step       = $StepTitle
        timestamp  = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
    $SCRIPT:_backupPending.Add($entry)
    Write-DebugLog "Backup-ScheduledTask: '$TaskName' existed=$existed wasEnabled=$wasEnabled"
}

function Backup-NicAdapterProperty {
    <#  Records the current value of a NIC adapter property before modification.
        Entries are buffered in memory and flushed at step boundaries.
        PropertyType is "DisplayName" for Set-NetAdapterAdvancedProperty -DisplayName
        or "RegistryKeyword" for -RegistryKeyword calls.  #>
    param(
        [string]$AdapterName,
        [string]$PropertyName,
        [string]$OriginalValue,
        [string]$PropertyType,
        [string]$StepTitle
    )
    if ($SCRIPT:DryRun) { return }
    # Capture InterfaceDescription for cross-adapter detection on restore
    $ifDesc = ""
    try {
        $adapter = Get-NetAdapter -Name $AdapterName -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($adapter) { $ifDesc = $adapter.InterfaceDescription }
    } catch { Write-DebugLog "Backup-NicAdapterProperty: could not resolve InterfaceDescription for '$AdapterName'" }
    $entry = [ordered]@{
        type                 = "nic_adapter"
        adapterName          = $AdapterName
        interfaceDescription = $ifDesc
        propertyName         = $PropertyName
        originalValue        = $OriginalValue
        propertyType         = $PropertyType
        step                 = $StepTitle
        timestamp            = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
    $SCRIPT:_backupPending.Add($entry)
    Write-DebugLog "Backup-NicAdapterProperty: '$PropertyName' = '$OriginalValue' on '$AdapterName' ($ifDesc)"
}

function Backup-QosAndUro {
    <#  Records existing QoS policy names and URO state before modification.
        Entries are buffered in memory and flushed at step boundaries.  #>
    param(
        [string[]]$PolicyNames,
        [string]$UroState,
        [string]$StepTitle
    )
    if ($SCRIPT:DryRun) { return }
    $entry = [ordered]@{
        type        = "qos_uro"
        policies    = $PolicyNames
        uroState    = $UroState
        step        = $StepTitle
        timestamp   = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
    $SCRIPT:_backupPending.Add($entry)
    Write-DebugLog "Backup-QosAndUro: policies=[$($PolicyNames -join ', ')] uro=$UroState"
}

function Backup-DefenderExclusions {
    <#  Records Defender exclusion paths and processes added by this tool.
        Entries are buffered in memory and flushed at step boundaries.  #>
    param(
        [string[]]$ExclusionPaths,
        [string[]]$ExclusionProcesses,
        [string]$StepTitle
    )
    if ($SCRIPT:DryRun) { return }
    $entry = [ordered]@{
        type               = "defender"
        exclusionPaths     = $ExclusionPaths
        exclusionProcesses = $ExclusionProcesses
        step               = $StepTitle
        timestamp          = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
    $SCRIPT:_backupPending.Add($entry)
    Write-DebugLog "Backup-DefenderExclusions: $($ExclusionPaths.Count) paths, $($ExclusionProcesses.Count) processes"
}

function Backup-PagefileConfig {
    <#  Records current pagefile configuration before modification.
        Entries are buffered in memory and flushed at step boundaries.
        Manual restoration: System Properties -> Advanced -> Performance -> Virtual Memory  #>
    param(
        [bool]$AutomaticManaged,
        [string]$PagefilePath,
        [int]$InitialSize,
        [int]$MaximumSize,
        [string]$StepTitle
    )
    if ($SCRIPT:DryRun) { return }
    $entry = [ordered]@{
        type              = "pagefile"
        automaticManaged  = $AutomaticManaged
        pagefilePath      = $PagefilePath
        initialSize       = $InitialSize
        maximumSize       = $MaximumSize
        step              = $StepTitle
        timestamp         = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
    $SCRIPT:_backupPending.Add($entry)
    Write-DebugLog "Backup-PagefileConfig: auto=$AutomaticManaged path=$PagefilePath init=$InitialSize max=$MaximumSize"
}

function Backup-DnsConfig {
    <#  Records current DNS server addresses before modification.
        Entries are buffered in memory and flushed at step boundaries.  #>
    param(
        [string]$AdapterName,
        [int]$InterfaceIndex,
        [string[]]$OriginalDnsServers,
        [string]$StepTitle
    )
    if ($SCRIPT:DryRun) { return }
    $entry = [ordered]@{
        type               = "dns"
        adapterName        = $AdapterName
        interfaceIndex     = $InterfaceIndex
        originalDnsServers = $OriginalDnsServers
        step               = $StepTitle
        timestamp          = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
    $SCRIPT:_backupPending.Add($entry)
    Write-DebugLog "Backup-DnsConfig: adapter=$AdapterName dns=[$($OriginalDnsServers -join ', ')]"
}

function Backup-DrsSettings {
    <#
    .SYNOPSIS  Records current NVIDIA DRS setting values before overwrite.
    .DESCRIPTION
        Reads each setting ID from the DRS profile and stores the current value
        (or null if the setting doesn't exist yet) in backup.json.
        Called from Apply-NvidiaCS2ProfileDrs before writing new values.
        Uses the in-memory buffer (flushed at step boundaries).
    #>
    param(
        [IntPtr]$Session,
        [IntPtr]$DrsProfile,
        [uint32[]]$SettingIds,
        [string]$StepTitle,
        [string]$ProfileName,
        [bool]$ProfileCreated
    )
    if ($SCRIPT:DryRun) { return }

    $settings = @()
    foreach ($id in $SettingIds) {
        [uint32]$currentValue = 0
        $status = [NvApiDrs]::GetDwordSetting($Session, $DrsProfile, $id, [ref]$currentValue)
        # Store previousValue as [double] to preserve uint32 values through JSON round-trip.
        # ConvertTo-Json/ConvertFrom-Json loses uint32 type info; values >2^31 would become
        # negative Int32 or Int64. Casting to [double] ensures lossless round-trip for all
        # uint32 values (double has 53-bit mantissa, uint32 needs only 32 bits).
        $settings += [ordered]@{
            id            = [double]$id
            previousValue = $(if ($status -eq 0) { [double]$currentValue } else { $null })
            existed       = ($status -eq 0)
        }
    }

    $entry = [ordered]@{
        type           = "drs"
        step           = $StepTitle
        profile        = $ProfileName
        profileCreated = $ProfileCreated
        settings       = $settings
        timestamp      = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
    $SCRIPT:_backupPending.Add($entry)
    Write-DebugLog "Backup-DrsSettings: saved $($SettingIds.Count) DRS settings for '$StepTitle'"
}

function Restore-DrsSettings {
    <#
    .SYNOPSIS  Restores NVIDIA DRS settings from a backup entry.
    .DESCRIPTION
        For each backed up setting:
        - If it existed before: writes the previous value back via DRS
        - If it didn't exist: skips (no previous value to restore)
        If the profile was created by us, deletes it entirely.
        Returns $true on success, $false on failure.
    #>
    [Diagnostics.CodeAnalysis.SuppressMessageAttribute('PSReviewUnusedParameter', 'Entry',
        Justification = 'Entry is captured by the Invoke-DrsSession scriptblock closure')]
    param($Entry)

    if (-not (Initialize-NvApiDrs)) {
        Write-Warn "Cannot restore DRS settings — nvapi64.dll unavailable (driver uninstalled or 32-bit PowerShell)."
        Write-Warn "To restore DRS settings: reinstall the NVIDIA driver, then re-run Restore."
        return $false
    }

    try {
        # Use a single-element array as a mutable container that survives child scope
        # created by & $Action inside Invoke-DrsSession (scriptblock closures in PS
        # capture by reference, but & creates a new scope for simple variable writes).
        $result = @{ ok = $true }
        Invoke-DrsSession -Action {
            param($session)

            $drsProfile = [IntPtr]::Zero
            if ($Entry.profile -and $Entry.profile -ne $SCRIPT:DRS_FOUND_VIA_APP) {
                $drsProfile = [NvApiDrs]::FindProfileByName($session, $Entry.profile)
            }
            if ($drsProfile -eq [IntPtr]::Zero) {
                $drsProfile = [NvApiDrs]::FindApplicationProfile($session, "cs2.exe")
            }
            if ($drsProfile -eq [IntPtr]::Zero) {
                Write-Warn "DRS restore: CS2 profile not found — may have been deleted already."
                $result.ok = $false
                return
            }

            if ($Entry.profileCreated) {
                # We created this profile — delete it entirely
                try {
                    [NvApiDrs]::DeleteProfile($session, $drsProfile)
                    Write-OK "Deleted DRS profile: $($Entry.profile)"
                } catch {
                    Write-Warn "DRS restore: could not delete profile — $_"
                    $result.ok = $false
                }
            } else {
                # Profile existed before — restore individual settings
                $restored = 0
                $skipped  = 0
                $errors   = 0
                foreach ($s in $Entry.settings) {
                    try {
                        if ($s.existed) {
                            # Cast through [double] -> [uint32] to handle JSON round-trip
                            # (ConvertFrom-Json may produce Int64 or Double for numeric values)
                            [NvApiDrs]::SetDwordSetting($session, $drsProfile, [uint32][double]$s.id, [uint32][double]$s.previousValue)
                            $restored++
                        } else {
                            # Setting didn't exist before — skip (writing 0 is NOT equivalent to "not set"
                            # for many DRS settings, e.g., VSync tear control 0 = enabled, not "remove")
                            $skipped++
                        }
                    } catch {
                        $errors++
                        # Cast $s.id to [uint32] before .ToString('X') — JSON round-trip
                        # may produce [double], which does not support hex format specifier.
                        Write-DebugLog "DRS restore: failed for 0x$([uint32]([double]$s.id).ToString('X')): $_"
                    }
                }
                if ($errors -eq 0) {
                    Write-OK "Restored $restored DRS settings in profile '$($Entry.profile)'"
                } else {
                    Write-Warn "DRS restore: $restored restored, $errors failed in profile '$($Entry.profile)'"
                    $result.ok = $false
                }
                if ($skipped -gt 0) {
                    Write-Info "DRS restore: $skipped setting(s) were new (no previous value) — left as-is."
                }
            }
        }
        return $result.ok
    } catch {
        Write-Warn "DRS restore failed: $_"
        return $false
    }
}

function Invoke-PagefileRestoreAutomation {
    param($Entry)

    $computerSystem = Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction Stop
    if (-not $computerSystem) {
        throw "Win32_ComputerSystem instance not found"
    }

    if ($Entry.automaticManaged) {
        try {
            Invoke-PagefileCimUpdate -InputObject $computerSystem -Property @{ AutomaticManagedPagefile = $true }
        } catch {
            throw "failed to restore automatic pagefile management: $($_.Exception.Message)"
        }
        return [PSCustomObject]@{
            Success = $true
            Detail  = "automatic management restored"
        }
    }

    $pagefilePathWmi = $Entry.pagefilePath -replace '\\', '\\'
    try {
        $pagefileSetting = Get-CimInstance -ClassName Win32_PageFileSetting -Filter "Name='$pagefilePathWmi'" -ErrorAction Stop
        if (-not $pagefileSetting) {
            throw "pagefile setting not found for $($Entry.pagefilePath)"
        }

        Invoke-PagefileCimUpdate -InputObject $pagefileSetting -Property @{
            InitialSize = [int]$Entry.initialSize
            MaximumSize = [int]$Entry.maximumSize
        }
    } catch {
        throw "failed to restore custom pagefile size for $($Entry.pagefilePath): $($_.Exception.Message)"
    }

    try {
        Invoke-PagefileCimUpdate -InputObject $computerSystem -Property @{ AutomaticManagedPagefile = $false }
    } catch {
        throw "failed to disable automatic pagefile management after restoring custom size: $($_.Exception.Message)"
    }

    return [PSCustomObject]@{
        Success = $true
        Detail  = "custom size restored on $($Entry.pagefilePath)"
    }
}

function Invoke-PagefileCimUpdate {
    param(
        $InputObject,
        [hashtable]$Property
    )

    Set-CimInstance -InputObject $InputObject -Property $Property -ErrorAction Stop | Out-Null
}

function Show-BackupSummary {
    $backup = Get-BackupData
    if (-not $backup.entries -or $backup.entries.Count -eq 0) {
        Write-Info "No backups recorded yet."
        return
    }

    Write-Blank
    Write-Host "  ╔══════════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
    Write-Host "  ║  BACKUP SUMMARY — Recorded Settings Before Changes              ║" -ForegroundColor Cyan
    Write-Host "  ╠══════════════════════════════════════════════════════════════════╣" -ForegroundColor Cyan

    $grouped = $backup.entries | Group-Object -Property step
    foreach ($group in $grouped) {
        Write-Host "  ║  $($group.Name)  ($($group.Count) change(s))" -ForegroundColor White
        foreach ($e in $group.Group) {
            $detail = switch ($e.type) {
                "registry"   { "REG  $($e.name) = $(if($e.existed){"$($e.originalValue)"}else{'(not set)'})" }
                "service"    { "SVC  $($e.name) was $($e.originalStartType) / $($e.originalStatus)" }
                "bootconfig" { "BCD  $($e.key) = $(if($e.existed){"$($e.originalValue)"}else{'(not set)'})" }
                "powerplan"  { "PWR  was $($e.originalName) ($($e.originalGuid))" }
                "drs"           { "DRS  profile '$($e.profile)' — $($e.settings.Count) setting(s)" }
                "scheduledtask" { "TASK $($e.taskName) $(if($e.existed){'(existed before)'}else{'(created by us)'})" }
                "nic_adapter"   { "NIC  $($e.adapterName): $($e.propertyName) = $($e.originalValue)" }
                "qos_uro"       { "QOS  policies: [$($e.policies -join ', ')] | URO: $($e.uroState)" }
                "defender"      { "DEF  $(if($e.exclusionPaths){@($e.exclusionPaths).Count}else{0}) path(s), $(if($e.exclusionProcesses){@($e.exclusionProcesses).Count}else{0}) process(es)" }
                "pagefile"      { "PGF  auto=$($e.automaticManaged) init=$($e.initialSize)MB max=$($e.maximumSize)MB" }
                "dns"           { "DNS  $($e.adapterName): [$($e.originalDnsServers -join ', ')]" }
                default         { "???  Unknown type '$($e.type)'" }
            }
            Write-Host "  ║    $detail" -ForegroundColor DarkGray
        }
    }

    Write-Host "  ╚══════════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
    Write-Host "  Total: $($backup.entries.Count) setting(s) backed up" -ForegroundColor DarkGray
    Write-Host "  File:  $CFG_BackupFile" -ForegroundColor DarkGray
}

function Restore-StepChanges {
    [CmdletBinding()]
    param([string]$StepTitle)
    $backup = Get-BackupData
    $entries = @($backup.entries | Where-Object { $_.step -eq $StepTitle })
    if ($entries.Count -eq 0) {
        Write-Warn "No backup found for: $StepTitle"
        return $false
    }

    Write-Step "Restoring $($entries.Count) setting(s) from: $StepTitle"
    $restoreOk = 0
    $restoreFail = 0
    $restorePartial = 0
    $failedEntries = [System.Collections.Generic.List[object]]::new()
    $partialEntries = [System.Collections.Generic.List[object]]::new()
    foreach ($e in $entries) {
        $failBefore = $restoreFail
        $partialBefore = $restorePartial
        try {
            switch ($e.type) {
                "registry" {
                    # SECURITY: Validate registry path from backup.json — a tampered backup could
                    # inject writes to arbitrary registry locations (e.g., Run keys for persistence).
                    if ($e.path -notmatch '^(HKLM:|HKCU:|HKCR:|HKU:|HKCC:|Microsoft\.PowerShell\.Core\\Registry::(HKLM|HKCU|HKCR|HKU|HKCC)\\)') {
                        Write-Warn "Registry restore: invalid path — rejected: $($e.path)"
                        $restoreFail++
                        continue
                    }
                    if ($e.name -match '[\\/\x00]') {
                        Write-Warn "Registry restore: name contains invalid characters — rejected: $($e.name)"
                        $restoreFail++
                        continue
                    }
                    if ($e.existed) {
                        $restoreType = if ($e.originalType) { $e.originalType } else { "DWord" }
                        $restoreValue = $e.originalValue
                        # Binary values are serialized as int arrays in JSON; cast back to byte[]
                        if ($restoreType -eq "Binary" -and $restoreValue -is [array]) {
                            # Validate each element is in [0,255] before casting — JSON may
                            # contain Int64 values from manual editing or corruption.
                            $badValues = @($restoreValue | Where-Object { $_ -lt 0 -or $_ -gt 255 })
                            if ($badValues.Count -gt 0) {
                                Write-Warn "Binary restore for $($e.name): $($badValues.Count) byte(s) outside [0,255] — skipping (backup may be corrupted)."
                                $restoreFail++
                                continue
                            }
                            $restoreValue = [byte[]]@($restoreValue | ForEach-Object { [byte]$_ })
                        }
                        # MultiString values are deserialized as Object[] from JSON; ensure string[].
                        # PS 5.1 ConvertFrom-Json unwraps single-element arrays to scalars, so
                        # a MultiString backup with one entry arrives as a plain string — wrap it.
                        if ($restoreType -eq "MultiString") {
                            if ($null -eq $restoreValue) {
                                $restoreValue = [string[]]@()
                            } elseif ($restoreValue -is [array]) {
                                $restoreValue = [string[]]@($restoreValue)
                            } elseif ($restoreValue -is [string]) {
                                $restoreValue = [string[]]@($restoreValue)
                            }
                        }
                        # ExpandString: Set-ItemProperty -Type ExpandString is valid in PowerShell;
                        # no special handling needed — the value passes through as-is.
                        if (-not (Test-Path $e.path)) {
                            New-Item -Path $e.path -Force -ErrorAction Stop | Out-Null
                        }
                        Set-ItemProperty -Path $e.path -Name $e.name -Value $restoreValue -Type $restoreType -ErrorAction Stop
                        Write-OK "Restored: $($e.name) = $($e.originalValue)"
                    } else {
                        if (Test-Path $e.path) {
                            # Check if the value still exists before trying to remove — another tool
                            # or a reboot may have already cleaned it up. Without this check,
                            # Remove-ItemProperty throws, the entry stays in backup.json, and
                            # subsequent restore attempts fail forever on this entry.
                            $existingVal = Get-ItemProperty -Path $e.path -Name $e.name -ErrorAction SilentlyContinue
                            $valueProperty = if ($existingVal) { $existingVal.PSObject.Properties[[string]$e.name] } else { $null }
                            if ($null -ne $valueProperty) {
                                Remove-ItemProperty -Path $e.path -Name $e.name -ErrorAction Stop
                                Write-OK "Removed: $($e.name) (was not set before)"
                            } else {
                                Write-DebugLog "Restore: value '$($e.name)' already absent from '$($e.path)' — skip"
                            }
                        } else {
                            Write-DebugLog "Restore: path '$($e.path)' no longer exists — skip remove for '$($e.name)'"
                        }
                    }
                    $restoreOk++
                }
                "service" {
                    # SECURITY: Validate service name — a tampered backup.json could inject
                    # path traversal or special characters into registry paths and WMI queries.
                    if ($e.name -notmatch '^[a-zA-Z0-9_\-\. ]+$' -or $e.name.Length -gt 256) {
                        Write-Warn "Service restore skipped — invalid service name: '$($e.name)'"
                        $restoreFail++
                        continue
                    }
                    $startMap = @{ "Auto"="Automatic"; "Manual"="Manual"; "Disabled"="Disabled"; "Auto Delayed"="AutomaticDelayedStart" }
                    $mapped = if ($startMap[$e.originalStartType]) { $startMap[$e.originalStartType] } else { $e.originalStartType }
                    # Boot/System/Unknown are kernel driver start types — Set-Service cannot change them.
                    # These are not failures — kernel drivers manage their own start type and
                    # no user action is needed, so count as handled (not failed).
                    if ($e.originalStartType -in @("Boot","System","Unknown")) {
                        Write-Info "Service $($e.name) has start type '$($e.originalStartType)' — kernel driver, no restore needed."
                        $restoreOk++
                        continue
                    } else {
                        # Verify the service still exists before attempting restore — if it was
                        # uninstalled (e.g., Xbox services removed by system update), Set-Service
                        # with -ErrorAction SilentlyContinue silently fails and we'd report success.
                        $svcExists = Get-Service -Name $e.name -ErrorAction SilentlyContinue
                        if (-not $svcExists) {
                            Write-Warn "Service '$($e.name)' no longer exists — cannot restore."
                            $restoreFail++
                            continue
                        }
                        Set-Service -Name $e.name -StartupType $mapped -ErrorAction Stop
                        # Restore DelayedAutoStart flag if it was set (Auto + Delayed = "Automatic (Delayed Start)")
                        if ($e.delayedAutoStart) {
                            $regPath = "HKLM:\SYSTEM\CurrentControlSet\Services\$($e.name)"
                            Set-ItemProperty -Path $regPath -Name "DelayedAutostart" -Value 1 -Type DWord -ErrorAction SilentlyContinue
                        }
                        if ($e.originalStatus -eq "Running") {
                            Start-Service -Name $e.name -ErrorAction SilentlyContinue
                        }
                        $delayTag = if ($e.delayedAutoStart) { " (Delayed)" } else { "" }
                        Write-OK "Restored: $($e.name) -> $($e.originalStartType)$delayTag"
                        $restoreOk++
                    }
                }
                "bootconfig" {
                    # SECURITY: Validate bcdedit key/value from backup.json — a tampered backup
                    # could inject arbitrary bcdedit arguments for code execution or boot corruption.
                    if ($e.key -notmatch '^[a-zA-Z][a-zA-Z0-9_]*$') {
                        Write-Warn "bcdedit restore: invalid key format '$($e.key)' — skipping (security)"
                        $restoreFail++
                        continue
                    }
                    if ($e.existed) {
                        if ($e.originalValue -notmatch '^[a-zA-Z0-9_.{}\-]+$') {
                            Write-Warn "bcdedit restore: invalid value format '$($e.originalValue)' — skipping (security)"
                            $restoreFail++
                            continue
                        }
                        $bcdOut = Invoke-BootConfigRestoreCommand -Arguments @('/set', $e.key, $e.originalValue)
                        if ($LASTEXITCODE -ne 0) { Write-Warn "bcdedit restore failed for $($e.key): $bcdOut"; $restoreFail++ }
                        else { Write-OK "Restored: bcdedit $($e.key) = $($e.originalValue)"; $restoreOk++ }
                    } else {
                        $bcdOut = Invoke-BootConfigRestoreCommand -Arguments @('/deletevalue', $e.key)
                        if ($LASTEXITCODE -ne 0) { Write-Warn "bcdedit deletevalue failed for $($e.key): $bcdOut"; $restoreFail++ }
                        else { Write-OK "Removed: bcdedit $($e.key)"; $restoreOk++ }
                    }
                }
                "powerplan" {
                    # SECURITY: Validate GUID before passing to powercfg — backup.json is in
                    # C:\CS2_OPTIMIZE\ and could be tampered to inject arbitrary powercfg args.
                    if ($e.originalGuid -notmatch '^[a-fA-F0-9\-]{36}$') {
                        Write-Warn "Power plan restore: invalid GUID format '$($e.originalGuid)' — skipping (security)"
                        $restoreFail++
                        continue
                    }
                    # Restore original power plan and delete the imported one
                    powercfg /setactive $e.originalGuid 2>&1 | Out-Null
                    if ($LASTEXITCODE -ne 0) {
                        Write-Warn "Failed to restore power plan '$($e.originalName)' ($($e.originalGuid)) — plan may no longer exist."
                        $restoreFail++
                        continue
                    }
                    Write-OK "Restored power plan: $($e.originalName) ($($e.originalGuid))"
                    # Delete any FPSHeaven/CS2 Optimized plans we created
                    $allPlans = powercfg /list 2>&1
                    foreach ($line in $allPlans) {
                        if ($line -match "([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})") {
                            $planGuid = $Matches[1]
                            if (($line -imatch "FPSHeaven" -or $line -imatch "CS2 Optimized") -and $planGuid -ne $e.originalGuid) {
                                powercfg /delete $planGuid 2>&1 | Out-Null
                                if ($LASTEXITCODE -eq 0) {
                                    Write-OK "Deleted imported plan: $planGuid"
                                } else {
                                    Write-Warn "Could not delete imported plan: $planGuid"
                                }
                            }
                        }
                    }
                    $restoreOk++
                }
                "drs" {
                    $drsResult = Restore-DrsSettings -Entry $e
                    if ($drsResult -eq $false) { $restoreFail++ } else { $restoreOk++ }
                }
                "scheduledtask" {
                    $taskRestoreFailed = $false
                    if (-not $e.existed) {
                        # Task didn't exist before we created it — remove it entirely
                        try {
                            $task = Get-ScheduledTask -TaskName $e.taskName -ErrorAction SilentlyContinue
                            if ($task) {
                                # Stop the task first if it's running to avoid Unregister failure
                                if ($task.State -eq "Running") {
                                    Stop-ScheduledTask -TaskName $e.taskName -ErrorAction SilentlyContinue
                                }
                                Unregister-ScheduledTask -TaskName $e.taskName -Confirm:$false
                                Write-OK "Removed scheduled task: $($e.taskName)"
                            }
                        } catch {
                            Write-Warn "Could not remove scheduled task $($e.taskName): $_"
                            $taskRestoreFailed = $true
                        }
                        if ($e.scriptPath) {
                            if (-not (Test-TrustedSuiteScriptPath -Path $e.scriptPath)) {
                                Write-Warn "Scheduled task restore: refusing to delete untrusted scriptPath '$($e.scriptPath)'"
                                $taskRestoreFailed = $true
                            } elseif (Test-Path $e.scriptPath) {
                                Remove-Item $e.scriptPath -Force -ErrorAction SilentlyContinue
                                Write-OK "Removed: $($e.scriptPath)"
                            }
                        }
                    } else {
                        # Task existed before — restore its enabled/disabled state
                        # Use wasEnabled field (added in batch buffer update) to avoid
                        # blindly re-enabling tasks that were already disabled before optimization.
                        $shouldBeEnabled = if ($e.PSObject.Properties['wasEnabled'] -and $null -ne $e.wasEnabled) { $e.wasEnabled } else { $true }
                        try {
                            $task = Get-ScheduledTask -TaskName $e.taskName -ErrorAction SilentlyContinue
                            if (-not $task) {
                                Write-Warn "Scheduled task '$($e.taskName)' no longer exists — cannot restore."
                                $taskRestoreFailed = $true
                            } elseif ($shouldBeEnabled -and $task.State -eq "Disabled") {
                                Enable-ScheduledTask -TaskName $e.taskName -ErrorAction Stop | Out-Null
                                Write-OK "Re-enabled scheduled task: $($e.taskName)"
                            } elseif (-not $shouldBeEnabled -and $task.State -ne "Disabled") {
                                Disable-ScheduledTask -TaskName $e.taskName -ErrorAction Stop | Out-Null
                                Write-OK "Re-disabled scheduled task: $($e.taskName) (was disabled before optimization)"
                            } else {
                                Write-Info "Scheduled task '$($e.taskName)' already in correct state — kept."
                            }
                        } catch {
                            Write-Warn "Could not restore task $($e.taskName): $_"
                            $taskRestoreFailed = $true
                        }
                    }
                    if ($taskRestoreFailed) { $restoreFail++ } else { $restoreOk++ }
                }
                "nic_adapter" {
                    try {
                        # Cross-adapter detection: verify the current adapter matches what was backed up.
                        # If a different NIC now uses the same name, restoring properties could misconfigure it.
                        if ($e.interfaceDescription) {
                            $currentAdapter = Get-NetAdapter -Name $e.adapterName -ErrorAction SilentlyContinue | Select-Object -First 1
                            if ($currentAdapter -and $currentAdapter.InterfaceDescription -ne $e.interfaceDescription) {
                                Write-Warn "NIC restore skipped for '$($e.propertyName)' on '$($e.adapterName)': adapter changed from '$($e.interfaceDescription)' to '$($currentAdapter.InterfaceDescription)'"
                                $restoreFail++
                                continue
                            }
                        }
                        if ($e.propertyType -eq "RegistryKeyword") {
                            Set-NetAdapterAdvancedProperty -Name $e.adapterName `
                                -RegistryKeyword $e.propertyName -RegistryValue $e.originalValue -ErrorAction Stop
                        } else {
                            Set-NetAdapterAdvancedProperty -Name $e.adapterName `
                                -DisplayName $e.propertyName -DisplayValue $e.originalValue -ErrorAction Stop
                        }
                        Write-OK "Restored NIC: $($e.adapterName) $($e.propertyName) = $($e.originalValue)"
                        $restoreOk++
                    } catch {
                        Write-Warn "NIC restore failed for $($e.propertyName) on $($e.adapterName): $_"
                        $restoreFail++
                    }
                }
                "qos_uro" {
                    # Remove QoS policies that were created (only if they still exist)
                    $qosFailed = $false
                    foreach ($policyName in $e.policies) {
                        try {
                            $existingPolicy = Get-NetQosPolicy -Name $policyName -ErrorAction SilentlyContinue
                            if ($existingPolicy) {
                                Remove-NetQosPolicy -Name $policyName -Confirm:$false -ErrorAction Stop
                                Write-OK "Removed QoS policy: $policyName"
                            } else {
                                Write-DebugLog "QoS policy '$policyName' does not exist — nothing to remove"
                            }
                        } catch {
                            Write-Warn "Could not remove QoS policy '$policyName': $_"
                            $qosFailed = $true
                        }
                    }
                    # Restore URO state
                    if ($e.uroState -and $e.uroState -ne "n/a") {
                        try {
                            $uroOut = netsh int udp set global uro=$($e.uroState) 2>&1
                            if ($LASTEXITCODE -eq 0) {
                                Write-OK "Restored URO state: $($e.uroState)"
                            } else {
                                Write-DebugLog "URO restore: netsh returned error — $uroOut"
                                $qosFailed = $true
                            }
                        } catch { Write-DebugLog "URO restore failed: $_"; $qosFailed = $true }
                    }
                    if ($qosFailed) { $restoreFail++ } else { $restoreOk++ }
                }
                "defender" {
                    try {
                        if ($e.exclusionPaths -and $e.exclusionPaths.Count -gt 0) {
                            Remove-MpPreference -ExclusionPath $e.exclusionPaths -ErrorAction Stop
                            Write-OK "Removed $($e.exclusionPaths.Count) Defender exclusion path(s)"
                        }
                        if ($e.exclusionProcesses -and $e.exclusionProcesses.Count -gt 0) {
                            Remove-MpPreference -ExclusionProcess $e.exclusionProcesses -ErrorAction Stop
                            Write-OK "Removed $($e.exclusionProcesses.Count) Defender exclusion process(es)"
                        }
                        $restoreOk++
                    } catch {
                        Write-Warn "Defender exclusion restore failed: $_"
                        $restoreFail++
                    }
                }
                "pagefile" {
                    try {
                        $pagefileResult = Invoke-PagefileRestoreAutomation -Entry $e
                        Write-OK "Pagefile restore: automated restore completed ($($pagefileResult.Detail))"
                        Write-Info "Pagefile restore note: a reboot is required for the change to take effect."
                        $restoreOk++
                    } catch {
                        Write-Warn "Pagefile restore: automated restore failed — falling back to manual instructions. $_"
                        Write-Info "Pagefile restore: original config was AutoManaged=$($e.automaticManaged), InitialSize=$($e.initialSize)MB, MaxSize=$($e.maximumSize)MB"
                        Write-Info "Manual restore: System Properties -> Advanced -> Performance -> Virtual Memory"
                        if ($e.automaticManaged) {
                            Write-Info "  Set 'Automatically manage paging file size for all drives' = checked"
                        } else {
                            Write-Info "  Set custom size: Initial=$($e.initialSize)MB, Maximum=$($e.maximumSize)MB on $($e.pagefilePath)"
                        }
                        Write-Info "Pagefile restore note: a reboot is required for the change to take effect."
                        Write-Warn "Pagefile restore recorded as partial success — manual completion still required."
                        $restorePartial++
                    }
                }
                "dns" {
                    try {
                        # Resolve the current InterfaceIndex — the stored index may be stale
                        # if the adapter was re-plugged or the system was rebooted.
                        $ifIndex = $e.interfaceIndex
                        # Validate InterfaceIndex non-destructively by checking adapter name
                        try {
                            $currentAdapter = Get-NetAdapter -Name $e.adapterName -ErrorAction SilentlyContinue |
                                Select-Object -First 1
                            if ($currentAdapter -and $currentAdapter.InterfaceIndex -ne $ifIndex) {
                                Write-DebugLog "DNS restore: InterfaceIndex changed from $ifIndex to $($currentAdapter.InterfaceIndex)"
                                $ifIndex = $currentAdapter.InterfaceIndex
                            } elseif (-not $currentAdapter) {
                                Write-DebugLog "DNS restore: adapter '$($e.adapterName)' not found, using stored InterfaceIndex $ifIndex"
                            }
                        } catch {
                            Write-DebugLog "DNS restore: adapter lookup failed, using stored InterfaceIndex $ifIndex"
                        }
                        if ($e.originalDnsServers -and $e.originalDnsServers.Count -gt 0) {
                            Set-DnsClientServerAddress -InterfaceIndex $ifIndex `
                                -ServerAddresses $e.originalDnsServers -ErrorAction Stop
                            Write-OK "Restored DNS on $($e.adapterName) (ifIndex $ifIndex): $($e.originalDnsServers -join ', ')"
                        } else {
                            Set-DnsClientServerAddress -InterfaceIndex $ifIndex `
                                -ResetServerAddresses -ErrorAction Stop
                            Write-OK "Restored DNS on $($e.adapterName) (ifIndex $ifIndex): reset to automatic (DHCP)"
                        }
                        $restoreOk++
                    } catch {
                        Write-Warn "DNS restore failed for $($e.adapterName): $_"
                        $restoreFail++
                    }
                }
                default {
                    Write-Warn "Unknown backup type '$($e.type)' — cannot restore (skipping)"
                    $restoreFail++
                }
            }
        } catch {
            $restoreFail++
            $entryLabel = if ($e.PSObject.Properties['name'] -and $e.name) {
                $e.name
            } elseif ($e.PSObject.Properties['profile'] -and $e.profile) {
                $e.profile
            } elseif ($e.PSObject.Properties['originalName'] -and $e.originalName) {
                $e.originalName
            } elseif ($e.PSObject.Properties['taskName'] -and $e.taskName) {
                $e.taskName
            } else {
                $e.type
            }
            Write-Warn ("Restore failed for {0} {1}: {2}" -f $e.type, $entryLabel, $_)
        }
        if ($restoreFail -gt $failBefore) { $failedEntries.Add($e) }
        if ($restorePartial -gt $partialBefore) { $partialEntries.Add($e) }
    }

    if ($restoreFail -gt 0) {
        Write-Warn "Restore '$StepTitle': $restoreOk succeeded, $restoreFail failed — check warnings above."
    }
    if ($restorePartial -gt 0) {
        Write-Warn "Restore '$StepTitle': $restorePartial partial/manual step(s) still need completion."
    }

    # Remove successfully restored entries; keep failed and partial/manual ones for retry.
    $retainedEntries = @($failedEntries) + @($partialEntries)
    $backup.entries = @($backup.entries | Where-Object { $_.step -ne $StepTitle -or $_ -in $retainedEntries })
    Save-BackupData $backup
    if ($restoreFail -gt 0) {
        Write-Warn "$restoreFail failed entry/entries retained for '$StepTitle' — retry restore to complete."
    }
    if ($restorePartial -gt 0) {
        Write-Warn "$restorePartial partial entry/entries retained for '$StepTitle' — complete the manual pagefile step, then retry if needed."
    }
    return ($restoreFail -eq 0 -and $restorePartial -eq 0)
}

function Restore-AllChanges {
    # Drain any pending in-memory entries before restore to prevent re-pollution
    Flush-BackupBuffer

    # Acquire backup lock to prevent races with concurrent Flush-BackupBuffer
    if (Test-BackupLock) {
        Write-Warn "Another CS2 Optimization process is currently running (backup.json is locked)."
        Write-Warn "Wait for it to finish, or close it manually before restoring."
        return
    }
    Set-BackupLock

    try {
        $backup = Get-BackupData
        if (-not $backup.entries -or $backup.entries.Count -eq 0) {
            Write-Info "No backups to restore."
            return
        }

        Show-BackupSummary
        Write-Blank
        Write-Warn "This will restore ALL $($backup.entries.Count) backed up setting(s)."
        $r = Read-Host "  Proceed with full restore? [y/N]"
        if ($r -notmatch "^[jJyY]$") { Write-Info "Cancelled."; return }

        $stepNames = @(($backup.entries | Group-Object -Property step).Name)
        $failures = 0
        foreach ($stepName in $stepNames) {
            $result = Restore-StepChanges -StepTitle $stepName
            if (-not $result) { $failures++ }
        }
        if ($failures -eq 0) {
            Write-OK "All settings restored to pre-optimization state."
        } else {
            Write-Warn "$failures step group(s) had restore failures — check output above."
        }
    } finally {
        Remove-BackupLock
    }
}

function Restore-Interactive {
    if (Test-BackupLock) {
        Write-Warn "Another CS2 Optimization process is currently running (backup.json is locked)."
        Write-Warn "Wait for it to finish, or close it manually before restoring."
        return
    }
    Set-BackupLock
    try {
        $backup = Get-BackupData
        if (-not $backup.entries -or $backup.entries.Count -eq 0) {
            Write-Info "No backups to restore."
            return
        }

        Show-BackupSummary
        Write-Blank
        $grouped = $backup.entries | Group-Object -Property step
        Write-Host "  Select step to restore:" -ForegroundColor White
        for ($i = 0; $i -lt $grouped.Count; $i++) {
            Write-Host "  [$($i+1)]  $($grouped[$i].Name)  ($($grouped[$i].Count) change(s))" -ForegroundColor White
        }
        Write-Host "  [A]  Restore ALL" -ForegroundColor Yellow
        Write-Host "  [0]  Cancel" -ForegroundColor DarkGray

        $choice = Read-Host "  Choice"
        if ($choice -eq "0" -or [string]::IsNullOrWhiteSpace($choice)) { return }
        if ($choice -match "^[aA]$") {
            $stepNames = @(($backup.entries | Group-Object -Property step).Name)
            $failures = 0
            $skippedSteps = [System.Collections.Generic.List[string]]::new()
            foreach ($stepName in $stepNames) {
                Write-Host "" 
                Write-Host "  [$stepName]" -ForegroundColor Cyan
                Write-Host "  [R]  Restore and continue" -ForegroundColor White
                Write-Host "  [S]  Skip this step" -ForegroundColor Yellow
                Write-Host "  [A]  Abort interactive restore" -ForegroundColor DarkGray
                do { $stepAction = Read-Host "  [R/S/A]" } while ($stepAction -notmatch "^[rRsSaA]$")
                if ($stepAction -match "^[aA]$") {
                    Write-Warn "Interactive restore aborted — remaining entries left in backup.json."
                    return
                }
                if ($stepAction -match "^[sS]$") {
                    Write-Info "Skipped step '$stepName' — entry remains in backup.json."
                    $skippedSteps.Add($stepName) | Out-Null
                    continue
                }
                $result = Restore-StepChanges -StepTitle $stepName
                if (-not $result) { $failures++ }
            }
            if ($failures -eq 0 -and $skippedSteps.Count -eq 0) { Write-OK "All settings restored to pre-optimization state." }
            elseif ($failures -eq 0) { Write-Warn "Restore completed with $($skippedSteps.Count) skipped step group(s): $(@($skippedSteps) -join ', ')." }
            else { Write-Warn "$failures step group(s) had restore failures — check output above." }
            return
        }

        $idx = 0
        if ([int]::TryParse($choice, [ref]$idx) -and $idx -ge 1 -and $idx -le $grouped.Count) {
            Restore-StepChanges -StepTitle $grouped[$idx-1].Name
        } else { Write-Warn "Invalid selection." }
    } finally {
        Remove-BackupLock
    }
}
