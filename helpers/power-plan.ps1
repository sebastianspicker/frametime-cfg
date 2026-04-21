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
        Write-Host "  [DRY-RUN] Would set power plan: $Label = $Value" -ForegroundColor Magenta
        return
    }
    $ppOut = powercfg /setacvalueindex $PlanGuid $SubgroupGuid $SettingGuid $Value 2>&1
    if ($LASTEXITCODE -ne 0) {
        $msg = "$ppOut"
        if ($msg -match "does not exist") {
            # Setting GUID not exposed on this hardware (e.g., no Wi-Fi, no USB-C PD, no SATA).
            # Expected and harmless — downgrade from warning to sub-message.
            Write-Sub "$Label — not present on this hardware (skipped)"
        } elseif ($msg -match "malformed|not within the range") {
            # Value outside platform-supported range (e.g., AMD EPP2, boost mode max differs
            # from Intel, perf decrease time cap varies). Expected on cross-vendor configs.
            Write-Sub "$Label — value $Value not supported on this platform (skipped)"
        } else {
            Write-Warn "powercfg failed for '$Label': $ppOut"
        }
    } else { Write-DebugLog "Power plan: $Label = $Value" }
}


function New-CS2PowerPlan {
    <#
    .SYNOPSIS  Creates a fresh "CS2 Optimized" power plan, removing any existing duplicate.
    .OUTPUTS   GUID string of the new plan.
    .NOTES
        Duplicates Windows High Performance (8c5e7fda) as the base. Re-running Step 6
        is always safe — any existing "CS2 Optimized" plan is deleted first.
        In DRY-RUN mode, skips deletion and creation (nothing is persisted).
    #>
    if ($SCRIPT:DryRun) {
        Write-Host "  [DRY-RUN] Would remove existing CS2 Optimized plans and create fresh duplicate" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN] Would name plan: CS2 Optimized" -ForegroundColor Magenta
        return "DRY-RUN-GUID"
    }

    # Remove any existing "CS2 Optimized" plans — idempotent re-run safety
    $existing = powercfg /list 2>&1
    foreach ($line in $existing) {
        if ($line -match "CS2 Optimized" -and $line -match '\b([a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12})\b') {
            $oldGuid = $Matches[1]
            Write-DebugLog "Removing existing CS2 Optimized plan: $oldGuid"
            powercfg /setactive SCHEME_BALANCED 2>&1 | Out-Null   # switch away first
            powercfg /delete $oldGuid 2>&1 | Out-Null
        }
    }

    # Duplicate High Performance as base; fall back to Balanced on OEM systems where High Perf is removed
    $output = powercfg /duplicatescheme 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c 2>&1
    if (-not ($output -match "([a-fA-F0-9-]{36})")) {
        Write-Warn "High Performance plan not found — falling back to Balanced as base."
        Write-Warn "The Balanced plan uses conservative defaults for some settings. All tiered"
        Write-Warn "settings (T1/T2/T3) will still be applied and override the Balanced defaults."
        $output = powercfg /duplicatescheme 381b4222-f694-41f0-9685-ff5bb260df2e 2>&1
    }
    if ($output -match "([a-fA-F0-9-]{36})") {
        $guid = $Matches[1]
    } else {
        throw "Failed to create power plan (duplicatescheme returned no GUID). Output: $output"
    }

    powercfg /changename $guid "CS2 Optimized" `
        "Tiered low-latency plan: T1 proven, T2 vendor-aware CPU/disk/USB, T3 C-states off" 2>&1 | Out-Null

    return $guid
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
        Write-Host "  NOTE: T3 disables deep C-states. Expect +5–15°C CPU temp at idle." -ForegroundColor DarkYellow
        Write-Host "  Safe with adequate cooling. Revert via Restore/Rollback if temps spike." -ForegroundColor DarkYellow

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
