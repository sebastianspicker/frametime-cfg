# ==============================================================================
#  helpers/backup-restore.ps1  -  Setting Backup & Restore System
# ==============================================================================
#
#  Automatically captures registry, service, and boot config state BEFORE
#  modifications. Enables per-step or full rollback if something goes wrong.
#
#  Integration:
#    Set-RegistryValue / Set-BootConfig auto-backup via $SCRIPT:CurrentStepTitle
#    Backup-DrsSettings / Restore-DrsSettings for NVIDIA DRS profile settings
#    Manual: Backup-ServiceState, Restore-StepChanges, Restore-Interactive

$CFG_BackupFile = "$CFG_WorkDir\backup.json"
$CFG_BackupLockFile = "$CFG_WorkDir\backup.lock"
# Snapshot the configured Step 14 names into the same script scope as the
# restore functions. Pester and GUI runspaces can introduce child scopes, so
# looking up CFG_Autostart_Remove later is not reliable.
$SCRIPT:CFG_AutostartRestoreAllowlist = @($CFG_Autostart_Remove)
$SCRIPT:CFG_SuiteQosPolicyNames = @('CS2_UDP_Ports', 'CS2_App')
$SCRIPT:CFG_SuiteUroStates = @('enabled', 'disabled')
# Sentinel used when DRS profile was found via app registration rather than by name.
# Must match between Backup-DrsSettings (write) and Restore-DrsSettings (read).
$SCRIPT:DRS_FOUND_VIA_APP = "(found via cs2.exe)"
if (-not (Get-Variable _backupLockToken -Scope Script -ErrorAction SilentlyContinue)) {
    $SCRIPT:_backupLockToken = $null
}
if (-not (Get-Variable _backupLockStream -Scope Script -ErrorAction SilentlyContinue)) {
    $SCRIPT:_backupLockStream = $null
}

# ── In-memory batch buffer ─────────────────────────────────────────────────
# Backup entries use an in-memory buffer. Safety-critical callers flush and
# verify the relevant record before mutation. Other callers are flushed by
# Invoke-TieredStep after the action and by Get-BackupData before reads.
#
# DRY-RUN guard pattern:
#   Every Backup-* function owns its own `if ($SCRIPT:DryRun) { return }` guard
#   as the first statement. Callers should invoke backup capture unconditionally
#   when they have enough context; the backup function itself decides whether the
#   current mode allows persisting an entry.
$SCRIPT:_backupPending = [System.Collections.Generic.List[object]]::new()

