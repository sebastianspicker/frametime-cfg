# ==============================================================================
#  helpers/gpu-driver-clean.ps1  —  Safe Mode GPU Driver Removal (DDU replacement)
# ==============================================================================

function Get-GpuCacheCleanupTargets {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [ValidateSet("NVIDIA", "AMD", "Intel")]
        [string]$GpuVendor
    )

    # Resolve roots through the platform known-folder API. Environment
    # variables are mutable process input and must not select recursive-delete
    # targets in this elevated cleanup path.
    $localRoot = [Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)
    $commonRoot = [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)
    $relativeTargets = switch ($GpuVendor) {
        "NVIDIA" { @(
            @{ Root = $localRoot; Relative = "NVIDIA\DXCache" },
            @{ Root = $localRoot; Relative = "NVIDIA\GLCache" },
            @{ Root = $localRoot; Relative = "NVIDIA Corporation\NV_Cache" },
            @{ Root = $commonRoot; Relative = "NVIDIA Corporation\NV_Cache" }
        ) }
        "AMD" { @(
            @{ Root = $localRoot; Relative = "AMD\DxCache" },
            @{ Root = $localRoot; Relative = "AMD\GLCache" },
            @{ Root = $localRoot; Relative = "AMD\DxcCache" }
        ) }
        "Intel" { @(
            @{ Root = $localRoot; Relative = "Intel\ShaderCache" },
            @{ Root = $localRoot; Relative = "Intel\GPUCache" }
        ) }
    }
    $relativeTargets += @{ Root = $localRoot; Relative = "D3DSCache" }

    foreach ($target in $relativeTargets) {
        if ([string]::IsNullOrWhiteSpace([string]$target.Root)) { continue }
        [PSCustomObject]@{
            Root = [IO.Path]::GetFullPath([string]$target.Root)
            Path = [IO.Path]::GetFullPath((Join-Path ([string]$target.Root) ([string]$target.Relative)))
        }
    }
}

function Get-GpuVendorApplicationCleanupTargets {
    <#
    .SYNOPSIS
        Returns fixed vendor application descendants rooted at OS-known folders.
    .DESCRIPTION
        Do not derive elevated recursive-delete targets from mutable environment
        variables.  Each result is a specific vendor-owned descendant which is
        validated immediately before deletion.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [ValidateSet("NVIDIA", "AMD", "Intel")]
        [string]$GpuVendor
    )

    $programFiles = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
    $programFilesX86 = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFilesX86)
    $commonRoot = [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)
    $localRoot = [Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)
    $relativeTargets = switch ($GpuVendor) {
        'NVIDIA' { @(
            @{ Root = $programFiles; Relative = 'NVIDIA Corporation' },
            @{ Root = $programFilesX86; Relative = 'NVIDIA Corporation' },
            @{ Root = $commonRoot; Relative = 'NVIDIA Corporation' },
            @{ Root = $commonRoot; Relative = 'NVIDIA' },
            @{ Root = $localRoot; Relative = 'NVIDIA Corporation' },
            @{ Root = $localRoot; Relative = 'NVIDIA' }
        ) }
        'AMD' { @(
            @{ Root = $programFiles; Relative = 'AMD' },
            @{ Root = $programFilesX86; Relative = 'AMD' },
            @{ Root = $commonRoot; Relative = 'AMD' },
            @{ Root = $localRoot; Relative = 'AMD' }
        ) }
        default { @() }
    }

    foreach ($target in $relativeTargets) {
        if ([string]::IsNullOrWhiteSpace([string]$target.Root)) { continue }
        [PSCustomObject]@{
            Root = [IO.Path]::GetFullPath([string]$target.Root)
            Path = [IO.Path]::GetFullPath((Join-Path ([string]$target.Root) ([string]$target.Relative)))
        }
    }
}

