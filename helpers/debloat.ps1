# ==============================================================================
#  helpers/debloat.ps1  —  Targeted Bloatware + Telemetry Removal
# ==============================================================================

function Invoke-GamingDebloat {
    <#
    .SYNOPSIS  Removes known bloatware AppX packages, disables telemetry services
               and scheduled tasks. Pure PowerShell — no external tools.
    #>

    Write-Step "Removing bloatware AppX packages..."

    # Windows Server Core / LTSC may not have the Appx module at all.
    # Guard against CommandNotFoundException to avoid a hard failure.
    if (-not (Get-Command Get-AppxPackage -ErrorAction SilentlyContinue)) {
        Write-Info "AppX cmdlets not available (Windows Server Core or LTSC). Skipping package removal."
        # Fall through to telemetry services and consumer features below
    } else {

    $bloatPackages = @(
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
        "Microsoft.549981C3F5F10",          # Cortana
        "Microsoft.MixedReality.Portal",
        "Microsoft.SkypeApp",
        "Microsoft.WindowsCommunicationsApps"  # Mail & Calendar
    )

    # Pre-fetch provisioned packages once (avoid querying per-package in the loop)
    $provisionedPkgs = $null
    if (-not $SCRIPT:DryRun) {
        $provisionedPkgs = Get-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue
    }

    $removedCount = 0
    foreach ($pkg in $bloatPackages) {
        $apps = Get-AppxPackage -Name $pkg -AllUsers -ErrorAction SilentlyContinue
        if ($apps) {
            if (-not $SCRIPT:DryRun) {
                try {
                    $apps | Remove-AppxPackage -AllUsers -ErrorAction Stop
                    Write-OK "Removed: $pkg"
                    $removedCount++
                } catch {
                    Write-DebugLog "Failed to remove $($pkg): $_"
                }
                # Also remove provisioned package to prevent reinstall on Windows feature updates
                # Runs independently — provisioned removal should not be blocked by AppxPackage failure
                try {
                    if ($provisionedPkgs) {
                        $provisionedPkgs |
                            Where-Object { $_.DisplayName -eq $pkg } |
                            Remove-AppxProvisionedPackage -Online -ErrorAction Stop | Out-Null
                    }
                } catch {
                    Write-Warn "Failed to remove provisioned package $($pkg): $_"
                }
            } else {
                Write-Host "  [DRY-RUN] Would remove AppX: $pkg" -ForegroundColor Magenta
                $removedCount++
            }
        }
    }
    if ($removedCount -eq 0 -and -not $SCRIPT:DryRun) {
        Write-Info "No bloatware packages found or removal failed for all packages."
    } elseif ($removedCount -eq 0 -and $SCRIPT:DryRun) {
        Write-Info "No bloatware packages found (already clean)."
    } else {
        $verb = if ($SCRIPT:DryRun) { "would be removed" } else { "removed" }
        Write-OK "$removedCount bloatware packages $verb."
    }

    } # end: AppX cmdlets available guard

    # ── Disable Telemetry Services ───────────────────────────────────────────
    Write-Step "Disabling telemetry services..."
    $telemetryServices = @(
        @{ Name = "DiagTrack";        Label = "Connected User Experiences and Telemetry" },
        @{ Name = "dmwappushservice"; Label = "Device Management WAP Push"; Optional = $true }
    )
    foreach ($svc in $telemetryServices) {
        $svcObj = Get-Service -Name $svc.Name -ErrorAction SilentlyContinue
        if (-not $svcObj) {
            if ($svc.Contains('Optional') -and $svc.Optional) { Write-Info "Service '$($svc.Name)' not present (removed in newer Windows) — skipping." }
            else { Write-Warn "Service '$($svc.Name)' not found on this system — skipping." }
            continue
        }
        # Skip if already disabled (idempotent — avoids redundant backup entries on re-run)
        if ($svcObj.StartType -eq 'Disabled') {
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
                Write-Host "  [DRY-RUN] Would stop + disable service: $($svc.Label) ($($svc.Name))" -ForegroundColor Magenta
            }
        } catch {
            Write-Warn "Could not disable $($svc.Label) ($($svc.Name)): $_"
        }
    }

    # ── Disable Telemetry Scheduled Tasks ────────────────────────────────────
    Write-Step "Disabling telemetry scheduled tasks..."
    $taskPaths = @(
        "\Microsoft\Windows\Application Experience\",
        "\Microsoft\Windows\Customer Experience Improvement Program\"
    )
    foreach ($tp in $taskPaths) {
        $tasks = Get-ScheduledTask -TaskPath $tp -ErrorAction SilentlyContinue
        if (-not $tasks) {
            Write-Warn "Task path '$tp' not found on this system — skipping."
            continue
        }
        foreach ($t in $tasks) {
            # Skip already-disabled tasks (idempotent re-run)
            if ($t.PSObject.Properties['State'] -and $t.State -eq "Disabled") {
                Write-Sub "Task '$($t.TaskName)': already disabled — skipped."
                continue
            }
            if (-not $SCRIPT:DryRun) {
                Backup-ScheduledTask -TaskName $t.TaskName -StepTitle $SCRIPT:CurrentStepTitle
                try {
                    Disable-ScheduledTask -TaskName $t.TaskName -TaskPath $t.TaskPath -ErrorAction Stop | Out-Null
                    Write-OK "Disabled task: $($t.TaskName)"
                } catch {
                    Write-Warn "Failed to disable task $($t.TaskName): $_"
                }
            } else {
                Write-Host "  [DRY-RUN] Would disable task: $($t.TaskName)" -ForegroundColor Magenta
            }
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
