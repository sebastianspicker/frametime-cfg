# ==============================================================================
#  helpers/system-utils.ps1  -  Download, Registry, Boot Config, Filesystem
# ==============================================================================

function Invoke-Download {
    [CmdletBinding()]
    param([string]$url, [string]$dest, [string]$name)
    Write-Step "Download: $name"
    Write-DebugLog "URL: $url -> $dest"

    # SECURITY: Defense-in-depth URL allowlist. Currently only NVIDIA drivers are
    # downloaded; the caller (nvidia-driver.ps1) already validates the domain, but
    # this catches any future callers or data-flow changes that bypass that check.
    $allowedDomains = @('nvidia.com', 'download.nvidia.com', 'us.download.nvidia.com',
                        'international.download.nvidia.com')
    try {
        $uri = [System.Uri]::new($url)
        if ($uri.Scheme -ne 'https') {
            Write-Err "Invoke-Download: only HTTPS URLs are allowed - rejected: $url"
            return $false
        }
        $host_ = $uri.Host
        $domainMatch = $allowedDomains | Where-Object { $host_ -eq $_ -or $host_.EndsWith(".$_") }
        if (-not $domainMatch) {
            Write-Err "Invoke-Download: domain '$host_' is not in the download allowlist - rejected."
            Write-Warn "Allowed: $($allowedDomains -join ', ')"
            return $false
        }
    } catch {
        Write-Err "Invoke-Download: invalid URL - $url"
        return $false
    }

    $maxAttempts = 2
    # Set $global: scope so PS 5.1's Invoke-WebRequest sees it (function-scope has no effect in 5.1)
    $oldProgressPref = $global:ProgressPreference
    try {
        $global:ProgressPreference = 'SilentlyContinue'
        for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
            try {
                Invoke-WebRequest -Uri $url -OutFile $dest -UseBasicParsing -TimeoutSec 120
                $fileSize = (Get-Item $dest).Length
                $mb = [math]::Round($fileSize / 1MB, 2)
                # Sanity check: NVIDIA drivers are >100 MB; reject obviously truncated files
                if ($fileSize -lt 1MB) {
                    Write-Warn "Download appears incomplete ($mb MB, expected >100 MB) - removing corrupt file."
                    Remove-Item $dest -Force -ErrorAction SilentlyContinue
                    if ($attempt -lt $maxAttempts) { Write-Info "Retrying..."; continue }
                    Write-Err "Download failed after $maxAttempts attempts (file too small)."
                    Write-ConsoleLine "  $([char]0x2139) What to do: Your internet connection may be unstable." -ForegroundColor Cyan
                    Write-ConsoleLine "    Download manually from the URL below and provide the path when prompted." -ForegroundColor Cyan
                    Write-Warn "URL: $url"
                    return $false
                }
                Write-OK "$name ($mb MB)"
                return $true
            } catch {
                if ($attempt -lt $maxAttempts) {
                    Write-Warn "Download attempt $attempt failed: $_ - retrying..."
                    Remove-Item $dest -Force -ErrorAction SilentlyContinue
                } else {
                    Write-Err "Download failed after $maxAttempts attempts: $_"
                    Write-ConsoleLine "  $([char]0x2139) What to do: Download the file manually from the URL below" -ForegroundColor Cyan
                    Write-ConsoleLine "    and provide the path when prompted." -ForegroundColor Cyan
                    Write-Warn "URL: $url"
                    Remove-Item $dest -Force -ErrorAction SilentlyContinue
                    return $false
                }
            }
        }
        return $false
    } finally {
        $global:ProgressPreference = $oldProgressPref
    }
}

function Invoke-AtomicJsonFileCommit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$TemporaryPath,
        [Parameter(Mandatory)][string]$DestinationPath
    )

    if ([IO.File]::Exists($DestinationPath)) {
        if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
            # Windows PowerShell 5.1 requires File.Replace for an atomic
            # existing-file swap. A real same-directory backup path is required
            # by all supported runtimes; if the process dies after replacement,
            # both the new destination and recoverable old backup still exist.
            $replacementBackup = "$DestinationPath.replace-backup-$([Guid]::NewGuid().ToString('N'))"
            try {
                [IO.File]::Replace($TemporaryPath, $DestinationPath, $replacementBackup)
            } finally {
                if ([IO.File]::Exists($replacementBackup)) {
                    try { [IO.File]::Delete($replacementBackup) } catch {
                        Write-Warn "Atomic JSON replacement committed, but its backup could not be removed: $replacementBackup"
                    }
                }
            }
        } else {
            # .NET on Unix maps overwrite=true to the same-filesystem atomic
            # rename(2) replacement primitive.
            [IO.File]::Move($TemporaryPath, $DestinationPath, $true)
        }
    } else {
        # Both paths are in the same directory, so this is one metadata rename.
        [IO.File]::Move($TemporaryPath, $DestinationPath)
    }
}

function Save-JsonAtomic {
    <#  Writes JSON to a file atomically (write-to-temp-then-rename).
        Prevents corruption if interrupted by crash or power loss.
        NOTE: This does NOT prevent lost updates from concurrent read-modify-write
        cycles. Callers modifying shared files (backup.json, progress.json) should
        acquire the advisory backup lock (Set-BackupLock) before the read step.  #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object]$Data,
        [Parameter(Mandatory)][string]$Path,
        [int]$Depth = 10
    )
    # Ensure parent directory exists - callers usually call Ensure-Dir early,
    # but defensive creation here prevents silent failures from edge-case paths.
    $nativePath = $Path -replace '\\', [IO.Path]::DirectorySeparatorChar
    $parentDir = [IO.Path]::GetDirectoryName($nativePath)
    if ($parentDir -and -not (Test-Path $parentDir)) {
        New-Item -ItemType Directory -Path $parentDir -Force -ErrorAction Stop | Out-Null
    }
    $leafName = [IO.Path]::GetFileName($nativePath)
    $tmpName = "{0}.{1}.{2}.tmp" -f $leafName, $PID, ([System.IO.Path]::GetRandomFileName())
    $tmp = if ($parentDir) { Join-Path $parentDir $tmpName } else { $tmpName }
    try {
        $json = $Data | ConvertTo-Json -Depth $Depth
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes($json)
        $stream = [IO.File]::Open(
            $tmp,
            [IO.FileMode]::CreateNew,
            [IO.FileAccess]::Write,
            [IO.FileShare]::None
        )
        try {
            $stream.Write($bytes, 0, $bytes.Length)
            $stream.Flush($true)
        } finally {
            $stream.Dispose()
        }
        Invoke-AtomicJsonFileCommit -TemporaryPath $tmp -DestinationPath $nativePath
    } catch {
        Remove-Item $tmp -Force -ErrorAction SilentlyContinue
        throw "Save-JsonAtomic failed for '$Path': $_"
    }
}

function Set-SecureAcl {
    <#  Applies an Administrators/SYSTEM-only ACL to a sensitive file or directory.
        NOTE: C:\FRAMETIME_CFG should also inherit restrictive ACLs so newly created
        temp files from Save-JsonAtomic stay protected before this file ACL is re-applied.  #>
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)][string]$Path,
        [switch]$Required
    )

    if (-not (Test-Path -LiteralPath $Path)) { return }
    if (-not (Test-HostIsWindows)) { return }

    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]$identity
    $isAdmin = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $isAdmin) {
        Write-DebugLog "Set-SecureAcl: skipped ACL hardening for '$Path' in non-elevated session."
        return
    }

    $admins = New-Object System.Security.Principal.NTAccount("BUILTIN", "Administrators")
    $system = New-Object System.Security.Principal.NTAccount("NT AUTHORITY", "SYSTEM")

    function Set-SuiteDacl {
        param(
            [Parameter(Mandatory)][string]$TargetPath,
            [Parameter(Mandatory)]$AdministratorsAccount,
            [Parameter(Mandatory)]$SystemAccount,
            [switch]$SetOwner
        )

        $item = Get-Item -LiteralPath $TargetPath -Force -ErrorAction Stop
        $isDirectory = $item.PSObject.Properties.Match("PSIsContainer").Count -gt 0 -and $item.PSIsContainer
        $acl = Get-Acl -LiteralPath $TargetPath -ErrorAction Stop
        $inheritance = if ($isDirectory) {
            [System.Security.AccessControl.InheritanceFlags]"ContainerInherit,ObjectInherit"
        } else {
            [System.Security.AccessControl.InheritanceFlags]::None
        }
        $propagation = [System.Security.AccessControl.PropagationFlags]::None
        $adminRule = New-Object System.Security.AccessControl.FileSystemAccessRule($AdministratorsAccount, "FullControl", $inheritance, $propagation, "Allow")
        $systemRule = New-Object System.Security.AccessControl.FileSystemAccessRule($SystemAccount, "FullControl", $inheritance, $propagation, "Allow")

        if ($SetOwner) {
            $acl.SetOwner($AdministratorsAccount)
        }
        $acl.SetAccessRuleProtection($true, $false)
        foreach ($rule in @($acl.Access)) {
            [void]$acl.RemoveAccessRuleAll($rule)
        }
        $acl.SetAccessRule($adminRule)
        $acl.SetAccessRule($systemRule)
        if (-not $PSCmdlet.ShouldProcess($Path, "Apply restricted Administrators/SYSTEM ACL")) { return }
        Set-Acl -LiteralPath $Path -AclObject $acl -ErrorAction Stop
    }

    try {
        Set-SuiteDacl -TargetPath $Path -AdministratorsAccount $admins -SystemAccount $system -SetOwner
    } catch {
        $ownerError = $_
        try {
            # Some elevated contexts may still be unable to change ownership.
            # Keep the restrictive DACL when possible; non-elevated sandboxes
            # return above so they do not lock themselves out of temp files.
            Set-SuiteDacl -TargetPath $Path -AdministratorsAccount $admins -SystemAccount $system
            Write-DebugLog "Set-SecureAcl: owner assignment skipped for '$Path': $ownerError"
        } catch {
            if ($Required) { throw "Failed to secure ACL on '$Path': $_" }
            Write-Warn "Failed to secure ACL on '$Path': $_"
        }
    }
}