function New-BackupDataObject {
    param()

    return [PSCustomObject]@{
        entries = @()
        created = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
}

function New-BackupFile {
    [CmdletBinding(SupportsShouldProcess)]
    param()

    if ($PSCmdlet.ShouldProcess($CFG_BackupFile, "Create backup file")) {
        Save-JsonAtomic -Data (New-BackupDataObject) -Path $CFG_BackupFile
        Set-SecureAcl -Path $CFG_BackupFile -Required
    }
}

function Initialize-Backup {
    $dryRunActive = (Get-Variable DryRun -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:DryRun
    if ($dryRunActive) {
        Write-DebugLog "DRY-RUN: backup initialization skipped."
        return
    }

    # Acquire lock before creating/repairing the active backup file.
    if (Test-BackupLock) {
        Write-Warn "Another frametime.cfg window appears to be open already."
        Write-ConsoleLine "  $([char]0x2139) What to do: Close the other window first, then try again." -ForegroundColor Cyan
        Write-ConsoleLine "    If no other window is open, this will clear itself automatically." -ForegroundColor DarkGray
        throw "Backup lock is already held by another active frametime.cfg process."
    }
    try {
        Set-BackupLock | Out-Null
    } catch {
        Write-Warn "Another frametime.cfg window acquired the backup lock first."
        Write-ConsoleLine "  $([char]0x2139) What to do: Close the other window first, then try again." -ForegroundColor Cyan
        throw "Backup lock is already held by another active frametime.cfg process."
    }

    try {
        if (-not (Test-Path $CFG_BackupFile)) {
            New-BackupFile
        } else {
            # Existing backup bytes are authority for rollback. Never repair an
            # untrusted legacy file and then treat its contents as trustworthy.
            [void](Assert-TrustedExistingControlFile -Path $CFG_BackupFile)
        }
    } catch {
        # Initialization owns the lock at this point.  Do not strand it when
        # backup creation or ACL hardening fails, otherwise every retry is
        # misreported as a competing live process.
        Remove-BackupLock
        throw
    }
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
    $nativeLockPath = $CFG_BackupLockFile -replace '\\', [System.IO.Path]::DirectorySeparatorChar
    if (-not (Test-Path -LiteralPath $CFG_BackupLockFile)) { return $false }
    if ($SCRIPT:_backupLockToken -and $SCRIPT:_backupLockStream) { return $true }

    try {
        # A live owner keeps the file open without write sharing.  Acquiring an
        # exclusive read/write handle therefore proves that no owner currently
        # holds the lock before any stale-file cleanup begins.
        try {
            $probe = [System.IO.File]::Open(
                $nativeLockPath,
                [System.IO.FileMode]::Open,
                [System.IO.FileAccess]::ReadWrite,
                [System.IO.FileShare]::None
            )
        } catch [System.IO.IOException] {
            return $true
        }

        try {
            $probe.Position = 0
            $reader = New-Object System.IO.StreamReader($probe, [System.Text.Encoding]::UTF8, $true, 1024, $true)
            $isStale = $false
            try {
                $lockData = $reader.ReadToEnd() | ConvertFrom-Json -ErrorAction Stop
            } catch {
                $isStale = $true
                Write-DebugLog "Backup lock is corrupt - claiming it for safe stale cleanup."
            } finally {
                $reader.Dispose()
            }

            # Auto-expire legacy locks that are not protected by a live owner
            # handle. No optimization run should take more than four hours.
            if (-not $isStale -and $lockData.started) {
                try {
                    $parsedDate = [datetime]::Parse([string]$lockData.started)
                    $lockAge = (Get-Date) - $parsedDate
                    if ($lockAge.TotalHours -gt 4) {
                        $isStale = $true
                        Write-DebugLog "Found expired backup lock (age: $([math]::Round($lockAge.TotalHours, 1))h, PID $($lockData.pid))."
                    }
                } catch {
                    $isStale = $true
                    Write-DebugLog "Backup lock has unparseable timestamp '$($lockData.started)' - marking stale."
                }
            }

            $proc = $null
            $lockPid = 0
            if (-not $isStale -and [int]::TryParse([string]$lockData.pid, [ref]$lockPid)) {
                $proc = Get-Process -Id $lockPid -ErrorAction SilentlyContinue
            }
            if (-not $isStale -and $proc) {
                # Mitigate PID reuse: verify the executable and, for new-format
                # locks, the process start instant as well as the recycled PID.
                $isPowerShell = $proc.ProcessName -match '^(?:powershell|pwsh|powershell_ise)$'
                if ($isPowerShell) {
                    if (-not $lockData.processStartUtc) { return $true }
                    try {
                        $actualStart = $proc.StartTime.ToUniversalTime().ToString('o')
                        $expectedStart = ([datetime]::Parse([string]$lockData.processStartUtc)).ToUniversalTime().ToString('o')
                        if ($actualStart -eq $expectedStart) { return $true }
                    } catch { return $true }
                }
                Write-DebugLog "Found stale backup lock (PID $lockPid reused by '$($proc.ProcessName)')."
                $isStale = $true
            } elseif (-not $isStale) {
                $isStale = $true
                Write-DebugLog "Found stale backup lock (PID $($lockData.pid) no longer running)."
            }

            if (-not $isStale) { return $true }

            # Claim the stale path while the exclusive handle is held.  Other
            # cleaners see the cleanup token as busy, and CreateNew contenders
            # cannot replace the path until this exact claimed file is removed.
            $cleanupToken = [guid]::NewGuid().ToString('N')
            $cleanupData = @{
                pid = $PID
                started = (Get-Date).ToUniversalTime().ToString('o')
                processStartUtc = (Get-Process -Id $PID).StartTime.ToUniversalTime().ToString('o')
                token = $cleanupToken
                state = 'cleanup'
            } | ConvertTo-Json -Compress
            $bytes = [System.Text.Encoding]::UTF8.GetBytes($cleanupData)
            $probe.SetLength(0)
            $probe.Write($bytes, 0, $bytes.Length)
            $probe.Flush($true)
        } finally {
            $probe.Dispose()
        }
        Remove-BackupLock -Token $cleanupToken | Out-Null
        return $false
    } catch {
        # A corrupt legacy lock can only be removed after obtaining the same
        # exclusive handle/claim path above.  If that was not possible, fail
        # closed and report the lock as held.
        Write-DebugLog "Backup lock could not be validated safely: $_"
        return $true
    }
}

function Set-BackupLock {
    <#  Called at the start of optimization and restore operations.  #>
    [CmdletBinding(SupportsShouldProcess)]
    param([switch]$PassThru)

    if (-not $PSCmdlet.ShouldProcess($CFG_BackupLockFile, "Create backup lock")) { return $null }

    $nativeLockPath = $CFG_BackupLockFile -replace '\\', [System.IO.Path]::DirectorySeparatorChar
    $token = [guid]::NewGuid().ToString('N')
    $processStartUtc = (Get-Process -Id $PID -ErrorAction Stop).StartTime.ToUniversalTime().ToString('o')
    $lockData = @{
        pid = $PID
        started = (Get-Date).ToUniversalTime().ToString('o')
        processStartUtc = $processStartUtc
        token = $token
        state = 'owned'
    } | ConvertTo-Json -Compress
    $stream = $null
    try {
        # FileMode.CreateNew is the acquisition primitive: exactly one
        # contender succeeds and an existing lock is never overwritten.
        $stream = [System.IO.File]::Open(
            $nativeLockPath,
            [System.IO.FileMode]::CreateNew,
            [System.IO.FileAccess]::ReadWrite,
            [System.IO.FileShare]::Read
        )
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($lockData)
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
        Set-SecureAcl -Path $CFG_BackupLockFile -Required
        $SCRIPT:_backupLockToken = $token
        $SCRIPT:_backupLockStream = $stream
        if ($PassThru) { return $token }
    } catch {
        if ($stream) {
            $stream.Dispose()
            Remove-Item -LiteralPath $CFG_BackupLockFile -Force -ErrorAction SilentlyContinue
        }
        throw
    }
}

function Remove-BackupLock {
    [CmdletBinding(SupportsShouldProcess)]
    param([string]$Token)

    $nativeLockPath = $CFG_BackupLockFile -replace '\\', [System.IO.Path]::DirectorySeparatorChar
    $ownerToken = if ($Token) { $Token } else { $SCRIPT:_backupLockToken }
    if (-not $ownerToken) { return }
    if (-not $PSCmdlet.ShouldProcess($CFG_BackupLockFile, "Remove owned backup lock")) { return }

    if (-not $Token -and $SCRIPT:_backupLockStream) {
        $SCRIPT:_backupLockStream.Dispose()
        $SCRIPT:_backupLockStream = $null
    }
    try {
        $stream = [System.IO.File]::Open(
            $nativeLockPath,
            [System.IO.FileMode]::Open,
            [System.IO.FileAccess]::ReadWrite,
            [System.IO.FileShare]::None
        )
        try {
            $reader = New-Object System.IO.StreamReader($stream, [System.Text.Encoding]::UTF8, $true, 1024, $true)
            try { $lockData = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
            if ($lockData.token -ne $ownerToken) { return }
            $stream.SetLength(0)
            $releaseData = @{ token = $ownerToken; state = 'releasing' } | ConvertTo-Json -Compress
            $bytes = [System.Text.Encoding]::UTF8.GetBytes($releaseData)
            $stream.Write($bytes, 0, $bytes.Length)
            $stream.Flush($true)
        } finally {
            $stream.Dispose()
        }
        Remove-Item -LiteralPath $CFG_BackupLockFile -Force -ErrorAction Stop
        if (-not $Token) { $SCRIPT:_backupLockToken = $null }
    } catch [System.IO.FileNotFoundException] {
        if (-not $Token) { $SCRIPT:_backupLockToken = $null }
    } catch [System.IO.IOException] {
        return
    }
}

function Flush-BackupBuffer {
    <#  Writes any pending in-memory backup entries to backup.json in a single I/O pass.
        Safe to call multiple times - no-op when the buffer is empty.
        On failure: entries stay in memory (Clear runs AFTER Save) for the next flush attempt.
        Callers that require crash-safe rollback must flush before mutation.  #>
    $dryRunActive = (Get-Variable DryRun -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:DryRun
    if ($dryRunActive -or $SCRIPT:_backupPending.Count -eq 0) { return }
    $backup = Get-BackupDataRaw
    $entries = [System.Collections.ArrayList]@($backup.entries)
    foreach ($e in $SCRIPT:_backupPending) {
        # Deduplicate: skip if an entry for the same key already exists (prevents
        # duplicate backups on re-run - the first backup holds the true original value)
        $isDupe = $false
        foreach ($existing in $entries) {
            if ($existing.step -eq $e.step -and $existing.type -eq $e.type) {
                switch ($e.type) {
                    "registry"      { $isDupe = ($existing.PSObject.Properties['path'] -and $e.Contains('path') -and $existing.path -eq $e.path -and $existing.name -eq $e.name) }
                    "service"       { $isDupe = ($existing.name -eq $e.name) }
                    "scheduledtask" { $isDupe = ($existing.PSObject.Properties['taskName'] -and $e.Contains('taskName') -and $existing.taskName -eq $e.taskName -and (Get-BackupTaskPath $existing) -eq (Get-BackupTaskPath $e)) }
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
        Internal use only - callers outside this module should use Get-BackupData.  #>
    if (-not (Test-Path $CFG_BackupFile)) { Initialize-Backup }
    # Keep the validation immediately adjacent to the authority read. Do not
    # let the corruption recovery path copy or replace an untrusted object.
    [void](Assert-TrustedExistingControlFile -Path $CFG_BackupFile)
    try {
        $raw = Get-Content $CFG_BackupFile -Raw -ErrorAction Stop | ConvertFrom-Json
        if ($null -eq $raw.entries) { $raw | Add-Member -NotePropertyName "entries" -NotePropertyValue @() -Force }
        # Force entries to array - PS 5.1 ConvertFrom-Json unwraps single-element arrays to scalars
        $raw.entries = @($raw.entries)
        return $raw
    } catch {
        # Preserve corrupted file for recovery before overwriting
        $ts = (Get-Date).ToString("yyyyMMdd-HHmmss")
        $backupDir = Split-Path $CFG_BackupFile -Parent
        $backupName = Split-Path $CFG_BackupFile -Leaf
        $backupStem = if ($backupName -match '^(.*)\.json$') { $Matches[1] } else { $backupName }
        $corruptPath = Join-Path $backupDir "$backupStem.corrupt.$ts.json"
        try {
            Copy-Item $CFG_BackupFile $corruptPath -Force -ErrorAction Stop
            if (-not (Test-Path -LiteralPath $corruptPath)) { throw "Preserved copy was not created." }
            if ((Get-Item -LiteralPath $CFG_BackupFile).Length -ne (Get-Item -LiteralPath $corruptPath).Length) {
                throw "Preserved copy length does not match the original."
            }
            $sourceHash = (Get-FileHash -LiteralPath $CFG_BackupFile -Algorithm SHA256 -ErrorAction Stop).Hash
            $preservedHash = (Get-FileHash -LiteralPath $corruptPath -Algorithm SHA256 -ErrorAction Stop).Hash
            if ($sourceHash -ne $preservedHash) { throw "Preserved copy hash does not match the original." }
        } catch {
            Write-Warn "backup.json is corrupted, but it could not be preserved safely. The original was left untouched."
            throw "Refusing to reset corrupted backup because preservation failed: $_"
        }
        Write-Warn "backup.json was corrupted - saved copy to $corruptPath before resetting."
        Write-Warn "Backup history reset - previous entries preserved in $corruptPath"
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
    Set-SecureAcl -Path $CFG_BackupFile -Required
}

function Get-BackupTaskPath($Entry) {
    if ($Entry -is [System.Collections.IDictionary] -and $Entry.Contains('taskPath') -and $Entry['taskPath']) { return [string]$Entry['taskPath'] }
    if ($Entry.PSObject.Properties['taskPath'] -and $Entry.taskPath) { return [string]$Entry.taskPath }
    return $null
}

function Test-ScheduledTaskBackupIdentity {
    [CmdletBinding()]
    param($Entry)

    $taskName = [string]$Entry.taskName
    $taskPath = Get-BackupTaskPath $Entry
    if ([string]::IsNullOrWhiteSpace($taskName) -or [string]::IsNullOrWhiteSpace($taskPath)) { return $false }
    if ($taskName -match '[\\/\*\?\[\]\x00]' -or $taskPath -match '[\*\?\[\]\x00]') { return $false }
    if (-not $taskPath.StartsWith("\")) { return $false }
    if (-not $taskPath.EndsWith("\")) { return $false }
    return $true
}

function Test-ScheduledTaskRestoreAllowed {
    [CmdletBinding()]
    param($Entry)

    if (-not (Test-ScheduledTaskBackupIdentity -Entry $Entry)) { return $false }
    $taskName = [string]$Entry.taskName
    $taskPath = Get-BackupTaskPath $Entry

    if ($taskPath -eq "\" -and $taskName -in @("frametime_cfg_cs2_affinity", "CS2_Optimize_CCD_Affinity")) { return $true }

    $allowedMicrosoftTaskPaths = @(
        "\Microsoft\Windows\Application Experience\",
        "\Microsoft\Windows\Customer Experience Improvement Program\"
    )
    foreach ($allowedPath in $allowedMicrosoftTaskPaths) {
        if ($taskPath.Equals($allowedPath, [System.StringComparison]::OrdinalIgnoreCase)) { return $true }
    }

    $allowedNvidiaTaskPatterns = @(
        "NvDriverUpdateCheckDaily*",
        "NVIDIA GeForce*",
        "NvNodeLauncher*",
        "NvBackend*",
        "NvTmRep*",
        "NvProfileUpdater*",
        "NvTelemetry*"
    )
    if ($taskPath.Equals("\", [System.StringComparison]::OrdinalIgnoreCase) -or
        $taskPath.Equals("\NVIDIA\", [System.StringComparison]::OrdinalIgnoreCase)) {
        foreach ($pattern in $allowedNvidiaTaskPatterns) {
            if ($taskName -like $pattern) { return $true }
        }
    }

    return $false
}

function Test-ServiceRestoreAllowed {
    [CmdletBinding()]
    param([AllowEmptyString()][string]$ServiceName)

    if ([string]::IsNullOrWhiteSpace($ServiceName)) { return $false }
    $allowedPatterns = @(
        "NVDisplay.ContainerLocalSystem",
        "NvTelemetryContainer",
        "NvContainerNetworkService",
        "NvContainerLocalSystem",
        "NVDisplay*",
        "nvsvc",
        "AMD External Events Utility",
        "amdlog",
        "amdfendr*",
        "igfxCUIService*",
        "IntelGraphicsControlPanel*",
        "DiagTrack",
        "dmwappushservice",
        "SysMain",
        "WSearch",
        "qWave",
        "XblAuthManager",
        "XblGameSave",
        "XboxNetApiSvc",
        "XboxGipSvc",
        "wuauserv",
        "UsoSvc",
        "WaaSMedicSvc"
    )
    foreach ($pattern in $allowedPatterns) {
        if ($ServiceName -like $pattern) { return $true }
    }
    return $false
}

function Test-BootConfigRestoreAllowed {
    [CmdletBinding()]
    param(
        [AllowEmptyString()][string]$Key,
        [AllowEmptyString()][string]$Value,
        [bool]$Existed
    )

    if ([string]::IsNullOrWhiteSpace($Key)) { return $false }
    $normalizedKey = $Key.ToLowerInvariant()
    if ($normalizedKey -notin @("safeboot", "disabledynamictick", "useplatformtick", "useplatformclock")) {
        return $false
    }
    if (-not $Existed) { return $true }
    if ([string]::IsNullOrWhiteSpace($Value)) { return $false }

    $normalizedValue = $Value.ToLowerInvariant()
    switch ($normalizedKey) {
        "safeboot" { return ($normalizedValue -in @("minimal", "network", "alternateshell")) }
        "disabledynamictick" { return ($normalizedValue -in @("yes", "no", "true", "false", "on", "off", "0", "1")) }
        "useplatformtick" { return ($normalizedValue -in @("yes", "no", "true", "false", "on", "off", "0", "1")) }
        "useplatformclock" { return ($normalizedValue -in @("yes", "no", "true", "false", "on", "off", "0", "1")) }
        default { return $false }
    }
}

function Test-RegistryRestoreAllowed {
    <#  Treat backup.json as untrusted restore input.
        This allowlist is intentionally narrower than Set-RegistryValue's write
        validation because restore runs later from persisted JSON that could be
        edited outside the suite.  #>
    [CmdletBinding()]
    param(
        [AllowEmptyString()][string]$Path,
        [AllowEmptyString()][string]$Name
    )

    if ([string]::IsNullOrWhiteSpace($Path) -or [string]::IsNullOrWhiteSpace($Name)) { return $false }
    if (-not (Test-RegistryValueNameAllowed -Path $Path -Name $Name)) { return $false }
    $normalized = $Path -replace '/', '\'
    $normalized = $normalized -replace '^Microsoft\.PowerShell\.Core\\Registry::HKEY_LOCAL_MACHINE\\', 'HKLM:\'
    $normalized = $normalized -replace '^Microsoft\.PowerShell\.Core\\Registry::HKEY_CURRENT_USER\\', 'HKCU:\'
    $normalized = $normalized -replace '^Microsoft\.PowerShell\.Core\\Registry::HKEY_CLASSES_ROOT\\', 'HKCR:\'
    $normalized = $normalized -replace '^Microsoft\.PowerShell\.Core\\Registry::HKEY_USERS\\', 'HKU:\'
    $normalized = $normalized -replace '^Microsoft\.PowerShell\.Core\\Registry::HKEY_CURRENT_CONFIG\\', 'HKCC:\'
    $normalized = $normalized -replace '^Microsoft\.PowerShell\.Core\\Registry::HKLM\\', 'HKLM:\'
    $normalized = $normalized -replace '^Microsoft\.PowerShell\.Core\\Registry::HKCU\\', 'HKCU:\'
    $normalized = $normalized -replace '^Microsoft\.PowerShell\.Core\\Registry::HKCR\\', 'HKCR:\'
    $normalized = $normalized -replace '^Microsoft\.PowerShell\.Core\\Registry::HKU\\', 'HKU:\'
    $normalized = $normalized -replace '^Microsoft\.PowerShell\.Core\\Registry::HKCC\\', 'HKCC:\'
    if ($normalized -notmatch '^(HKLM:|HKCU:|HKCR:|HKU:|HKCC:)\\') { return $false }

    # Step 14 removes only the configured value names from these two startup
    # keys. Keep the exception exact because backup.json is untrusted input.
    $runPaths = @(
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run',
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run'
    )
    if ($normalized -in $runPaths) {
        return ($Name -in $SCRIPT:CFG_AutostartRestoreAllowlist)
    }
    if ($normalized -match '\\CurrentVersion\\Run(Once|Services|ServicesOnce)?(\\|$)') { return $false }

    $allowedExactValues = @{
        'HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity' = @('Enabled')
        'HKLM:\SYSTEM\CurrentControlSet\Control\Power\PowerThrottling' = @('PowerThrottlingOff')
        'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' = @('NtfsDisableLastAccessUpdate', 'NtfsDisable8dot3NameCreation')
        'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' = @('HwSchMode', 'EnableWriteCombining')
        'HKLM:\SYSTEM\CurrentControlSet\Control\PriorityControl' = @('Win32PrioritySeparation')
        'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\kernel' = @('GlobalTimerResolutionRequests')
        'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management' = @('DisablePagingExecutive')
        'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power' = @('HiberbootEnabled')
        'HKLM:\SYSTEM\CurrentControlSet\Services\mouclass\Parameters' = @('MouseDataQueueSize')
        'HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\QoS' = @('Do not use NLA')
        'HKLM:\SOFTWARE\Microsoft\FTH' = @('Enabled')
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Device Installer' = @('DisableCoInstallers')
        'HKLM:\SOFTWARE\Microsoft\Windows\Dwm' = @('OverlayTestMode')
        'HKLM:\SOFTWARE\Policies\Microsoft\Windows\GameDVR' = @('AllowGameDVR')
        'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile' = @('SystemResponsiveness', 'NoLazyMode')
        'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games' = @('Priority', 'Scheduling Category', 'GPU Priority')
        'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\Maintenance' = @('MaintenanceDisabled')
        'HKLM:\SOFTWARE\Policies\Microsoft\Windows\CloudContent' = @('DisableWindowsConsumerFeatures', 'DisableSoftLanding')
        'HKLM:\SOFTWARE\NVIDIA Corporation\NvControlPanel2\Client' = @('OptInOrOutPreference')
        'HKLM:\SOFTWARE\NVIDIA Corporation\Global\FTS' = @('EnableRID44231', 'EnableRID64640', 'EnableRID66610')
        'HKLM:\SOFTWARE\NVIDIA Corporation\Global\NVTweak' = @('Gestalt')
        'HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d' = @(
            'OGL_THREAD_CONTROL_DEFAULT', 'OGL_QUALITY_ENHANCEMENTS_DEFAULT', 'OGL_QUALITY_ENHANCEMENTS',
            'OGL_FXAA_DEF', 'OGL_GAMMA_CORRECT_DEF', 'AA_MODE_SELECTOR', 'AA_LINE_GAMMA',
            'LOD_BIAS_ADJUST', 'PS_TEXFILTER_BILINEAR_QUAL', 'PS_TEXFILTER_ANISO_OPTS2',
            'PS_TEXFILTER_ANISO_OPTS', 'PS_TEXFILTER_LOD_BIAS', 'ANISO_SETTING',
            'ANISO_MODE_SELECTOR', 'MAX_PRERENDERED_FRAMES', 'VSYNC_MODE',
            'PRERENDERLIMIT_OPTION', 'ANSEL_ENABLE', 'FRL_VALUE', 'FRL_LOW_LATENCY',
            'PS_FRAMERATE_LIMITER', 'AFR_CONTROL'
        )
        'HKCU:\Control Panel\Desktop' = @('UserPreferencesMask', 'FontSmoothing')
        'HKCU:\Control Panel\Mouse' = @('MouseSpeed', 'MouseThreshold1', 'MouseThreshold2', 'SmoothMouseXCurve', 'SmoothMouseYCurve')
        'HKCU:\SOFTWARE\Microsoft\DirectX\UserGpuPreferences' = @()
        'HKCU:\SOFTWARE\Microsoft\GameBar' = @('AllowAutoGameMode', 'AutoGameModeEnabled', 'UseNexusForGameBarEnabled')
        'HKCU:\SOFTWARE\Microsoft\Multimedia\Audio' = @('UserDuckingPreference')
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo' = @('Enabled')
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects' = @('VisualFXSetting')
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\GameDVR' = @('AppCaptureEnabled')
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\VideoSettings' = @('AutoHDREnabled')
        'HKCU:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers' = @()
        'HKCU:\Software\Valve\Steam' = @('GameOverlayDisabled')
        'HKCU:\System\GameConfigStore' = @(
            'GameDVR_DXGIHonorFSEWindowsCompatible', 'GameDVR_FSEBehavior',
            'GameDVR_FSEBehaviorMode', 'GameDVR_HonorUserFSEBehaviorMode', 'GameDVR_Enabled'
        )
    }
    if ($allowedExactValues.ContainsKey($normalized)) {
        if ($normalized -in @(
            'HKCU:\SOFTWARE\Microsoft\DirectX\UserGpuPreferences',
            'HKCU:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers'
        )) {
            return (Test-SafeCs2RegistryValuePath -Name $Name)
        }
        return ($Name -in $allowedExactValues[$normalized])
    }

    if ($normalized -match '^HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options\\cs2\.exe\\PerfOptions$') {
        return ($Name -eq 'CpuPriorityClass')
    }
    if ($normalized -match '^HKLM:\\SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces\\\{[0-9a-f-]{36}\}$') {
        return ($Name -in @('TcpNoDelay', 'TcpAckFrequency'))
    }
    if ($normalized -match '^HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Class\\\{4d36e968-e325-11ce-bfc1-08002be10318\}\\\d{4}$') {
        return ($Name -in @('RMHdcpKeyglobZero', 'PerfLevelSrc', 'DisableDynamicPstate'))
    }
    if ($normalized -match '^HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Class\\\{4d36e972-e325-11ce-bfc1-08002be10318\}\\\d{4}$') {
        return ($Name -in @('*RSS', '*RSSProfile', '*RssBaseProcNumber', '*MaxRssProcessors', '*NumRssQueues'))
    }
    $deviceInterruptRoot = '^HKLM:\\SYSTEM\\CurrentControlSet\\Enum\\(?:PCI|ACPI|USB|ROOT)\\[^\\]+(?:\\[^\\]+)+\\Device Parameters\\Interrupt Management'
    if ($normalized -match "$deviceInterruptRoot\\MessageSignaledInterruptProperties$") {
        return ($Name -in @('MSISupported', 'MessageNumberLimit'))
    }
    if ($normalized -match "$deviceInterruptRoot\\Affinity Policy$") {
        return ($Name -in @('DevicePolicy', 'AssignmentSetOverride'))
    }
    return $false
}

function Backup-RegistryValue {
    <#  Records the current value of a registry key before modification.
        Set-RegistryValue flushes and verifies the entry before writing.  #>
    [CmdletBinding()]
    param([string]$Path, [string]$Name, [string]$StepTitle, [switch]$PassThru)
    if ($SCRIPT:DryRun) {
        if ($PassThru) {
            return [PSCustomObject]@{ Captured = $false; Entry = $null; Message = 'Dry-run mode does not persist registry backups.' }
        }
        return
    }
    $existing = $null
    $regType  = $null
    $captureError = $null
    try {
        if (Test-Path -Path $Path -ErrorAction Stop) {
            try {
                $prop = Get-ItemProperty -Path $Path -Name $Name -ErrorAction Stop
                $valueProperty = $prop.PSObject.Properties[$Name]
                if ($null -eq $valueProperty) {
                    throw "Registry provider returned no '$Name' property."
                }
                $existing = $valueProperty.Value
                try {
                    $regType = (Get-Item -Path $Path -ErrorAction Stop).GetValueKind($Name).ToString()
                } catch {
                    # Preserve a usable type when a provider cannot expose the
                    # value kind after the value itself was read successfully.
                    $regType = if ($Path -match '\\Run$' -or $Path -match '\\Run\\') {
                        'String'
                    } elseif ($existing -is [byte[]]) {
                        'Binary'
                    } elseif ($existing -is [string[]]) {
                        'MultiString'
                    } elseif ($existing -is [long] -or $existing -is [uint64]) {
                        'QWord'
                    } elseif ($existing -is [string]) {
                        'String'
                    } else {
                        'DWord'
                    }
                }
            } catch {
                # A missing value is a valid restore state. Confirm absence by
                # enumerating the key. Any inability to read the key fails the
                # capture instead of being recorded as "did not exist".
                try {
                    $key = Get-Item -Path $Path -ErrorAction Stop
                    $valueNames = @($key.GetValueNames())
                    if (@($valueNames | Where-Object { $_.Equals($Name, [System.StringComparison]::OrdinalIgnoreCase) }).Count -gt 0) {
                        throw "Registry value '$Name' exists but could not be read."
                    }
                } catch {
                    $captureError = $_
                }
            }
        }
    } catch {
        $captureError = $_
    }

    if ($captureError) {
        $message = "Backup-RegistryValue: could not capture '$Name' from '$Path': $captureError"
        Write-DebugLog $message
        if ($PassThru) { return [PSCustomObject]@{ Captured = $false; Entry = $null; Message = $message } }
        return
    }

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
    if ($PassThru) { return [PSCustomObject]@{ Captured = $true; Entry = $entry; Message = 'Registry state captured.' } }
}

function Backup-ServiceState {
    <#  Records current service start type, delayed-start flag, and status before modification.
        Entries are buffered in memory and flushed at step boundaries.  #>
    param([string]$ServiceName, [string]$StepTitle, [switch]$PassThru)
    if ($SCRIPT:DryRun) {
        if ($PassThru) {
            return [PSCustomObject]@{ Captured = $false; Entry = $null; Message = 'Dry-run mode does not persist service backups.' }
        }
        return
    }
    try {
        $svc = Get-Service -Name $ServiceName -ErrorAction Stop
        $escapedName = $ServiceName -replace "'", "''"
        $startType = (Get-CimInstance Win32_Service -Filter "Name='$escapedName'" -ErrorAction Stop).StartMode
        # Capture DelayedAutoStart flag - services with "Automatic (Delayed Start)" show StartMode=Auto
        # but have a separate registry flag. Without this, restore loses the "Delayed" qualifier.
        $regPath = "HKLM:\SYSTEM\CurrentControlSet\Services\$ServiceName"
        $serviceValues = Get-ItemProperty -Path $regPath -ErrorAction Stop
        $delayProperty = $serviceValues.PSObject.Properties['DelayedAutostart']
        $delayedStart = ($null -ne $delayProperty -and $delayProperty.Value -eq 1)
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
        if ($PassThru) { return [PSCustomObject]@{ Captured = $true; Entry = $entry; Message = 'Service state captured.' } }
    } catch {
        $message = "Backup-ServiceState: could not capture '$ServiceName': $_"
        Write-DebugLog $message
        if ($PassThru) { return [PSCustomObject]@{ Captured = $false; Entry = $null; Message = $message } }
    }
}

function Backup-PowerPlan {
    <#  Records the currently active power plan GUID before switching.
        Unlike ordinary setting backups, this restore point is flushed and
        verified immediately: the caller must not mutate power plans until the
        original active scheme is durably recoverable.  #>
    param([string]$StepTitle)
    if ($SCRIPT:DryRun) { return }
    $originalGuid = $null
    $originalName = $null
    try {
        $global:LASTEXITCODE = 0
        $activeOutput = powercfg /getactivescheme 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "powercfg /getactivescheme failed with exit code $LASTEXITCODE`: $activeOutput"
        }
        if ($activeOutput -match "([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})") {
            $originalGuid = $Matches[1].ToLowerInvariant()
            if ($activeOutput -match "\((.+)\)\s*$") {
                $originalName = $Matches[1]
            }
        }
    } catch {
        throw "Cannot create a durable power-plan restore point: $_"
    }

    if (-not $originalGuid) {
        throw "Cannot create a durable power-plan restore point: powercfg returned no active scheme GUID."
    }

    $ownedGuids = @(Get-RecordedSuiteOwnedPowerPlanGuids)
    $activeIsOwned = ($originalGuid -in $ownedGuids)
    # Re-run detection uses persisted GUID ownership, never the mutable display
    # name. A foreign same-name plan must still be backed up.
    if ($activeIsOwned) {
        Write-DebugLog "Backup-PowerPlan: active plan '$originalGuid' is suite-owned - verifying the existing rollback target."
    } else {
        $entry = [ordered]@{
            type          = "powerplan"
            originalGuid  = $originalGuid
            originalName  = $originalName
            suiteOwnedGuids = $ownedGuids
            step          = $StepTitle
            timestamp     = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
        }
        $SCRIPT:_backupPending.Add($entry)
        Write-DebugLog "Backup-PowerPlan: saved $originalGuid ($originalName)"
    }

    # This is a deliberate exception to the normal per-step batching policy.
    # A crash after plan activation must never precede persistence of the
    # original scheme. Flush failures propagate and abort the caller's action.
    Flush-BackupBuffer
    $durableBackup = Get-BackupDataRaw
    $durablePowerEntries = @($durableBackup.entries | Where-Object {
        $_.type -eq 'powerplan' -and
        [string]$_.originalGuid -match '^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$'
    })
    $restorePointVerified = if ($activeIsOwned) {
        @($durablePowerEntries | Where-Object {
            $recordedOwned = if ($_.PSObject.Properties['suiteOwnedGuids']) {
                @($_.suiteOwnedGuids | ForEach-Object { ([string]$_).ToLowerInvariant() })
            } else {
                @()
            }
            $originalGuid -in $recordedOwned
        }).Count -gt 0
    } else {
        @($durablePowerEntries | Where-Object {
            ([string]$_.originalGuid).ToLowerInvariant() -eq $originalGuid
        }).Count -gt 0
    }
    if (-not $restorePointVerified) {
        throw "Power-plan restore point verification failed; no power-plan mutation was attempted."
    }
}

function Get-RecordedSuiteOwnedPowerPlanGuids {
    $guids = [System.Collections.Generic.List[string]]::new()
    if (Test-Path -LiteralPath $CFG_StateFile) {
        try {
            [void](Assert-TrustedExistingControlFile -Path $CFG_StateFile)
            $state = Get-Content -LiteralPath $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
            $candidates = @()
            if ($state.PSObject.Properties['suiteOwnedPowerPlanGuids']) {
                $candidates += @($state.suiteOwnedPowerPlanGuids)
            }
            if ($state.PSObject.Properties['suiteOwnedPowerPlanGuid']) {
                $candidates += @($state.suiteOwnedPowerPlanGuid)
            }
            foreach ($candidate in $candidates) {
                if ($candidate -match '^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$') {
                    $normalized = ([string]$candidate).ToLowerInvariant()
                    if (-not $guids.Contains($normalized)) { $guids.Add($normalized) }
                }
            }
        } catch {
            Write-DebugLog "Could not read suite-owned power-plan GUIDs from state.json: $_"
        }
    }
    return @($guids)
}

function Get-PowerPlanGuidPresence {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$Guid)

    $normalizedGuid = $Guid.ToLowerInvariant()
    $listOutput = powercfg /list 2>&1
    $listExitCode = $LASTEXITCODE
    if ($listExitCode -ne 0) {
        return [PSCustomObject]@{
            Verified = $false
            Present = $null
            Message = "powercfg /list failed with exit code $listExitCode`: $listOutput"
        }
    }

    $installedGuids = @($listOutput | ForEach-Object {
        if ([string]$_ -match '(?i)([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})') {
            $Matches[1].ToLowerInvariant()
        }
    })
    return [PSCustomObject]@{
        Verified = $true
        Present = ($normalizedGuid -in $installedGuids)
        Message = "Installed power-plan inventory completed."
    }
}

function Update-PowerPlanBackupOwnership {
    [CmdletBinding(SupportsShouldProcess)]
    param([string[]]$OwnedGuids)

    $validGuids = @($OwnedGuids | Where-Object {
        $_ -match '^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$'
    } | ForEach-Object { $_.ToLowerInvariant() } | Select-Object -Unique)
    if (-not $PSCmdlet.ShouldProcess("power-plan backup ownership metadata", "Persist suite-owned power-plan identities")) { return }

    foreach ($entry in $SCRIPT:_backupPending) {
        $entryType = if ($entry -is [System.Collections.IDictionary]) { $entry['type'] } else { $entry.type }
        if ($entryType -eq 'powerplan') {
            if ($entry -is [System.Collections.IDictionary]) { $entry['suiteOwnedGuids'] = $validGuids }
            else { $entry | Add-Member -NotePropertyName suiteOwnedGuids -NotePropertyValue $validGuids -Force }
        }
    }

    if (-not (Test-Path -LiteralPath $CFG_BackupFile)) { return }
    $backup = Get-BackupDataRaw
    $changed = $false
    foreach ($entry in @($backup.entries)) {
        if ($entry.type -eq 'powerplan') {
            $entry | Add-Member -NotePropertyName suiteOwnedGuids -NotePropertyValue $validGuids -Force
            $changed = $true
        }
    }
    if ($changed) { Save-BackupData $backup }
}

function Set-RecordedSuiteOwnedPowerPlanGuids {
    [CmdletBinding(SupportsShouldProcess)]
    param([string[]]$OwnedGuids)

    if (-not (Test-Path -LiteralPath $CFG_StateFile)) { return }
    if (-not $PSCmdlet.ShouldProcess($CFG_StateFile, "Persist remaining suite-owned power-plan identities")) { return }
    [void](Assert-TrustedExistingControlFile -Path $CFG_StateFile)
    $state = Get-Content -LiteralPath $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    $validGuids = @($OwnedGuids | Where-Object {
        $_ -match '^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$'
    } | ForEach-Object { $_.ToLowerInvariant() } | Select-Object -Unique)
    $state | Add-Member -NotePropertyName suiteOwnedPowerPlanGuids -NotePropertyValue $validGuids -Force
    $state.PSObject.Properties.Remove('suiteOwnedPowerPlanGuid')
    Save-SuiteState -State $state
}

function Backup-BootConfig {
    <#  Records current bcdedit value before modification.
        Uses bcdedit /v to get raw BCD element names (hex IDs), which are locale-independent.
        Without /v, key names like "safeboot" are localized (e.g., German: "Abgesicherter Start")
        and the English key name match would fail on non-English Windows.  #>
    [CmdletBinding()]
    param([string]$Key, [string]$StepTitle, [switch]$PassThru)
    if ($SCRIPT:DryRun) {
        if ($PassThru) {
            return [PSCustomObject]@{ Captured = $false; Entry = $null; Message = 'Dry-run mode does not persist boot backups.' }
        }
        return
    }

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
        $global:LASTEXITCODE = 0
        $bcdOutput = bcdedit /enum "{current}" /v 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "bcdedit /enum returned exit code $LASTEXITCODE."
        }
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
    } catch {
        $message = "Backup-BootConfig: could not capture '$Key': $_"
        Write-DebugLog $message
        if ($PassThru) { return [PSCustomObject]@{ Captured = $false; Entry = $null; Message = $message } }
        return
    }

    $entry = [ordered]@{
        type          = "bootconfig"
        key           = $Key
        originalValue = $existing
        existed       = ($null -ne $existing)
        step          = $StepTitle
        timestamp     = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
    $SCRIPT:_backupPending.Add($entry)
    if ($PassThru) { return [PSCustomObject]@{ Captured = $true; Entry = $entry; Message = 'Boot configuration captured.' } }
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
    param(
        [string]$TaskName,
        [string]$StepTitle,
        [string]$ScriptPath = "",
        [string]$TaskPath = "\",
        [switch]$PassThru
    )
    if ($SCRIPT:DryRun) {
        if ($PassThru) {
            return [PSCustomObject]@{ Captured = $false; Entry = $null; Message = 'Dry-run mode does not persist scheduled-task backups.' }
        }
        return
    }
    $identity = [PSCustomObject]@{ taskName = $TaskName; taskPath = $TaskPath }
    if (-not (Test-ScheduledTaskBackupIdentity -Entry $identity)) {
        $message = "Backup-ScheduledTask: invalid task identity '$TaskPath$TaskName' - skipped."
        Write-Warn $message
        if ($PassThru) { return [PSCustomObject]@{ Captured = $false; Entry = $null; Message = $message } }
        return
    }
    $existed = $false
    $wasEnabled = $false
    try {
        # Enumerating once distinguishes a missing task from a provider or
        # permission failure. A targeted Get-ScheduledTask call reports both as
        # no result when callers use SilentlyContinue.
        $task = Get-ScheduledTask -ErrorAction Stop | Where-Object {
            $null -ne $_ -and $_.PSObject.Properties['TaskName'] -and $_.TaskName -eq $TaskName -and
            ((-not $_.PSObject.Properties['TaskPath']) -or (-not $_.TaskPath) -or $_.TaskPath -eq $TaskPath)
        } | Select-Object -First 1
        $existed = ($null -ne $task)
        if ($existed) {
            $wasEnabled = ($task.State -ne "Disabled")
            if ($task.PSObject.Properties['TaskPath'] -and $task.TaskPath) { $TaskPath = $task.TaskPath }
        }
    } catch {
        $message = "Backup-ScheduledTask: could not query '$TaskPath$TaskName': $_"
        Write-DebugLog $message
        if ($PassThru) { return [PSCustomObject]@{ Captured = $false; Entry = $null; Message = $message } }
        return
    }

    $entry = [ordered]@{
        type       = "scheduledtask"
        taskName   = $TaskName
        taskPath   = $TaskPath
        existed    = $existed
        wasEnabled = $wasEnabled
        scriptPath = $ScriptPath
        step       = $StepTitle
        timestamp  = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss")
    }
    $SCRIPT:_backupPending.Add($entry)
    Write-DebugLog "Backup-ScheduledTask: '$TaskPath$TaskName' existed=$existed wasEnabled=$wasEnabled"
    if ($PassThru) { return [PSCustomObject]@{ Captured = $true; Entry = $entry; Message = 'Scheduled-task state captured.' } }
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
    <#  Records the two suite QoS policies and URO state before modification.
        The persisted definition is deliberately closed over the exact policy
        shapes this suite creates.  A same-named foreign policy is not safe to
        replace because it cannot be restored losslessly, so capture fails
        before the caller mutates it. #>
    param(
        [object[]]$Policies,
        [string]$UroState,
        [string]$StepTitle
    )
    if ($SCRIPT:DryRun) { return }

    $normalizedUroState = ([string]$UroState).ToLowerInvariant()
    if ($normalizedUroState -ne 'n/a' -and $normalizedUroState -notin $SCRIPT:CFG_SuiteUroStates) {
        throw "QoS/URO backup blocked: unsupported URO state '$UroState'."
    }

    $policyStates = [System.Collections.Generic.List[object]]::new()
    foreach ($policyName in $SCRIPT:CFG_SuiteQosPolicyNames) {
        $existing = @($Policies | Where-Object { $_ -and ([string]$_.Name) -eq $policyName } | Select-Object -First 1)
        $existed = $existing.Count -gt 0
        if ($existed -and -not (Test-SuiteQosPolicyDefinition -Name $policyName -Policy $existing[0])) {
            throw "QoS backup blocked: existing policy '$policyName' is outside the supported lossless restore definition."
        }
        $policyStates.Add([PSCustomObject]@{
            name = $policyName
            originalExisted = $existed
            originalDefinition = if ($existed) { Get-SuiteQosPolicyDefinition -Name $policyName } else { $null }
        }) | Out-Null
    }

    $entry = [ordered]@{
        type                = 'qos_uro'
        contractVersion     = 2
        suiteManagedPolicies = @($SCRIPT:CFG_SuiteQosPolicyNames)
        policyStates        = @($policyStates)
        uroState            = $normalizedUroState
        step                = $StepTitle
        timestamp           = (Get-Date).ToString('yyyy-MM-dd HH:mm:ss')
    }
    $SCRIPT:_backupPending.Add($entry)
    Write-DebugLog "Backup-QosAndUro: captured fixed suite policies and uro=$normalizedUroState"
}

function Get-SuiteQosPolicyDefinition {
    param([Parameter(Mandatory)][string]$Name)

    switch ($Name) {
        'CS2_UDP_Ports' {
            return [PSCustomObject]@{
                ipProtocolMatchCondition = 'UDP'
                ipDstPortStartMatchCondition = 27015
                ipDstPortEndMatchCondition = 27036
                dscpAction = 46
                networkProfile = 'All'
            }
        }
        'CS2_App' {
            return [PSCustomObject]@{
                appPathNameMatchCondition = '*\cs2.exe'
                dscpAction = 46
                networkProfile = 'All'
            }
        }
        default { throw "Unsupported suite QoS policy '$Name'." }
    }
}

function Get-QosPolicyPropertyValue {
    param($Policy, [string[]]$Names)

    foreach ($propertyName in $Names) {
        $property = $Policy.PSObject.Properties[$propertyName]
        if ($null -ne $property) { return $property.Value }
    }
    return $null
}

function Test-SuiteQosPolicyDefinition {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)]$Policy)

    if ($Name -notin $SCRIPT:CFG_SuiteQosPolicyNames) { return $false }
    $recordedName = $Policy.PSObject.Properties['Name']
    if ($recordedName -and ([string]$recordedName.Value) -ne $Name) { return $false }
    $expected = Get-SuiteQosPolicyDefinition -Name $Name
    $actualProfile = [string](Get-QosPolicyPropertyValue -Policy $Policy -Names @('NetworkProfile'))
    $actualDscp = Get-QosPolicyPropertyValue -Policy $Policy -Names @('DSCPAction', 'DSCPValue')
    if ($actualProfile -ne $expected.networkProfile -or [string]$actualDscp -ne [string]$expected.dscpAction) { return $false }

    if ($Name -eq 'CS2_UDP_Ports') {
        return ([string](Get-QosPolicyPropertyValue -Policy $Policy -Names @('IPProtocolMatchCondition', 'IPProtocol')) -eq $expected.ipProtocolMatchCondition -and
            [string](Get-QosPolicyPropertyValue -Policy $Policy -Names @('IPDstPortStartMatchCondition', 'IPDstPortStart')) -eq [string]$expected.ipDstPortStartMatchCondition -and
            [string](Get-QosPolicyPropertyValue -Policy $Policy -Names @('IPDstPortEndMatchCondition', 'IPDstPortEnd')) -eq [string]$expected.ipDstPortEndMatchCondition)
    }
    return ([string](Get-QosPolicyPropertyValue -Policy $Policy -Names @('AppPathNameMatchCondition', 'AppPathName')) -eq $expected.appPathNameMatchCondition)
}

function Test-QosUroRestoreEntry {
    param([Parameter(Mandatory)]$Entry)

    $recordedNames = @($Entry.suiteManagedPolicies | ForEach-Object { [string]$_ } | Sort-Object)
    $expectedNames = @($SCRIPT:CFG_SuiteQosPolicyNames | Sort-Object)
    if ($Entry.contractVersion -ne 2 -or $recordedNames.Count -ne $expectedNames.Count -or
        (($recordedNames -join '|') -ne ($expectedNames -join '|')) -or
        ([string]$Entry.uroState).ToLowerInvariant() -notin @('n/a', 'enabled', 'disabled')) { return $false }
    $states = @($Entry.policyStates)
    if ($states.Count -ne $SCRIPT:CFG_SuiteQosPolicyNames.Count) { return $false }
    foreach ($policyName in $SCRIPT:CFG_SuiteQosPolicyNames) {
        $state = @($states | Where-Object { $_ -and $_.name -eq $policyName })
        if ($state.Count -ne 1 -or $state[0].originalExisted -isnot [bool]) { return $false }
        if ($state[0].originalExisted) {
            if (-not $state[0].originalDefinition -or -not (Test-SuiteQosPolicyDefinition -Name $policyName -Policy $state[0].originalDefinition)) { return $false }
        } elseif ($null -ne $state[0].originalDefinition) { return $false }
    }
    return $true
}

function New-SuiteQosPolicyFromDefinition {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)]$Definition)

    if (-not (Test-SuiteQosPolicyDefinition -Name $Name -Policy $Definition)) {
        throw "QoS restore rejected an unsupported definition for '$Name'."
    }
    if ($Name -eq 'CS2_UDP_Ports') {
        New-NetQosPolicy -Name $Name -IPProtocolMatchCondition UDP -IPDstPortStartMatchCondition 27015 `
            -IPDstPortEndMatchCondition 27036 -DSCPAction 46 -NetworkProfile All -ErrorAction Stop | Out-Null
        return
    }
    New-NetQosPolicy -Name $Name -AppPathNameMatchCondition '*\cs2.exe' -DSCPAction 46 `
        -NetworkProfile All -ErrorAction Stop | Out-Null
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
        Write-Warn "Cannot restore DRS settings - nvapi64.dll unavailable (driver uninstalled or 32-bit PowerShell)."
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
                Write-Warn "DRS restore: CS2 profile not found - may have been deleted already."
                $result.ok = $false
                return
            }

            if ($Entry.profileCreated) {
                # We created this profile - delete it entirely
                try {
                    [NvApiDrs]::DeleteProfile($session, $drsProfile)
                    Write-OK "Deleted DRS profile: $($Entry.profile)"
                } catch {
                    Write-Warn "DRS restore: could not delete profile - $_"
                    $result.ok = $false
                }
            } else {
                # Profile existed before - restore individual settings
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
                            # Setting didn't exist before - skip (writing 0 is NOT equivalent to "not set"
                            # for many DRS settings, e.g., VSync tear control 0 = enabled, not "remove")
                            $skipped++
                        }
                    } catch {
                        $errors++
                        # Cast $s.id to [uint32] before .ToString('X') - JSON round-trip
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
                    Write-Info "DRS restore: $skipped setting(s) were new (no previous value) - left as-is."
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
    Write-ConsoleLine "  ╔══════════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
    Write-ConsoleLine "  ║  BACKUP SUMMARY - Recorded Settings Before Changes              ║" -ForegroundColor Cyan
    Write-ConsoleLine "  ╠══════════════════════════════════════════════════════════════════╣" -ForegroundColor Cyan

    $grouped = $backup.entries | Group-Object -Property step
    foreach ($group in $grouped) {
        Write-ConsoleLine "  ║  $($group.Name)  ($($group.Count) change(s))" -ForegroundColor White
        foreach ($e in $group.Group) {
            $detail = switch ($e.type) {
                "registry"   { "REG  $($e.name) = $(if($e.existed){"$($e.originalValue)"}else{'(not set)'})" }
                "service"    { "SVC  $($e.name) was $($e.originalStartType) / $($e.originalStatus)" }
                "bootconfig" { "BCD  $($e.key) = $(if($e.existed){"$($e.originalValue)"}else{'(not set)'})" }
                "powerplan"  { "PWR  was $($e.originalName) ($($e.originalGuid))" }
                "drs"           { "DRS  profile '$($e.profile)' - $($e.settings.Count) setting(s)" }
                "scheduledtask" { "TASK $($e.taskName) $(if($e.existed){'(existed before)'}else{'(created by us)'})" }
                "nic_adapter"   { "NIC  $($e.adapterName): $($e.propertyName) = $($e.originalValue)" }
                "qos_uro"       { "QOS  suite policies: [$($e.suiteManagedPolicies -join ', ')] | URO: $($e.uroState)" }
                "defender"      { "DEF  $(if($e.exclusionPaths){@($e.exclusionPaths).Count}else{0}) path(s), $(if($e.exclusionProcesses){@($e.exclusionProcesses).Count}else{0}) process(es)" }
                "pagefile"      { "PGF  auto=$($e.automaticManaged) init=$($e.initialSize)MB max=$($e.maximumSize)MB" }
                "dns"           { "DNS  $($e.adapterName): [$($e.originalDnsServers -join ', ')]" }
                default         { "???  Unknown type '$($e.type)'" }
            }
            Write-ConsoleLine "  ║    $detail" -ForegroundColor DarkGray
        }
    }

    Write-ConsoleLine "  ╚══════════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
    Write-ConsoleLine "  Total: $($backup.entries.Count) setting(s) backed up" -ForegroundColor DarkGray
    Write-ConsoleLine "  File:  $CFG_BackupFile" -ForegroundColor DarkGray
}

function Restore-StepChanges {
    [CmdletBinding()]
    param(
        [string]$StepTitle,
        [ValidateRange(0, [int]::MaxValue)][int]$EntryIndex
    )
    $backup = Get-BackupData
    $restoreByIndex = $PSBoundParameters.ContainsKey('EntryIndex')
    if ($restoreByIndex) {
        if ($EntryIndex -ge @($backup.entries).Count) {
            Write-Warn "Backup entry index $EntryIndex is no longer available."
            return $false
        }
        $entries = @($backup.entries[$EntryIndex])
        $StepTitle = [string]$entries[0].step
    } else {
        $entries = @($backup.entries | Where-Object { $_.step -eq $StepTitle })
        # Interrupted runs can append another record for a step. Restore its
        # captures in reverse order so later mutations are undone first.
        [array]::Reverse($entries)
    }
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
                    if (-not (Test-RegistryRestoreAllowed -Path $e.path -Name $e.name)) {
                        Write-Warn "Registry restore: path/name outside restore allowlist - rejected: $($e.path) :: $($e.name)"
                        $restoreFail++
                        break
                    }
                    $restoreName = ([string]$e.name) -replace '/', '\'
                    if ($e.existed) {
                        $restoreType = if ($e.originalType) { $e.originalType } else { "DWord" }
                        $restoreValue = $e.originalValue
                        # Binary values are serialized as int arrays in JSON; cast back to byte[]
                        if ($restoreType -eq "Binary" -and $restoreValue -is [array]) {
                            # Validate each element is in [0,255] before casting - JSON may
                            # contain Int64 values from manual editing or corruption.
                            $badValues = @($restoreValue | Where-Object { $_ -lt 0 -or $_ -gt 255 })
                            if ($badValues.Count -gt 0) {
                                Write-Warn "Binary restore for ${restoreName}: $($badValues.Count) byte(s) outside [0,255] - skipping (backup may be corrupted)."
                                $restoreFail++
                                break
                            }
                            $restoreValue = [byte[]]@($restoreValue | ForEach-Object { [byte]$_ })
                        }
                        # MultiString values are deserialized as Object[] from JSON; ensure string[].
                        # PS 5.1 ConvertFrom-Json unwraps single-element arrays to scalars, so
                        # a MultiString backup with one entry arrives as a plain string - wrap it.
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
                        # no special handling needed - the value passes through as-is.
                        if (-not (Test-Path $e.path)) {
                            New-Item -Path $e.path -Force -ErrorAction Stop | Out-Null
                        }
                        Set-ItemProperty -Path $e.path -Name $restoreName -Value $restoreValue -Type $restoreType -ErrorAction Stop
                        Write-OK "Restored: $restoreName = $($e.originalValue)"
                    } else {
                        if (Test-Path -Path $e.path -ErrorAction Stop) {
                            # Read the whole key so an absent value is distinct
                            # from an access or provider error. A failed read must
                            # keep the retry record.
                            $existingVal = Get-ItemProperty -Path $e.path -ErrorAction Stop
                            $valueProperty = $existingVal.PSObject.Properties[$restoreName]
                            if ($null -ne $valueProperty) {
                                Remove-ItemProperty -Path $e.path -Name $restoreName -ErrorAction Stop
                                $postRemove = Get-ItemProperty -Path $e.path -ErrorAction Stop
                                if ($null -ne $postRemove.PSObject.Properties[$restoreName]) {
                                    throw "Registry value '$restoreName' is still present after removal."
                                }
                                Write-OK "Removed: $restoreName (was not set before)"
                            } else {
                                Write-DebugLog "Restore: value '$restoreName' already absent from '$($e.path)' - skip"
                            }
                        } else {
                            Write-DebugLog "Restore: path '$($e.path)' no longer exists - skip remove for '$restoreName'"
                        }
                    }
                    $restoreOk++
                }
                "service" {
                    # SECURITY: Validate service name - a tampered backup.json could inject
                    # path traversal or special characters into registry paths and WMI queries.
                    if ($e.name -notmatch '^[a-zA-Z0-9_\-\. ]+$' -or $e.name.Length -gt 256) {
                        Write-Warn "Service restore skipped - invalid service name: '$($e.name)'"
                        $restoreFail++
                        break
                    }
                    if (-not (Test-ServiceRestoreAllowed -ServiceName $e.name)) {
                        Write-Warn "Service restore skipped - service outside restore allowlist: '$($e.name)'"
                        $restoreFail++
                        break
                    }
                    $startMap = @{ "Auto"="Automatic"; "Manual"="Manual"; "Disabled"="Disabled"; "Auto Delayed"="AutomaticDelayedStart" }
                    $mapped = if ($startMap[$e.originalStartType]) { $startMap[$e.originalStartType] } else { $e.originalStartType }
                    # Boot/System/Unknown are kernel driver start types - Set-Service cannot change them.
                    # These are not failures - kernel drivers manage their own start type and
                    # no user action is needed, so count as handled (not failed).
                    if ($e.originalStartType -in @("Boot","System","Unknown")) {
                        Write-Info "Service $($e.name) has start type '$($e.originalStartType)' - kernel driver, no restore needed."
                        $restoreOk++
                        break
                    } else {
                        if ($mapped -notin @("Automatic", "Manual", "Disabled", "AutomaticDelayedStart")) {
                            Write-Warn "Service restore skipped - unsupported start type '$($e.originalStartType)' for '$($e.name)'"
                            $restoreFail++
                            break
                        }
                        # Verify the service still exists before attempting restore - if it was
                        # uninstalled (e.g., Xbox services removed by system update), Set-Service
                        # with -ErrorAction SilentlyContinue silently fails and we'd report success.
                        $svcExists = Get-Service -Name $e.name -ErrorAction SilentlyContinue
                        if (-not $svcExists) {
                            Write-Warn "Service '$($e.name)' no longer exists - cannot restore."
                            $restoreFail++
                            break
                        }
                        Set-Service -Name $e.name -StartupType $mapped -ErrorAction Stop
                        # Restore DelayedAutoStart flag if it was set (Auto + Delayed = "Automatic (Delayed Start)")
                        if ($e.delayedAutoStart) {
                            $regPath = "HKLM:\SYSTEM\CurrentControlSet\Services\$($e.name)"
                            Set-ItemProperty -Path $regPath -Name "DelayedAutostart" -Value 1 -Type DWord -ErrorAction Stop
                        }
                        if ($e.originalStatus -eq "Running") {
                            Start-Service -Name $e.name -ErrorAction Stop
                        }
                        $delayTag = if ($e.delayedAutoStart) { " (Delayed)" } else { "" }
                        Write-OK "Restored: $($e.name) -> $($e.originalStartType)$delayTag"
                        $restoreOk++
                    }
                }
                "bootconfig" {
                    # SECURITY: backup.json is untrusted; restore only the BCD elements this
                    # suite actually manages, with per-key value allowlists.
                    if (-not (Test-BootConfigRestoreAllowed -Key $e.key -Value $e.originalValue -Existed ([bool]$e.existed))) {
                        Write-Warn "bcdedit restore: key/value outside restore allowlist '$($e.key)' = '$($e.originalValue)' - skipping (security)"
                        $restoreFail++
                        break
                    }
                    if ($e.existed) {
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
                    # SECURITY: Validate GUID before passing to powercfg - backup.json is in
                    # C:\FRAMETIME_CFG\ and could be tampered to inject arbitrary powercfg args.
                    if ($e.originalGuid -notmatch '^[a-fA-F0-9\-]{36}$') {
                        Write-Warn "Power plan restore: invalid GUID format '$($e.originalGuid)' - skipping (security)"
                        $restoreFail++
                        break
                    }
                    # Restore the original power plan first.  Cleanup below is
                    # strictly GUID-based and limited to identities explicitly
                    # recorded as suite-owned at successful installation time.
                    powercfg /setactive $e.originalGuid 2>&1 | Out-Null
                    if ($LASTEXITCODE -ne 0) {
                        Write-Warn "Failed to restore power plan '$($e.originalName)' ($($e.originalGuid)) - plan may no longer exist."
                        $restoreFail++
                        break
                    }
                    Write-OK "Restored power plan: $($e.originalName) ($($e.originalGuid))"

                    # A backup entry alone is not authority to delete a plan:
                    # it is user-editable JSON.  Only the exact intersection
                    # with separately persisted state ownership is authenticated.
                    $backupOwnedGuids = @()
                    if ($e.PSObject.Properties['suiteOwnedGuids']) {
                        $backupOwnedGuids += @($e.suiteOwnedGuids)
                    }
                    $backupOwnedGuids = @($backupOwnedGuids | Where-Object {
                        [string]$_ -match '^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$'
                    } | ForEach-Object { ([string]$_).ToLowerInvariant() } | Select-Object -Unique)
                    $stateOwnedGuids = @(Get-RecordedSuiteOwnedPowerPlanGuids)
                    $unauthenticatedGuids = @($backupOwnedGuids | Where-Object { $_ -notin $stateOwnedGuids })
                    if ($unauthenticatedGuids.Count -gt 0) {
                        Write-Warn "Power-plan restore rejected backup-only ownership GUID(s): $($unauthenticatedGuids -join ', ')."
                        $restoreFail++
                        break
                    }
                    $recordedOwnedGuids = @($backupOwnedGuids | Where-Object { $_ -in $stateOwnedGuids })
                    # State-only identities are never deleted by this restore
                    # point.  An absent identity can, however, be removed from
                    # stale bookkeeping after an authoritative inventory. This
                    # makes a retry converge if deletion succeeded but the
                    # following state write failed.
                    $stateOnlyGuids = @($stateOwnedGuids | Where-Object { $_ -notin $recordedOwnedGuids })
                    $retainedStateOnlyGuids = [System.Collections.Generic.List[string]]::new()
                    foreach ($stateOnlyGuid in $stateOnlyGuids) {
                        $presence = Get-PowerPlanGuidPresence -Guid $stateOnlyGuid
                        if ($presence.Verified -and -not $presence.Present) {
                            Write-DebugLog "Removed stale suite-owned power-plan bookkeeping for absent plan: $stateOnlyGuid"
                        } else {
                            $retainedStateOnlyGuids.Add($stateOnlyGuid)
                        }
                    }
                    $remainingOwnedGuids = [System.Collections.Generic.List[string]]::new()
                    foreach ($planGuid in @($recordedOwnedGuids | Select-Object -Unique)) {
                        if ($planGuid -notmatch '^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$' -or
                            $planGuid -eq $e.originalGuid) { continue }
                        powercfg /delete $planGuid 2>&1 | Out-Null
                        if ($LASTEXITCODE -eq 0) {
                            Write-OK "Deleted suite-owned plan: $planGuid"
                        } else {
                            $presence = Get-PowerPlanGuidPresence -Guid $planGuid
                            if ($presence.Verified -and -not $presence.Present) {
                                Write-DebugLog "Suite-owned power plan is already absent: $planGuid"
                            } else {
                                Write-Warn "Could not delete suite-owned plan: $planGuid. $($presence.Message)"
                                $remainingOwnedGuids.Add($planGuid)
                            }
                        }
                    }
                    # Narrow the in-memory retry record before either fallible
                    # persistence write. If state persistence fails, the retained
                    # backup entry still contains only plans verified to remain.
                    $e | Add-Member -NotePropertyName suiteOwnedGuids -NotePropertyValue @($remainingOwnedGuids) -Force
                    try { Set-RecordedSuiteOwnedPowerPlanGuids -OwnedGuids @($retainedStateOnlyGuids + $remainingOwnedGuids) } catch {
                        Write-Warn "Could not update suite-owned power-plan state after restore: $_"
                        $restoreFail++
                        break
                    }
                    if ($remainingOwnedGuids.Count -gt 0) {
                        Write-Warn "Power plan restore was partial; $($remainingOwnedGuids.Count) suite-owned plan(s) remain for retry."
                        $restoreFail++
                        break
                    }
                    $restoreOk++
                }
                "drs" {
                    $drsResult = Restore-DrsSettings -Entry $e
                    if ($drsResult -eq $false) { $restoreFail++ } else { $restoreOk++ }
                }
                "scheduledtask" {
                    $taskRestoreFailed = $false
                    $taskPath = Get-BackupTaskPath $e
                    if (-not (Test-ScheduledTaskRestoreAllowed -Entry $e)) {
                        Write-Warn "Scheduled task restore: task outside restore allowlist - rejected: $taskPath$($e.taskName)"
                        $restoreFail++
                        break
                    }
                    if (-not $e.existed) {
                        # Task didn't exist before we created it - remove it entirely
                        try {
                            $task = if ($taskPath -ne "\") {
                                Get-ScheduledTask -TaskName $e.taskName -TaskPath $taskPath -ErrorAction SilentlyContinue
                            } else {
                                Get-ScheduledTask -TaskName $e.taskName -ErrorAction SilentlyContinue
                            }
                            if ($task) {
                                # Stop the task first if it's running to avoid Unregister failure
                                if ($task.State -eq "Running") {
                                    if ($taskPath -ne "\") {
                                        Stop-ScheduledTask -TaskName $e.taskName -TaskPath $taskPath -ErrorAction Stop
                                    } else {
                                        Stop-ScheduledTask -TaskName $e.taskName -ErrorAction Stop
                                    }
                                }
                                if ($taskPath -ne "\") {
                                    Unregister-ScheduledTask -TaskName $e.taskName -TaskPath $taskPath -Confirm:$false -ErrorAction Stop
                                } else {
                                    Unregister-ScheduledTask -TaskName $e.taskName -Confirm:$false -ErrorAction Stop
                                }
                                $remainingTask = if ($taskPath -ne "\") {
                                    Get-ScheduledTask -TaskName $e.taskName -TaskPath $taskPath -ErrorAction SilentlyContinue
                                } else {
                                    @(Get-ScheduledTask -TaskName $e.taskName -ErrorAction SilentlyContinue) |
                                        Where-Object { -not $_.PSObject.Properties['TaskPath'] -or $_.TaskPath -eq "\" }
                                }
                                if ($remainingTask) {
                                    throw "Scheduled task '$taskPath$($e.taskName)' is still present after removal."
                                }
                                Write-OK "Removed scheduled task: $taskPath$($e.taskName)"
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
                                Remove-Item $e.scriptPath -Force -ErrorAction Stop
                                if (Test-Path -LiteralPath $e.scriptPath) {
                                    throw "Scheduled-task helper '$($e.scriptPath)' is still present after removal."
                                }
                                Write-OK "Removed: $($e.scriptPath)"
                            }
                        }
                    } else {
                        # Task existed before - restore its enabled/disabled state
                        # Use wasEnabled field (added in batch buffer update) to avoid
                        # blindly re-enabling tasks that were already disabled before optimization.
                        $shouldBeEnabled = if ($e.PSObject.Properties['wasEnabled'] -and $null -ne $e.wasEnabled) { $e.wasEnabled } else { $true }
                        try {
                            $task = if ($taskPath -ne "\") {
                                Get-ScheduledTask -TaskName $e.taskName -TaskPath $taskPath -ErrorAction SilentlyContinue
                            } else {
                                Get-ScheduledTask -TaskName $e.taskName -ErrorAction SilentlyContinue
                            }
                            if (-not $task) {
                                Write-Warn "Scheduled task '$taskPath$($e.taskName)' no longer exists - cannot restore."
                                $taskRestoreFailed = $true
                            } elseif ($shouldBeEnabled -and $task.State -eq "Disabled") {
                                if ($taskPath -ne "\") {
                                    Enable-ScheduledTask -TaskName $e.taskName -TaskPath $taskPath -ErrorAction Stop | Out-Null
                                } else {
                                    Enable-ScheduledTask -TaskName $e.taskName -ErrorAction Stop | Out-Null
                                }
                                Write-OK "Re-enabled scheduled task: $taskPath$($e.taskName)"
                            } elseif (-not $shouldBeEnabled -and $task.State -ne "Disabled") {
                                if ($taskPath -ne "\") {
                                    Disable-ScheduledTask -TaskName $e.taskName -TaskPath $taskPath -ErrorAction Stop | Out-Null
                                } else {
                                    Disable-ScheduledTask -TaskName $e.taskName -ErrorAction Stop | Out-Null
                                }
                                Write-OK "Re-disabled scheduled task: $taskPath$($e.taskName) (was disabled before optimization)"
                            } else {
                                Write-Info "Scheduled task '$taskPath$($e.taskName)' already in correct state - kept."
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
                                break
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
                    # backup.json is untrusted.  Validate the complete versioned
                    # record before changing either policy or URO, and retain it
                    # untouched if it is legacy, incomplete, or tampered.
                    $qosFailed = $false
                    if (-not (Test-QosUroRestoreEntry -Entry $e)) {
                        Write-Warn 'QoS/URO restore rejected an unsupported or tampered backup definition.'
                        $restoreFail++
                        break
                    }
                    # Preflight every managed identity before deleting any one
                    # of them, so a changed policy cannot cause a half-restore.
                    foreach ($policyName in $SCRIPT:CFG_SuiteQosPolicyNames) {
                        try {
                            $existingPolicy = Get-NetQosPolicy -Name $policyName -ErrorAction SilentlyContinue
                            if ($existingPolicy -and -not (Test-SuiteQosPolicyDefinition -Name $policyName -Policy $existingPolicy)) {
                                Write-Warn "QoS restore refused to remove '$policyName' because its current definition is not suite-managed."
                                $qosFailed = $true
                                break
                            }
                        } catch {
                            Write-Warn "Could not inspect QoS policy '$policyName' before restore: $_"
                            $qosFailed = $true
                            break
                        }
                    }
                    foreach ($policyName in $SCRIPT:CFG_SuiteQosPolicyNames) {
                        if ($qosFailed) { break }
                        try {
                            $existingPolicy = Get-NetQosPolicy -Name $policyName -ErrorAction SilentlyContinue
                            if ($existingPolicy) {
                                Remove-NetQosPolicy -Name $policyName -Confirm:$false -ErrorAction Stop
                                Write-OK "Removed suite-managed QoS policy: $policyName"
                            } else {
                                Write-DebugLog "QoS policy '$policyName' does not exist - nothing to remove"
                            }
                        } catch {
                            Write-Warn "Could not remove QoS policy '$policyName': $_"
                            $qosFailed = $true
                        }
                    }
                    if (-not $qosFailed) {
                        foreach ($policyState in @($e.policyStates)) {
                            if (-not $policyState.originalExisted) { continue }
                            try {
                                New-SuiteQosPolicyFromDefinition -Name $policyState.name -Definition $policyState.originalDefinition
                                Write-OK "Restored original QoS policy: $($policyState.name)"
                            } catch {
                                Write-Warn "Could not restore original QoS policy '$($policyState.name)': $_"
                                $qosFailed = $true
                                break
                            }
                        }
                    }
                    # Restore URO state
                    if (-not $qosFailed -and $e.uroState -ne 'n/a') {
                        try {
                            $uroOut = netsh int udp set global uro=$($e.uroState) 2>&1
                            if ($LASTEXITCODE -eq 0) {
                                Write-OK "Restored URO state: $($e.uroState)"
                            } else {
                                Write-DebugLog "URO restore: netsh returned error - $uroOut"
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
                        Write-Warn "Pagefile restore: automated restore failed - falling back to manual instructions. $_"
                        Write-Info "Pagefile restore: original config was AutoManaged=$($e.automaticManaged), InitialSize=$($e.initialSize)MB, MaxSize=$($e.maximumSize)MB"
                        Write-Info "Manual restore: System Properties -> Advanced -> Performance -> Virtual Memory"
                        if ($e.automaticManaged) {
                            Write-Info "  Set 'Automatically manage paging file size for all drives' = checked"
                        } else {
                            Write-Info "  Set custom size: Initial=$($e.initialSize)MB, Maximum=$($e.maximumSize)MB on $($e.pagefilePath)"
                        }
                        Write-Info "Pagefile restore note: a reboot is required for the change to take effect."
                        Write-Warn "Pagefile restore recorded as partial success - manual completion still required."
                        $restorePartial++
                    }
                }
                "dns" {
                    try {
                        # Resolve the current InterfaceIndex - the stored index may be stale
                        # if the adapter was re-plugged or the system was rebooted. Do not
                        # fall back to a stored index unless the adapter name still resolves:
                        # Windows can reuse interface indexes for different adapters.
                        $ifIndex = $e.interfaceIndex
                        if ([string]::IsNullOrWhiteSpace([string]$e.adapterName)) {
                            throw "adapter name missing from backup; refusing to restore DNS by stored InterfaceIndex $ifIndex"
                        }
                        $currentAdapter = Get-NetAdapter -Name $e.adapterName -ErrorAction Stop |
                            Select-Object -First 1
                        if (-not $currentAdapter) {
                            throw "adapter '$($e.adapterName)' not found; refusing to restore DNS by stored InterfaceIndex $ifIndex"
                        }
                        if ($currentAdapter.InterfaceIndex -ne $ifIndex) {
                            Write-DebugLog "DNS restore: InterfaceIndex changed from $ifIndex to $($currentAdapter.InterfaceIndex)"
                            $ifIndex = $currentAdapter.InterfaceIndex
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
                    Write-Warn "Unknown backup type '$($e.type)' - cannot restore (skipping)"
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
        Write-Warn "Restore '$StepTitle': $restoreOk succeeded, $restoreFail failed - check warnings above."
    }
    if ($restorePartial -gt 0) {
        Write-Warn "Restore '$StepTitle': $restorePartial partial/manual step(s) still need completion."
    }

    # Remove successfully restored entries; keep failed and partial/manual ones for retry.
    $retainedEntries = @($failedEntries) + @($partialEntries)
    if ($restoreByIndex) {
        if ($retainedEntries.Count -eq 0) {
            $remaining = [System.Collections.ArrayList]@($backup.entries)
            $remaining.RemoveAt($EntryIndex)
            $backup.entries = @($remaining)
        }
    } else {
        $backup.entries = @($backup.entries | Where-Object { $_.step -ne $StepTitle -or $_ -in $retainedEntries })
    }
    Save-BackupData $backup
    if ($restoreFail -gt 0) {
        Write-Warn "$restoreFail failed entry/entries retained for '$StepTitle' - retry restore to complete."
    }
    if ($restorePartial -gt 0) {
        Write-Warn "$restorePartial partial entry/entries retained for '$StepTitle' - complete the manual pagefile step, then retry if needed."
    }
    return ($restoreFail -eq 0 -and $restorePartial -eq 0)
}

function Restore-AllChanges {
    <#  Restore persisted mutations in strict reverse capture order.
        Removing an entry at the current index cannot change any lower,
        unprocessed index. Failed and skipped entries remain available.  #>
    [CmdletBinding()]
    param([string[]]$IncludeStep)

    $backup = Get-BackupData
    $entryCount = @($backup.entries).Count
    $filterSteps = $PSBoundParameters.ContainsKey('IncludeStep')
    $attempted = 0
    $failed = 0
    $skipped = 0

    for ($entryIndex = $entryCount - 1; $entryIndex -ge 0; $entryIndex--) {
        $stepName = [string]$backup.entries[$entryIndex].step
        if ($filterSteps -and $stepName -notin $IncludeStep) {
            $skipped++
            continue
        }

        $attempted++
        if (-not (Restore-StepChanges -StepTitle $stepName -EntryIndex $entryIndex)) {
            $failed++
        }
    }

    return [PSCustomObject]@{
        Succeeded = ($failed -eq 0)
        Attempted = $attempted
        Failed    = $failed
        Skipped   = $skipped
    }
}

function Restore-Interactive {
    if (Test-BackupLock) {
        Write-Warn "Another frametime.cfg process is currently running (backup.json is locked)."
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
        Write-ConsoleLine "  Select step to restore:" -ForegroundColor White
        for ($i = 0; $i -lt $grouped.Count; $i++) {
            Write-ConsoleLine "  [$($i+1)]  $($grouped[$i].Name)  ($($grouped[$i].Count) change(s))" -ForegroundColor White
        }
        Write-ConsoleLine "  [A]  Restore ALL" -ForegroundColor Yellow
        Write-ConsoleLine "  [0]  Cancel" -ForegroundColor DarkGray

        $choice = Read-Host "  Choice"
        if ($choice -eq "0" -or [string]::IsNullOrWhiteSpace($choice)) { return }
        if ($choice -match "^[aA]$") {
            # Ask for all decisions before applying changes, then delegate to
            # the reverse-order restore path. Group-Object sorts names and must
            # not determine mutation order.
            $stepNames = [System.Collections.Generic.List[string]]::new()
            for ($entryIndex = @($backup.entries).Count - 1; $entryIndex -ge 0; $entryIndex--) {
                $stepName = [string]$backup.entries[$entryIndex].step
                if ($stepName -notin $stepNames) { $stepNames.Add($stepName) | Out-Null }
            }
            $selectedSteps = [System.Collections.Generic.List[string]]::new()
            $skippedSteps = [System.Collections.Generic.List[string]]::new()
            foreach ($stepName in $stepNames) {
                Write-Blank
                Write-ConsoleLine "  [$stepName]" -ForegroundColor Cyan
                Write-ConsoleLine "  [R]  Restore and continue" -ForegroundColor White
                Write-ConsoleLine "  [S]  Skip this step" -ForegroundColor Yellow
                Write-ConsoleLine "  [A]  Abort interactive restore" -ForegroundColor DarkGray
                do { $stepAction = Read-Host "  [R/S/A]" } while ($stepAction -notmatch "^[rRsSaA]$")
                if ($stepAction -match "^[aA]$") {
                    Write-Warn "Interactive restore aborted - remaining entries left in backup.json."
                    return
                }
                if ($stepAction -match "^[sS]$") {
                    Write-Info "Skipped step '$stepName' - entry remains in backup.json."
                    $skippedSteps.Add($stepName) | Out-Null
                    continue
                }
                $selectedSteps.Add($stepName) | Out-Null
            }
            $result = if ($selectedSteps.Count -gt 0) {
                Restore-AllChanges -IncludeStep @($selectedSteps)
            } else {
                [PSCustomObject]@{ Succeeded = $true; Attempted = 0; Failed = 0; Skipped = @($backup.entries).Count }
            }
            if ($result.Succeeded -and $skippedSteps.Count -eq 0) { Write-OK "All recorded supported settings were restored." }
            elseif ($result.Succeeded) { Write-Warn "Restore completed with $($skippedSteps.Count) skipped step group(s): $(@($skippedSteps) -join ', ')." }
            else { Write-Warn "$($result.Failed) backup entry restore(s) failed - check output above." }
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
