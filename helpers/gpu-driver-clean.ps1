# ==============================================================================
#  helpers/gpu-driver-clean.ps1  —  Safe Mode GPU Driver Removal (DDU replacement)
# ==============================================================================

function Remove-GpuDriverClean {
    <#
    .SYNOPSIS  Removes GPU drivers cleanly in Safe Mode. Pure PowerShell replacement
               for Display Driver Uninstaller (DDU).
    .DESCRIPTION
        1. Stops and disables GPU-related services
        2. Removes GPU driver packages (CIM primary, pnputil text-parsing fallback)
        3. Cleans GPU registry entries
        4. Removes DriverStore orphans
        5. Cleans shader caches and temp folders
    #>
    [CmdletBinding()]
    param(
        [ValidateSet("NVIDIA","AMD","Intel")]
        [string]$GpuVendor = "NVIDIA"
    )

    Write-Step "GPU Driver Clean Removal — $GpuVendor"
    Write-Info "This replaces DDU with native PowerShell commands."

    if ($SCRIPT:DryRun) {
        Write-Host "  [DRY-RUN] Would perform complete GPU driver removal for $GpuVendor" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   1. Stop + disable GPU services" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   1.5 Remove GPU software (AppX, program files, tasks, registry)" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   2. Remove driver packages via pnputil" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   3. Clean GPU class registry keys" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   4. Remove DriverStore orphan folders" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   5. Clean shader caches" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN] No files or registry entries will be modified." -ForegroundColor Magenta
        return
    }

    # ── 1. Stop and Disable GPU Services ─────────────────────────────────────
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

    foreach ($pattern in $servicePatterns) {
        $services = Get-Service -Name $pattern -ErrorAction SilentlyContinue
        foreach ($svc in $services) {
            try {
                # Backup service state before disabling so it can be restored
                Backup-ServiceState -ServiceName $svc.Name -StepTitle "GPU Driver Clean ($GpuVendor)"
                Stop-Service $svc.Name -Force -ErrorAction SilentlyContinue
                Set-Service $svc.Name -StartupType Disabled -ErrorAction SilentlyContinue
                Write-OK "Stopped + disabled: $($svc.Name)"
            } catch {
                Write-DebugLog "Service $($svc.Name): $_"
            }
        }
    }

    # ── 1.5. Remove GPU Software / Applications ────────────────────────────
    # Remove vendor applications (NVIDIA App, Control Panel, GFE, PhysX, etc.)
    # that persist separately from the display driver package.
    # In Safe Mode, MSI service may not run — use direct file/registry removal.
    Write-Step "Removing $GpuVendor software and applications..."

    $removedApps = 0

    if ($GpuVendor -eq "NVIDIA") {
        # ── AppX / MSIX packages (NVIDIA App, NVIDIA Control Panel from Store) ──
        if (Get-Command Get-AppxPackage -ErrorAction SilentlyContinue) {
            # NOTE: Get-AppxPackage / Get-AppxProvisionedPackage throw terminating
            # errors in Safe Mode because AppXSVC cannot start. -ErrorAction
            # SilentlyContinue only suppresses non-terminating errors, so we must
            # wrap these calls in try/catch.
            try {
                $nvAppx = Get-AppxPackage -AllUsers -ErrorAction Stop |
                    Where-Object { $_.Name -match "NVIDIA" }
            } catch {
                Write-DebugLog "AppX enumeration unavailable (expected in Safe Mode): $_"
                $nvAppx = @()
            }
            foreach ($pkg in $nvAppx) {
                try {
                    Remove-AppxPackage -Package $pkg.PackageFullName -AllUsers -ErrorAction Stop
                    Write-OK "Removed AppX: $($pkg.Name)"
                    $removedApps++
                } catch {
                    Write-DebugLog "AppX removal (expected in Safe Mode): $($pkg.Name) — $_"
                }
            }
            # Remove provisioned packages to prevent reinstall on feature updates
            try {
                $nvProvisioned = Get-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue |
                    Where-Object { $_.DisplayName -match "NVIDIA" }
                foreach ($pkg in $nvProvisioned) {
                    try {
                        $pkg | Remove-AppxProvisionedPackage -Online -ErrorAction Stop | Out-Null
                        Write-OK "Removed provisioned: $($pkg.DisplayName)"
                    } catch {
                        Write-DebugLog "Provisioned removal: $($pkg.DisplayName) — $_"
                    }
                }
            } catch { Write-DebugLog "Provisioned package enumeration: $_" }
        } else {
            Write-DebugLog "AppX cmdlets not available — skipping AppX removal."
        }

        # ── NVIDIA scheduled tasks ──────────────────────────────────────────
        $nvTaskPatterns = @("NvDriverUpdateCheckDaily*", "NVIDIA GeForce*", "NvNodeLauncher*",
                            "NvBackend*", "NvTmRep*", "NvProfileUpdater*", "NvTelemetry*")
        foreach ($pattern in $nvTaskPatterns) {
            $tasks = Get-ScheduledTask -TaskName $pattern -ErrorAction SilentlyContinue
            foreach ($t in $tasks) {
                try {
                    Unregister-ScheduledTask -TaskName $t.TaskName -Confirm:$false -ErrorAction Stop
                    Write-OK "Removed task: $($t.TaskName)"
                } catch { Write-DebugLog "Task removal: $($t.TaskName) — $_" }
            }
        }
        # Also check \NVIDIA\ task folder
        $nvFolderTasks = Get-ScheduledTask -TaskPath "\NVIDIA\*" -ErrorAction SilentlyContinue
        foreach ($t in $nvFolderTasks) {
            try {
                Unregister-ScheduledTask -TaskName $t.TaskName -TaskPath $t.TaskPath -Confirm:$false -ErrorAction Stop
                Write-OK "Removed task: $($t.TaskPath)$($t.TaskName)"
            } catch { Write-DebugLog "Task removal: $($t.TaskName) — $_" }
        }

        # ── NVIDIA program directories ──────────────────────────────────────
        # Remove all NVIDIA application files. In Safe Mode, files are unlocked.
        $nvDirs = @(
            "$env:ProgramFiles\NVIDIA Corporation",
            "${env:ProgramFiles(x86)}\NVIDIA Corporation",
            "$env:ProgramData\NVIDIA Corporation",
            "$env:ProgramData\NVIDIA",
            "$env:LOCALAPPDATA\NVIDIA Corporation",
            "$env:LOCALAPPDATA\NVIDIA"
        )
        foreach ($dir in $nvDirs) {
            if (Test-Path $dir) {
                try {
                    Remove-Item $dir -Recurse -Force -ErrorAction Stop
                    Write-OK "Removed: $dir"
                    $removedApps++
                } catch {
                    # Some files may be locked even in Safe Mode (system-owned)
                    Write-Warn "Partial removal: $dir — some files locked"
                    $removedApps++
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
            if (-not (Test-Path $regPath)) { continue }
            Get-ChildItem $regPath -ErrorAction SilentlyContinue | ForEach-Object {
                $props = Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue
                if (-not $props) { return }  # skip keys that can't be read
                $pub = if ($props.PSObject.Properties['Publisher'])  { $props.Publisher }  else { "" }
                $dn  = if ($props.PSObject.Properties['DisplayName']){ $props.DisplayName } else { "" }
                if ($pub -match "NVIDIA" -or $dn -match "^NVIDIA ") {
                    try {
                        Remove-Item $_.PSPath -Recurse -Force -ErrorAction Stop
                        Write-DebugLog "Cleaned uninstall entry: $dn"
                        $cleanedEntries++
                    } catch {
                        Write-DebugLog "Failed to clean uninstall entry: $dn — $_"
                    }
                }
            }
        }
        if ($cleanedEntries -gt 0) {
            Write-OK "Cleaned $cleanedEntries NVIDIA uninstall registry entries."
        }

    } elseif ($GpuVendor -eq "AMD") {
        # AMD software directories
        $amdDirs = @(
            "$env:ProgramFiles\AMD",
            "${env:ProgramFiles(x86)}\AMD",
            "$env:LOCALAPPDATA\AMD",
            "$env:ProgramData\AMD"
        )
        foreach ($dir in $amdDirs) {
            if (Test-Path $dir) {
                try {
                    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
                    Write-OK "Removed: $dir"
                    $removedApps++
                } catch {
                    Write-Warn "Partial removal: $dir"
                    $removedApps++
                }
            }
        }
    }
    Write-Info "$removedApps $GpuVendor software items removed."

    # ── 2. Remove GPU Driver Packages via pnputil ────────────────────────────
    Write-Step "Enumerating GPU driver packages..."

    $vendorMatch = switch ($GpuVendor) {
        "NVIDIA" { "nvidia" }
        "AMD"    { "amd|ati|radeon" }
        "Intel"  { "intel" }
    }

    $driverPackages = @()

    # Primary method: CIM/WMI query — works on all Windows editions.
    # Get-CimInstance requires PS 3.0+ (Windows 10/11 always have it).
    # NOTE: Win32_PnPSignedDriver.DeviceClass is the localized class description (e.g.,
    # "Display adapters" in English, "Grafikkarten" in German). Use ClassGuid instead —
    # it's the device setup class GUID which is always {4d36e968-...} for display adapters.
    try {
        $displayGuid = $CFG_GUID_Display  # {4d36e968-e325-11ce-bfc1-08002be10318}
        $cimDrivers = Get-CimInstance Win32_PnPSignedDriver -ErrorAction Stop |
            Where-Object { $_.ClassGuid -eq $displayGuid -and $_.DriverProviderName -match $vendorMatch }

        foreach ($drv in $cimDrivers) {
            if ($drv.InfName -match "^oem\d+\.inf$") {
                $driverPackages += $drv.InfName
            }
        }

        if ($driverPackages.Count -gt 0) {
            Write-OK "CIM enumeration found $($driverPackages.Count) $GpuVendor display driver(s)."
        } else {
            Write-DebugLog "CIM query returned no matching display drivers."
        }
    } catch {
        Write-DebugLog "CIM enumeration failed: $_ — falling back to pnputil parsing"
    }

    # Fallback: parse pnputil text output. Field labels are English-only
    # ("Published Name", "Class Name", "Driver Package Provider") so this method
    # will not find drivers on non-English Windows installations. The CIM method
    # above is locale-independent and should be preferred.
    if ($driverPackages.Count -eq 0) {
        Write-DebugLog "Trying pnputil text parsing fallback..."
        $pnpOutput = pnputil /enum-drivers 2>&1
        $currentInf = $null
        $currentClass = $null
        $currentProvider = $null

        foreach ($line in $pnpOutput) {
            if ($line -match "Published Name\s*:\s*(oem\d+\.inf)") {
                $currentInf = $Matches[1]
            }
            if ($line -match "Class Name\s*:\s*(.+)") {
                $currentClass = $Matches[1].Trim()
            }
            if ($line -match "Driver Package Provider\s*:\s*(.+)") {
                $currentProvider = $Matches[1].Trim()
            }
            if (($line.Trim() -eq "" -or $line -match "^$") -and $currentInf) {
                if ($currentClass -match "Display" -and $currentProvider -match $vendorMatch) {
                    if ($currentInf -notin $driverPackages) {
                        $driverPackages += $currentInf
                    }
                }
                $currentInf = $null
                $currentClass = $null
                $currentProvider = $null
            }
        }
        # Check last entry (pnputil may not end with blank line)
        if ($currentInf -and $currentClass -match "Display" -and $currentProvider -match $vendorMatch) {
            if ($currentInf -notin $driverPackages) {
                $driverPackages += $currentInf
            }
        }

        if ($driverPackages.Count -gt 0) {
            Write-OK "pnputil fallback found $($driverPackages.Count) $GpuVendor display driver(s)."
        } else {
            Write-DebugLog "pnputil parsing found no matching drivers (expected on non-English Windows)."
        }
    }

    if ($driverPackages.Count -eq 0) {
        Write-Warn "No $GpuVendor display driver packages found. Driver may already be removed."
        Write-Warn "If on non-English Windows, pnputil text parsing may have failed due to locale. Run 'pnputil /enum-drivers' manually to verify."
        Write-Warn "Continuing with registry and cache cleanup."
    }

    $removedDrivers = 0
    $failedDrivers = 0
    foreach ($inf in $driverPackages) {
        # SECURITY: Validate inf filename format — must be oem<digits>.inf only.
        # The $inf values come from CIM query or pnputil text parsing. If either source
        # is compromised or the regex is tricked, an attacker could inject arbitrary
        # pnputil arguments. Strict validation prevents command injection.
        if ($inf -notmatch '^oem\d+\.inf$') {
            Write-Warn "Skipping invalid driver package name: $inf (expected oem<N>.inf format)"
            $failedDrivers++
            continue
        }
        Write-Step "Removing driver package: $inf"
        try {
            $result = pnputil /delete-driver $inf /uninstall /force 2>&1
            if ($LASTEXITCODE -eq 0) {
                Write-OK "Removed: $inf"
                $removedDrivers++
            } else {
                Write-Warn "Could not remove $inf (exit $LASTEXITCODE): $result"
                $failedDrivers++
            }
        } catch {
            Write-Warn "Error removing ${inf}: $_"
            $failedDrivers++
        }
    }
    Write-Info "$removedDrivers driver package(s) removed$(if($failedDrivers){", $failedDrivers failed"})."
    if ($failedDrivers -gt 0) {
        Write-Warn "Some drivers could not be removed. If in Normal Mode, try rebooting into Safe Mode."
        Write-Warn "Locked driver files are the most common cause — Safe Mode unlocks them."
    }

    # ── 3. Clean GPU Class Registry ──────────────────────────────────────────
    Write-Step "Cleaning GPU registry entries..."

    $classPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\$CFG_GUID_Display"

    if (Test-Path $classPath) {
        $subkeys = Get-ChildItem $classPath -ErrorAction SilentlyContinue |
            Where-Object { $_.PSChildName -match "^\d{4}$" }

        foreach ($key in $subkeys) {
            try {
                $props = Get-ItemProperty $key.PSPath -ErrorAction SilentlyContinue
                if (-not $props) { continue }
                $prov = if ($props.PSObject.Properties['ProviderName']) { $props.ProviderName } else { "" }
                $desc = if ($props.PSObject.Properties['DriverDesc'])   { $props.DriverDesc }   else { "" }
                $match = switch ($GpuVendor) {
                    "NVIDIA" { $prov -match "NVIDIA" -or $desc -match "NVIDIA" }
                    "AMD"    { $prov -match "AMD|ATI" -or $desc -match "AMD|Radeon" }
                    "Intel"  { $prov -match "Intel"   -or $desc -match "Intel.*Graphics" }
                }
                if ($match) {
                    Remove-Item $key.PSPath -Recurse -Force -ErrorAction SilentlyContinue
                    Write-OK "Registry cleaned: $($key.PSChildName) ($desc)"
                }
            } catch {
                Write-DebugLog "Registry key $($key.PSChildName): $_"
            }
        }
    }

    # Clean vendor registry paths only if at least one driver was actually removed.
    # If all removals failed, the driver is still loaded and deleting its registry
    # config would leave it in an inconsistent state.
    if ($failedDrivers -gt 0 -and $removedDrivers -eq 0) {
        Write-Warn "Skipping vendor registry cleanup — no drivers were successfully removed."
    } elseif ($failedDrivers -gt 0) {
        Write-Warn "Partial removal ($removedDrivers removed, $failedDrivers failed) — skipping vendor registry cleanup to avoid inconsistent state."
    } elseif ($GpuVendor -eq "NVIDIA") {
        $nvRegPaths = @(
            "HKLM:\SOFTWARE\NVIDIA Corporation",
            "HKCU:\SOFTWARE\NVIDIA Corporation",
            "HKLM:\SOFTWARE\WOW6432Node\NVIDIA Corporation"
        )
        foreach ($p in $nvRegPaths) {
            if (Test-Path $p) {
                Remove-Item $p -Recurse -Force -ErrorAction SilentlyContinue
                Write-OK "Registry cleaned: $p"
            }
        }
    } elseif ($GpuVendor -eq "AMD") {
        $amdRegPaths = @(
            "HKLM:\SOFTWARE\AMD",
            "HKLM:\SOFTWARE\ATI Technologies",
            "HKCU:\SOFTWARE\AMD"
        )
        foreach ($p in $amdRegPaths) {
            if (Test-Path $p) {
                Remove-Item $p -Recurse -Force -ErrorAction SilentlyContinue
                Write-OK "Registry cleaned: $p"
            }
        }
    }

    # ── 4. Clean DriverStore Orphans ─────────────────────────────────────────
    Write-Step "Cleaning DriverStore orphan folders..."

    $driverStore = "$env:SystemRoot\System32\DriverStore\FileRepository"
    # Use precise patterns to avoid matching non-GPU drivers (e.g., nvdimm, amdppm)
    $patterns = switch ($GpuVendor) {
        "NVIDIA" { @("nv_dispi*","nvdsp*","nvlddmkm*","nview*","nvhdc*","nvmdi*") }
        "AMD"    { @("atiilhag*","atiumdag*","amdkmdap*","amdxe*","atihdw*","radeon*") }
        "Intel"  { @("iigd_dch*","igfx*","igd10*","igd11*","igd12*") }
    }

    $cleanedFolders = 0
    $lockedFolders = 0
    if (Test-Path $driverStore) {
        foreach ($p in $patterns) {
            $folders = Get-ChildItem $driverStore -Directory -Filter $p -ErrorAction SilentlyContinue
            foreach ($f in $folders) {
                try {
                    Remove-Item $f.FullName -Recurse -Force -ErrorAction Stop
                    Write-OK "DriverStore cleaned: $($f.Name)"
                    $cleanedFolders++
                } catch {
                    Write-Warn "Could not remove $($f.Name) (locked): $_"
                    $lockedFolders++
                }
            }
        }
    }
    Write-Info "$cleanedFolders DriverStore folders cleaned$(if($lockedFolders){", $lockedFolders locked (will be removed on next reboot)"})."

    # ── 5. Clean Shader Caches ───────────────────────────────────────────────
    Write-Step "Cleaning shader caches..."

    $cachePaths = switch ($GpuVendor) {
        "NVIDIA" { @(
            "$env:LOCALAPPDATA\NVIDIA\DXCache",
            "$env:LOCALAPPDATA\NVIDIA\GLCache",
            "$env:TEMP\NVIDIA Corporation",
            "$env:ProgramData\NVIDIA Corporation"
        )}
        "AMD" { @(
            "$env:LOCALAPPDATA\AMD\DxCache",
            "$env:LOCALAPPDATA\AMD\GLCache",
            "$env:TEMP\AMDRSServ"
        )}
        "Intel" { @(
            "$env:LOCALAPPDATA\Intel"
        )}
    }

    # Common shader caches
    $cachePaths += "$env:LOCALAPPDATA\D3DSCache"

    foreach ($cp in $cachePaths) {
        if (Test-Path $cp) {
            $items = @(Get-ChildItem $cp -Recurse -File -ErrorAction SilentlyContinue)
            $count = $items.Count
            try {
                Remove-Item $cp -Recurse -Force -ErrorAction Stop
                Write-OK "Cache cleaned: $cp ($count items)"
            } catch {
                Write-Warn "Partial cache clean: $cp — some files locked: $_"
            }
        }
    }

    # ── Summary ──────────────────────────────────────────────────────────────
    Write-Blank
    Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Green
    Write-Host "  │  GPU DRIVER CLEAN REMOVAL COMPLETE                          │" -ForegroundColor Green
    Write-Host "  │                                                              │" -ForegroundColor Green
    Write-Host "  │  Vendor:          $GpuVendor$((' ' * (39 - $GpuVendor.Length)))│" -ForegroundColor White
    Write-Host "  │  Software removed:$removedApps$((' ' * (39 - "$removedApps".Length)))│" -ForegroundColor White
    Write-Host "  │  Drivers removed: $removedDrivers$((' ' * (39 - "$removedDrivers".Length)))│" -ForegroundColor White
    Write-Host "  │  Folders cleaned: $cleanedFolders$((' ' * (39 - "$cleanedFolders".Length)))│" -ForegroundColor White
    Write-Host "  │                                                              │" -ForegroundColor Green
    Write-Host "  │  Ready for clean driver installation.                       │" -ForegroundColor White
    Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Green
}
