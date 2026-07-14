# ==============================================================================
#  helpers/power-plan.ps1  —  Native CS2 Power Plan (Tiered, Vendor-Aware)
# ==============================================================================
#
#  Creates "CS2 Optimized (FPSHeaven 2026)" by duplicating High Performance and
#  applying a curated subset of FPSHeaven settings with full tier/profile gating,
#  AMD vs Intel vendor branching, DRY-RUN support, and auto-backup integration.
#
#  Source: Reverse-engineered FPSHEAVEN2026.pow hive (python-registry).
#  4 bugs corrected vs. original. PERFAUTONOMOUS and DC/battery settings excluded.
#
#  Tier assignment:
#    T1  SAFE+      : PROCTHROTTLEMAX, CPMAXCORES, USBSS, DISKIDLE, DISKPOWERMGMT,
#                     STANDBYIDLE, HIBERNATEIDLE, SYSCOOLPOL
#    T2  RECOMMENDED+: PROCTHROTTLEMIN (vendor-aware), PERFEPP, PERFEPP2, PERFBOOSTPOL,
#                     PERFBOOSTMODE, IDLESTATEMAX, CPMINCORES, CPMINCORES1 (Intel),
#                     DISKLPM, DISKNV, DISKNVIDLE, DISKADAPTIVE,
#                     USBC, USBHUB, WIFIPOWERSAVE, GPUPREF
#    T3  COMPETITIVE+: IDLEDISABLE, DUTYCYCLING, PERFHISTCOUNT, PERFINCRTIME, PERFDECRTIME
#
#  FPSHeaven bugs fixed:
#    SYSCOOLPOL=0  → 1 (passive cooling causes thermal throttle on desktops)
#    STANDBYIDLE=1 → 0 (1-second sleep timer was a data entry error)
#    PERFAUTONOMOUS=0 → not set (breaks CPPC2/PB2 on AMD Ryzen and Intel 12th+ gen)
#    DUTYCYCLING=1  → 0 (duty cycling creates periodic freq dips; we invert)
#
#  Settings intentionally excluded (see plan doc for full rationale):
#    PERFAUTONOMOUS, all DC/battery settings, display timeout, VIDEOIDLE,
#    DEVICEIDLE, screen saver, adaptive display.
# ==============================================================================

# ── Power Plan Subgroup GUIDs ──────────────────────────────────────────────────
$PP_SUB_PROCESSOR  = "54533251-82be-4824-96c1-47b60b740d00"
$PP_SUB_DISK       = "0012ee47-9041-4b5d-9b77-535fba8b1442"
$PP_SUB_USB        = "2a737441-1930-4402-8d77-b2bebba308a3"
$PP_SUB_SLEEP      = "238c9fa8-0aad-41ed-83f4-97be242c8f20"
$PP_SUB_NETWORK    = "f905f51b-3de9-4be5-9ef8-2b7b6e31cbdb"
$PP_SUB_GPUPREF    = "48672f38-7a9a-4bb2-8bf8-3d85be19de4e"
$PP_SUB_COOLING    = "5fb4938d-1ee8-4b0f-9a3c-5036b0ab995c"

# ── Processor Setting GUIDs ────────────────────────────────────────────────────
$PP_PERFBOOSTMODE  = "be337238-0d82-4146-a960-4f3749d470c7"  # Boost mode: 255=all boost states
$PP_PERFBOOSTPOL   = "b000397d-9b0b-483d-98c9-692a6060cfbf"  # Boost policy: 254=AGGRESSIVE_AT_GUARANTEED
$PP_PERFEPP        = "4e4450b3-6179-4e91-b8f1-5bb9938f81a1"  # Energy Perf Preference: 0=max performance
$PP_PERFEPP2       = "2ddd5a84-5a71-437e-912a-db0b8c788732"  # Secondary EPP register: same rationale
$PP_PROCTHROTTLEMAX= "bc5038f7-23e0-4960-96da-33abaf5935ec"  # Max perf state: 100=no ceiling
$PP_PROCTHROTTLEMIN= "893dee8e-2bef-41e0-89c6-b55d0929964c"  # Min perf state: vendor-aware (AMD:0, Intel:100)
$PP_IDLEDISABLE    = "4009efa7-e72d-4cba-9edf-91084ea8cbc3"  # C-state disable: 1=off (T3 — thermal trade-off)
$PP_IDLESTATEMAX   = "9943e905-9a30-4ec1-9b99-44dd3b76f7a2"  # Max idle state: 2=C1/C1E only (<100µs exit)
$PP_DUTYCYCLING    = "4e4d2049-be1a-4064-b872-bcc8dccebce4"  # Duty cycling: 0=off (inverted vs FPSHeaven)
$PP_PERFHISTCOUNT  = "7d24baa7-0b84-480f-840c-1b0743c00f5f"  # Perf history count: 1=minimal (faster response)
$PP_PERFINCRTIME   = "984cf492-3bed-4488-a8f9-4286c97bf5aa"  # Perf increase time: 0 intervals (fastest documented ramp-up)
$PP_PERFDECRTIME   = "d8edeb9b-95cf-4f95-a73c-b061973693c8"  # Perf decrease time: 100 intervals (slowest documented drop)
$PP_CPMINCORES     = "0cc5b647-c1df-4637-891a-dec35c318583"  # Core parking min cores %: 100=no parking
$PP_CPMAXCORES     = "ea062031-0e34-4ff1-9b6d-eb1059334028"  # Core parking max cores %: 100=use all cores
$PP_CPMINCORES1    = "4d2b0152-7d5c-498b-88e2-34345392a2c5"  # Intel secondary ring min cores (Intel-only)

