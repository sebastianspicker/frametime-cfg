#Requires -RunAsAdministrator
<#
.SYNOPSIS  CS2 Optimization Suite — Cleanup / Soft-Reset

  [1]  QUICK REFRESH  (~2 min, no restart)
       CS2 Shader Cache [T1] · Temp · DNS · RAM Working Set

  [2]  FULL CLEANUP  (~5 min, no restart)
       + NVIDIA/AMD/DX Cache [T1] · Prefetch · Event Logs
       + Winsock Reset · Steam verification

  [3]  DRIVER REFRESH  (~20 min, restart required)
       + NVIDIA Cache · Restart Phase 2+3 (native driver clean + install)

  When to use?
  -> After Windows/driver updates (shader invalidation)  -> Full
  -> Sudden stutter that wasn't there before             -> Full
  -> Routine before matches / tournaments                -> Quick
#>
param([switch]$SmokeTest)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ScriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
. "$ScriptRoot\config.env.ps1"
. "$ScriptRoot\helpers.ps1"

Initialize-ScriptDefaults
Initialize-PhaseCounters
Ensure-Dir $CFG_LogDir

if ($SmokeTest) {
    Write-Host "SMOKE TEST OK: Cleanup" -ForegroundColor Green
    exit 0
}

Write-LogoBanner "Cleanup / Soft-Reset  ·  CS2 Optimization Suite"

Write-Host @"
  [1]  QUICK REFRESH  (~2 min, no restart)
       [T1] CS2 Shader Cache · Temp · DNS · RAM Working Set

  [2]  FULL CLEANUP  (~5 min, no restart)
       [T1] + NVIDIA/DX Shader Cache · Prefetch · Winsock Reset
       [T2] Steam verification

  [3]  DRIVER REFRESH  (~20 min, restart required)
       [T1] + Native GPU driver clean + reinstall (Phase 2+3)
       -> For persistent driver problems

  [4]  Back
"@ -ForegroundColor White

do { $choice = Read-Host "  [1/2/3/4]" } while ($choice -notin @("1","2","3","4"))
if ($choice -eq "4") { exit 0 }

$doFull   = $choice -in @("2","3")
$doDriver = $choice -eq "3"
$logTag   = switch ($choice) { "1" {"QUICK"} "2" {"FULL"} "3" {"DRIVER"} }
$start    = Get-Date
Write-Log "SECTION" "=== CLEANUP START: $logTag ==="
$total    = 0

# ══════════════════════════════════════════════════════════════════════════════
# QUICK REFRESH  [T1]
# ══════════════════════════════════════════════════════════════════════════════
Write-Section "Quick Refresh  [T1 — measurable effect after driver updates]"

# CS2 Shader Cache
Write-TierBadge 1 "Clear CS2 Shader Cache"
$steamBase  = Get-SteamPath
$cachePaths = [System.Collections.Generic.List[string]]$CFG_ShaderCache_Paths
if ($steamBase) { $cachePaths.Add("$steamBase\steamapps\shadercache\730") }
$cs2Found = $false
foreach ($p in ($cachePaths | Select-Object -Unique)) {
    if (Test-Path $p) {
        if (-not $SCRIPT:DryRun) {
            $total += Clear-Dir $p "CS2 Shader Cache"
        } else {
            $n = @(Get-ChildItem $p -Recurse -ErrorAction SilentlyContinue | Where-Object { -not $_.PSIsContainer }).Count
            Write-Host "  [DRY-RUN] Would clear: $p ($n files)" -ForegroundColor Magenta
        }
        $cs2Found = $true
    }
}
if (-not $cs2Found) {
    Write-Warn "CS2 Shader Cache not found."
    Write-Info "Manual: [Steam]\steamapps\shadercache\730"
}

# Windows Temp
Write-TierBadge 3 "Temp files  (no direct 1%-low effect — system hygiene)"
if (-not $SCRIPT:DryRun) {
    $total += Clear-Dir "$env:SystemRoot\Temp" "Windows Temp"
    $total += Clear-Dir $env:TEMP              "User Temp"
} else {
    Write-Host "  [DRY-RUN] Would clear: Windows Temp + User Temp" -ForegroundColor Magenta
}