function Save-SuiteState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$State,
        [switch]$AllowDryRunPersistence
    )

    if (-not $AllowDryRunPersistence -and
        (Get-Variable DryRun -Scope Script -ErrorAction SilentlyContinue) -and
        $SCRIPT:DryRun) {
        Write-DebugLog "DRY-RUN: suite state persistence skipped."
        return
    }

    Ensure-SecureWorkDir -Path (Split-Path $CFG_StateFile -Parent) `
        -AllowDryRunPersistence:$AllowDryRunPersistence
    Save-JsonAtomic -Data $State -Path $CFG_StateFile
    Set-SecureAcl -Path $CFG_StateFile -Required
}

function Ensure-SecureWorkDir {
    [CmdletBinding()]
    param(
        [string]$Path = $CFG_WorkDir,
        [switch]$AllowDryRunPersistence
    )

    $dryRunActive = (Get-Variable DryRun -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:DryRun
    if ($dryRunActive -and -not $AllowDryRunPersistence) { return }

    Ensure-Dir -Path $Path -AllowDryRunPersistence:$AllowDryRunPersistence
    if (-not (Test-HostIsWindows)) { return }

    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Work directory '$Path' is a reparse point - refusing to trust runtime files."
    }
    Set-SecureAcl -Path $Path -Required
}

function Test-TrustedSuiteScriptPath {
    [CmdletBinding()]
    param([AllowEmptyString()][string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) { return $false }
    $normalizedPath = $Path -replace '/', '\'
    return (
        $normalizedPath -match '^C:\\FRAMETIME_CFG\\' -and
        $normalizedPath -notmatch '\\\.\.(\\|$)' -and
        $normalizedPath -match '\.ps1$'
    )
}

function Get-LegacyPhaseHandoffs {
    [CmdletBinding()]
    param()

    if (-not (Test-HostIsWindows)) { return @() }

    $registrations = @(
        [PSCustomObject]@{
            Key  = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce"
            Name = "*CS2_Phase2"
        }
        [PSCustomObject]@{
            Key  = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"
            Name = "CS2_OPTIMIZE_CS2_Phase3"
        }
    )
    $armed = [System.Collections.Generic.List[object]]::new()
    foreach ($registration in $registrations) {
        try {
            if (-not (Test-Path -LiteralPath $registration.Key -ErrorAction Stop)) { continue }
            $properties = Get-ItemProperty -LiteralPath $registration.Key -ErrorAction Stop
            if ($properties.PSObject.Properties[$registration.Name]) {
                $armed.Add($registration)
            }
        } catch {
            throw "Could not verify whether legacy phase handoff '$($registration.Name)' is armed: $_"
        }
    }
    return @($armed)
}

function Assert-NoLegacyPhaseHandoff {
    [CmdletBinding()]
    param()

    $armed = @(Get-LegacyPhaseHandoffs)
    if ($armed.Count -eq 0) { return }

    $names = @($armed | ForEach-Object { $_.Name }) -join ", "
    throw "A v2.3 phase handoff is still armed ($names). Complete or roll back v2.3 before starting frametime.cfg v3; v2.3 state and backups are not migrated."
}

function Get-PhaseRuntimePayloadRelativePaths {
    [CmdletBinding()]
    param()

    # This is intentionally explicit. Adding a runtime dependency requires a
    # reviewed manifest change rather than silently broadening privileged code.
    return @(
        "SafeMode-DriverClean.ps1",
        "PostReboot-Setup.ps1",
        "Guide-VideoSettings.ps1",
        "helpers.ps1",
        "config.env.ps1",
        "helpers/backup-restore.ps1",
        "helpers/benchmark-history.ps1",
        "helpers/debloat.ps1",
        "helpers/gpu-driver-clean.ps1",
        "helpers/hardware-detect.ps1",
        "helpers/logging.ps1",
        "helpers/msi-interrupts.ps1",
        "helpers/network-diagnostics.ps1",
        "helpers/nvidia-driver.ps1",
        "helpers/nvidia-drs.ps1",
        "helpers/nvidia-profile.ps1",
        "helpers/power-plan.ps1",
        "helpers/process-priority.ps1",
        "helpers/step-state.ps1",
        "helpers/storage-health.ps1",
        "helpers/system-utils.ps1",
        "helpers/tier-system.ps1",
        "cfgs/audio_lowlatency_001.cfg",
        "cfgs/audio_lowlatency_025.cfg",
        "cfgs/audio_stable.cfg",
        "cfgs/autoexec.cfg.example",
        "cfgs/debug_hud.cfg",
        "cfgs/debug_hud_off.cfg",
        "cfgs/net_bad.cfg",
        "cfgs/net_highping.cfg",
        "cfgs/net_stable.cfg",
        "cfgs/net_unstable.cfg",
        "cfgs/valve-latency-targets.json",
        "docs/video.txt",
        "docs/nvidia-drs-settings.md"
    )
}

function Get-PhaseRuntimeRoot {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$DestinationRoot)

    $pointerPath = Join-Path $DestinationRoot "runtime-current.json"
    if (-not (Test-Path -LiteralPath $pointerPath -PathType Leaf)) {
        # Backward-compatible fallback for payloads published before immutable
        # runtime generations were introduced.
        return (Join-Path $DestinationRoot "runtime")
    }

    try {
        $pointerItem = Get-Item -LiteralPath $pointerPath -Force -ErrorAction Stop
        if (($pointerItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "runtime pointer is a reparse point"
        }
        $pointer = Get-Content -LiteralPath $pointerPath -Raw -ErrorAction Stop |
            ConvertFrom-Json -ErrorAction Stop
        if ($pointer.schemaVersion -ne 1) { throw "unsupported pointer schema" }
        $relativePath = [string]$pointer.relativePath
        if ($relativePath -notmatch '^runtime-generations/[a-f0-9]{32}$') {
            throw "invalid generation path"
        }

        $destinationFullPath = [IO.Path]::GetFullPath($DestinationRoot).TrimEnd([char[]]@('\', '/'))
        $runtimeRoot = [IO.Path]::GetFullPath((Join-Path $DestinationRoot ($relativePath -replace '/', [IO.Path]::DirectorySeparatorChar)))
        if (-not $runtimeRoot.StartsWith("$destinationFullPath$([IO.Path]::DirectorySeparatorChar)", [StringComparison]::OrdinalIgnoreCase)) {
            throw "generation path escapes the work directory"
        }
        if (Test-Path -LiteralPath $runtimeRoot) {
            $runtimeItem = Get-Item -LiteralPath $runtimeRoot -Force -ErrorAction Stop
            if (($runtimeItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "runtime generation is a reparse point"
            }
        }
        return $runtimeRoot
    } catch {
        throw "Phase runtime pointer is invalid: $_"
    }
}

function Get-PhaseRuntimePayloadContractId {
    [CmdletBinding()]
    param()

    $contractText = (@(Get-PhaseRuntimePayloadRelativePaths | Sort-Object) -join "`n")
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return (([BitConverter]::ToString($sha256.ComputeHash([Text.Encoding]::UTF8.GetBytes($contractText))) -replace '-', '').ToLowerInvariant())
    } finally {
        $sha256.Dispose()
    }
}

function Get-PhaseRuntimeRelativePath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][string]$Path
    )

    $rootPath = [IO.Path]::GetFullPath($Root).TrimEnd([char[]]@('\', '/'))
    $fullPath = [IO.Path]::GetFullPath($Path)
    if (-not $fullPath.StartsWith($rootPath, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Runtime path escapes the payload root: $Path"
    }
    return (($fullPath.Substring($rootPath.Length) -replace '^[\\/]+', '') -replace '\\', '/')
}

function Test-PhaseRuntimePayload {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$RuntimeRoot)

    $manifestPath = Join-Path $RuntimeRoot "runtime-manifest.json"
    try {
        if (-not (Test-Path -LiteralPath $RuntimeRoot -PathType Container)) { throw "Runtime directory is missing." }
        if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw "Runtime manifest is missing." }
        $manifest = Get-Content -LiteralPath $manifestPath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
        if ($manifest.schemaVersion -ne 1) { throw "Unsupported runtime manifest schema." }
        if ($manifest.payloadContract -ne (Get-PhaseRuntimePayloadContractId)) { throw "Runtime manifest payload contract is invalid." }

        $expectedPaths = @(Get-PhaseRuntimePayloadRelativePaths | Sort-Object)
        $manifestFiles = @($manifest.files)
        $manifestPaths = @($manifestFiles | ForEach-Object { [string]$_.path } | Sort-Object)
        if (@($manifestPaths | Group-Object | Where-Object Count -gt 1).Count -gt 0) { throw "Runtime manifest contains duplicate paths." }
        if (@(Compare-Object -ReferenceObject $expectedPaths -DifferenceObject $manifestPaths).Count -gt 0) {
            throw "Runtime manifest does not match the fixed payload file set."
        }

        $actualPaths = @(Get-ChildItem -LiteralPath $RuntimeRoot -File -Recurse -Force -ErrorAction Stop |
            Where-Object { $_.FullName -ne $manifestPath } |
            ForEach-Object { Get-PhaseRuntimeRelativePath -Root $RuntimeRoot -Path $_.FullName } |
            Sort-Object)
        if (@(Compare-Object -ReferenceObject $expectedPaths -DifferenceObject $actualPaths).Count -gt 0) {
            throw "Published runtime contains missing or extra files."
        }

        foreach ($entry in $manifestFiles) {
            $relativePath = [string]$entry.path
            $expectedHash = [string]$entry.sha256
            if ($expectedHash -notmatch '^[A-Fa-f0-9]{64}$') { throw "Invalid hash in runtime manifest: $relativePath" }
            $filePath = Join-Path $RuntimeRoot ($relativePath -replace '/', [IO.Path]::DirectorySeparatorChar)
            $actualHash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256 -ErrorAction Stop).Hash
            if ($actualHash -ne $expectedHash) { throw "Runtime hash mismatch: $relativePath" }
        }
        return [PSCustomObject]@{ Valid = $true; Status = "Success"; Message = "Runtime payload manifest verified." }
    } catch {
        return [PSCustomObject]@{ Valid = $false; Status = "Failed"; Message = "Runtime payload validation failed: $_" }
    }
}

