# ==============================================================================
#  helpers/debloat.ps1  —  Targeted Bloatware + Telemetry Removal
# ==============================================================================

function Get-GamingDebloatPackageNames {
    @(
        "Microsoft.BingNews",
        "Microsoft.BingWeather",
        "Microsoft.GetHelp",
        "Microsoft.Getstarted",
        "Microsoft.MicrosoftOfficeHub",
        "Microsoft.MicrosoftSolitaireCollection",
        "Microsoft.People",
        "Microsoft.Todos",
        "Microsoft.WindowsFeedbackHub",
        "Microsoft.YourPhone",
        "Microsoft.Windows.PhoneLink",
        "MicrosoftCorporationII.PhoneLink",
        "Microsoft.WindowsMaps",
        "Microsoft.ZuneMusic",
        "Microsoft.ZuneVideo",
        "Clipchamp.Clipchamp",
        "Microsoft.549981C3F5F10",              # Cortana
        "Microsoft.MixedReality.Portal",
        "Microsoft.SkypeApp",
        "Microsoft.WindowsCommunicationsApps",  # Mail & Calendar
        "Microsoft.OutlookForWindows",
        "Microsoft.Windows.DevHome",
        "MSTeams",
        "Microsoft.BingSearch",
        "Microsoft.PowerAutomateDesktop"
    )
}

function Get-GamingDebloatTelemetryServices {
    @(
        @{ Name = "DiagTrack";        Label = "Connected User Experiences and Telemetry" },
        @{ Name = "dmwappushservice"; Label = "Device Management WAP Push"; Optional = $true }
    )
}

function Get-GamingDebloatTelemetryTaskPaths {
    @(
        "\Microsoft\Windows\Application Experience\",
        "\Microsoft\Windows\Customer Experience Improvement Program\"
    )
}

function Get-GamingDebloatInventory {
    param(
        [string[]]$PackageNames = (Get-GamingDebloatPackageNames),
        [hashtable[]]$TelemetryServices = (Get-GamingDebloatTelemetryServices),
        [string[]]$TaskPaths = (Get-GamingDebloatTelemetryTaskPaths)
    )

    $appxAvailable = [bool](Get-Command Get-AppxPackage -ErrorAction SilentlyContinue)
    $installedPackages = @()
    $provisionedPackages = @()

    if ($appxAvailable) {
        foreach ($pkg in $PackageNames) {
            $apps = @(Get-AppxPackage -Name $pkg -AllUsers -ErrorAction SilentlyContinue | Where-Object { $null -ne $_ })
            foreach ($app in $apps) {
                $installedPackages += [PSCustomObject]@{
                    Name            = $pkg
                    PackageFullName = $app.PackageFullName
                    Package         = $app
                }
            }
        }

        if (Get-Command Get-AppxProvisionedPackage -ErrorAction SilentlyContinue) {
            $allProvisioned = @(Get-AppxProvisionedPackage -Online:$true -ErrorAction SilentlyContinue | Where-Object { $null -ne $_ })
            foreach ($pkg in $PackageNames) {
                $provisionedMatches = @($allProvisioned | Where-Object { $_.DisplayName -eq $pkg })
                foreach ($match in $provisionedMatches) {
                    $provisionedPackages += [PSCustomObject]@{
                        Name        = $pkg
                        PackageName = $match.PackageName
                        Package     = $match
                    }
                }
            }
        }
    }

    $serviceStates = @()
    foreach ($svc in $TelemetryServices) {
        $svcObj = Get-Service -Name $svc.Name -ErrorAction SilentlyContinue
        $exists = $null -ne $svcObj
        $startType = if ($exists) { $svcObj.StartType } else { $null }
        $status = if ($exists) { $svcObj.Status } else { $null }
        $isOptional = $svc.Contains('Optional') -and $svc.Optional
        $serviceStates += [PSCustomObject]@{
            Name         = $svc.Name
            Label        = $svc.Label
            Optional     = $isOptional
            Exists       = $exists
            StartType    = $startType
            Status       = $status
            NeedsDisable = ($exists -and $startType -ne 'Disabled')
        }
    }

    $taskPathStates = @()
    $taskStates = @()
    foreach ($tp in $TaskPaths) {
        $tasks = @(Get-ScheduledTask -TaskPath $tp -ErrorAction SilentlyContinue | Where-Object { $null -ne $_ })
        $taskPathStates += [PSCustomObject]@{
            TaskPath = $tp
            Exists   = ($tasks.Count -gt 0)
        }

        foreach ($t in $tasks) {
            $state = if ($t.PSObject.Properties['State']) { $t.State } else { $null }
            $taskStates += [PSCustomObject]@{
                TaskName     = $t.TaskName
                TaskPath     = $t.TaskPath
                State        = $state
                NeedsDisable = (-not ($t.PSObject.Properties['State'] -and $t.State -eq "Disabled"))
            }
        }
    }

    [PSCustomObject]@{
        AppxAvailable       = $appxAvailable
        InstalledPackages   = @($installedPackages)
        ProvisionedPackages = @($provisionedPackages)
        Services            = @($serviceStates)
        TaskPaths           = @($taskPathStates)
        Tasks               = @($taskStates)
    }
}