# DNS Cache
Write-TierBadge 3 "Flush DNS cache"
if (-not $SCRIPT:DryRun) {
    try { ipconfig /flushdns | Out-Null; Write-OK "DNS cache flushed." } catch { Write-Warn "DNS flush failed." }
} else {
    Write-Host "  [DRY-RUN] Would run: ipconfig /flushdns" -ForegroundColor Magenta
}

# RAM Working Set
Write-TierBadge 3 "Trim RAM working set"
if (-not $SCRIPT:DryRun) {
    try {
        # Add-Type is blocked under Constrained Language Mode (AppLocker/WDAC).
        if ($ExecutionContext.SessionState.LanguageMode -eq 'ConstrainedLanguage') {
            Write-Info "Working set trim skipped (Constrained Language Mode — Add-Type blocked)."
            # Fall through to the catch handler for consistent flow
            throw "CLM"
        }
        Add-Type @"
using System; using System.Runtime.InteropServices;
public class MemHelper {
    [DllImport("kernel32.dll")] public static extern bool SetSystemFileCacheSize(UIntPtr min, UIntPtr max, uint flags);
}
"@ -ErrorAction SilentlyContinue
        $before = [math]::Round((Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue).FreePhysicalMemory / 1KB, 0)
        [MemHelper]::SetSystemFileCacheSize([UIntPtr]::new(1), [UIntPtr]::new(1), 0) | Out-Null
        # Reset file cache limits back to system defaults (flag 0x4 = FILE_CACHE_MAX_HARD_DISABLE)
        [MemHelper]::SetSystemFileCacheSize([UIntPtr]::Zero, [UIntPtr]::Zero, 4) | Out-Null
        $after  = [math]::Round((Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue).FreePhysicalMemory / 1KB, 0)
        Write-OK "RAM: ${after} MB free (before: ${before} MB)"
    } catch {
        if ($_.Exception.Message -ne "CLM") {
            Write-Info "Working set trim skipped."
        }
    }
} else {
    Write-Host "  [DRY-RUN] Would trim RAM working set" -ForegroundColor Magenta
}