function Write-PhaseRuntimePublishLockRecord {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][IO.Stream]$Stream,
        [Parameter(Mandatory)][string]$Record
    )

    $recordBytes = [Text.Encoding]::UTF8.GetBytes($Record)
    $Stream.SetLength(0)
    $Stream.Position = 0
    $Stream.Write($recordBytes, 0, $recordBytes.Length)
    $Stream.Flush()
}

function Initialize-PhaseRuntimePublishLockOwner {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][IO.Stream]$Stream,
        [Parameter(Mandatory)][string]$OwnerRecord
    )

    Write-PhaseRuntimePublishLockRecord -Stream $Stream -Record $OwnerRecord
}

function Remove-FailedPhaseRuntimePublishLockInitialization {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)][string]$LockPath,
        [Parameter(Mandatory)][string]$ExpectedOwnerRecord,
        [Parameter(Mandatory)][string]$ExpectedCreationUtc,
        [Parameter(Mandatory)][string]$OwnerStartUtc,
        [Parameter(Mandatory)][string]$OwnerProcessName
    )

    $cleanupToken = [Guid]::NewGuid().ToString("N")
    $probe = $null
    try {
        $probe = [IO.File]::Open($LockPath, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        $actualCreationUtc = [IO.File]::GetCreationTimeUtc($LockPath).ToString("o")
        $reader = New-Object IO.StreamReader($probe, [Text.Encoding]::UTF8, $false, 1024, $true)
        try { $partialRecord = $reader.ReadToEnd() } finally { $reader.Dispose() }
        if ($actualCreationUtc -ne $ExpectedCreationUtc -or
            -not $ExpectedOwnerRecord.StartsWith($partialRecord, [StringComparison]::Ordinal)) {
            return $false
        }

        $cleanupRecord = [PSCustomObject]@{
            token = $cleanupToken
            state = "cleanup"
            pid = $PID
            processStartUtc = $OwnerStartUtc
            processName = $OwnerProcessName
        } | ConvertTo-Json -Compress
        Write-PhaseRuntimePublishLockRecord -Stream $probe -Record $cleanupRecord
    } catch {
        return $false
    } finally {
        if ($probe) { $probe.Dispose() }
    }

    try {
        $claimedData = Get-Content -LiteralPath $LockPath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
        if ($claimedData.token -ne $cleanupToken -or $claimedData.state -ne "cleanup") { return $false }
        if (-not $PSCmdlet.ShouldProcess($LockPath, "Remove failed runtime publication lock")) {
            return $false
        }
        Remove-Item -LiteralPath $LockPath -Force -ErrorAction Stop
        return $true
    } catch {
        return $false
    }
}