function Write-GamingDebloatInventorySummary {
    param([Parameter(Mandatory = $true)]$Inventory)

    Write-Step "Debloat preflight inventory..."

    if (-not $Inventory.AppxAvailable) {
        Write-Info "AppX cmdlets not available; package removal will be skipped."
    } else {
        Write-Info "$(@($Inventory.InstalledPackages).Count) installed AppX package instance(s) matched."
        Write-Info "$(@($Inventory.ProvisionedPackages).Count) provisioned AppX package(s) matched."
        $packageNames = @()
        foreach ($pkg in @($Inventory.InstalledPackages)) {
            if ($pkg -and $pkg.PSObject.Properties['Name']) { $packageNames += $pkg.Name }
        }
        foreach ($pkg in @($Inventory.ProvisionedPackages)) {
            if ($pkg -and $pkg.PSObject.Properties['Name']) { $packageNames += $pkg.Name }
        }
        foreach ($name in @($packageNames | Where-Object { $_ } | Select-Object -Unique)) {
            Write-Sub "AppX: $name"
        }
    }

    $serviceChanges = @($Inventory.Services | Where-Object { $_.NeedsDisable })
    Write-Info "$($serviceChanges.Count) telemetry service(s) need disabling."
    foreach ($svc in $serviceChanges) {
        Write-Sub "Service: $($svc.Name) ($($svc.StartType))"
    }

    $taskChanges = @($Inventory.Tasks | Where-Object { $_.NeedsDisable })
    Write-Info "$($taskChanges.Count) telemetry scheduled task(s) need disabling."
    foreach ($task in $taskChanges) {
        Write-Sub "Task: $($task.TaskPath)$($task.TaskName)"
    }
}