function Test-GpuCacheCleanupTarget {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][string]$Path
    )

    try {
        $rootPath = [IO.Path]::GetFullPath($Root).TrimEnd([char[]]@('\', '/'))
        $targetPath = [IO.Path]::GetFullPath($Path).TrimEnd([char[]]@('\', '/'))
        if ($rootPath -match '[*?\[]' -or $targetPath -match '[*?\[]') { return $false }
        if ($rootPath -match '^(?:\\\\|//)' -or $targetPath -match '^(?:\\\\|//)') { return $false }
        $rootPrefix = $rootPath + [IO.Path]::DirectorySeparatorChar
        if (-not $targetPath.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) { return $false }

        $rootItem = Get-Item -LiteralPath $rootPath -Force -ErrorAction Stop
        if (-not $rootItem.PSIsContainer -or
            ($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }

        if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
            $rootAncestor = $rootItem.Parent
            while ($rootAncestor) {
                if (($rootAncestor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
                $rootAncestor = $rootAncestor.Parent
            }

            $volumeRoot = [IO.Path]::GetPathRoot($rootPath)
            if ($volumeRoot -notmatch '^[A-Za-z]:\\$' -or
                [IO.DriveInfo]::new($volumeRoot).DriveType -ne [IO.DriveType]::Fixed) {
                return $false
            }

            # LocalApplicationData is user-configurable. For elevated deletion,
            # require it to remain beneath the current SID's machine-owned
            # ProfileList path rather than trusting the known-folder value alone.
            $localAppData = [Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)
            if ([string]::Equals($rootPath, [IO.Path]::GetFullPath($localAppData).TrimEnd([char[]]@('\', '/')),
                    [StringComparison]::OrdinalIgnoreCase)) {
                $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
                $profileKey = "Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\$sid"
                $profileValue = (Get-ItemProperty -LiteralPath $profileKey -Name ProfileImagePath -ErrorAction Stop).ProfileImagePath
                $profilePath = [IO.Path]::GetFullPath(
                    [Environment]::ExpandEnvironmentVariables([string]$profileValue)
                ).TrimEnd([char[]]@('\', '/'))
                $profilePrefix = $profilePath + [IO.Path]::DirectorySeparatorChar
                if (-not $rootPath.StartsWith($profilePrefix, [StringComparison]::OrdinalIgnoreCase)) {
                    return $false
                }
            }
        }

        # Reject a cache directory, or any child directory below the known
        # folder, when it is a junction/symlink. Recursive deletion must never
        # escape through a reparse point.
        $item = Get-Item -LiteralPath $targetPath -Force -ErrorAction Stop
        while (-not [string]::Equals(
            [IO.Path]::GetFullPath($item.FullName).TrimEnd([char[]]@('\', '/')),
            $rootPath,
            [StringComparison]::OrdinalIgnoreCase
        )) {
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
            $item = $item.Parent
            if ($null -eq $item) { return $false }
        }

        # Inspect every descendant without following reparse points. A trusted
        # target can still contain a nested junction that would redirect a
        # recursive elevated delete outside the approved root.
        $pending = New-Object 'System.Collections.Generic.Queue[string]'
        $pending.Enqueue($targetPath)
        while ($pending.Count -gt 0) {
            $directory = $pending.Dequeue()
            foreach ($child in @(Get-ChildItem -LiteralPath $directory -Force -ErrorAction Stop)) {
                if (($child.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
                if ($child.PSIsContainer) { $pending.Enqueue($child.FullName) }
            }
        }
        return $true
    } catch {
        Write-DebugLog "GPU cache path validation failed for '$Path': $_"
        return $false
    }
}

function Get-GpuPnpUtilPath {
    <# Resolves the trusted inbox pnputil executable without PATH lookup. #>
    [CmdletBinding()]
    param()

    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        # Unit tests run cross-platform; production callers are Windows-only.
        return 'pnputil'
    }

    $systemDirectory = [Environment]::SystemDirectory
    if ([string]::IsNullOrWhiteSpace($systemDirectory)) {
        throw 'Windows did not provide a System directory for pnputil.exe.'
    }
    $path = Join-Path $systemDirectory 'pnputil.exe'
    $item = Get-Item -LiteralPath $path -Force -ErrorAction Stop
    $canonicalSystem = [IO.Path]::GetFullPath($systemDirectory).TrimEnd([char[]]@('\', '/'))
    if ($item.PSIsContainer -or
        ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
        -not [string]::Equals($item.Directory.FullName, $canonicalSystem, [StringComparison]::OrdinalIgnoreCase)) {
        throw "The inbox pnputil path is not a trusted regular file: $path"
    }
    $volumeRoot = [IO.Path]::GetPathRoot($item.FullName)
    if ($item.FullName -match '^\\\\' -or $volumeRoot -notmatch '^[A-Za-z]:\\$' -or
        [IO.DriveInfo]::new($volumeRoot).DriveType -ne [IO.DriveType]::Fixed) {
        throw "The inbox pnputil path is not on a local fixed volume: $($item.FullName)"
    }
    return $item.FullName
}

function Invoke-GpuPnpUtilDelete {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$PnpUtilPath,
        [Parameter(Mandatory)][ValidatePattern('^oem\d+\.inf$')][string]$InfName
    )

    $output = & $PnpUtilPath /delete-driver $InfName /uninstall /force 2>&1
    [PSCustomObject]@{
        ExitCode = $LASTEXITCODE
        Output = @($output)
    }
}

function Get-GpuDriverCleanResult {
    <#
    .SYNOPSIS
        Creates the stable result contract for GPU cleanup callers.
    .DESCRIPTION
        `DriverRemovalVerified` is deliberately distinct from pnputil's exit
        code.  Only a second, locale-independent CIM query can prove that an
        INF package is no longer installed.  Cleanup failures and Safe Mode
        skips are likewise exposed separately so Phase 2 never promotes a
        best-effort cleanup to a successful handoff.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Status,
        [Parameter(Mandatory)][bool]$Applied,
        [Parameter(Mandatory)][bool]$CanCompleteStep,
        [int]$FoundDriverPackages = 0,
        [int]$RemovedDriverPackages = 0,
        [int]$FailedDriverPackages = 0,
        [int]$UnknownDriverPackages = 0,
        [bool]$DriverRemovalVerified = $false,
        [bool]$AlreadyAbsent = $false,
        [int]$SoftwareRemoved = 0,
        [int]$FoldersCleaned = 0,
        [int]$LockedFolders = 0,
        [int]$CleanupFailures = 0,
        [int]$CleanupSkipped = 0,
        [int]$CleanupDeferred = 0,
        [string]$Message = ''
    )

    [PSCustomObject]@{
        Status = $Status
        Applied = $Applied
        CanCompleteStep = $CanCompleteStep
        FoundDriverPackages = $FoundDriverPackages
        RemovedDriverPackages = $RemovedDriverPackages
        FailedDriverPackages = $FailedDriverPackages
        UnknownDriverPackages = $UnknownDriverPackages
        DriverRemovalVerified = $DriverRemovalVerified
        AlreadyAbsent = $AlreadyAbsent
        SoftwareRemoved = $SoftwareRemoved
        FoldersCleaned = $FoldersCleaned
        LockedFolders = $LockedFolders
        CleanupFailures = $CleanupFailures
        CleanupSkipped = $CleanupSkipped
        CleanupDeferred = $CleanupDeferred
        Message = $Message
    }
}

function Remove-GpuDriverClean {
    <#
    .SYNOPSIS  Removes GPU drivers cleanly in Safe Mode. Pure PowerShell replacement
               for Display Driver Uninstaller (DDU).
    .DESCRIPTION
        1. Discovers display-driver packages through locale-independent CIM data
        2. Removes only validated oem<N>.inf packages through pnputil
        3. After verified package removal, cleans vendor services/software/registry
        4. Leaves display-class keys and DriverStore ownership to Windows
        5. Cleans rebuildable shader caches and temp folders
    #>
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [ValidateSet("NVIDIA","AMD","Intel")]
        [string]$GpuVendor = "NVIDIA",
        [switch]$PassThru
    )

    Write-Step "GPU Driver Clean Removal — $GpuVendor"
    Write-Info "This replaces DDU with native PowerShell commands."

    if ($SCRIPT:DryRun) {
        Write-ConsoleLine "  [DRY-RUN] Would perform complete GPU driver removal for $GpuVendor" -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN]   1. Discover vendor display packages through CIM" -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN]   2. Remove validated oem<N>.inf packages via pnputil" -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN]   3. After verified removal, clean vendor services/software/registry" -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN]   4. Leave display-class keys and DriverStore ownership to Windows" -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN]   5. Clean rebuildable shader caches" -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN] No files or registry entries will be modified." -ForegroundColor Magenta
        if ($PassThru) {
            return Get-GpuDriverCleanResult -Status 'DryRun' -Applied $false -CanCompleteStep $false -Message "GPU driver cleanup previewed for $GpuVendor"
        }
        return
    }
    if (-not $PSCmdlet.ShouldProcess($GpuVendor, "Remove GPU driver packages, software, services, registry entries, and caches")) {
        if ($PassThru) {
            return Get-GpuDriverCleanResult -Status 'DryRun' -Applied $false -CanCompleteStep $false -Message "GPU driver cleanup previewed for $GpuVendor"
        }
        return
    }

    # ── 1. Prove and remove the vendor driver package before any cleanup ─────
    # CIM is the authoritative, locale-independent source here.  Do not use the
    # pnputil text output as proof: its labels are localized and a parse failure
    # must never turn into permission to delete vendor state.
    Write-Step "Enumerating GPU driver packages..."
    $vendorMatch = switch ($GpuVendor) {
        "NVIDIA" { "nvidia" }
        "AMD"    { "advanced\s+micro\s+devices|\bamd\b|\bati\b|radeon" }
        "Intel"  { "intel" }
    }
    $driverPackages = @()
    $invalidDriverRows = @()
    $cimEnumerationSucceeded = $false
    try {
        $displayGuid = $CFG_GUID_Display
        $cimDrivers = @(Get-CimInstance Win32_PnPSignedDriver -ErrorAction Stop |
            Where-Object { $_.ClassGuid -eq $displayGuid -and $_.DriverProviderName -match $vendorMatch })
        $cimEnumerationSucceeded = $true
        foreach ($drv in $cimDrivers) {
            if ($drv.InfName -match '^oem\d+\.inf$') {
                if ($drv.InfName -notin $driverPackages) {
                    $driverPackages += $drv.InfName
                }
            } else {
                $invalidDriverRows += $drv
            }
        }
    } catch {
        Write-DebugLog "CIM enumeration failed: $_"
    }

    if (-not $cimEnumerationSucceeded) {
        # Do not parse `pnputil /enum-drivers` as a destructive fallback. Its
        # text is localized, so it cannot establish either package ownership or
        # authoritative absence. Keep the command name in the diagnostic for
        # operators who need to investigate manually.
        Write-Warn "Could not authoritatively enumerate $GpuVendor display drivers. Run 'pnputil /enum-drivers' manually to investigate."
        if ($PassThru) {
            return Get-GpuDriverCleanResult -Status 'Failed' -Applied $false -CanCompleteStep $false -Message 'Driver package discovery was not verified; no cleanup was performed.'
        }
        return
    }

    $foundDriverPackages = $driverPackages.Count + $invalidDriverRows.Count
    if ($invalidDriverRows.Count -gt 0) {
        $invalidNames = @($invalidDriverRows | ForEach-Object {
            if ([string]::IsNullOrWhiteSpace([string]$_.InfName)) { '<missing>' } else { [string]$_.InfName }
        }) -join ', '
        Write-Warn "CIM returned $($invalidDriverRows.Count) matching $GpuVendor display row(s) with unsafe or unusable INF names: $invalidNames"
        if ($PassThru) {
            return Get-GpuDriverCleanResult -Status 'Failed' -Applied $false -CanCompleteStep $false `
                -FoundDriverPackages $foundDriverPackages -FailedDriverPackages $invalidDriverRows.Count `
                -UnknownDriverPackages $driverPackages.Count -Message 'Matching display-driver rows could not be mapped safely to OEM INF packages; no cleanup was performed.'
        }
        return
    }

    $removedDrivers = 0
    $failedDrivers = 0
    $unknownDrivers = 0
    $pnputilSucceeded = @()
    $pnputilFailed = @()
    $removedApps = 0
    $alreadyAbsent = ($driverPackages.Count -eq 0)
    # A zero pnputil exit code is not proof of removal.  This remains false
    # until the original CIM query can no longer find every targeted INF.
    $driverRemovalVerified = $alreadyAbsent
    if (-not $alreadyAbsent) {
        Write-OK "CIM enumeration found $($driverPackages.Count) $GpuVendor display driver(s)."
        try {
            $pnpUtilPath = Get-GpuPnpUtilPath
            foreach ($inf in $driverPackages) {
                # SECURITY: only CIM-published oem<N>.inf names may reach the
                # absolute inbox pnputil path.
                Write-Step "Removing driver package: $inf"
                try {
                    $nativeResult = Invoke-GpuPnpUtilDelete -PnpUtilPath $pnpUtilPath -InfName $inf
                    if ($nativeResult.ExitCode -eq 0) {
                        $pnputilSucceeded += $inf
                    } else {
                        Write-Warn "Could not remove $inf (exit $($nativeResult.ExitCode)): $($nativeResult.Output -join ' ')"
                        $pnputilFailed += $inf
                    }
                } catch {
                    Write-Warn "Error removing ${inf}: $_"
                    $pnputilFailed += $inf
                }
            }
        } catch {
            Write-Warn "Could not resolve the trusted inbox pnputil executable: $_"
            $pnputilFailed = @($driverPackages)
        }
    } else {
        Write-Info "No $GpuVendor display driver packages found by CIM. Driver package state is already absent."
    }

    if ($pnputilSucceeded.Count -gt 0) {
        try {
            # Re-query CIM rather than parsing pnputil output.  DriverProviderName
            # and ClassGuid remain locale independent. A clean-install handoff
            # requires the complete matching vendor package set to be empty, not
            # merely the originally selected INF names to disappear.
            $postRemovalDrivers = @(Get-CimInstance Win32_PnPSignedDriver -ErrorAction Stop |
                Where-Object { $_.ClassGuid -eq $displayGuid -and $_.DriverProviderName -match $vendorMatch })
            $remainingPackages = @(
                $postRemovalDrivers |
                Where-Object { $_.InfName -match '^oem\d+\.inf$' } |
                ForEach-Object { $_.InfName }
            )
            $postInvalidRows = @($postRemovalDrivers | Where-Object { $_.InfName -notmatch '^oem\d+\.inf$' })
            foreach ($inf in $driverPackages) {
                if ($inf -in $remainingPackages) {
                    Write-Warn "CIM still reports installed driver package: $inf"
                    $failedDrivers++
                } else {
                    Write-OK "Removed and verified absent: $inf"
                    $removedDrivers++
                }
            }
            if ($postInvalidRows.Count -gt 0) {
                Write-Warn "Post-removal CIM returned matching display rows with unusable INF names; cleanup cannot be verified."
            }
            $unexpectedPackages = @($remainingPackages | Where-Object { $_ -notin $driverPackages })
            if ($unexpectedPackages.Count -gt 0) {
                Write-Warn "Post-removal CIM reports additional matching vendor package(s): $($unexpectedPackages -join ', ')"
            }
            $driverRemovalVerified = ($failedDrivers -eq 0 -and $removedDrivers -eq $driverPackages.Count -and
                $remainingPackages.Count -eq 0 -and $postInvalidRows.Count -eq 0)
        } catch {
            # Without a post-removal authoritative query, the package may still
            # be present.  Do not clean related state or report a safe handoff.
            Write-Warn "Could not verify $GpuVendor driver package removal through CIM: $_"
            $failedDrivers = $pnputilFailed.Count
            $unknownDrivers = $pnputilSucceeded.Count
            $driverRemovalVerified = $false
        }
    } elseif (-not $alreadyAbsent) {
        $failedDrivers = $driverPackages.Count
    }

    # A failed or partial pnputil operation leaves the installed driver intact.
    # Fail closed before stopping services or deleting any vendor-owned state.
    if (-not $driverRemovalVerified) {
        $status = if ($removedDrivers -gt 0) { "Partial" } else { "Failed" }
        if ($PassThru) {
            return Get-GpuDriverCleanResult -Status $status -Applied ($removedDrivers -gt 0) -CanCompleteStep $false `
                -FoundDriverPackages $foundDriverPackages -RemovedDriverPackages $removedDrivers -FailedDriverPackages $failedDrivers `
                -UnknownDriverPackages $unknownDrivers `
                -DriverRemovalVerified $driverRemovalVerified -Message 'Driver package removal was not fully successful or could not be verified; no cleanup was performed.'
        }
        return
    }

    $cleanupFailures = 0
    $cleanupSkipped = 0
    $cleanupDeferred = 0
    $serviceMutations = 0
    $taskMutations = 0
    $registryMutations = 0

    # ── 2. Stop and Disable GPU Services ─────────────────────────────────────
    Write-Step "Stopping $GpuVendor services..."

    $servicePatterns = switch ($GpuVendor) {
        "NVIDIA" { @(
            "NVDisplay.ContainerLocalSystem",
            "NvTelemetryContainer",
            "NvContainerNetworkService",
            "NvContainerLocalSystem",
            "NVDisplay*",
            "nvsvc"
        )}
        "AMD" { @(
            "AMD External Events Utility",
            "AMDRyzenMasterDriverV*",
            "amdlog",
            "amdfendr*"
        )}
        "Intel" { @(
            "igfxCUIService*",
            "IntelGraphicsControlPanel*"
        )}
    }

    try {
        # Enumerate once. A successful empty enumeration is authoritative
        # absence; provider failure is not.
        $allServices = @(Get-Service -ErrorAction Stop)
        $services = @($allServices | Where-Object {
            $serviceName = $_.Name
            @($servicePatterns | Where-Object { $serviceName -like $_ }).Count -gt 0
        })
        foreach ($svc in $services) {
            try {
                # Backup service state before disabling so it can be restored
                Backup-ServiceState -ServiceName $svc.Name -StepTitle "GPU Driver Clean ($GpuVendor)"
                Stop-Service $svc.Name -Force -ErrorAction Stop
                Set-Service $svc.Name -StartupType Disabled -ErrorAction Stop
                Write-OK "Stopped + disabled: $($svc.Name)"
                $serviceMutations++
            } catch {
                $cleanupFailures++
                Write-Warn "Could not stop and disable service $($svc.Name): $_"
            }
        }
    } catch {
        $cleanupFailures++
        Write-Warn "Could not enumerate $GpuVendor services authoritatively: $_"
    }

    # ── 1.5. Remove GPU Software / Applications ────────────────────────────
    # Remove vendor companion applications (NVIDIA App, GFE, PhysX, etc.) that
    # persist separately from the display driver package. The Store-delivered
    # NVIDIA Control Panel is intentionally preserved.
    # In Safe Mode, MSI service may not run — use direct file/registry removal.
    Write-Step "Removing $GpuVendor software and applications..."

    $removedApps = 0

    if ($GpuVendor -eq "NVIDIA") {
        # AppXSVC is unavailable in Safe Mode. Phase 3 owns verified AppX and
        # provisioned-package cleanup before installing the replacement driver.
        $cleanupDeferred++
        Write-Info "NVIDIA AppX cleanup deferred to Phase 3 Normal Mode."

        # ── NVIDIA scheduled tasks ──────────────────────────────────────────
        $nvTaskPatterns = @("NvDriverUpdateCheckDaily*", "NVIDIA GeForce*", "NvNodeLauncher*",
                            "NvBackend*", "NvTmRep*", "NvProfileUpdater*", "NvTelemetry*")
        try {
            $allTasks = @(Get-ScheduledTask -ErrorAction Stop)
            $nvTasks = @($allTasks | Where-Object {
                $taskName = $_.TaskName
                $taskPath = [string]$_.TaskPath
                $taskPath -like '\NVIDIA\*' -or
                    @($nvTaskPatterns | Where-Object { $taskName -like $_ }).Count -gt 0
            })
        } catch {
            # Task Scheduler commonly cannot be queried in Safe Mode. Keep the
            # handoff usable, but expose the exact Normal-Mode obligation.
            Write-DebugLog "NVIDIA scheduled-task cleanup deferred to Phase 3: $_"
            $cleanupDeferred++
            $nvTasks = @()
        }
        foreach ($t in $nvTasks) {
            try {
                Unregister-ScheduledTask -TaskName $t.TaskName -TaskPath $t.TaskPath -Confirm:$false -ErrorAction Stop
                Write-OK "Removed task: $($t.TaskPath)$($t.TaskName)"
                $taskMutations++
            } catch {
                $cleanupFailures++
                Write-Warn "Could not remove scheduled task $($t.TaskName): $_"
            }
        }

        # ── NVIDIA program directories ──────────────────────────────────────
        # Fixed vendor descendants only; canonical containment and reparse
        # checks are required before recursive removal.
        foreach ($target in (Get-GpuVendorApplicationCleanupTargets -GpuVendor $GpuVendor)) {
            $dir = $target.Path
            try {
                $dirExists = Test-Path -LiteralPath $dir -ErrorAction Stop
            } catch {
                $cleanupFailures++
                Write-Warn "Could not inspect application directory ${dir}: $_"
                continue
            }
            if ($dirExists) {
                if (-not (Test-GpuCacheCleanupTarget -Root $target.Root -Path $dir)) {
                    Write-Warn "Refusing untrusted or redirected application path: $dir"
                    $cleanupFailures++
                    continue
                }
                try {
                    if (-not (Test-GpuCacheCleanupTarget -Root $target.Root -Path $dir)) {
                        throw 'Cleanup path trust changed before deletion.'
                    }
                    Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction Stop
                    if (Test-Path -LiteralPath $dir -ErrorAction Stop) {
                        throw 'Application directory still exists after removal.'
                    }
                    Write-OK "Removed: $dir"
                    $removedApps++
                } catch {
                    # Some files may be locked even in Safe Mode (system-owned)
                    $cleanupFailures++
                    Write-Warn "Could not remove application directory: $dir — $_"
                }
            }
        }

        # ── Uninstall registry entries ──────────────────────────────────────
        # Clean Programs & Features / Apps & Features entries for NVIDIA software
        # so the system doesn't show stale NVIDIA app entries after driver reinstall.
        $uninstallPaths = @(
            "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"
        )
        $cleanedEntries = 0
        foreach ($regPath in $uninstallPaths) {
            try {
                if (-not (Test-Path -Path $regPath -ErrorAction Stop)) { continue }
                $uninstallEntries = @(Get-ChildItem $regPath -ErrorAction Stop)
            } catch {
                $cleanupFailures++
                Write-Warn "Could not enumerate uninstall registry path ${regPath}: $_"
                continue
            }
            $uninstallEntries | ForEach-Object {
                try {
                    $props = Get-ItemProperty -LiteralPath $_.PSPath -ErrorAction Stop
                } catch {
                    $cleanupFailures++
                    Write-Warn "Could not read uninstall registry entry $($_.PSPath): $_"
                    return
                }
                $pub = if ($props.PSObject.Properties['Publisher'])  { $props.Publisher }  else { "" }
                $dn  = if ($props.PSObject.Properties['DisplayName']){ $props.DisplayName } else { "" }
                if ($pub -match "NVIDIA" -or $dn -match "^NVIDIA ") {
                    try {
                        Remove-Item $_.PSPath -Recurse -Force -ErrorAction Stop
                        Write-DebugLog "Cleaned uninstall entry: $dn"
                        $cleanedEntries++
                        $registryMutations++
                    } catch {
                        $cleanupFailures++
                        Write-Warn "Could not clean uninstall registry entry ${dn}: $_"
                    }
                }
            }
        }
        if ($cleanedEntries -gt 0) {
            Write-OK "Cleaned $cleanedEntries NVIDIA uninstall registry entries."
        }

    } elseif ($GpuVendor -eq "AMD") {
        # AMD software directories use the same fixed-root trust boundary.
        foreach ($target in (Get-GpuVendorApplicationCleanupTargets -GpuVendor $GpuVendor)) {
            $dir = $target.Path
            try {
                $dirExists = Test-Path -LiteralPath $dir -ErrorAction Stop
            } catch {
                $cleanupFailures++
                Write-Warn "Could not inspect application directory ${dir}: $_"
                continue
            }
            if ($dirExists) {
                if (-not (Test-GpuCacheCleanupTarget -Root $target.Root -Path $dir)) {
                    Write-Warn "Refusing untrusted or redirected application path: $dir"
                    $cleanupFailures++
                    continue
                }
                try {
                    if (-not (Test-GpuCacheCleanupTarget -Root $target.Root -Path $dir)) {
                        throw 'Cleanup path trust changed before deletion.'
                    }
                    Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction Stop
                    if (Test-Path -LiteralPath $dir -ErrorAction Stop) {
                        throw 'Application directory still exists after removal.'
                    }
                    Write-OK "Removed: $dir"
                    $removedApps++
                } catch {
                    $cleanupFailures++
                    Write-Warn "Could not remove application directory: $dir — $_"
                }
            }
        }
    }
    Write-Info "$removedApps $GpuVendor software items removed."

    # Intentionally do not delete display-class keys. Their entries are shared
    # device configuration, and a broad provider/description match cannot prove
    # that an entry belongs only to the INF packages removed above.

    # Driver absence is now authoritative, whether established by this run or
    # already present at entry. Inspect and remove vendor residue in both cases.
    if ($GpuVendor -eq "NVIDIA") {
        $nvRegPaths = @(
            "HKLM:\SOFTWARE\NVIDIA Corporation",
            "HKCU:\SOFTWARE\NVIDIA Corporation",
            "HKLM:\SOFTWARE\WOW6432Node\NVIDIA Corporation"
        )
        foreach ($p in $nvRegPaths) {
            try {
                if (Test-Path -Path $p -ErrorAction Stop) {
                    Remove-Item $p -Recurse -Force -ErrorAction Stop
                    if (Test-Path -Path $p -ErrorAction Stop) { throw 'Registry path still exists after removal.' }
                    Write-OK "Registry cleaned: $p"
                    $registryMutations++
                }
            } catch {
                $cleanupFailures++
                Write-Warn "Could not inspect or clean registry path ${p}: $_"
            }
        }
    } elseif ($GpuVendor -eq "AMD") {
        $amdRegPaths = @(
            "HKLM:\SOFTWARE\AMD",
            "HKLM:\SOFTWARE\ATI Technologies",
            "HKCU:\SOFTWARE\AMD"
        )
        foreach ($p in $amdRegPaths) {
            try {
                if (Test-Path -Path $p -ErrorAction Stop) {
                    Remove-Item $p -Recurse -Force -ErrorAction Stop
                    if (Test-Path -Path $p -ErrorAction Stop) { throw 'Registry path still exists after removal.' }
                    Write-OK "Registry cleaned: $p"
                    $registryMutations++
                }
            } catch {
                $cleanupFailures++
                Write-Warn "Could not inspect or clean registry path ${p}: $_"
            }
        }
    }

    # pnputil owns FileRepository reference accounting. Never remove its raw
    # folders manually; that can corrupt a package which failed to uninstall.
    $cleanedFolders = 0
    $lockedFolders = 0

    # ── 3. Clean rebuildable shader caches ───────────────────────────────────
    Write-Step "Cleaning shader caches..."

    foreach ($target in (Get-GpuCacheCleanupTargets -GpuVendor $GpuVendor)) {
        $cp = $target.Path
        try {
            $cacheExists = Test-Path -LiteralPath $cp -ErrorAction Stop
        } catch {
            $lockedFolders++
            $cleanupFailures++
            Write-Warn "Could not inspect cache path ${cp}: $_"
            continue
        }
        if ($cacheExists) {
            if (-not (Test-GpuCacheCleanupTarget -Root $target.Root -Path $cp)) {
                Write-Warn "Refusing untrusted or redirected cache path: $cp"
                $lockedFolders++
                $cleanupFailures++
                continue
            }
            try {
                if (-not (Test-GpuCacheCleanupTarget -Root $target.Root -Path $cp)) {
                    throw 'Cleanup path trust changed before deletion.'
                }
                Remove-Item -LiteralPath $cp -Recurse -Force -ErrorAction Stop
                if (Test-Path -LiteralPath $cp -ErrorAction Stop) { throw 'Cache path still exists after removal.' }
                Write-OK "Cache cleaned: $cp"
                $cleanedFolders++
            } catch {
                Write-Warn "Partial cache clean: $cp — some files locked: $_"
                $lockedFolders++
                $cleanupFailures++
            }
        }
    }

    # ── Summary ──────────────────────────────────────────────────────────────
    $alreadyAbsent = ($foundDriverPackages -eq 0 -and $cimEnumerationSucceeded)
    $cleanupStatus = if ($alreadyAbsent) {
        "AlreadyAbsent"
    } elseif ($driverPackages.Count -eq 0) {
        "Failed"
    } elseif ($removedDrivers -gt 0 -and $failedDrivers -eq 0) {
        "Success"
    } elseif ($removedDrivers -gt 0) {
        "Partial"
    } else {
        "Failed"
    }
    # A package removal alone is insufficient for a clean-install handoff. Any
    # discovered target that could not be removed is explicitly partial. Work
    # that fundamentally requires Normal Mode is separately exposed as deferred.
    if (($cleanupStatus -eq 'Success' -or $cleanupStatus -eq 'AlreadyAbsent') -and
        ($cleanupFailures -gt 0 -or $cleanupSkipped -gt 0)) {
        $cleanupStatus = 'Partial'
    }
    $cleanupMessage = switch ($cleanupStatus) {
        "Success" { "Driver cleanup removed $removedDrivers package(s); $cleanupDeferred Normal-Mode cleanup obligation(s) deferred." }
        "AlreadyAbsent" { "No $GpuVendor display driver packages are present; Safe Mode residue cleanup completed and $cleanupDeferred Normal-Mode obligation(s) were deferred." }
        "Partial" {
            "Driver cleanup is incomplete: $failedDrivers failed driver package(s), $unknownDrivers unknown package(s), $cleanupFailures cleanup failure(s), $cleanupSkipped unclassified skip(s)."
        }
        default {
            if ($driverPackages.Count -eq 0) {
                "No display driver packages were found; removal was not verified."
            } else {
                "No display driver packages were removed."
            }
        }
    }

    Write-Blank
    $canCompleteStep = (($cleanupStatus -eq "Success" -or $cleanupStatus -eq "AlreadyAbsent") -and
        $driverRemovalVerified -and $cleanupFailures -eq 0 -and $cleanupSkipped -eq 0)
    $summaryColor = if ($canCompleteStep) { "Green" } elseif ($cleanupStatus -eq "Partial") { "Yellow" } else { "Red" }
    $summaryTitle = if ($cleanupStatus -eq "Success") { "GPU DRIVER CLEAN REMOVAL COMPLETE" } elseif ($cleanupStatus -eq "AlreadyAbsent") { "GPU DRIVER CLEAN ALREADY ABSENT" } elseif ($cleanupStatus -eq "Partial") { "GPU DRIVER CLEAN REMOVAL PARTIAL" } else { "GPU DRIVER CLEAN REMOVAL NOT VERIFIED" }
    Write-ConsoleLine "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor $summaryColor
    Write-ConsoleLine "  │  $summaryTitle$((' ' * (58 - $summaryTitle.Length)))│" -ForegroundColor $summaryColor
    Write-ConsoleLine "  │                                                              │" -ForegroundColor $summaryColor
    Write-ConsoleLine "  │  Vendor:          $GpuVendor$((' ' * (39 - $GpuVendor.Length)))│" -ForegroundColor White
    Write-ConsoleLine "  │  Software removed:$removedApps$((' ' * (39 - "$removedApps".Length)))│" -ForegroundColor White
    Write-ConsoleLine "  │  Drivers removed: $removedDrivers$((' ' * (39 - "$removedDrivers".Length)))│" -ForegroundColor White
    Write-ConsoleLine "  │  Folders cleaned: $cleanedFolders$((' ' * (39 - "$cleanedFolders".Length)))│" -ForegroundColor White
    Write-ConsoleLine "  │                                                              │" -ForegroundColor Green
    if ($canCompleteStep) {
        Write-ConsoleLine "  │  Ready for clean driver installation.                       │" -ForegroundColor White
    } else {
        Write-ConsoleLine "  │  Review warnings before continuing to driver installation.   │" -ForegroundColor White
    }
    Write-ConsoleLine "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor $summaryColor

    if ($PassThru) {
        $anyMutation = ($removedDrivers + $removedApps + $cleanedFolders + $serviceMutations +
            $taskMutations + $registryMutations) -gt 0
        return Get-GpuDriverCleanResult -Status $cleanupStatus -Applied $anyMutation -CanCompleteStep $canCompleteStep `
            -FoundDriverPackages $foundDriverPackages -RemovedDriverPackages $removedDrivers -FailedDriverPackages $failedDrivers `
            -UnknownDriverPackages $unknownDrivers `
            -DriverRemovalVerified $driverRemovalVerified -AlreadyAbsent $alreadyAbsent -SoftwareRemoved $removedApps `
            -FoldersCleaned $cleanedFolders -LockedFolders $lockedFolders -CleanupFailures $cleanupFailures -CleanupSkipped $cleanupSkipped `
            -CleanupDeferred $cleanupDeferred `
            -Message $cleanupMessage
    }
}