# ── Disk Setting GUIDs ─────────────────────────────────────────────────────────
$PP_DISKIDLE       = "6738e2c4-e8a5-4a42-b16a-e040e769756e"  # Idle timeout: 0=never spin down
$PP_DISKPOWERMGMT  = "0b2d69d7-a2a1-449c-9680-f91c70521c60"  # AHCI LPM: T1→1 (HIPM-only), T2→0 (fully off)
# NOTE: $PP_DISKLPM shares GUID with $PP_DISKPOWERMGMT. T2 intentionally overrides
# T1's partial HIPM-only state with fully-off (0). This is the tier progression:
# SAFE users get HIPM-only (safer); RECOMMENDED+ get HIPM+DIPM fully off (max latency).
$PP_DISKLPM        = "0b2d69d7-a2a1-449c-9680-f91c70521c60"  # ALPM/DIPM: 0=fully off (T2 override)
$PP_DISKAHCIADAPTIVE = "dab60367-53fe-4fbc-825e-521d069d2456"  # AHCI adaptive link-power timeout: 0=partial-state only
$PP_DISKNVIDLE     = "d3d55efd-c1ff-424e-9dc3-441be7833010"  # NVMe idle timeout: 0=never
$PP_DISKADAPTIVE   = "dbc9e238-6de9-49d9-a138-611ececd40d0"  # Disk adaptive power (DIPM): 0=off

# ── USB Setting GUIDs ──────────────────────────────────────────────────────────
# Windows enum: 0=Enabled (suspend active), 1=Disabled (USB stays on)
$PP_USBSS          = "48e6b7a6-50f5-4782-a5d4-53bb8f07e226"  # USB selective suspend: 1=disabled
$PP_USBHUB         = "0853a681-27c8-4100-a2fd-82013e970683"  # USB hub selective suspend: 1=disabled
$PP_USBC           = "25dfa149-5dd1-4736-b5ab-e8a37b5b8187"  # USB-C connector power: 1=disabled

# ── Network / GPU / Sleep Setting GUIDs ───────────────────────────────────────
$PP_WIFIPOWERSAVE  = "12bbebe6-58d6-4636-95bb-3217ef867c1a"  # Wi-Fi power saving: 0=off (prevents ping spikes)
$PP_GPUPREF        = "2bfc24f9-5ea2-4801-8213-3dbae01aa39d"  # GPU preference: 4=high performance
$PP_SYSCOOLPOL     = "dd848b2a-8a5d-4451-9ae2-39cd41658f6c"  # Cooling: 1=active (FPSHeaven had 0=passive=bug)
$PP_STANDBYIDLE    = "29f6c1db-86da-48c5-9fdb-f2b67b1f44da"  # Standby timeout: 0=never
$PP_HIBERNATEIDLE  = "9d7815a6-7ee4-497e-8888-515a05f02364"  # Hibernate timeout: 0=never

# ── PCIe / Link State Power Management GUIDs ──────────────────────────────────
# Windows maintains an independent software ASPM layer on top of BIOS ASPM settings.
# Even if BIOS ASPM is "disabled", the Windows power plan can still pull PCIe devices
# (GPU, NIC, NVMe) into lower link states between frames, causing exit-latency spikes.
$PP_SUB_PCIE       = "501a4d13-42af-4429-9fd1-a8218c268e20"  # PCIe Express subgroup (Microsoft SUB_PCIEXPRESS)
$PP_ASPM           = "ee12f906-d277-404b-b6da-e5fa1a576df5"  # Link State Power Management: 0=Off