function Invoke-GamingDebloat {
    <#
    .SYNOPSIS  Removes known bloatware AppX packages, disables telemetry services
               and scheduled tasks. Pure PowerShell — no external tools.
    #>

    $packageNames = Get-GamingDebloatPackageNames
    $telemetryServices = Get-GamingDebloatTelemetryServices
    $taskPaths = Get-GamingDebloatTelemetryTaskPaths
    $inventory = Get-GamingDebloatInventory -PackageNames $packageNames -TelemetryServices $telemetryServices -TaskPaths $taskPaths
    Write-GamingDebloatInventorySummary -Inventory $inventory

    Write-Step "Removing bloatware AppX packages..."

    # Windows Server Core / LTSC may not have the Appx module at all.
    # Guard against CommandNotFoundException to avoid a hard failure.
    if (-not $inventory.AppxAvailable) {
        Write-Info "AppX cmdlets not available (Windows Server Core or LTSC). Skipping package removal."
        # Fall through to telemetry services and consumer features below
    } else {

    $removedCount = 0
    $provisionedRemovedCount = 0
    foreach ($pkg in $packageNames) {
        $apps = @($inventory.InstalledPackages | Where-Object { $_.Name -eq $pkg })
        if ($apps) {
            if (-not $SCRIPT:DryRun) {
                try {
                    $apps | ForEach-Object { $_.Package } | Remove-AppxPackage -AllUsers -ErrorAction Stop
                    Write-OK "Removed: $pkg"
                    $removedCount++
                } catch {
                    Write-DebugLog "Failed to remove $($pkg): $_"
                }
            } else {
                Write-ConsoleLine "  [DRY-RUN] Would remove AppX: $pkg" -ForegroundColor Magenta
                $removedCount++
            }
        }

        # Also remove provisioned package to prevent reinstall on Windows feature updates.
        # This is independent from installed packages so provisioned-only matches are handled.
        $provisioned = @($inventory.ProvisionedPackages | Where-Object { $_.Name -eq $pkg })
        if ($provisioned) {
            if (-not $SCRIPT:DryRun) {
                try {
                    $provisioned | ForEach-Object { $_.Package } | Remove-AppxProvisionedPackage -Online:$true -ErrorAction Stop | Out-Null
                    Write-OK "Removed provisioned package: $pkg"
                    $provisionedRemovedCount++
                } catch {
                    Write-Warn "Failed to remove provisioned package $($pkg): $_"
                }
            } else {
                Write-ConsoleLine "  [DRY-RUN] Would remove provisioned AppX: $pkg" -ForegroundColor Magenta
                $provisionedRemovedCount++
            }
        }
    }
    if ($removedCount -eq 0 -and $provisionedRemovedCount -eq 0 -and -not $SCRIPT:DryRun) {
        Write-Info "No bloatware packages found or removal failed for all package IDs."
    } elseif ($removedCount -eq 0 -and $provisionedRemovedCount -eq 0 -and $SCRIPT:DryRun) {
        Write-Info "No bloatware packages found (already clean)."
    } else {
        $verb = if ($SCRIPT:DryRun) { "would be removed" } else { "removed" }
        Write-OK "$removedCount installed package ID(s), $provisionedRemovedCount provisioned package ID(s) $verb."
    }

    } # end: AppX cmdlets available guard

    # ── Disable Telemetry Services ───────────────────────────────────────────
    Write-Step "Disabling telemetry services..."
    foreach ($svc in $inventory.Services) {
        if (-not $svc.Exists) {
            if ($svc.Optional) { Write-Info "Service '$($svc.Name)' not present (removed in newer Windows) — skipping." }
            else { Write-Warn "Service '$($svc.Name)' not found on this system — skipping." }
            continue
        }
        # Skip if already disabled (idempotent — avoids redundant backup entries on re-run)
        if (-not $svc.NeedsDisable) {
            Write-Sub "$($svc.Label) ($($svc.Name)): already disabled — skipped."
            continue
        }
        try {
            if (-not $SCRIPT:DryRun) {
                Backup-ServiceState -ServiceName $svc.Name -StepTitle $SCRIPT:CurrentStepTitle
                Stop-Service $svc.Name -Force -ErrorAction SilentlyContinue
                Set-Service $svc.Name -StartupType Disabled -ErrorAction Stop
                Write-OK "Disabled: $($svc.Label) ($($svc.Name))"
            } else {
                Write-ConsoleLine "  [DRY-RUN] Would stop + disable service: $($svc.Label) ($($svc.Name))" -ForegroundColor Magenta
            }
        } catch {
            Write-Warn "Could not disable $($svc.Label) ($($svc.Name)): $_"
        }
    }

    # ── Disable Telemetry Scheduled Tasks ────────────────────────────────────
    Write-Step "Disabling telemetry scheduled tasks..."
    foreach ($tp in $inventory.TaskPaths) {
        if (-not $tp.Exists) {
            Write-Warn "Task path '$($tp.TaskPath)' not found on this system — skipping."
        }
    }
    foreach ($t in $inventory.Tasks) {
        # Skip already-disabled tasks (idempotent re-run)
        if (-not $t.NeedsDisable) {
            Write-Sub "Task '$($t.TaskName)': already disabled — skipped."
            continue
        }
        if (-not $SCRIPT:DryRun) {
            Backup-ScheduledTask -TaskName $t.TaskName -TaskPath $t.TaskPath -StepTitle $SCRIPT:CurrentStepTitle
            try {
                Disable-ScheduledTask -TaskName $t.TaskName -TaskPath $t.TaskPath -ErrorAction Stop | Out-Null
                Write-OK "Disabled task: $($t.TaskName)"
            } catch {
                Write-Warn "Failed to disable task $($t.TaskName): $_"
            }
        } else {
            Write-ConsoleLine "  [DRY-RUN] Would disable task: $($t.TaskName)" -ForegroundColor Magenta
        }
    }

    # ── Disable Consumer Features ────────────────────────────────────────────
    Write-Step "Disabling consumer features..."
    $consumerPath = "HKLM:\SOFTWARE\Policies\Microsoft\Windows\CloudContent"
    Set-RegistryValue $consumerPath "DisableWindowsConsumerFeatures" 1 "DWord" "Disable consumer features (app suggestions)"
    Set-RegistryValue $consumerPath "DisableSoftLanding" 1 "DWord" "Disable soft landing (app suggestions)"
    Write-ActionOK "Consumer features disabled (no app suggestions)."

    # ── Disable Advertising ID ───────────────────────────────────────────────
    $adPath = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo"
    Set-RegistryValue $adPath "Enabled" 0 "DWord" "Disable advertising ID"
    Write-ActionOK "Advertising ID disabled."

    # NOTE: Autostart cleanup is handled separately by Step 14 (Optimize-Hardware.ps1).
    # Keeping it out of debloat ensures the user's choice to skip Step 14 is honored.
}