# ══════════════════════════════════════════════════════════════════════════════
# FULL CLEANUP
# ══════════════════════════════════════════════════════════════════════════════
if ($doFull) {
    Write-Section "Full Cleanup"

    # GPU Shader Caches  [T1]
    Write-TierBadge 1 "Clear GPU Shader Caches"
    if (-not $SCRIPT:DryRun) {
        $total += Clear-Dir $CFG_NV_ShaderCache "NVIDIA DX Cache"
        $total += Clear-Dir $CFG_NV_GLCache     "NVIDIA GL Cache"
        $total += Clear-Dir $CFG_DX_ShaderCache "DirectX D3D Cache"
        $amdCache = "$env:LOCALAPPDATA\AMD\DxCache"
        if (Test-Path $amdCache) { $total += Clear-Dir $amdCache "AMD DX Cache" }
    } else {
        foreach ($cp in @($CFG_NV_ShaderCache, $CFG_NV_GLCache, $CFG_DX_ShaderCache)) {
            if (Test-Path $cp) { Write-Host "  [DRY-RUN] Would clear: $cp" -ForegroundColor Magenta }
        }
        $amdCache = "$env:LOCALAPPDATA\AMD\DxCache"
        if (Test-Path $amdCache) { Write-Host "  [DRY-RUN] Would clear: $amdCache" -ForegroundColor Magenta }
    }

    # Prefetch  [T3]
    Write-TierBadge 3 "Prefetch  (no 1%-low effect — system hygiene)"
    $pfPath = "$env:SystemRoot\Prefetch"
    if (Test-Path $pfPath) {
        $n = @(Get-ChildItem $pfPath -Filter "*.pf" -ErrorAction SilentlyContinue).Count
        if (-not $SCRIPT:DryRun) {
            Get-ChildItem $pfPath -Filter "*.pf" -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
            Write-OK "Prefetch: $n .pf files deleted"
            $total += $n
        } else {
            Write-Host "  [DRY-RUN] Would delete $n prefetch files from $pfPath" -ForegroundColor Magenta
        }
    }

    # Winsock Reset  [T2]
    Invoke-TieredStep -Tier 2 -Title "Winsock Reset" `
        -Why "Corrupted socket entries can cause connection pauses." `
        -Evidence "Helpful for server connection issues. No effect on lows if connection is stable." `
        -Caveat "Harmless, but restart recommended afterwards for full effect." `
        -Risk "SAFE" -Depth "NETWORK" `
        -Improvement "Fixes corrupted Winsock entries — helps with connection issues" `
        -SideEffects "May need restart for full effect" `
        -Undo "N/A (resets to clean state)" `
        -Action {
            if (-not $SCRIPT:DryRun) { netsh winsock reset | Out-Null; Write-OK "Winsock reset." }
            else { Write-Host "  [DRY-RUN] Would run: netsh winsock reset" -ForegroundColor Magenta }
        }

    # Event Logs  [Hygiene]
    Write-TierBadge 3 "Clear Event Logs  (no 1%-low effect)"
    if (-not $SCRIPT:DryRun) {
        try {
            foreach ($l in @("Application","System","Setup")) {
                wevtutil cl $l 2>&1 | Out-Null
            }
            Write-OK "Event Logs cleared."
        } catch { Write-Warn "Event Logs partially not cleared." }
    } else {
        Write-Host "  [DRY-RUN] Would clear: Application, System, Setup event logs" -ForegroundColor Magenta
    }

    # Steam Verification  [T2]
    Invoke-TieredStep -Tier 2 -Title "Verify CS2 game integrity (Steam)" `
        -Why "Corrupt or outdated CS2 files can cause shader stutter and crashes." `
        -Evidence "T2: Especially useful after CS2 updates or shader cache clearing." `
        -Caveat "Takes 2-5 min. Steam must be running." `
        -Risk "SAFE" -Depth "APP" `
        -Improvement "Fixes corrupt or outdated CS2 game files" `
        -SideEffects "Takes 2-5 minutes. Steam must be running." `
        -Undo "N/A (verification only)" `
        -Action {
            if (-not $SCRIPT:DryRun) {
                $steamExe = @(
                    "${env:ProgramFiles(x86)}\Steam\steam.exe",
                    "$env:ProgramFiles\Steam\steam.exe"
                )
                if ($steamBase) { $steamExe += "$steamBase\steam.exe" }
                $steamExe = $steamExe | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
                if ($steamExe) {
                    Write-Step "Starting CS2 verification..."
                    Start-Process "steam://validate/730" -ErrorAction SilentlyContinue
                    Write-OK "Steam verification started — wait for Steam to finish."
                } else {
                    Write-Warn "Steam.exe not found."
                    Write-Info "Manual: Steam -> CS2 -> Properties -> Local Files -> Verify"
                }
            } else {
                Write-Host "  [DRY-RUN] Would trigger: Steam CS2 game integrity verification" -ForegroundColor Magenta
            }
        }
}