function Set-PowerPlanValue {
    <#
    .SYNOPSIS  DRY-RUN-aware wrapper for powercfg /setacvalueindex.
    .DESCRIPTION
        Applies a power plan setting for AC (plugged in) mode only.
        DC/battery settings are intentionally not touched — preserves laptop battery behavior.
        When $SCRIPT:DryRun is set, prints what would be applied without calling powercfg.
    #>
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [string]$PlanGuid,
        [string]$SubgroupGuid,
        [string]$SettingGuid,
        [int]$Value,
        [string]$Label
    )
    # SECURITY: Validate all GUIDs — these are passed as powercfg command-line arguments.
    # A tampered backup.json or crafted GUID could inject arbitrary powercfg commands.
    $guidPattern = '^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$'
    if ($PlanGuid -notmatch $guidPattern -and $PlanGuid -ne "DRY-RUN-GUID") {
        Write-Warn "Set-PowerPlanValue: invalid PlanGuid '$PlanGuid' — rejected (security)"
        return
    }
    if ($SubgroupGuid -notmatch $guidPattern) {
        Write-Warn "Set-PowerPlanValue: invalid SubgroupGuid '$SubgroupGuid' — rejected (security)"
        return
    }
    if ($SettingGuid -notmatch $guidPattern) {
        Write-Warn "Set-PowerPlanValue: invalid SettingGuid '$SettingGuid' — rejected (security)"
        return
    }

    if ($SCRIPT:DryRun) {
        Write-ConsoleLine "  [DRY-RUN] Would set power plan: $Label = $Value" -ForegroundColor Magenta
        return
    }
    if (-not $PSCmdlet.ShouldProcess($PlanGuid, "Set power plan value '$Label' to $Value")) { return }
    $ppOut = powercfg /setacvalueindex $PlanGuid $SubgroupGuid $SettingGuid $Value 2>&1
    if ($LASTEXITCODE -ne 0) {
        $msg = "$ppOut"
        if ($msg -match "does not exist") {
            # Setting GUID not exposed on this hardware (e.g., no Wi-Fi, no USB-C PD, no SATA).
            # Expected and harmless — downgrade from warning to sub-message.
            Write-Sub "$Label — not present on this hardware (skipped)"
            return
        } elseif ($msg -match "malformed|not within the range") {
            # Value outside platform-supported range (e.g., AMD EPP2, boost mode max differs
            # from Intel, perf decrease time cap varies). Expected on cross-vendor configs.
            Write-Sub "$Label — value $Value not supported on this platform (skipped)"
            return
        } else {
            throw "powercfg failed for '$Label': $ppOut"
        }
    } else {
        Write-DebugLog "Power plan: $Label = $Value"
    }
}