function Enter-PhaseRuntimePublishLock {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$DestinationRoot)

    $lockPath = Join-Path $DestinationRoot ".runtime-publish.lock"
    $ownerToken = [Guid]::NewGuid().ToString("N")
    $ownerProcess = Get-Process -Id $PID -ErrorAction Stop
    $ownerStartUtc = $ownerProcess.StartTime.ToUniversalTime().ToString("o")
    $ownerProcessName = $ownerProcess.ProcessName
    $ownerRecord = [PSCustomObject]@{
        token = $ownerToken
        state = "owned"
        pid = $PID
        processStartUtc = $ownerStartUtc
        processName = $ownerProcessName
    } | ConvertTo-Json -Compress

    for ($attempt = 0; $attempt -lt 2; $attempt++) {
        $stream = $null
        try {
            $stream = [IO.File]::Open($lockPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        } catch [IO.IOException] {
            # An active publisher holds FileShare.None, so this probe succeeds
            # only after a crashed owner released its OS handle.
            $cleanupToken = $null
            try {
                $probe = [IO.File]::Open($lockPath, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
                try {
                    $reader = New-Object IO.StreamReader($probe, [Text.Encoding]::UTF8, $false, 1024, $true)
                    try { $rawLockData = $reader.ReadToEnd() } finally { $reader.Dispose() }
                    $lockData = $null
                    try { $lockData = $rawLockData | ConvertFrom-Json -ErrorAction Stop } catch { $lockData = $null }
                    $lockPid = 0
                    $liveOwner = $false
                    if ($lockData -and $lockData.state -eq "owned" -and [int]::TryParse([string]$lockData.pid, [ref]$lockPid)) {
                        $process = Get-Process -Id $lockPid -ErrorAction SilentlyContinue
                        if ($process -and $process.ProcessName -match '^(?:powershell|pwsh|powershell_ise)$' -and
                            $lockData.processName -eq $process.ProcessName -and $lockData.processStartUtc) {
                            try {
                                $actualStartUtc = $process.StartTime.ToUniversalTime().ToString("o")
                                $expectedStartUtc = ([DateTime]::Parse([string]$lockData.processStartUtc)).ToUniversalTime().ToString("o")
                                $liveOwner = $actualStartUtc -eq $expectedStartUtc
                            } catch {
                                $liveOwner = $false
                            }
                        }
                    }
                    if ($liveOwner) { throw "Phase runtime publication is already in progress (owner PID $lockPid)." }

                    # Claim either a valid stale record or a corrupt unlocked
                    # record while its path is exclusively open.
                    $cleanupToken = [Guid]::NewGuid().ToString("N")
                    $cleanupRecord = [PSCustomObject]@{
                        token = $cleanupToken
                        state = "cleanup"
                        pid = $PID
                        processStartUtc = $ownerStartUtc
                        processName = $ownerProcessName
                    } | ConvertTo-Json -Compress
                    Write-PhaseRuntimePublishLockRecord -Stream $probe -Record $cleanupRecord
                } finally {
                    $probe.Dispose()
                }
                $claimedData = Get-Content -LiteralPath $lockPath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
                if ($claimedData.token -ne $cleanupToken -or $claimedData.state -ne "cleanup") {
                    throw "Stale publication lock ownership changed during cleanup."
                }
                Remove-Item -LiteralPath $lockPath -Force -ErrorAction Stop
            } catch {
                throw "Phase runtime publication is already in progress or its lock cannot be recovered: $_"
            }
        }

        if ($stream) {
            $createdUtc = $null
            try {
                $createdUtc = [IO.File]::GetCreationTimeUtc($lockPath).ToString("o")
                Initialize-PhaseRuntimePublishLockOwner -Stream $stream -OwnerRecord $ownerRecord
                return [PSCustomObject]@{ Path = $lockPath; Token = $ownerToken; Stream = $stream }
            } catch {
                $initializationError = $_
                $stream.Dispose()
                $stream = $null
                $removed = $false
                if ($createdUtc) {
                    $removed = Remove-FailedPhaseRuntimePublishLockInitialization `
                        -LockPath $lockPath `
                        -ExpectedOwnerRecord $ownerRecord `
                        -ExpectedCreationUtc $createdUtc `
                        -OwnerStartUtc $ownerStartUtc `
                        -OwnerProcessName $ownerProcessName
                }
                $cleanupMessage = if ($removed) { "The exact failed lock was removed." } else { "The failed lock could not be proven safe to remove." }
                throw "Could not initialize the Phase runtime publication lock: $initializationError $cleanupMessage"
            }
        }
    }
    throw "Could not acquire the Phase runtime publication lock."
}

function Exit-PhaseRuntimePublishLock {
    [CmdletBinding()]
    param([Parameter(Mandatory)]$Lock)

    if ($Lock.Stream) { $Lock.Stream.Dispose() }
    try {
        if (Test-Path -LiteralPath $Lock.Path -PathType Leaf) {
            $lockData = Get-Content -LiteralPath $Lock.Path -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
            if ($lockData.token -eq $Lock.Token -and $lockData.pid -eq $PID -and $lockData.state -eq "owned") {
                Remove-Item -LiteralPath $Lock.Path -Force -ErrorAction Stop
            }
        }
    } catch {
        Write-Warn "Could not release Phase runtime publication lock '$($Lock.Path)': $_"
    }
}

function Remove-LegacyPhaseRuntimePayload {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)][string]$SourceRoot,
        [Parameter(Mandatory)][string]$DestinationRoot
    )

    if ([IO.Path]::GetFullPath($SourceRoot) -eq [IO.Path]::GetFullPath($DestinationRoot)) { return }
    foreach ($legacyPath in @(
        "SafeMode-DriverClean.ps1", "PostReboot-Setup.ps1", "Guide-VideoSettings.ps1",
        "helpers.ps1", "config.env.ps1", "helpers", "cfgs", "docs"
    )) {
        $legacyFullPath = Join-Path $DestinationRoot $legacyPath
        if ((Test-Path -LiteralPath $legacyFullPath) -and
            $PSCmdlet.ShouldProcess($legacyFullPath, "Remove legacy Phase runtime payload")) {
            Remove-Item -LiteralPath $legacyFullPath -Recurse -Force -ErrorAction Stop
        }
    }
}

function Copy-PhaseRuntimePayload {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)][string]$SourceRoot,
        [Parameter(Mandatory)][string]$DestinationRoot
    )

    if ($SCRIPT:DryRun) { return $null }
    if (-not $PSCmdlet.ShouldProcess($DestinationRoot, "Publish immutable Phase 2/3 runtime generation")) {
        return $null
    }

    Ensure-SecureWorkDir -Path $DestinationRoot
    $publishLock = Enter-PhaseRuntimePublishLock -DestinationRoot $DestinationRoot
    $generationId = [Guid]::NewGuid().ToString("N")
    $generationsRoot = Join-Path $DestinationRoot "runtime-generations"
    $stageRoot = Join-Path $DestinationRoot (".runtime-staging-{0}" -f $generationId)
    $runtimeRoot = Join-Path $generationsRoot $generationId
    $pointerPath = Join-Path $DestinationRoot "runtime-current.json"
    $generationPublished = $false
    $pointerCommitted = $false

    try {
        Ensure-Dir $generationsRoot
        Set-SecureAcl -Path $generationsRoot -Required
        Ensure-Dir $stageRoot
        $manifestFiles = @()
        foreach ($relativePath in (Get-PhaseRuntimePayloadRelativePaths)) {
            $sourcePath = Join-Path $SourceRoot ($relativePath -replace '/', [IO.Path]::DirectorySeparatorChar)
            if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) { throw "Required runtime file missing: $relativePath" }
            $stagePath = Join-Path $stageRoot ($relativePath -replace '/', [IO.Path]::DirectorySeparatorChar)
            Ensure-Dir (Split-Path -Path $stagePath -Parent)
            Copy-Item -LiteralPath $sourcePath -Destination $stagePath -Force -ErrorAction Stop
            Set-SecureAcl -Path $stagePath -Required
            $manifestFiles += [PSCustomObject]@{
                path = $relativePath
                sha256 = (Get-FileHash -LiteralPath $stagePath -Algorithm SHA256 -ErrorAction Stop).Hash
            }
        }
        $manifest = [PSCustomObject]@{
            schemaVersion = 1
            payloadContract = Get-PhaseRuntimePayloadContractId
            createdUtc = [DateTime]::UtcNow.ToString("o")
            files = @($manifestFiles)
        }
        Save-JsonAtomic -Data $manifest -Path (Join-Path $stageRoot "runtime-manifest.json")
        Set-SecureAcl -Path $stageRoot -Required

        $stageValidation = Test-PhaseRuntimePayload -RuntimeRoot $stageRoot
        if (-not $stageValidation.Valid) { throw $stageValidation.Message }

        # A generation is renamed only into a new, never-before-used path. Any
        # RunOnce value already armed against an older generation remains valid
        # across crashes, retries, and later publications.
        Move-Item -LiteralPath $stageRoot -Destination $runtimeRoot -ErrorAction Stop
        $generationPublished = $true
        $publishedValidation = Test-PhaseRuntimePayload -RuntimeRoot $runtimeRoot
        if (-not $publishedValidation.Valid) { throw $publishedValidation.Message }

        # Save-JsonAtomic is the publication commit point. Before it, readers use
        # the previous complete pointer; after it, they use this verified immutable
        # generation. There is no interval where the armed target is absent.
        $pointer = [PSCustomObject]@{
            schemaVersion = 1
            relativePath = "runtime-generations/$generationId"
            publishedUtc = [DateTime]::UtcNow.ToString("o")
        }
        Save-JsonAtomic -Data $pointer -Path $pointerPath
        $pointerCommitted = $true
        Set-SecureAcl -Path $pointerPath -Required

        try {
            Remove-LegacyPhaseRuntimePayload -SourceRoot $SourceRoot -DestinationRoot $DestinationRoot
        } catch {
            Write-Warn "Verified runtime published, but legacy payload cleanup was incomplete: $_"
        }
        Write-OK "Published and verified Phase 2/3 runtime payload: $runtimeRoot"
        return $runtimeRoot
    } catch {
        $publicationError = $_
        if (-not $pointerCommitted) {
            foreach ($uncommittedPath in @($stageRoot, $(if ($generationPublished) { $runtimeRoot }))) {
                if ($uncommittedPath -and (Test-Path -LiteralPath $uncommittedPath)) {
                    try {
                        Remove-Item -LiteralPath $uncommittedPath -Recurse -Force -ErrorAction Stop
                    } catch {
                        throw "Phase runtime publication failed: $($publicationError.Exception.Message) Uncommitted generation cleanup also failed for '$uncommittedPath': $($_.Exception.Message)"
                    }
                }
            }
        }
        throw $publicationError
    } finally {
        Exit-PhaseRuntimePublishLock -Lock $publishLock
    }
}

function Test-Phase1SafeModeReady {
    [CmdletBinding()]
    param($State)

    return (
        $null -ne $State -and
        $State.PSObject.Properties['phase1SafeModeReady'] -and
        $State.phase1SafeModeReady -eq $true
    )
}

function Set-Phase1SafeModeReadyFlag {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)][string]$Path,
        [bool]$Ready = $true
    )

    if (-not (Test-Path $Path)) {
        throw "Settings file not found at '$Path' - run Phase 1 first (START.bat -> [1])."
    }

    $state = Get-Content $Path -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    $state | Add-Member -NotePropertyName "phase1SafeModeReady" -NotePropertyValue $Ready -Force
    if (-not $PSCmdlet.ShouldProcess($Path, "Persist Phase 1 Safe Mode readiness flag")) { return $state }
    Ensure-SecureWorkDir -Path (Split-Path $Path -Parent)
    Save-JsonAtomic -Data $state -Path $Path
    Set-SecureAcl -Path $Path -Required
    return $state
}

function Get-StateStringValue {
    [CmdletBinding()]
    param(
        $State,
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Default,
        [string[]]$AllowedValues = @()
    )

    if (-not $State -or -not $State.PSObject.Properties[$Name]) {
        Write-DebugLog "State field '$Name' missing - defaulting to $Default"
        return $Default
    }

    $value = $State.$Name
    if ($null -eq $value -or $value -is [array] -or $value -is [hashtable] -or $value -is [pscustomobject]) {
        Write-DebugLog "State field '$Name' has invalid shape - defaulting to $Default"
        return $Default
    }

    $stringValue = ([string]$value).Trim().ToUpperInvariant()
    if ([string]::IsNullOrWhiteSpace($stringValue)) {
        Write-DebugLog "State field '$Name' is empty - defaulting to $Default"
        return $Default
    }

    if ($AllowedValues.Count -gt 0 -and $stringValue -notin $AllowedValues) {
        Write-DebugLog "State field '$Name' has unsupported value '$stringValue' - defaulting to $Default"
        return $Default
    }

    return $stringValue
}

function Get-PhaseGpuInput {
    <#
    .SYNOPSIS
    Reads the required GPU branch from persisted or injected phase state.

    .DESCRIPTION
    The phase scripts accept only the four selections created by Phase 1. A
    missing, compound, or unsupported value returns null so that callers can
    stop before vendor-specific operations. In particular, invalid state must
    never be interpreted as an NVIDIA selection.
    #>
    [CmdletBinding()]
    param([AllowNull()]$State)

    $gpuInput = Get-StateStringValue `
        -State $State `
        -Name "gpuInput" `
        -Default "INVALID" `
        -AllowedValues @("1", "2", "3", "4")
    if ($gpuInput -notin @("1", "2", "3", "4")) { return $null }
    return $gpuInput
}

function Get-ModeForProfile {
    [CmdletBinding()]
    param(
        [Alias("Profile")]
        [string]$SuiteProfile = "RECOMMENDED",
        [switch]$DryRun
    )

    if ($DryRun) { return "DRY-RUN" }

    switch (([string]$SuiteProfile).Trim().ToUpperInvariant()) {
        "SAFE"        { "AUTO" }
        "RECOMMENDED" { "AUTO" }
        "COMPETITIVE" { "CONTROL" }
        "CUSTOM"      { "INFORMED" }
        "YOLO"        { "YOLO" }
        default       { "AUTO" }
    }
}

function Set-ScriptStateFromStateObject {
    [CmdletBinding(SupportsShouldProcess)]
    param($State)

    $stateProfile = Get-StateStringValue `
        -State $State `
        -Name "profile" `
        -Default "RECOMMENDED" `
        -AllowedValues @("SAFE", "RECOMMENDED", "COMPETITIVE", "CUSTOM", "YOLO")
    $defaultMode = Get-ModeForProfile -Profile $stateProfile
    $stateMode = Get-StateStringValue `
        -State $State `
        -Name "mode" `
        -Default $defaultMode `
        -AllowedValues @("AUTO", "CONTROL", "INFORMED", "YOLO", "DRY-RUN")
    $stateLogLevel = Get-StateStringValue `
        -State $State `
        -Name "logLevel" `
        -Default "NORMAL" `
        -AllowedValues @("MINIMAL", "NORMAL", "VERBOSE")
    if (-not $PSCmdlet.ShouldProcess("script runtime state", "Apply loaded mode/profile/log-level defaults")) { return }
    $SCRIPT:Mode = $stateMode
    $SCRIPT:LogLevel = $stateLogLevel
    $SCRIPT:Profile = $stateProfile
    $SCRIPT:DryRun = ($SCRIPT:Mode -eq "DRY-RUN")
}

function Load-State {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Path,
        [switch]$ReadOnly
    )

    if (-not $ReadOnly) {
        Ensure-SecureWorkDir -Path (Split-Path $Path -Parent)
    }
    if (-not (Test-Path -LiteralPath $Path)) { throw "Settings file not found at '$Path' - run Phase 1 first (START.bat -> [1])." }
    # -Raw ensures the entire file is read as a single string (consistent with backup-restore.ps1).
    # Without -Raw, multi-line JSON could be split into a string array, causing ConvertFrom-Json to
    # receive individual lines instead of a complete JSON document.
    $raw = Get-Content -LiteralPath $Path -Raw -ErrorAction Stop
    try {
        $s = $raw | ConvertFrom-Json
    } catch {
        if ($ReadOnly) {
            throw "State file corrupt - read-only mode left it unchanged."
        }
        $corruptPath = "$Path.corrupt.$(Get-Date -Format 'yyyyMMdd_HHmmss')"
        try { Copy-Item -LiteralPath $Path -Destination $corruptPath -Force -ErrorAction Stop } catch { Write-DebugLog "Could not preserve corrupted state file." }
        Write-Warn "State file corrupt - preserved as $corruptPath"
        throw "State file corrupt - preserved as $corruptPath"
    }
    Set-ScriptStateFromStateObject -State $s
    return $s
}

function Initialize-ScriptDefaults {
    <#  Soft state loader for entry-point scripts (Cleanup, FpsCap, Verify).
        Loads state.json if present, otherwise sets safe defaults. Never exits.  #>
    Ensure-SecureWorkDir -Path (Split-Path $CFG_StateFile -Parent)
    if (Test-Path $CFG_StateFile) {
        try {
            # -ErrorAction Stop ensures Get-Content failures throw into the catch block.
            $st = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json
            Set-ScriptStateFromStateObject -State $st
        } catch {
            $SCRIPT:Mode = "CONTROL"; $SCRIPT:LogLevel = "NORMAL"; $SCRIPT:Profile = "RECOMMENDED"; $SCRIPT:DryRun = $false
        }
    } else {
        $SCRIPT:Mode = "CONTROL"; $SCRIPT:LogLevel = "NORMAL"; $SCRIPT:Profile = "RECOMMENDED"; $SCRIPT:DryRun = $false
    }
}

function New-WriteOperationResult {
    param(
        [Parameter(Mandatory)]
        [ValidateSet("Success", "Failed", "Skipped", "DryRun")]
        [string]$Status,

        [string]$Message = ""
    )

    return [PSCustomObject]@{
        Status  = $Status
        Applied = ($Status -eq "Success")
        Message = $Message
    }
}

function Set-RunOnce {
    [CmdletBinding(SupportsShouldProcess)]
    param([string]$name, [string]$scriptPath, [switch]$SafeMode, [switch]$PassThru)
    # SECURITY: Validate RunOnce name - alphanumeric + underscore only.
    # Prevents injection into the HKLM RunOnce key namespace.
    if ($name -notmatch '^[a-zA-Z0-9_]+$') {
        $message = "Set-RunOnce: invalid name '$name' - rejected (security: registry injection prevention)"
        Write-Warn $message
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message $message) }
        return
    }
    # Safe Mode handoffs use both documented RunOnce prefixes. '*' permits
    # execution in Safe Mode, and '!' defers value deletion until the command
    # has run. Normal-mode handoffs use a durable HKCU Run bootstrap and remove
    # it only after Phase 3 completes successfully.
    $registrationName = if ($SafeMode) { "*!$name" } else { "FRAMETIME_CFG_$name" }
    # SECURITY: Validate script path - must be under C:\FRAMETIME_CFG\ and end in .ps1.
    # If an attacker could set $scriptPath to an arbitrary location, the normal
    # handoff would execute it through a highest-privilege task.
    $normalizedPath = $scriptPath -replace '/', '\'
    if (-not (Test-TrustedSuiteScriptPath -Path $normalizedPath)) {
        $message = "Set-RunOnce: script path must be under C:\FRAMETIME_CFG\ and end in .ps1 - rejected: $scriptPath"
        Write-Warn $message
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message $message) }
        return
    }
    if ($normalizedPath -notmatch '^C:\\FRAMETIME_CFG\\(?:(?:runtime\\)|(?:runtime-generations\\[a-fA-F0-9]{32}\\))?[a-zA-Z0-9_.-]+\.ps1$') {
        $message = "Set-RunOnce: phase handoff path contains unsupported characters: $scriptPath"
        Write-Warn $message
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message $message) }
        return
    }

    if ($SCRIPT:DryRun) {
        Write-ConsoleLine "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Would register phase handoff: $registrationName -> $scriptPath" -ForegroundColor Magenta
        if ($PassThru) { return (New-WriteOperationResult -Status "DryRun" -Message "Phase handoff previewed: $registrationName -> $normalizedPath") }
        return
    }
    # Validate target script exists before registering - a RunOnce pointing to a missing
    # file would silently fail on next boot, leaving Phase 3 unexecuted with no error.
    if (-not (Test-Path $scriptPath)) {
        $message = "RunOnce target does not exist: $scriptPath"
        Write-Warn $message
        Write-ConsoleLine "  $([char]0x2139) What to do: Phase 3 will NOT auto-start on next boot." -ForegroundColor Cyan
        Write-ConsoleLine "    After rebooting, launch Phase 3 manually: START.bat -> [P]" -ForegroundColor Cyan
        if ($PassThru) { return (New-WriteOperationResult -Status "Failed" -Message $message) }
        return
    }
    $allowedPolicies = @("Bypass", "RemoteSigned", "AllSigned")
    $executionPolicy = [string]$CFG_RunOnceExecutionPolicy
    if ($executionPolicy -eq "Undefined") {
        $message = "Set-RunOnce: CFG_RunOnceExecutionPolicy 'Undefined' is unsupported on client systems due to policy precedence and GPOs; use one of: $($allowedPolicies -join ', ')"
        Write-Warn $message
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message $message) }
        return
    }
    if ($executionPolicy -notin $allowedPolicies) {
        $message = "Set-RunOnce: invalid CFG_RunOnceExecutionPolicy '$executionPolicy' - expected one of: $($allowedPolicies -join ', ')"
        Write-Warn $message
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message $message) }
        return
    }
    $directCommand = "powershell.exe -NoProfile -ExecutionPolicy $executionPolicy -WindowStyle Normal -File `"$normalizedPath`""
    $elevatedArguments = "-NoProfile -ExecutionPolicy $executionPolicy -WindowStyle Normal -File $normalizedPath"
    $bootstrap = "Start-Process -FilePath 'powershell.exe' -Verb RunAs -Wait -ArgumentList '$elevatedArguments'"
    $elevatedCommand = "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Normal -Command `"$bootstrap`""
    if (-not $SafeMode -and $elevatedCommand.Length -gt 260) {
        $message = "Set-RunOnce: generated phase handoff exceeds the Windows Run command-line limit"
        Write-Warn $message
        if ($PassThru) { return (New-WriteOperationResult -Status "Failed" -Message $message) }
        return
    }
    if (-not $PSCmdlet.ShouldProcess($registrationName, "Register phase handoff for $normalizedPath")) {
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message "Phase handoff skipped: $registrationName -> $normalizedPath") }
        return
    }
    try {
        Ensure-SecureWorkDir -Path $CFG_WorkDir
        Set-SecureAcl -Path $scriptPath -Required
        if ($SafeMode) {
            Set-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce" -Name $registrationName -Value $directCommand -ErrorAction Stop
        } else {
            $runKey = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"
            if (-not (Test-Path $runKey)) { New-Item -Path $runKey -Force -ErrorAction Stop | Out-Null }
            Set-ItemProperty $runKey -Name $registrationName -Value $elevatedCommand -ErrorAction Stop
        }
        Write-OK "Phase handoff: $registrationName -> $normalizedPath"
        if ($PassThru) { return (New-WriteOperationResult -Status "Success" -Message "Phase handoff set: $registrationName -> $normalizedPath") }
    } catch {
        $message = "Failed to register phase handoff '$registrationName': $_"
        Write-Err $message
        Write-ConsoleLine "  $([char]0x2139) What to do: Phase 3 will NOT auto-start after reboot." -ForegroundColor Cyan
        Write-ConsoleLine "    After rebooting, run Phase 3 manually: START.bat -> [P]" -ForegroundColor Cyan
        if ($PassThru) { return (New-WriteOperationResult -Status "Failed" -Message $message) }
    }
}

function Remove-PhaseHandoff {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)][ValidatePattern('^[a-zA-Z0-9_]+$')][string]$Name,
        [switch]$SafeMode,
        [switch]$PassThru
    )

    # Safe Mode handoffs are stored in HKLM RunOnce with '*' and '!' prefixes.
    # Normal Phase 3 handoffs remain in HKCU Run until completion removes them.
    $registrationName = if ($SafeMode) { "*!$Name" } else { "FRAMETIME_CFG_$Name" }
    $runKey = if ($SafeMode) {
        "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce"
    } else {
        "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"
    }
    if ($SCRIPT:DryRun) {
        if ($PassThru) { return (New-WriteOperationResult -Status "DryRun" -Message "Phase handoff removal previewed: $registrationName") }
        return
    }
    if (-not $PSCmdlet.ShouldProcess($registrationName, "Remove phase handoff")) {
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message "Phase handoff removal skipped: $registrationName") }
        return
    }
    try {
        $keyExists = Test-Path -LiteralPath $runKey -ErrorAction Stop
        if (-not $keyExists) {
            Write-DebugLog "Phase handoff '$registrationName' is already absent (registry key missing)."
            if ($PassThru) { return (New-WriteOperationResult -Status "Success" -Message "Phase handoff already absent: $registrationName") }
            return
        }

        $keyProperties = Get-ItemProperty -LiteralPath $runKey -ErrorAction Stop
        if ($null -eq $keyProperties) { throw "Registry query returned no result for '$runKey'." }
        $valueExists = $null -ne $keyProperties.PSObject.Properties[$registrationName]
        if (-not $valueExists) {
            Write-DebugLog "Phase handoff '$registrationName' is already absent."
            if ($PassThru) { return (New-WriteOperationResult -Status "Success" -Message "Phase handoff already absent: $registrationName") }
            return
        }

        Remove-ItemProperty -LiteralPath $runKey -Name $registrationName -Force -ErrorAction Stop

        $keyExistsAfterRemoval = Test-Path -LiteralPath $runKey -ErrorAction Stop
        if ($keyExistsAfterRemoval) {
            $postRemovalProperties = Get-ItemProperty -LiteralPath $runKey -ErrorAction Stop
            if ($null -eq $postRemovalProperties) { throw "Post-delete registry query returned no result for '$runKey'." }
            if ($null -ne $postRemovalProperties.PSObject.Properties[$registrationName]) {
                throw "Phase handoff remains present after deletion."
            }
        }

        Write-DebugLog "Removed phase handoff '$registrationName' and verified its absence."
        if ($PassThru) { return (New-WriteOperationResult -Status "Success" -Message "Phase handoff removed and verified absent: $registrationName") }
    } catch {
        $message = "Failed to remove phase handoff '$registrationName' or verify its absence: $_"
        Write-Err $message
        if ($PassThru) { return (New-WriteOperationResult -Status "Failed" -Message $message) }
    }
}

function Enable-Phase2SafeModeTransaction {
    <#
    .SYNOPSIS
    Prepares the only valid Phase 2 reboot handoff as one ordered transaction.

    Payload and RunOnce must exist before BCD is changed. After a failed BCD
    write or verification, SafeBoot is cleared and verified before RunOnce is
    disarmed. If clearance cannot be proved, the recovery handoff is retained
    and re-armed. The readiness marker is persisted only after live BCD
    verification succeeds.
    #>
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)][string]$SourceRoot,
        [Parameter(Mandatory)][string]$DestinationRoot,
        [Parameter(Mandatory)][string]$StatePath,
        [string]$Why = "Safe Mode for GPU driver clean"
    )

    $phase2Name = "FRAMETIME_Phase2"
    if ($SCRIPT:DryRun) {
        Write-ConsoleLine "  [DRY-RUN] Would publish and verify the immutable Phase 2/3 runtime payload." -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN] Would register the Safe Mode handoff: $phase2Name." -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN] Would set and verify: bcdedit /set safeboot minimal ($Why)." -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN] Would persist the verified Phase 1 readiness marker." -ForegroundColor Magenta
        return [PSCustomObject]@{
            Status = "DryRun"; Applied = $false; Verified = $false
            Message = "Phase 2 Safe Mode transaction previewed; no reboot handoff was armed."
        }
    }
    if (-not $PSCmdlet.ShouldProcess($DestinationRoot, "Publish payload and arm verified Phase 2 Safe Mode transaction")) {
        return [PSCustomObject]@{
            Status = "Skipped"; Applied = $false; Verified = $false
            Message = "Phase 2 Safe Mode transaction skipped; no reboot handoff was armed."
        }
    }

    $runtimeRoot = $null
    try {
        $runtimeRoot = Copy-PhaseRuntimePayload -SourceRoot $SourceRoot -DestinationRoot $DestinationRoot
        if ([string]::IsNullOrWhiteSpace([string]$runtimeRoot)) {
            throw "Runtime publisher did not return a committed generation."
        }
    } catch {
        return [PSCustomObject]@{
            Status = "Failed"; Applied = $false; Verified = $false
            Message = "Phase 2 payload preparation failed; Safe Mode was not armed: $_"
        }
    }
    $phase2Script = Join-Path $runtimeRoot "SafeMode-DriverClean.ps1"

    $handoffResult = Set-RunOnce $phase2Name $phase2Script -SafeMode -PassThru
    if (-not $handoffResult.Applied) {
        return [PSCustomObject]@{
            Status = "Failed"; Applied = $false; Verified = $false
            Message = "Phase 2 RunOnce registration failed; Safe Mode was not armed. $($handoffResult.Message)"
        }
    }

    $bootResult = $null
    $bootVerified = $false
    $bootFailureDetail = ""
    try {
        $bootResult = Set-BootConfig "safeboot" "minimal" $Why -PassThru
        $bootVerified = $bootResult.Applied -and (Test-BootConfigSet "safeboot")
        $bootFailureDetail = $bootResult.Message
    } catch {
        $bootFailureDetail = "Safe Mode setup or live verification raised an error: $_"
    }
    if (-not $bootVerified) {
        return (Undo-Phase2SafeModeTransaction -Phase2Script $phase2Script -FailureMessage "Safe Mode boot flag could not be set and verified. $bootFailureDetail")
    }

    try {
        Set-Phase1SafeModeReadyFlag -Path $StatePath | Out-Null
    } catch {
        return (Undo-Phase2SafeModeTransaction -Phase2Script $phase2Script -FailureMessage "Safe Mode was verified but the Phase 1 readiness marker could not be saved; reboot is blocked. $_")
    }

    return [PSCustomObject]@{
        Status = "Success"; Applied = $true; Verified = $true
        Message = "Phase 2 Safe Mode transaction is ready and verified."
    }
}

function Undo-Phase2SafeModeTransaction {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Phase2Script,
        [Parameter(Mandatory)][string]$FailureMessage
    )

    # A bcdedit /set command can partially succeed even when its exit status or
    # subsequent verification fails. Prove SafeBoot is clear before disarming
    # the recovery RunOnce; otherwise retain/re-arm it for an accidental reboot.
    try {
        $clearResult = Clear-SafeBootVerified
    } catch {
        $clearResult = [PSCustomObject]@{ Verified = $false; Message = "Safe Mode rollback raised an error: $_" }
    }
    if ($clearResult.Verified) {
        $disarmResult = Remove-PhaseHandoff -Name "FRAMETIME_Phase2" -SafeMode -PassThru
        $cleanupMessage = if ($disarmResult.Applied) {
            "Safe Mode was cleared and verified before the Phase 2 handoff was disarmed."
        } else {
            'Safe Mode was cleared, but Phase 2 handoff removal failed. Remove it manually: reg delete "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce" /v "*!FRAMETIME_Phase2" /f'
        }
        return [PSCustomObject]@{
            Status = "Failed"; Applied = $false; Verified = $false; SafeBootCleared = $true
            RecoveryHandoffApplied = (-not $disarmResult.Applied)
            Message = "$FailureMessage $cleanupMessage"
        }
    }

    try {
        $rearmResult = Set-RunOnce "FRAMETIME_Phase2" $Phase2Script -SafeMode -PassThru
        if ($null -eq $rearmResult) { throw "Phase 2 handoff registration returned no result." }
    } catch {
        $rearmResult = [PSCustomObject]@{ Applied = $false; Message = "Phase 2 handoff re-arm raised an error: $_" }
    }
    $handoffMessage = if ($rearmResult.Applied) {
        "The Phase 2 recovery handoff remains armed for an accidental reboot."
    } else {
        "The Phase 2 recovery handoff could not be re-armed: $($rearmResult.Message)"
    }
    return [PSCustomObject]@{
        Status = "Failed"; Applied = $false; Verified = $false; SafeBootCleared = $false
        RecoveryHandoffApplied = $rearmResult.Applied
        Message = "$FailureMessage CRITICAL: Safe Mode could not be verified cleared. $handoffMessage Manual recovery from elevated cmd.exe: bcdedit /deletevalue safeboot ; bcdedit /enum {current} /v"
    }
}

function Set-BootConfig {
    [CmdletBinding(SupportsShouldProcess)]
    param([string]$key, [string]$val, [string]$why, [switch]$PassThru)
    # SECURITY: Validate bcdedit key/value - these are passed as command-line arguments.
    # An attacker who controls state.json or backup.json could inject arbitrary bcdedit args.
    # bcdedit keys are alphanumeric identifiers; values are alphanumeric, hex, or simple tokens.
    if ($key -notmatch '^[a-zA-Z][a-zA-Z0-9_]*$') {
        $message = "Set-BootConfig: invalid key format '$key' - rejected (security: command injection prevention)"
        Write-Warn $message
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message $message) }
        return $false
    }
    if ($val -notmatch '^[a-zA-Z0-9_.{}\-]+$') {
        $message = "Set-BootConfig: invalid value format '$val' - rejected (security: command injection prevention)"
        Write-Warn $message
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message $message) }
        return $false
    }

    if ($SCRIPT:DryRun) {
        Write-ConsoleLine "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Would set: bcdedit /set $key $val  ($why)" -ForegroundColor Magenta
        if ($PassThru) { return (New-WriteOperationResult -Status "DryRun" -Message "Boot config previewed: $key = $val") }
        return $true
    }
    Write-Step "bcdedit /set $key $val  ($why)"
    if (-not $PSCmdlet.ShouldProcess($key, "Set boot configuration value to $val")) {
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message "Boot config skipped: $key = $val") }
        return $false
    }
    # Backups are part of the approved mutation. They must not be queued by
    # -WhatIf or dry-run calls.
    if ((Get-Variable -Name CurrentStepTitle -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:CurrentStepTitle) {
        $capture = Backup-BootConfig -Key $key -StepTitle $SCRIPT:CurrentStepTitle -PassThru
        if (-not $capture -or -not $capture.Captured) {
            $detail = if ($capture -and $capture.Message) { $capture.Message } else { 'No capture result was returned.' }
            $message = "Boot config change blocked because the original value was not captured: $detail"
            Write-Warn $message
            if ($PassThru) { return (New-WriteOperationResult -Status "Failed" -Message $message) }
            return $false
        }
        try {
            Flush-BackupBuffer
            $durableBackup = Get-BackupDataRaw
            $capturePersisted = @($durableBackup.entries | Where-Object {
                $_.type -eq 'bootconfig' -and [string]$_.step -eq [string]$SCRIPT:CurrentStepTitle -and $_.key -eq $key
            }).Count -gt 0
            if (-not $capturePersisted) {
                throw "backup.json does not contain the expected boot restore record."
            }
        } catch {
            $message = "Boot config change blocked because its restore record was not persisted: $_"
            Write-Warn $message
            if ($PassThru) { return (New-WriteOperationResult -Status "Failed" -Message $message) }
            return $false
        }
    }
    $output = bcdedit /set $key $val 2>&1
    $bcdeditExit = $LASTEXITCODE
    $outputStr = $output | Out-String
    if ($bcdeditExit -ne 0) {
        $message = "Boot config change failed: bcdedit /set $key $val"
        Write-Warn $message
        Write-ConsoleLine "  $([char]0x2139) This is usually fine - Windows may not support this setting on your PC." -ForegroundColor Cyan
        Write-DebugLog "bcdedit exit $bcdeditExit - $outputStr"
        if ($PassThru) { return (New-WriteOperationResult -Status "Failed" -Message $message) }
        return $false
    }
    Write-OK "Set: $key = $val"
    if ($PassThru) { return (New-WriteOperationResult -Status "Success" -Message "Boot config set: $key = $val") }
    return $true
}

function Invoke-BcdEditCaptured {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string[]]$Arguments)

    $output = $null
    $exitCode = $null
    try {
        $output = & bcdedit @Arguments 2>&1
        $exitCode = $LASTEXITCODE
    } catch {
        $output = $_
    }

    return [PSCustomObject]@{
        Output   = $output
        ExitCode = $exitCode
    }
}

function Clear-SafeBootVerified {
    <#  Removes the SafeBoot element from the current loader and then verifies
        its absence using the locale-independent raw BCD element identifier.
        The verification is authoritative: neither a successful delete nor an
        unattended profile may bypass a failed /enum check or a remaining
        0x26000081 element.  #>
    [CmdletBinding()]
    param()

    $deleteResult = Invoke-BcdEditCaptured -Arguments @('/deletevalue', 'safeboot')
    $deleteExitCode = $deleteResult.ExitCode

    $enumResult = Invoke-BcdEditCaptured -Arguments @('/enum', '{current}', '/v')
    $enumOutput = $enumResult.Output
    $enumExitCode = $enumResult.ExitCode

    $applied = $deleteExitCode -eq 0
    if ($enumExitCode -ne 0) {
        return [PSCustomObject]@{
            Status         = "Failed"
            Verified       = $false
            Applied        = $applied
            DeleteExitCode = $deleteExitCode
            EnumExitCode   = $enumExitCode
            Message        = "Safe Mode state could not be verified: bcdedit /enum failed (delete exit $deleteExitCode, enum exit $enumExitCode)."
        }
    }

    $enumText = ($enumOutput | ForEach-Object { $_.ToString() }) -join "`n"
    if ($enumText -match '(?im)^\s*0x26000081(?:\s|$)') {
        return [PSCustomObject]@{
            Status         = "Failed"
            Verified       = $false
            Applied        = $applied
            DeleteExitCode = $deleteExitCode
            EnumExitCode   = $enumExitCode
            Message        = "Safe Mode remains enabled: BCD element 0x26000081 is still present (delete exit $deleteExitCode, enum exit $enumExitCode)."
        }
    }

    $message = if ($applied) {
        "Safe Mode disabled and verified (delete exit $deleteExitCode, enum exit $enumExitCode)."
    } else {
        "Safe Mode was already absent and is verified disabled (delete exit $deleteExitCode, enum exit $enumExitCode)."
    }
    return [PSCustomObject]@{
        Status         = "Success"
        Verified       = $true
        Applied        = $applied
        DeleteExitCode = $deleteExitCode
        EnumExitCode   = $enumExitCode
        Message        = $message
    }
}

function Test-BootConfigSet($key) {
    <#  Verifies a bcdedit value is present in the current BCD entry.
        Uses hex element IDs via /v for locale-independent matching.
        Returns $true if the key exists, $false otherwise.  #>
    $bcdElementMap = @{
        "safeboot"           = "0x26000081"
        "disabledynamictick" = "0x26000060"
        "useplatformtick"    = "0x26000092"
        "useplatformclock"   = "0x26000091"
    }
    $hexId = if ($bcdElementMap.ContainsKey($key)) { $bcdElementMap[$key] } else { $null }
    try {
        # Run bcdedit and capture output - stringify each line to avoid ErrorRecord
        # objects from 2>&1 that would fail regex matching.
        $bcdOutput = bcdedit /enum "{current}" /v 2>&1
        $bcdExit = $LASTEXITCODE
        if ($bcdExit -ne 0) {
            # Fallback: try without /v (some builds return non-zero with /v)
            $bcdOutput = bcdedit /enum "{current}" 2>&1
            $bcdExit = $LASTEXITCODE
            if ($bcdExit -ne 0) { return $false }
            # Without /v, match the friendly key name instead of hex ID
            foreach ($line in $bcdOutput) {
                $s = "$line"
                if ($s -match "^\s*$([regex]::Escape($key))\s+") { return $true }
            }
            return $false
        }
        foreach ($line in $bcdOutput) {
            $s = "$line"   # force to string (ErrorRecords from 2>&1 won't match otherwise)
            if ($hexId -and $s -match "^\s*$hexId\s+") { return $true }
            elseif (-not $hexId -and $s -match "^\s*$([regex]::Escape($key))\s+") { return $true }
        }
    } catch { Write-DebugLog "Test-BootConfigSet: bcdedit enum failed for '$key': $_" }
    return $false
}

function ConvertTo-SuiteRegistryPath {
    [CmdletBinding()]
    param([AllowEmptyString()][string]$Path)

    if ($null -eq $Path) { return "" }
    $normalized = $Path -replace '/', '\'
    $normalized = $normalized -replace '^Microsoft\.PowerShell\.Core\\Registry::HKEY_CURRENT_USER\\', 'HKCU:\'
    $normalized = $normalized -replace '^Microsoft\.PowerShell\.Core\\Registry::HKCU\\', 'HKCU:\'
    return $normalized.TrimEnd('\')
}

function Test-SafeCs2RegistryValuePath {
    [CmdletBinding()]
    param([AllowEmptyString()][string]$Name)

    if ([string]::IsNullOrWhiteSpace($Name)) { return $false }
    $normalizedName = $Name -replace '/', '\'
    if ($normalizedName -notmatch '^[A-Za-z]:\\') { return $false }
    if ($normalizedName -match '[\x00-\x1F"*?<>|]') { return $false }
    if ($normalizedName.Substring(2) -match ':') { return $false }

    $segments = @($normalizedName.Substring(3) -split '\\')
    if ($segments.Count -eq 0) { return $false }
    foreach ($segment in $segments) {
        if ([string]::IsNullOrEmpty($segment) -or
            $segment -in @('.', '..') -or
            $segment.EndsWith(' ') -or
            $segment.EndsWith('.')) {
            return $false
        }
    }

    return $segments[-1].Equals('cs2.exe', [System.StringComparison]::OrdinalIgnoreCase)
}

function Test-RegistryValueNameAllowed {
    [CmdletBinding()]
    param(
        [AllowEmptyString()][string]$Path,
        [AllowEmptyString()][string]$Name
    )

    if ($Name -notmatch '[\\/\x00]') { return $true }
    if (-not (Test-SafeCs2RegistryValuePath -Name $Name)) { return $false }

    $normalizedPath = ConvertTo-SuiteRegistryPath -Path $Path
    return $normalizedPath -in @(
        'HKCU:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers',
        'HKCU:\SOFTWARE\Microsoft\DirectX\UserGpuPreferences'
    )
}

function Set-RegistryValue {
    [CmdletBinding(SupportsShouldProcess)]
    param([string]$path, [string]$name, $value, [string]$type, [string]$why, [switch]$PassThru)
    # SECURITY: Validate registry path - must start with a known hive prefix.
    # An attacker who controls backup.json or state.json could inject arbitrary paths
    # to write to sensitive registry locations outside the expected scope.
    if ($path -notmatch '^(HKLM:|HKCU:|HKCR:|HKU:|HKCC:|Microsoft\.PowerShell\.Core\\Registry::HK)') {
        $message = "Set-RegistryValue: path does not start with a valid registry hive - rejected: $path"
        Write-Warn $message
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message $message) }
        return
    }
    # SECURITY: Registry value names normally cannot contain path separators or
    # null bytes. Windows uses the full executable path as the value name on two
    # exact per-user application keys, so permit only a lexically safe local
    # cs2.exe path for those keys.
    if (-not (Test-RegistryValueNameAllowed -Path $path -Name $name)) {
        $message = "Set-RegistryValue: name contains invalid characters - rejected: $name"
        Write-Warn $message
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message $message) }
        return
    }
    $registryValueName = $name -replace '/', '\'

    if ($SCRIPT:DryRun) {
        Write-ConsoleLine "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)  Would set: $registryValueName = $value [$type]  ($why)" -ForegroundColor Magenta
        Write-ConsoleLine "  $([char]0x2588)$([char]0x2588) DRY-RUN $([char]0x2588)$([char]0x2588)    Path: $path" -ForegroundColor DarkMagenta
        if ($PassThru) { return (New-WriteOperationResult -Status "DryRun" -Message "Registry previewed: $path | $registryValueName") }
        return
    }
    Write-DebugLog "Registry: $path | $registryValueName = $value [$type] - $why"
    if (-not $PSCmdlet.ShouldProcess("$path\$registryValueName", "Set registry value to $value [$type]")) {
        if ($PassThru) { return (New-WriteOperationResult -Status "Skipped" -Message "Registry skipped: $path | $registryValueName") }
        return
    }
    # Backups are part of the approved mutation. They must not be queued by
    # -WhatIf or dry-run calls.
    if ((Get-Variable -Name CurrentStepTitle -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:CurrentStepTitle) {
        $capture = Backup-RegistryValue -Path $path -Name $registryValueName `
            -StepTitle $SCRIPT:CurrentStepTitle -PassThru
        if (-not $capture -or -not $capture.Captured) {
            $captureMessage = if ($capture -and $capture.Message) { $capture.Message } else { 'No capture result was returned.' }
            $message = "Registry write blocked because the original value was not captured: $captureMessage"
            Write-Warn $message
            if ($PassThru) { return (New-WriteOperationResult -Status "Failed" -Message $message) }
            return
        }
        try {
            # Registry mutations are journaled before they are applied. This is
            # deliberate I/O: a crash must not leave a changed value whose
            # original state existed only in memory.
            Flush-BackupBuffer
            $durableBackup = Get-BackupDataRaw
            $durableCapture = @($durableBackup.entries | Where-Object {
                $_.type -eq 'registry' -and
                [string]$_.step -eq [string]$SCRIPT:CurrentStepTitle -and
                $_.path -eq $path -and
                $_.name -eq $registryValueName
            }).Count -gt 0
            if (-not $durableCapture) {
                throw "backup.json does not contain the expected registry restore record."
            }
        } catch {
            $message = "Registry write blocked because its restore record was not persisted: $_"
            Write-Warn $message
            if ($PassThru) { return (New-WriteOperationResult -Status "Failed" -Message $message) }
            return
        }
    }
    try {
        if (-not (Test-Path $path)) { New-Item -Path $path -Force -ErrorAction Stop | Out-Null }
        Set-ItemProperty -Path $path -Name $registryValueName -Value $value -Type $type -ErrorAction Stop
        Write-OK "Registry: $registryValueName = $value"
        if ($PassThru) { return (New-WriteOperationResult -Status "Success" -Message "Registry set: $path | $registryValueName") }
    } catch {
        $message = "Registry write failed ($registryValueName): $_"
        Write-Warn $message
        Write-ConsoleLine "  $([char]0x2139) This is not critical - the optimization will be skipped for this setting." -ForegroundColor Cyan
        if ($PassThru) { return (New-WriteOperationResult -Status "Failed" -Message $message) }
    }
}

function Ensure-Dir {
    param(
        [Parameter(Mandatory)][string]$Path,
        [switch]$AllowDryRunPersistence
    )

    $dryRunActive = (Get-Variable DryRun -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:DryRun
    if ($dryRunActive -and -not $AllowDryRunPersistence) { return }
    if (-not (Test-Path $Path)) { New-Item -ItemType Directory -Path $Path -Force -ErrorAction SilentlyContinue | Out-Null }
}

function Set-ClipboardSafe {
    <#  Wraps Set-Clipboard in try/catch. Set-Clipboard can fail on headless/remote
        sessions, minimal Windows Server editions, or when the clipboard service is
        unavailable. Non-critical - failure is logged, not thrown.  #>
    [CmdletBinding(SupportsShouldProcess)]
    param([Parameter(ValueFromPipeline)][string]$Text)
    process {
        $dryRunActive = (Get-Variable DryRun -Scope Script -ErrorAction SilentlyContinue) -and $SCRIPT:DryRun
        if ($dryRunActive) {
            Write-ConsoleLine "  [DRY-RUN] Would copy text to the clipboard: $Text" -ForegroundColor Magenta
            return
        }
        if (-not $PSCmdlet.ShouldProcess("clipboard", "Set clipboard text")) { return }
        try { $Text | Set-Clipboard -ErrorAction Stop }
        catch { Write-DebugLog "Set-Clipboard failed (headless/remote session?): $_" }
    }
}

function Clear-Dir($path, $label) {
    if ($SCRIPT:DryRun) { Write-DebugLog "DRY-RUN: Clear-Dir skipped for $path"; return 0 }
    if (-not (Test-Path $path)) { Write-DebugLog "${label}: not found ($path)"; return 0 }
    $items = Get-ChildItem $path -Recurse -Force -ErrorAction SilentlyContinue
    $files = @($items | Where-Object { -not $_.PSIsContainer })
    $n = $files.Count
    $mb = [math]::Round(([int64]($files | Measure-Object -Property Length -Sum -ErrorAction SilentlyContinue).Sum) / 1MB, 1)
    Write-Step "$label  ($n files · $mb MB)"
    $items | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
    $remaining = @(Get-ChildItem $path -Recurse -Force -ErrorAction SilentlyContinue | Where-Object { -not $_.PSIsContainer }).Count
    $del = [math]::Max(0, $n - $remaining)
    Write-OK "${label}: $del deleted$(if($remaining){" ($remaining locked - normal)"})"
    Write-DebugLog "${label}: del=$del locked=$remaining path=$path"
    return $del
}

# ── System Compatibility Checks ──────────────────────────────────────────────
# Runs once at startup to detect and warn about edge-case environments.
# All issues are non-fatal - the suite degrades gracefully.

function Test-SystemCompatibility {
    <#
    .SYNOPSIS  Detects environment limitations and logs warnings.
    .DESCRIPTION
        Checks for: ARM64, Constrained Language Mode, Windows Server/LTSC,
        PowerShell 7 (missing Get-WmiObject), missing AppX cmdlets.
        Does not block execution - all limitations have graceful fallbacks.
    #>
    $warnings = 0

    # ARM64 Windows - nvapi64.dll and some x64 P/Invoke won't work
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
        Write-Warn "ARM64 Windows detected. NVIDIA DRS writes will fall back to registry-only method."
        $warnings++
    }

    # Constrained Language Mode - Add-Type blocked (AppLocker, WDAC, DeviceGuard)
    if ($ExecutionContext.SessionState.LanguageMode -eq 'ConstrainedLanguage') {
        Write-Warn "Constrained Language Mode active. NVIDIA DRS and RAM trim will be skipped."
        Write-Warn "Registry-only paths will be used where available."
        $warnings++
    }

    # Windows Server / LTSC - missing AppX, Xbox services, some consumer features
    $productType = (Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue).ProductType
    # ProductType: 1=Workstation, 2=DomainController, 3=Server
    if ($productType -and $productType -ne 1) {
        Write-Warn "Windows Server/DC edition detected (ProductType=$productType)."
        Write-Warn "AppX debloat, Xbox services, and some consumer features may not exist."
        $warnings++
    }

    # PowerShell 7+ - Get-WmiObject removed (pagefile step affected)
    if ($PSVersionTable.PSVersion.Major -ge 7) {
        Write-Warn "PowerShell $($PSVersionTable.PSVersion) detected. Pagefile configuration"
        Write-Warn "requires Get-WmiObject (PS 5.1 only). Run with Windows PowerShell for full support."
        $warnings++
    }

    # Missing AppX cmdlets (Server Core, minimal installs)
    if (-not (Get-Command Get-AppxPackage -ErrorAction SilentlyContinue)) {
        Write-DebugLog "AppX cmdlets not available - debloat package removal will be skipped."
        $warnings++
    }

    if ($warnings -gt 0) {
        Write-Info "Detected $warnings compatibility note(s). Affected operations may be skipped or use reduced behavior."
    }
}

# ── Verification counter infrastructure ─────────────────────────────────────
# Uses $Script: scope (caller's scope via dot-sourcing). Entry-point scripts
# must call Initialize-VerifyCounters before use to reset stale values.

function Initialize-VerifyCounters {
    $Script:_verifyOkCount      = 0
    $Script:_verifyChangedCount = 0
    $Script:_verifyMissingCount = 0
    $Script:_verifyInfoCount    = 0
}

function Get-VerifyCounters {
    return @{
        okCount      = [int]$Script:_verifyOkCount
        changedCount = [int]$Script:_verifyChangedCount
        missingCount = [int]$Script:_verifyMissingCount
        infoCount    = [int]$Script:_verifyInfoCount
    }
}

function Test-RegistryCheck {
    param(
        [string] $Path,
        [string] $Name,
        $Expected,
        [string] $Label,
        [switch] $Quiet   # Returns structured @{Status; Value} without console output or counter updates
    )
    $result = $null
    try {
        if (Test-Path $Path) {
            $val = Get-ItemProperty -Path $Path -Name $Name -ErrorAction Stop
            $result = $val.$Name
        }
    } catch { Write-DebugLog "Test-RegistryCheck: could not read '$Name' from '$Path'" }

    $status = if ($null -eq $result) { "MISSING" } elseif ($result -eq $Expected) { "OK" } else { "CHANGED" }

    if ($Quiet) {
        return @{ Status = $status; Value = $result }
    }

    switch ($status) {
        "MISSING" {
            Write-ConsoleLine "  ?  MISSING   $Label" -ForegroundColor Red
            Write-ConsoleLine "               $Path\$Name" -ForegroundColor DarkGray
            $Script:_verifyMissingCount++
        }
        "OK" {
            Write-ConsoleLine "  ✓  OK        $Label  ($result)" -ForegroundColor Green
            $Script:_verifyOkCount++
        }
        "CHANGED" {
            Write-ConsoleLine "  ✗  CHANGED   $Label  (is: $result, expected: $Expected)" -ForegroundColor Yellow
            Write-ConsoleLine "               $Path\$Name" -ForegroundColor DarkGray
            # Warn if the key lives under a Policies path - Group Policy may override user writes
            if ($Path -match '\\Policies\\') {
                Write-ConsoleLine "               NOTE: This key is under a Policies path - may be managed by Group Policy" -ForegroundColor DarkYellow
            }
            $Script:_verifyChangedCount++
        }
    }
    # No return value when not -Quiet - prevents stdout clutter in Verify-Settings.ps1
}

function Test-ServiceCheck {
    param(
        [string] $ServiceName,
        [string] $ExpectedStartType,
        [string] $Label
    )
    try {
        $svc = Get-Service -Name $ServiceName -ErrorAction Stop
        # Escape single quotes in the service name to prevent WQL injection
        $escapedName = $ServiceName -replace "'", "''"
        $cimSvc = Get-CimInstance Win32_Service -Filter "Name='$escapedName'" -ErrorAction SilentlyContinue
        $rawStartType = if ($cimSvc) { $cimSvc.StartMode } else { $svc.StartType.ToString() }
        # WMI returns "Auto" but Set-Service uses "Automatic" - normalize for comparison
        $startType = switch ($rawStartType) {
            "Auto"         { "Automatic" }
            "Auto Delayed" { "AutomaticDelayedStart" }
            default        { $rawStartType }
        }
        if ($startType -eq $ExpectedStartType) {
            Write-ConsoleLine "  ✓  OK        $Label  (StartType: $startType, Status: $($svc.Status))" -ForegroundColor Green
            $Script:_verifyOkCount++
        } else {
            Write-ConsoleLine "  ✗  CHANGED   $Label  (StartType: $startType, expected: $ExpectedStartType)" -ForegroundColor Yellow
            $Script:_verifyChangedCount++
        }
    } catch {
        Write-ConsoleLine "  ?  MISSING   $Label  (Service not found)" -ForegroundColor Red
        $Script:_verifyMissingCount++
    }
}