# ══════════════════════════════════════════════════════════════════════════════
# DRIVER REFRESH
# ══════════════════════════════════════════════════════════════════════════════
if ($doDriver) {
    Write-Section "Driver Refresh  [T1 — restart required]"
    Write-TierBadge 1 "Run native GPU driver clean + reinstall"

    if ($SCRIPT:DryRun) {
        Write-Host "  [DRY-RUN] Would perform Driver Refresh:" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   1. Copy scripts to $CFG_WorkDir" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   2. Reset progress tracking" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   3. Set RunOnce for Phase 2" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN]   4. Boot into Safe Mode → GPU driver clean → Phase 3" -ForegroundColor Magenta
        Write-Host "  [DRY-RUN] No changes made." -ForegroundColor Magenta
        exit 0
    }

    # Validate GPU vendor from state — re-detect if state is missing or stale
    $gpuVendorOk = $false
    if (Test-Path $CFG_StateFile) {
        try {
            $st = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json
            if ($st.gpuInput -in @("1","2","3","4")) { $gpuVendorOk = $true }
        } catch { Write-DebugLog "State file read failed: $_" }
    }
    if (-not $gpuVendorOk) {
        Write-Warn "GPU vendor not found in state — please confirm:"
        Write-Host "  [1]  NVIDIA (RTX 5000 series)" -ForegroundColor White
        Write-Host "  [2]  NVIDIA (any other)" -ForegroundColor White
        Write-Host "  [3]  AMD" -ForegroundColor White
        Write-Host "  [4]  Intel" -ForegroundColor White
        do { $gpuChoice = Read-Host "  [1/2/3/4]" } while ($gpuChoice -notin @("1","2","3","4"))
        # Persist to state for SafeMode-DriverClean.ps1
        try {
            if (Test-Path $CFG_StateFile) {
                $st = Get-Content $CFG_StateFile -Raw -ErrorAction Stop | ConvertFrom-Json
            } else {
                # State file doesn't exist yet — create minimal state so SafeMode-DriverClean
                # can read gpuInput after reboot. Without this, the GPU choice is lost.
                $st = [PSCustomObject]@{ mode = $SCRIPT:Mode; profile = $SCRIPT:Profile }
            }
            $st | Add-Member -NotePropertyName "gpuInput" -NotePropertyValue $gpuChoice -Force
            Save-JsonAtomic -Data $st -Path $CFG_StateFile
        } catch { Write-DebugLog "Could not persist GPU choice: $_" }
    }

    Write-Warn "Requires restart into Safe Mode."
    Write-Host "  GPU Driver Clean: ✔  (native PowerShell — no tools needed)" -ForegroundColor Green
    Write-Host "  Driver Install:   ✔  (native extract + install)" -ForegroundColor Green

    if (-not (Confirm-Risk "Restart into Safe Mode now?" "Save all open files!")) {
        Write-Info "Cancelled."; exit 0
    }

    try {
        Copy-PhaseRuntimePayload -SourceRoot $ScriptRoot -DestinationRoot $CFG_WorkDir
    } catch {
        Write-Err "Driver Refresh: failed to copy scripts — $_"
        Write-Info "Ensure all suite files are in: $ScriptRoot"
        throw
    }

    # Initialize backup system before Set-BootConfig (which auto-backs up the current value)
    Initialize-Backup

    # Reset all progress unconditionally — Phase 2+3 will re-run from scratch
    Clear-Progress $null
    Set-RunOnce "CS2_Phase2" "$CFG_WorkDir\SafeMode-DriverClean.ps1" -SafeMode
    $SCRIPT:CurrentStepTitle = "Driver Refresh — Safe Mode boot"
    $null = Set-BootConfig "safeboot" "minimal" "Driver Refresh — Safe Mode for GPU driver clean"
    try { Flush-BackupBuffer } catch { Write-DebugLog "Flush after boot config backup failed: $_" }

    # Verify safeboot flag — retry with explicit {current} if first attempt failed
    if (-not (Test-BootConfigSet "safeboot")) {
        Write-Warn "Safeboot flag not detected — retrying with explicit identifier..."
        $null = bcdedit /set "{current}" safeboot minimal 2>&1
    }
    if (-not (Test-BootConfigSet "safeboot")) {
        Write-Err "Could not set Safe Mode boot flag. Run manually: bcdedit /set {current} safeboot minimal"
        Write-Host "  Then restart to enter Safe Mode for Phase 2." -ForegroundColor Cyan
        return
    }

    if ($SCRIPT:DryRun) { Write-Info "[DRY-RUN] Would restart into Safe Mode."; return }
    Write-Host "  Restarting in 10 seconds..." -ForegroundColor Yellow
    Start-Sleep 10; Restart-Computer -Force; exit 0
}

# ── Summary ──────────────────────────────────────────────────────────────────
$elapsed = [math]::Round(((Get-Date) - $start).TotalSeconds, 0)
Write-Log "SECTION" "=== CLEANUP DONE: $logTag | ${elapsed}s | $total entries ==="
$summaryTitle = if ($SCRIPT:DryRun) { "CLEANUP PREVIEW (DRY-RUN)" } else { "CLEANUP COMPLETE" }
$summaryColor = if ($SCRIPT:DryRun) { "Magenta" } else { "Green" }
Write-Blank
Write-Host "  ╔══════════════════════════════════════════════════════╗" -ForegroundColor $summaryColor
Write-Host "  ║  $($summaryTitle.PadRight(52))║" -ForegroundColor $summaryColor
Write-Host "  ║  $("Mode:     $logTag".PadRight(52))║" -ForegroundColor $summaryColor
Write-Host "  ║  $("Duration: ${elapsed}s".PadRight(52))║" -ForegroundColor $summaryColor
Write-Host "  ║  $("Deleted:  $total entries".PadRight(52))║" -ForegroundColor $summaryColor
Write-Host "  ╚══════════════════════════════════════════════════════╝" -ForegroundColor $summaryColor
Write-Blank
Write-Sub "Launch CS2 -> 'Compiling Shaders' briefly visible -> normal"
Write-Sub "Run benchmark -> update FPS cap"
Write-Info "Log: $CFG_LogFile"