function New-CS2PowerPlan {
    <#
    .SYNOPSIS  Creates a fresh "CS2 Optimized" power plan without replacing any prior plan.
    .OUTPUTS   GUID string of the new plan.
    .NOTES
        Duplicates Windows High Performance (8c5e7fda) as the base. Ownership is
        tracked by GUID; display names are never used to decide what may be deleted.
        In DRY-RUN mode, skips creation (nothing is persisted).
    #>
    [CmdletBinding(SupportsShouldProcess)]
    param()

    if ($SCRIPT:DryRun) {
        Write-ConsoleLine "  [DRY-RUN] Would create and configure a fresh CS2 Optimized plan" -ForegroundColor Magenta
        Write-ConsoleLine "  [DRY-RUN] Would name plan: CS2 Optimized" -ForegroundColor Magenta
        return "DRY-RUN-GUID"
    }
    if (-not $PSCmdlet.ShouldProcess("CS2 Optimized", "Create fresh optimized power plan")) {
        return "DRY-RUN-GUID"
    }

    # Duplicate High Performance as base; fall back to Balanced on OEM systems where High Perf is removed
    $guidPattern = '(?i)(?<![a-f0-9])[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}(?![a-f0-9])'
    $output = powercfg /duplicatescheme 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c 2>&1
    $duplicateExitCode = $LASTEXITCODE
    $outputText = @($output) -join "`n"
    if ($duplicateExitCode -ne 0 -or -not ($outputText -match $guidPattern)) {
        Write-Warn "High Performance plan not found — falling back to Balanced as base."
        Write-Warn "The Balanced plan uses conservative defaults for some settings. All tiered"
        Write-Warn "settings (T1/T2/T3) will still be applied and override the Balanced defaults."
        $output = powercfg /duplicatescheme 381b4222-f694-41f0-9685-ff5bb260df2e 2>&1
        $duplicateExitCode = $LASTEXITCODE
        $outputText = @($output) -join "`n"
    }
    if ($duplicateExitCode -eq 0 -and $outputText -match $guidPattern) {
        $guid = $Matches[0]
    } else {
        throw "Failed to create power plan (duplicatescheme exit $duplicateExitCode returned no new GUID). Output: $output"
    }

    $renameOutput = powercfg /changename $guid "CS2 Optimized" `
        "Tiered low-latency plan: T1 proven, T2 vendor-aware CPU/disk/USB, T3 C-states off" 2>&1
    if ($LASTEXITCODE -ne 0) {
        $deleteOutput = powercfg /delete $guid 2>&1
        $deleteExitCode = $LASTEXITCODE
        if ($deleteExitCode -ne 0) {
            $exception = [InvalidOperationException]::new(
                "Failed to name new power plan '$guid', and cleanup failed (exit $deleteExitCode): $deleteOutput."
            )
            $exception.Data['CreatedPowerPlanGuid'] = $guid
            throw $exception
        }
        throw "Failed to name new power plan '$guid': $renameOutput"
    }

    return $guid
}

function Get-ActivePowerPlanGuid {
    $output = powercfg /getactivescheme 2>&1
    if ($LASTEXITCODE -eq 0 -and $output -match '([a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12})') {
        return $Matches[1].ToLowerInvariant()
    }
    throw "Could not determine the active power plan: $output"
}

function Get-SuiteOwnedPowerPlanGuids {
    $guids = [System.Collections.Generic.List[string]]::new()
    if (Test-Path -LiteralPath $CFG_StateFile) {
        try {
            $state = Get-Content -LiteralPath $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
            $candidates = @()
            if ($state.PSObject.Properties['suiteOwnedPowerPlanGuids']) {
                $candidates += @($state.suiteOwnedPowerPlanGuids)
            }
            if ($state.PSObject.Properties['suiteOwnedPowerPlanGuid']) {
                $candidates += @($state.suiteOwnedPowerPlanGuid)
            }
            foreach ($candidate in $candidates) {
                if ($candidate -match '^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$' -and
                    -not $guids.Contains(([string]$candidate).ToLowerInvariant())) {
                    $guids.Add(([string]$candidate).ToLowerInvariant())
                }
            }
        } catch {
            throw "Could not read suite-owned power-plan identity from state.json: $_"
        }
    }
    return @($guids)
}

function Set-SuiteOwnedPowerPlanGuids {
    [CmdletBinding(SupportsShouldProcess)]
    param([string[]]$Guids)

    $validGuids = @($Guids | Where-Object {
        $_ -match '^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$'
    } | ForEach-Object { $_.ToLowerInvariant() } | Select-Object -Unique)
    if (-not $PSCmdlet.ShouldProcess($CFG_StateFile, "Persist suite-owned power-plan identities")) { return }
    if (Test-Path -LiteralPath $CFG_StateFile) {
        $state = Get-Content -LiteralPath $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    } else {
        $state = [PSCustomObject]@{}
    }
    $state | Add-Member -NotePropertyName suiteOwnedPowerPlanGuids -NotePropertyValue $validGuids -Force
    $state.PSObject.Properties.Remove('suiteOwnedPowerPlanGuid')
    Save-SuiteState -State $state
}

function Invoke-CS2PowerPlanTransaction {
    <# Creates, configures, activates, records, then retires only prior owned GUIDs. #>
    [CmdletBinding(SupportsShouldProcess)]
    param()

    if ($SCRIPT:DryRun) {
        $dryGuid = New-CS2PowerPlan
        Apply-PowerPlan -PlanGuid $dryGuid
        return $dryGuid
    }
    if (-not $PSCmdlet.ShouldProcess(
        "CS2 Optimized power plan",
        "Create, configure, activate, persist ownership, and retire prior suite-owned plans"
    )) {
        return "DRY-RUN-GUID"
    }

    $previousActiveGuid = Get-ActivePowerPlanGuid
    $priorOwnedGuids = @(Get-SuiteOwnedPowerPlanGuids)
    $newGuid = $null
    $activated = $false
    try {
        $newGuid = New-CS2PowerPlan
        Apply-PowerPlan -PlanGuid $newGuid

        $activationOutput = powercfg /setactive $newGuid 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to activate new power plan '$newGuid': $activationOutput"
        }
        $activated = $true

        # Record both generations before deleting anything.  A failed cleanup
        # therefore remains explicitly owned and can be retried/restored safely.
        $trackedGuids = @($priorOwnedGuids + $newGuid | Select-Object -Unique)
        Set-SuiteOwnedPowerPlanGuids -Guids $trackedGuids
        if (Get-Command Update-PowerPlanBackupOwnership -ErrorAction SilentlyContinue) {
            Update-PowerPlanBackupOwnership -OwnedGuids $trackedGuids
        }

        # Retirement only begins after both durable ownership surfaces contain
        # both generations.  It is deliberately best-effort: a failure while
        # deleting or narrowing the retired set must never roll back activation
        # to a plan that may already have been deleted.
        try {
            $remainingGuids = [System.Collections.Generic.List[string]]::new()
            $remainingGuids.Add($newGuid)
            foreach ($oldGuid in $priorOwnedGuids) {
                if ($oldGuid -eq $newGuid) { continue }
                $deleteOutput = powercfg /delete $oldGuid 2>&1
                if ($LASTEXITCODE -ne 0) {
                    $presence = Get-PowerPlanGuidPresence -Guid $oldGuid
                    if ($presence.Verified -and -not $presence.Present) {
                        Write-DebugLog "Prior suite-owned power plan '$oldGuid' is already absent."
                    } else {
                        Write-Warn "Could not delete prior suite-owned power plan '$oldGuid': $deleteOutput $($presence.Message)"
                        $remainingGuids.Add($oldGuid)
                    }
                }
            }
            Set-SuiteOwnedPowerPlanGuids -Guids @($remainingGuids)
            if (Get-Command Update-PowerPlanBackupOwnership -ErrorAction SilentlyContinue) {
                Update-PowerPlanBackupOwnership -OwnedGuids @($remainingGuids)
            }
        } catch {
            Write-Warn "Power-plan retirement cleanup was incomplete after the replacement was committed: $_"
        }
        return $newGuid
    } catch {
        $transactionError = $_
        if (-not $newGuid -and $transactionError.Exception.Data.Contains('CreatedPowerPlanGuid')) {
            $candidateGuid = [string]$transactionError.Exception.Data['CreatedPowerPlanGuid']
            if ($candidateGuid -match '^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$') {
                $newGuid = $candidateGuid.ToLowerInvariant()
            }
        }
        if ($activated -and $previousActiveGuid) {
            powercfg /setactive $previousActiveGuid 2>&1 | Out-Null
            if ($LASTEXITCODE -ne 0) {
                Write-Warn "Power-plan rollback could not reactivate '$previousActiveGuid'; the new plan was left in place."
                # The active replacement cannot be deleted safely. Preserve its
                # identity alongside prior ownership so a later restore/retry
                # never mistakes it for a foreign plan. This is best effort: the
                # original persistence failure may also affect this recovery write.
                $recoveryGuids = @($priorOwnedGuids + $newGuid | Where-Object { $_ } | Select-Object -Unique)
                try {
                    Set-SuiteOwnedPowerPlanGuids -Guids $recoveryGuids
                    if (Get-Command Update-PowerPlanBackupOwnership -ErrorAction SilentlyContinue) {
                        Update-PowerPlanBackupOwnership -OwnedGuids $recoveryGuids
                    }
                } catch {
                    Write-Warn "Could not persist the active replacement power-plan identity during rollback recovery: $_"
                }
                throw "Power-plan transaction failed and the previous active plan could not be restored. The replacement '$newGuid' remains active and was retained for recovery. Original error: $transactionError"
            }
        }
        $rollbackDeleteFailed = $false
        $rollbackDeleteOutput = $null
        if ($newGuid) {
            $rollbackDeleteOutput = powercfg /delete $newGuid 2>&1
            $rollbackDeleteFailed = ($LASTEXITCODE -ne 0)
        }

        if ($rollbackDeleteFailed) {
            # The failed transaction still created a suite-owned object. Keep
            # that identity recorded rather than silently orphaning it.
            $recoveryGuids = @($priorOwnedGuids + $newGuid | Where-Object { $_ } | Select-Object -Unique)
            $ownershipPersisted = $true
            try {
                Set-SuiteOwnedPowerPlanGuids -Guids $recoveryGuids
                if (Get-Command Update-PowerPlanBackupOwnership -ErrorAction SilentlyContinue) {
                    Update-PowerPlanBackupOwnership -OwnedGuids $recoveryGuids
                }
            } catch {
                $ownershipPersisted = $false
                Write-Warn "Could not persist rollback ownership for '$newGuid': $_"
            }
            $trackingText = if ($ownershipPersisted) {
                "Its GUID remains recorded for a later restore/retry."
            } else {
                "Its GUID could not be recorded; remove it manually with 'powercfg /delete $newGuid'."
            }
            throw "Power-plan transaction failed and rollback could not delete replacement '$newGuid': $rollbackDeleteOutput. $trackingText Original error: $transactionError"
        }

        # Preserve the pre-transaction ownership record in both state and the
        # durable restore point when the replacement was removed successfully
        # (or creation never produced a GUID). These writes are independent so
        # one failed persistence surface does not prevent repair of the other.
        try { Set-SuiteOwnedPowerPlanGuids -Guids $priorOwnedGuids } catch {
            Write-Warn "Could not restore prior suite-owned power-plan identity in state: $_"
        }
        if (Get-Command Update-PowerPlanBackupOwnership -ErrorAction SilentlyContinue) {
            try { Update-PowerPlanBackupOwnership -OwnedGuids $priorOwnedGuids } catch {
                Write-Warn "Could not restore prior suite-owned power-plan identity in backup metadata: $_"
            }
        }
        throw $transactionError
    }
}

function Invoke-CS2PowerPlanWithFallback {
    <# Applies the intended owned plan, or reports a truthful fallback outcome. #>
    [CmdletBinding()]
    param()

    try {
        $guid = Invoke-CS2PowerPlanTransaction
        return [PSCustomObject]@{
            Status = if ($SCRIPT:DryRun -or $guid -eq 'DRY-RUN-GUID') { 'DryRun' } else { 'Success' }
            CanCompleteStep = (-not $SCRIPT:DryRun -and $guid -ne 'DRY-RUN-GUID')
            Guid = $guid
            Message = "CS2 Optimized power plan applied."
        }
    } catch {
        $transactionError = $_
        Write-Warn "Power plan creation failed: $transactionError"
        Write-Info "Fallback: activating Windows High Performance..."
        if ($SCRIPT:DryRun) {
            Write-ConsoleLine "  [DRY-RUN] Would fallback to High Performance plan" -ForegroundColor Magenta
            return [PSCustomObject]@{
                Status = 'DryRun'; CanCompleteStep = $false; Guid = $null
                Message = "Power-plan fallback previewed after transaction failure."
            }
        }

        $highOutput = powercfg /setactive SCHEME_MIN 2>&1
        $highExitCode = $LASTEXITCODE
        if ($highExitCode -eq 0) {
            Write-OK "Windows High Performance active (fallback)."
            return [PSCustomObject]@{
                Status = 'Fallback'; CanCompleteStep = $false; Guid = $null
                Message = "CS2 Optimized failed; Windows High Performance was activated as a fallback."
            }
        }

        Write-Warn "High Performance not available — falling back to Balanced."
        $balancedOutput = powercfg /setactive SCHEME_BALANCED 2>&1
        $balancedExitCode = $LASTEXITCODE
        if ($balancedExitCode -eq 0) {
            Write-OK "Balanced power plan active (fallback)."
            return [PSCustomObject]@{
                Status = 'Fallback'; CanCompleteStep = $false; Guid = $null
                Message = "CS2 Optimized failed; Balanced was activated as a fallback."
            }
        }

        return [PSCustomObject]@{
            Status = 'Failed'; CanCompleteStep = $false; Guid = $null
            Message = "CS2 Optimized failed and neither fallback could be activated. High Performance: $highOutput (exit $highExitCode). Balanced: $balancedOutput (exit $balancedExitCode). Original error: $transactionError"
        }
    }
}


function Apply-PowerPlan {
    <#
    .SYNOPSIS  Applies tiered power plan settings to the given plan GUID.
    .DESCRIPTION
        T1 settings always apply (SAFE+).
        T2 applies when Profile is RECOMMENDED, COMPETITIVE, or CUSTOM.
        T3 applies when Profile is COMPETITIVE or CUSTOM.
        AMD vs Intel branching is applied automatically for PROCTHROTTLEMIN and CPMINCORES1.
    .PARAMETER PlanGuid  GUID of the plan to configure (from New-CS2PowerPlan).
    #>
    param([string]$PlanGuid)

    if (-not $PlanGuid) { Write-Warn "Apply-PowerPlan: No plan GUID provided."; return }

    $chipVendor = Get-ChipsetVendor
    $isAMD   = $chipVendor -eq "AMD"
    $isIntel = $chipVendor -eq "Intel"
    $vendor  = if ($isAMD) { "AMD" } elseif ($isIntel) { "Intel" } else { "Unknown" }
    $applyT2 = $SCRIPT:Profile -in @("RECOMMENDED", "COMPETITIVE", "CUSTOM", "YOLO")
    $applyT3 = $SCRIPT:Profile -in @("COMPETITIVE", "CUSTOM", "YOLO")

    # ── T1: Proven, always applied (SAFE+) ────────────────────────────────────
    Write-Step "T1: proven settings (always applied)..."
    $t1Count = 0

    # CPU max perf state — hard ceiling: never throttle under load
    Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_PROCTHROTTLEMAX 100 "CPU max perf state (100%)"
    $t1Count++

    # Core parking max — use all cores; no parking penalty
    Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_CPMAXCORES 100 "Core parking max (100%)"
    $t1Count++

    # USB selective suspend — 1=disabled: prevents mouse/audio glitches and DPC spikes
    Set-PowerPlanValue $PlanGuid $PP_SUB_USB $PP_USBSS 1 "USB selective suspend (disabled)"
    $t1Count++

    # Disk idle — 0=never: prevents HDD/SSD spin-down mid-game stutter
    Set-PowerPlanValue $PlanGuid $PP_SUB_DISK $PP_DISKIDLE 0 "Disk idle timeout (never)"
    $t1Count++

    # AHCI HIPM only (T1 safe state) — only apply if T2 won't override to fully-off
    if (-not $applyT2) {
        Set-PowerPlanValue $PlanGuid $PP_SUB_DISK $PP_DISKPOWERMGMT 1 "AHCI LPM (HIPM-only, T1 safe)"
        $t1Count++
    }

    # Sleep/hibernate — never sleep during long gaming sessions
    Set-PowerPlanValue $PlanGuid $PP_SUB_SLEEP $PP_STANDBYIDLE 0 "Standby timeout (never)"
    $t1Count++
    Set-PowerPlanValue $PlanGuid $PP_SUB_SLEEP $PP_HIBERNATEIDLE 0 "Hibernate timeout (never)"
    $t1Count++

    # Cooling policy — active (proactive fan), NOT passive. FPSHeaven used passive = thermal throttle bug.
    Set-PowerPlanValue $PlanGuid $PP_SUB_COOLING $PP_SYSCOOLPOL 1 "System cooling (active)"
    $t1Count++

    # PCIe ASPM off — Windows has a software ASPM layer independent of BIOS ASPM setting.
    # Without this, Windows can still pull GPU/NIC/NVMe into lower PCIe link states between
    # frames even when BIOS ASPM is disabled, causing exit-latency spikes mid-frame.
    Set-PowerPlanValue $PlanGuid $PP_SUB_PCIE $PP_ASPM 0 "PCIe ASPM (off — prevents mid-frame link state exit)"
    $t1Count++

    $t1Verb = if ($SCRIPT:DryRun) { "previewed" } else { "applied" }
    Write-OK "T1: $t1Count settings $t1Verb."

    # ── T2: RECOMMENDED+ — setup-dependent, vendor-aware ──────────────────────
    if ($applyT2) {
        Write-Step "T2: vendor-aware CPU/storage/USB settings ($vendor)..."

        # PROCTHROTTLEMIN: AMD=0 (allows OS freq hints to PB2); Intel=100 (locks base clock)
        # FPSHeaven used 100 universally — breaks AMD Precision Boost 2.
        # Unknown vendor: use 0 (safe default — allows OS frequency scaling on any CPU)
        $minState = if ($isIntel) { 100 } else { 0 }
        Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_PROCTHROTTLEMIN $minState "CPU min perf state (${vendor}: ${minState}%)"

        # EPP = 0: tells CPPC2 "maximum performance" — measurable boost frequency improvement
        Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_PERFEPP 0 "Energy Perf Preference (max perf)"
        Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_PERFEPP2 0 "Energy Perf Preference 2 (max perf)"

        # Boost policy + mode: AGGRESSIVE_AT_GUARANTEED (254) + all boost states (255)
        Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_PERFBOOSTPOL 254 "Perf boost policy (254=aggressive)"
        Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_PERFBOOSTMODE 255 "Perf boost mode (255=all states)"

        # Max idle state = 2: allow only C1/C1E; deeper C-states take >100µs to exit
        Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_IDLESTATEMAX 2 "Max idle state (C1/C1E only)"

        # Core parking min = 100%: no parking at all
        Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_CPMINCORES 100 "Core parking min (100%, no parking)"

        # Intel-only: secondary ring min cores (E-core ring on hybrid architectures)
        # Only apply for confirmed Intel CPUs — skip for AMD and unknown vendors
        if ($isIntel) {
            Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_CPMINCORES1 100 "Intel ring min cores (100%)"
        }

        # AHCI LPM fully off (T2 overrides T1's HIPM-only with HIPM+DIPM inactive)
        Set-PowerPlanValue $PlanGuid $PP_SUB_DISK $PP_DISKLPM 0 "AHCI HIPM+DIPM (fully off)"

        # AHCI adaptive timeout — 0 means "partial state only" when AHCI HIPM/DIPM is enabled.
        # This is an AHCI/SATA setting, not NVMe APST.
        Set-PowerPlanValue $PlanGuid $PP_SUB_DISK $PP_DISKAHCIADAPTIVE 0 "AHCI adaptive link-power timeout (0 ms / partial only)"
        Set-PowerPlanValue $PlanGuid $PP_SUB_DISK $PP_DISKNVIDLE 0 "NVMe idle timeout (never)"

        # Disk adaptive power (DIPM) off — SATA drives only, prevents adaptive spin-down
        Set-PowerPlanValue $PlanGuid $PP_SUB_DISK $PP_DISKADAPTIVE 0 "Disk adaptive power (off)"

        # USB hub + USB-C suspend off — prevents hub re-enumeration and controller DPC spikes
        Set-PowerPlanValue $PlanGuid $PP_SUB_USB $PP_USBC 1 "USB-C connector power (disabled)"
        Set-PowerPlanValue $PlanGuid $PP_SUB_USB $PP_USBHUB 1 "USB hub suspend (disabled)"

        # Wi-Fi power saving off — prevents ping spikes on wireless connections
        Set-PowerPlanValue $PlanGuid $PP_SUB_NETWORK $PP_WIFIPOWERSAVE 0 "Wi-Fi power saving (off)"

        # GPU high performance mode — even when GPU load is momentarily low
        Set-PowerPlanValue $PlanGuid $PP_SUB_GPUPREF $PP_GPUPREF 4 "GPU preference (high performance)"

        $t2Count = if ($isIntel) { 16 } else { 15 }
        $t2Verb = if ($SCRIPT:DryRun) { "previewed" } else { "applied" }
        Write-OK "T2: $t2Count settings $t2Verb ($vendor config)."
    }

    # ── T3: COMPETITIVE+ — community consensus, thermal trade-offs ─────────────
    if ($applyT3) {
        Write-Step "T3: C-states off + fast governor settings (COMPETITIVE)..."
        Write-ConsoleLine "  NOTE: T3 disables deep C-states. Expect +5–15°C CPU temp at idle." -ForegroundColor DarkYellow
        Write-ConsoleLine "  Safe with adequate cooling. Revert via Restore/Rollback if temps spike." -ForegroundColor DarkYellow

        # C-states: X3D guide (B21) says keep enabled on single-CCD X3D (irrelevant for
        # single-CCD, saves power, no latency impact). Only disable on non-X3D or dual-CCD.
        $amdCpu = Get-AmdCpuInfo
        $skipCstateDisable = ($amdCpu -and $amdCpu.IsX3D -and $amdCpu.IsSingleCCD)
        if ($skipCstateDisable) {
            Write-Info "X3D single-CCD detected ($($amdCpu.CpuName)): keeping C-states enabled."
            Write-Info "X3D guide (B21): C-state disable is irrelevant on single-CCD X3D — no latency benefit, saves power."
        } else {
            # C-states fully disabled — eliminates >100µs C-state exit latency
            # Trade-off: +5–15°C CPU idle temp. Safe with good cooling; not recommended for laptops.
            Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_IDLEDISABLE 1 "CPU idle disable (C-states off)"
        }

        # Duty cycling off — prevents periodic forced freq pauses (we invert FPSHeaven's value of 1)
        Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_DUTYCYCLING 0 "Duty cycling (off)"

        # Fast governor response using the documented 0..100 interval range:
        # 0 = fastest allowed increase, 100 = slowest allowed decrease.
        Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_PERFHISTCOUNT 1 "Perf history count (1 sample)"
        Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_PERFINCRTIME 0 "Perf increase time (0 intervals)"
        Set-PowerPlanValue $PlanGuid $PP_SUB_PROCESSOR $PP_PERFDECRTIME 100 "Perf decrease time (100 intervals)"

        $t3Count = if ($skipCstateDisable) { 4 } else { 5 }
        $t3Verb = if ($SCRIPT:DryRun) { "previewed" } else { "applied" }
        Write-OK "T3: $t3Count settings $t3Verb.$(if ($skipCstateDisable) { ' (C-states kept enabled — X3D single-CCD)' })"
    }
}
