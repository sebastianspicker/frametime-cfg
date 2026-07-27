# ==============================================================================
#  Optimize-SystemBase.ps1 - Steps 2-9: XMP, Shader, FSO, NVIDIA, Power,
#                               HAGS, Pagefile, ReBAR
# ==============================================================================

# ══════════════════════════════════════════════════════════════════════════════
# STEP 2 - XMP/EXPO CHECK  [T1 check; T2 effect unclear]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 2) {
    Write-Section "Step 2 - XMP/EXPO Check"
    Write-Blank
    Write-Host "  HONEST ASSESSMENT OF XMP/EXPO IN CS2:" -ForegroundColor DarkYellow
    Write-Host @"
  Hardware context: XMP and EXPO request a firmware memory profile.
                    Platform support and stability vary by configuration.
  CS2-specific:     This repository contains no isolated benchmark that
                    quantifies a frame-time effect.
  Action:           Review the firmware profile and run a memory stability
                    test if you enable it. This step does not modify firmware.
"@ -ForegroundColor DarkGray
    Write-Blank
    $null = Invoke-TieredStep -Tier 1 -Title "Compare reported active and rated memory speeds" `
        -Why "Reports whether Windows exposes an active memory speed below the module's rated speed." `
        -Evidence "The repository can compare reported active and rated speeds. It contains no local CS2 benchmark for this setting." `
        -Risk "SAFE" -Depth "CHECK" `
        -Improvement "Identifies a reported active/rated memory-speed difference for manual review" `
        -SideEffects "None - read-only check with BIOS instructions" `
        -Undo "N/A (check only)" `
        -Action {
            $ram = Get-RamInfo
            if (-not $ram) {
                Write-Warn "Could not read RAM info. Check manually:"
                Write-Info "Task Manager -> Performance -> Memory -> Speed"
            } else {
                Write-Info "RAM:           $($ram.TotalGB) GB | $($ram.Sticks) stick(s)"
                if ($ram.IsDDR5) {
                    Write-Info "Rated speed:   $($ram.SpeedMhz) MT/s  (reported by Windows)"
                    Write-Info "Active:        $($ram.ActiveMhz) MT/s  (reported by Windows)"
                } else {
                    Write-Info "Rated speed:   $($ram.SpeedMhz) MHz  (per SPD)"
                    Write-Info "Active:        $($ram.ActiveMhz) MHz  (actual)"
                }

                if ($ram.XmpActive) {
                    Write-OK "Reported active memory speed matches the reported rated speed."
                } else {
                    Write-Warn "Reported active memory speed is below the reported rated speed."
                    Write-Blank
                    Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Yellow
                    $activeLabel = if ($ram.IsDDR5) { "$($ram.ActiveMhz) MT/s" } else { "$($ram.ActiveMhz) MHz" }
                    $ratedLabel  = if ($ram.IsDDR5) { "$($ram.SpeedMhz) MT/s" } else { "$($ram.SpeedMhz) MHz" }
                    Write-Host "  │  $("RAM running at $activeLabel instead of $ratedLabel".PadRight(60))│" -ForegroundColor Yellow
                    Write-Host "  │                                                              │" -ForegroundColor Yellow
                    Write-Host "  │  This repository does not quantify a CS2 frame-time effect. │" -ForegroundColor DarkGray
                    Write-Host "  │  Firmware support and memory stability must be verified.    │" -ForegroundColor DarkGray
                    Write-Host "  │                                                              │" -ForegroundColor Yellow
                    Write-Host "  │  BIOS GUIDE:                                                │" -ForegroundColor White
                    Write-Host "  │  1.  Restart PC -> BIOS (DEL / F2 / F12)                   │" -ForegroundColor White
                    Write-Host "  │  2.  Look for: XMP / EXPO / DOCP / Memory Profile          │" -ForegroundColor White
                    Write-Host "  │  3.  Enable Profile 1                                      │" -ForegroundColor White
                    Write-Host "  │  4.  Save + restart                                        │" -ForegroundColor White
                    Write-Host "  │  5.  Verify: Task Manager -> Performance -> Memory          │" -ForegroundColor White
                    $verifyLabel = if ($ram.IsDDR5) { "$($ram.SpeedMhz) MT/s" } else { "$($ram.SpeedMhz) MHz" }
                    Write-Host "  │  $("    -> Should show '$verifyLabel'".PadRight(60))│" -ForegroundColor White
                    Write-Host "  │                                                              │" -ForegroundColor Yellow
                    Write-Host "  │  AFTERWARDS: RAM stability test recommended                 │" -ForegroundColor DarkGray
                    Write-Host "  │  TM5 / HCI MemTest  (github.com/integrityhf/TM5)           │" -ForegroundColor DarkGray
                    Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Yellow
                    Write-Blank
                    if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] to continue (activate XMP in BIOS afterwards)" }
                }
            }

            # Firmware review is informational. The repository cannot read most
            # effective firmware values and does not prescribe overclock values.
            $amdCpu = Get-AmdCpuInfo
            $ddr5Info = Get-Ddr5TimingInfo
            $boardInfo = Get-MotherboardInfo

            Write-Blank
            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor DarkGray
            Write-Host "  │  FIRMWARE REVIEW (manual)                                   │" -ForegroundColor DarkGray
            Write-Host "  │                                                              │" -ForegroundColor DarkGray
            if ($amdCpu) {
                Write-Host "  │  CPU reported by Windows:                                  │" -ForegroundColor White
                Write-Host "  │  $($amdCpu.CpuName)" -ForegroundColor DarkGray
            }
            if ($boardInfo) {
                Write-Host "  │  Board reported by Windows:                                │" -ForegroundColor White
                Write-Host "  │  $($boardInfo.Manufacturer) $($boardInfo.Product)" -ForegroundColor DarkGray
            }
            if ($ddr5Info -and $ddr5Info.IsDDR5) {
                Write-Host "  │  DDR5 reported: $($ddr5Info.ActiveMTs) MT/s active, $($ddr5Info.RatedMTs) MT/s rated" -ForegroundColor DarkGray
            }
            Write-Host "  │                                                              │" -ForegroundColor DarkGray
            Write-Host "  │  Use current CPU, board, and memory-vendor documentation.  │" -ForegroundColor White
            Write-Host "  │  Record defaults before changing firmware settings.        │" -ForegroundColor White
            Write-Host "  │  Do not copy PBO, Curve Optimizer, clock, timing, or       │" -ForegroundColor DarkYellow
            Write-Host "  │  voltage values from another system.                       │" -ForegroundColor DarkYellow
            Write-Host "  │  If using XMP/EXPO, select a validated profile and run     │" -ForegroundColor White
            Write-Host "  │  memory and workload stability tests.                      │" -ForegroundColor White
            Write-Host "  │  Verify ReBAR and GPU link state in the OS after changes.  │" -ForegroundColor White
            Write-Host "  │  WHEA events are diagnostic evidence, not a root-cause     │" -ForegroundColor DarkGray
            Write-Host "  │  diagnosis by themselves.                                  │" -ForegroundColor DarkGray
            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor DarkGray

            Complete-Step $PHASE 2 "XMP-Check"
        } `
        -SkipAction { Skip-Step $PHASE 2 "XMP-Check" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 3 - SHADER CACHE CLEAR  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 3) {
    Write-Section "Step 3 - Clear CS2 + GPU Shader Cache"
    $null = Invoke-TieredStep -Tier 1 -Title "Clear Shader Cache" `
        -Why "Removes existing CS2 and driver shader-cache files so they can be rebuilt after a Windows or driver change." `
        -Evidence "Cache removal and subsequent rebuilding are observable. This repository does not quantify a frame-time effect." `
        -Risk "SAFE" -Depth "FILESYSTEM" `
        -Improvement "Forces cached shaders to be regenerated" `
        -SideEffects "The next launch can take longer while shaders are rebuilt" `
        -Undo "Shaders rebuild automatically on next launch" `
        -Action {
            # Warn if Steam or CS2 is running - locked files will silently fail to delete
            $steamRunning = Get-Process -Name "steam","cs2" -ErrorAction SilentlyContinue
            if ($steamRunning) {
                $procs = ($steamRunning | Select-Object -ExpandProperty Name -Unique) -join ", "
                Write-Warn "Running processes detected: $procs - some shader cache files may be locked."
                Write-Info "For a complete cache clear, close Steam and CS2 first."
            }
            $steamBase = Get-SteamPath
            $paths = [System.Collections.Generic.List[string]]$CFG_ShaderCache_Paths
            if ($steamBase) { $paths.Add("$steamBase\steamapps\shadercache\730") }
            $found = $false
            foreach ($p in ($paths | Select-Object -Unique)) {
                if (Test-Path $p) {
                    $n = @(Get-ChildItem $p -Recurse -ErrorAction SilentlyContinue).Count
                    Write-Step "CS2 Cache: $p  ($n files)"
                    if (-not $SCRIPT:DryRun) {
                        Get-ChildItem $p -Recurse -ErrorAction SilentlyContinue |
                            Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
                        $remaining = @(Get-ChildItem $p -Recurse -ErrorAction SilentlyContinue).Count
                        if ($remaining -gt 0) {
                            Write-Warn "Partially cleared: $p ($remaining files locked - close Steam/CS2 to clear fully)"
                        } else {
                            Write-OK "Cleared: $p"
                        }
                    } else {
                        Write-Host "  [DRY-RUN] Would clear: $p ($n files)" -ForegroundColor Magenta
                    }
                    $found = $true
                }
            }
            foreach ($c in @($CFG_NV_ShaderCache, $CFG_NV_GLCache, $CFG_DX_ShaderCache)) {
                if (Test-Path $c) {
                    $n = @(Get-ChildItem $c -Recurse -ErrorAction SilentlyContinue).Count
                    Write-Step "GPU Cache: $c  ($n files)"
                    if (-not $SCRIPT:DryRun) {
                        Get-ChildItem $c -Recurse -ErrorAction SilentlyContinue |
                            Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
                        $remaining = @(Get-ChildItem $c -Recurse -ErrorAction SilentlyContinue).Count
                        if ($remaining -gt 0) {
                            Write-Warn "Partially cleared: $c ($remaining files locked)"
                        } else {
                            Write-OK "Cleared: $c"
                        }
                    } else {
                        Write-Host "  [DRY-RUN] Would clear: $c ($n files)" -ForegroundColor Magenta
                    }
                    $found = $true
                }
            }
            if (-not $found) {
                Write-Warn "Shader cache not found. Manual: [Steam]\steamapps\shadercache\730"
            }
            Write-Info "Restart CS2 -> 'Compiling Shaders' appears briefly -> normal."
            Complete-Step $PHASE 3 "ShaderCache"
        } `
        -SkipAction { Skip-Step $PHASE 3 "ShaderCache" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 4 - FULLSCREEN OPTIMIZATIONS DISABLE  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 4) {
    Write-Section "Step 4 - Disable Fullscreen Optimizations (cs2.exe)"
    $null = Invoke-TieredStep -Tier 1 -Title "Disable Fullscreen Optimizations for cs2.exe" `
        -Why "Sets the Windows compatibility flag that disables fullscreen optimizations for cs2.exe." `
        -Evidence "The registry change is deterministic. This repository contains no isolated benchmark for its frame-time effect." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Allows the compatibility rendering path to be compared on the local system" `
        -SideEffects "Changes the Windows presentation path used for cs2.exe and may alter display behavior" `
        -Undo "Delete AppCompatFlags\Layers entry for cs2.exe" `
        -Action {
            # Use Get-CS2InstallPath which parses libraryfolders.vdf for custom library locations
            $cs2Install = Get-CS2InstallPath
            $cs2Exe = if ($cs2Install) { "$cs2Install\game\bin\win64\cs2.exe" } else { $null }
            # Verify the exe actually exists at the detected path
            if ($cs2Exe -and -not (Test-Path $cs2Exe)) { $cs2Exe = $null }

            if ($cs2Exe) {
                Write-DebugLog "cs2.exe: $cs2Exe"
                $regPath = "HKCU:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers"
                $fsoWriteResult = Set-RegistryValue $regPath $cs2Exe `
                    "~ DISABLEDXMAXIMIZEDWINDOWEDMODE" "String" `
                    "Disable fullscreen optimizations for cs2.exe" -PassThru
                if (-not $fsoWriteResult) {
                    throw "Fullscreen Optimizations registry write returned no result."
                }
                if ($SCRIPT:DryRun -and $fsoWriteResult.Status -eq "DryRun") {
                    Write-Info "Fullscreen Optimizations registry write previewed for: $cs2Exe"
                } elseif (-not $SCRIPT:DryRun -and
                    $fsoWriteResult.Status -eq "Success" -and
                    $fsoWriteResult.Applied) {
                    Write-ActionOK "Fullscreen Optimizations disabled: $cs2Exe"
                } else {
                    throw "Fullscreen Optimizations registry write did not complete: $($fsoWriteResult.Message)"
                }
                Write-DebugLog "AppCompatFlags set: $cs2Exe"
            } else {
                Write-Warn "cs2.exe not found - manual:"
                Write-Info "cs2.exe -> Right-click -> Properties -> Compatibility"
                Write-Info "-> Check 'Disable fullscreen optimizations'"
                Write-Info "Typical path: Steam\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\"
                Skip-Step $PHASE 4 "FSO"
                return
            }
            Complete-Step $PHASE 4 "FSO"
        } `
        -SkipAction { Skip-Step $PHASE 4 "FSO" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 5 - NVIDIA DRIVER VERSION INVENTORY  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 5 -and $gpuInput -in @("1","2")) {
    Write-Section "Step 5 - NVIDIA Driver Version Inventory"
    $nvDrv = Get-NvidiaDriverVersion
    if ($nvDrv) {
        Write-Info "Installed driver: $($nvDrv.Version)  ($($nvDrv.Name))"
        Write-Info "This step does not classify the driver or recommend a fixed rollback version."
        Write-Info "Compare versions with the same workload and verify current GPU, Windows, and game support."
        Complete-Step $PHASE 5 "NVDriverInventory"
    } else {
        Write-Info "No installed NVIDIA display-driver version was detected."
        Skip-Step $PHASE 5 "NVDriverInventory"
    }
} elseif ($startStep -le 5) {
    Skip-Step $PHASE 5 "NVDriverInventory (no NVIDIA)"
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 6 - FRAMETIME.CFG POWER PLAN  [T1]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 6) {
    Write-Section "Step 6 - frametime.cfg Power Plan"
    $null = Invoke-TieredStep -Tier 1 -Title "Create frametime.cfg Power Plan (native, tiered)" `
        -Why "Creates a PowerShell-defined plan that changes CPU parking, USB, disk, and vendor-specific processor settings." `
        -Evidence "The applied powercfg values are verified and logged. This repository does not include an isolated performance benchmark for the plan." `
        -Risk "MODERATE" -Depth "REGISTRY" `
        -Improvement "Applies the tiered repository power policy and preserves the previous active-plan identity" `
        -SideEffects "Can increase idle power and temperature. DC and battery settings are not changed." `
        -Undo "powercfg /setactive <original GUID> (auto-backed up) or START.bat [7] Restore/Rollback" `
        -Action {
            # Backup-PowerPlan flushes and verifies the original scheme before
            # this action is allowed to create or activate any replacement.
            Backup-PowerPlan -StepTitle "frametime.cfg Power Plan"
            $powerPlanResult = Invoke-FrametimePowerPlanWithFallback
            if ($powerPlanResult.Status -eq 'Failed') {
                throw $powerPlanResult.Message
            }
            if ($powerPlanResult.Status -eq 'Fallback') {
                Write-Warn $powerPlanResult.Message
                Skip-Step $PHASE 6 "PowerPlan (fallback active)"
            } else {
                $guid = $powerPlanResult.Guid
                if ($SCRIPT:DryRun) {
                    Write-Host "  [DRY-RUN] Would activate: frametime.cfg" -ForegroundColor Magenta
                }

                $isAMD   = (Get-ChipsetVendor) -eq "AMD"
                $vTag    = if ($isAMD) { "AMD" } else { "Intel" }
                $applyT2 = $SCRIPT:Profile -in @("RECOMMENDED", "COMPETITIVE", "CUSTOM", "YOLO")
                $applyT3 = $SCRIPT:Profile -in @("COMPETITIVE", "CUSTOM", "YOLO")

                Write-Blank
                Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Green
                Write-Host "  │  frametime.cfg  •  CPU vendor: $vTag$((' ' * (31 - $vTag.Length)))│" -ForegroundColor Green
                Write-Host "  ├──────────────────────────────────────────────────────────────┤" -ForegroundColor Green
                Write-Host "  │  [T1] CPU max=100%, no parking, USB suspend off,             │" -ForegroundColor Green
                Write-Host "  │       disk idle off, sleep/hibernate off, active cooling      │" -ForegroundColor Green
                if ($applyT2) {
                    Write-Host "  │  [T2] EPP=0, boost 254/255, max idle C1, NVMe/USB-C off      │" -ForegroundColor Yellow
                    if ($isAMD) {
                        Write-Host "  │       CPU min=0% (AMD - PB2 compatible)                      │" -ForegroundColor Yellow
                    } else {
                        Write-Host "  │       CPU min=100% + ring cores (Intel)                      │" -ForegroundColor Yellow
                    }
                } else {
                    Write-Host "  │  [T2] skipped - upgrade to RECOMMENDED for CPU/NVMe tweaks  │" -ForegroundColor DarkGray
                }
                if ($applyT3) {
                    Write-Host "  │  [T3] C-states off, duty cycling off, fast ramp   (+temp)   │" -ForegroundColor DarkYellow
                } else {
                    Write-Host "  │  [T3] skipped - COMPETITIVE profile for C-states off         │" -ForegroundColor DarkGray
                }
                Write-Host "  ├──────────────────────────────────────────────────────────────┤" -ForegroundColor DarkGray
                Write-Host "  │  4 imported values changed: cooling, standby,             │" -ForegroundColor DarkGray
                Write-Host "  │  PERFAUTONOMOUS unchanged, duty cycling off                 │" -ForegroundColor DarkGray
                Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Green
                Write-Blank
                Write-Info "Undo: Control Panel -> Power Options -> select original plan"
                Write-Info "      or START.bat [7] Restore/Rollback -> Power Plan"
                if ($powerPlanResult.CanCompleteStep) {
                    Complete-Step $PHASE 6 "PowerPlan"
                }
            }
        } `
        -SkipAction { Skip-Step $PHASE 6 "PowerPlan" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 7 - HAGS  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 7) {
    Write-Section "Step 7 - Hardware-accelerated GPU Scheduling (HAGS)"
    if ($gpuInput -in @("3","4")) {
        Write-Info "AMD/Intel: Configure HAGS via system settings or driver software."
        Skip-Step $PHASE 7 "HAGS"
    } else {
        # The repository defaults HAGS off for detected X3D CPUs.
        $amdInfo = Get-AmdCpuInfo
        $isX3D = ($amdInfo -and $amdInfo.IsX3D)
        $hagsState = if ($isX3D) { "OFF (repository default; benchmark both states)" }
            elseif ($gpuInput -eq "2") { "ON (repository default for RTX 40)" }
            else { "ON (repository default for RTX 50/40)" }
        $hagsVal = if ($isX3D) { 0 } else { 2 }
        $null = Invoke-TieredStep -Tier 2 -Title "HAGS $hagsState" `
            -Why "HAGS changes GPU scheduling ownership between Windows and supported GPU hardware." `
            -Evidence "Driver, Windows, and GPU combinations can behave differently. This repository contains no hardware matrix or benchmark artifact for HAGS." `
            -Caveat "Benchmark both states with the same workload. The selected state is a repository heuristic, not a compatibility guarantee." `
            -Risk "MODERATE" -Depth "REGISTRY" `
            -Improvement "Applies the repository default so both scheduling states can be compared" `
            -SideEffects "May improve or worsen frame timing depending on Windows, driver, and GPU versions" `
            -Undo "Set HwSchMode = 1 (or toggle in Windows Settings -> Display -> Graphics)" `
            -Action {
                # Report Secure Boot as security context only. It is not used to
                # determine HAGS compatibility.
                try {
                    $sb = Confirm-SecureBootUEFI -ErrorAction SilentlyContinue
                    if ($sb -eq $true) {
                        Write-Info "Secure Boot: enabled. This step does not use it to determine HAGS compatibility."
                    } elseif ($sb -eq $false) {
                        Write-Info "Secure Boot: disabled. This status is reported separately from HAGS compatibility."
                    }
                } catch { Write-Sub "Secure Boot status: not readable (non-UEFI or restricted access)" }

                # ── VBS/HVCI detection ────────────────────────────────────────────────
                # Virtualization-Based Security runs a hypervisor-backed isolation layer.
                # Its workload effect depends on the Windows and hardware configuration.
                try {
                    $dg = Get-CimInstance -ClassName Win32_DeviceGuard `
                        -Namespace root/Microsoft/Windows/DeviceGuard -ErrorAction SilentlyContinue
                    if ($dg) {
                        # VirtualizationBasedSecurityStatus: 0=off, 1=enabled-not-running, 2=running
                        $vbsStatus = $dg.VirtualizationBasedSecurityStatus
                        if ($vbsStatus -ge 2) {
                            Write-Blank
                            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Yellow
                            Write-Host "  │  VBS/HVCI ACTIVE                                           │" -ForegroundColor Yellow
                            Write-Host "  │                                                              │" -ForegroundColor Yellow
                            Write-Host "  │  Virtualization-Based Security is running on this system.   │" -ForegroundColor White
                            Write-Host "  │  Measure workload effects locally before changing it.      │" -ForegroundColor White
                            Write-Host "  │                                                              │" -ForegroundColor Yellow
                            Write-Host "  │  TO DISABLE (this reduces Windows security protections):   │" -ForegroundColor White
                            Write-Host "  │  1. Windows Security -> Device Security -> Core Isolation   │" -ForegroundColor DarkGray
                            Write-Host "  │     -> Memory Integrity: OFF  (reboot required)             │" -ForegroundColor DarkGray
                            Write-Host "  │  2. If still active: BIOS -> Virtualization (VT-d/AMD-Vi)  │" -ForegroundColor DarkGray
                            Write-Host "  │     -> OFF  (also disables WSL2, VMs, Docker)              │" -ForegroundColor DarkGray
                            Write-Host "  │  3. Verify: msinfo32 -> Virtualization-based security      │" -ForegroundColor DarkGray
                            Write-Host "  │     -> should read 'Not Enabled'                           │" -ForegroundColor DarkGray
                            Write-Host "  │                                                              │" -ForegroundColor Red
                            Write-Host "  │  WARNING: Reduces LSASS credential theft protection.       │" -ForegroundColor Red
                            Write-Host "  │  Verify security and software requirements first.          │" -ForegroundColor Red
                            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Yellow
                            Write-Blank
                        } elseif ($vbsStatus -eq 1) {
                            Write-Info "VBS is configured but not reported as running."
                        } else {
                            Write-OK "VBS/HVCI is not active."
                        }
                    }
                } catch { Write-DebugLog "VBS detection failed: $_" }

                try {
                    Set-RegistryValue "HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers" "HwSchMode" $hagsVal "DWord" "HAGS toggle"
                } catch { Write-Warn "Manual: Windows Settings -> System -> Display -> Graphics" }
                Complete-Step $PHASE 7 "HAGS"
            } `
            -SkipAction { Skip-Step $PHASE 7 "HAGS" }
    }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 8 - PAGEFILE  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 8) {
    Write-Section "Step 8 - Configure Pagefile"
    $ram = Get-RamInfo
    $ramGB = if ($ram) { $ram.TotalGB } else { 0 }

    if ($ramGB -eq 0) {
        Write-Warn "Could not detect RAM - skipping pagefile configuration."
        Skip-Step $PHASE 8 "Pagefile (no RAM info)"
    } elseif ($ramGB -ge 32) {
        Write-Info "RAM: ${ramGB} GB - pagefile fix has little effect on CS2 with >= 32 GB RAM."
        Write-Info "CS2 fits entirely in RAM. Pagefile only used on RAM overflow."
        # Still check for dangerous pagefile configs even with plenty of RAM:
        # disabled pagefile (0 MB) or system-managed with no guaranteed minimum
        # can cause hard crashes when RAM is fully consumed by CS2 + other apps.
        $pfCheck = Get-CimInstance Win32_PageFileSetting -ErrorAction SilentlyContinue
        $autoManaged = (Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue).AutomaticManagedPagefile
        if ($pfCheck -and $pfCheck.InitialSize -eq 0 -and $pfCheck.MaximumSize -eq 0 -and -not $autoManaged) {
            Write-Warn "Pagefile is DISABLED (0 MB). Even with ${ramGB} GB RAM, a missing pagefile can cause hard crashes under extreme memory pressure."
            Write-Warn "Recommendation: Enable system-managed pagefile or set a minimum of 4096 MB."
        } elseif ($autoManaged) {
            Write-Info "Pagefile is system-managed (OK for ${ramGB} GB RAM)."
        }
        Skip-Step $PHASE 8 "Pagefile (sufficient RAM)"
    } else {
        $pfMB = if ($ramGB -gt 0) { $ramGB * 1024 * 2 } else { 16384 }
        $null = Invoke-TieredStep -Tier 2 -Title "Fix pagefile at ${pfMB} MB (2x RAM: ${ramGB} GB)" `
            -Why "Creates a fixed pagefile instead of using dynamic or disabled pagefile behavior." `
            -Evidence "The allocated size and Windows pagefile state are verifiable. This repository includes no isolated frame-time benchmark." `
            -Caveat "A fixed size consumes disk space and may be unsuitable for some workloads. Windows system-managed mode remains an alternative." `
            -Risk "MODERATE" -Depth "REGISTRY" `
            -Improvement "Allocates a fixed pagefile capacity based on the selected value" `
            -SideEffects "Uses ${pfMB} MB fixed disk space for pagefile" `
            -Undo "Set AutomaticManagedPagefile = true in System Properties -> Advanced" `
            -Action {
                Write-Info "Pagefile size: ${pfMB} MB (you can adjust)"
                if (-not $SCRIPT:DryRun) {
                    $pfIn = if (Test-YoloProfile) { "" } else { Read-Host "  Accept MB value? [Enter] or type new number" }
                    if ($pfIn.Trim() -ne "") {
                        $pfV = 0
                        if ([int]::TryParse($pfIn,[ref]$pfV) -and $pfV -gt 0) { $pfMB = $pfV }
                    }
                }
                if (-not $SCRIPT:DryRun) {
                    try {
                        # NOTE: Uses Get-WmiObject (not CIM) because .Put() method is WMI-specific.
                        # Get-WmiObject is removed in PowerShell 7+. The tool targets PS 5.1
                        # (shipped with Windows 10/11). If running under PS 7, install the
                        # Microsoft.PowerShell.Management compatibility module or use PS 5.1.
                        $cs = Get-WmiObject Win32_ComputerSystem

                        # Backup current pagefile config before modification.
                        # Manual restore: System Properties -> Advanced -> Performance -> Virtual Memory
                        $wasAutoManaged = [bool]$cs.AutomaticManagedPagefile
                        $existingPf = Get-WmiObject -Class Win32_PageFileSetting -Filter "Name='C:\\pagefile.sys'" -ErrorAction SilentlyContinue
                        $origInit = if ($existingPf) { $existingPf.InitialSize } else { 0 }
                        $origMax  = if ($existingPf) { $existingPf.MaximumSize } else { 0 }
                        Backup-PagefileConfig -AutomaticManaged $wasAutoManaged `
                            -PagefilePath "C:\pagefile.sys" -InitialSize $origInit `
                            -MaximumSize $origMax -StepTitle $SCRIPT:CurrentStepTitle

                        $cs.AutomaticManagedPagefile = $false; $cs.Put() | Out-Null

                        # Detect existing pagefiles on all drives
                        $allPfs = @(Get-WmiObject -Class Win32_PageFileSetting -ErrorAction SilentlyContinue)
                        $sysDrive = $env:SystemDrive
                        if (-not $sysDrive) { $sysDrive = "C:" }
                        $nonSysPfs = $allPfs | Where-Object { $_.Name -and -not $_.Name.StartsWith("$sysDrive\") }
                        if ($nonSysPfs) {
                            $drives = ($nonSysPfs | ForEach-Object { $_.Name }) -join ", "
                            Write-Warn "Existing pagefile(s) on other drives: $drives"
                            Write-Info "These will remain unchanged. Remove via System Properties -> Advanced if not needed."
                        }

                        $pfPath = "$sysDrive\pagefile.sys"
                        $pfFilter = "Name='$($pfPath -replace '\\', '\\')'"
                        $pf = Get-WmiObject -Class Win32_PageFileSetting -Filter $pfFilter
                        if (-not $pf) { $pf = ([wmiclass]"Win32_PageFileSetting").CreateInstance() }
                        $pf.Name = $pfPath
                        $pf.InitialSize = $pfMB; $pf.MaximumSize = $pfMB; $pf.Put() | Out-Null
                        Write-OK "Pagefile: $pfPath | ${pfMB} MB fixed (takes effect after restart)"
                    } catch { Write-Warn "Pagefile configuration failed: $_" }
                } else {
                    $sysDriveDry = if ($env:SystemDrive) { $env:SystemDrive } else { "C:" }
                    Write-Host "  [DRY-RUN] Would set pagefile to ${pfMB} MB fixed on ${sysDriveDry}" -ForegroundColor Magenta
                }
                Complete-Step $PHASE 8 "Pagefile"
            } `
            -SkipAction { Skip-Step $PHASE 8 "Pagefile" }
    }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 9 - RESIZABLE BAR  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 9) {
    Write-Section "Step 9 - Resizable BAR / Smart Access Memory"
    if ($gpuInput -eq "3") {
        $null = Invoke-TieredStep -Tier 2 -Title "Check AMD Smart Access Memory (SAM)" `
            -Why "Checks whether Smart Access Memory is reported and provides firmware guidance." `
            -Evidence "Support depends on the CPU, GPU, motherboard, firmware, and driver. No local benchmark is included." `
            -Caveat "Confirm support for the exact hardware before changing firmware settings." `
            -Risk "SAFE" -Depth "CHECK" `
            -Improvement "Reports support state and provides firmware guidance" `
            -SideEffects "No automatic firmware change; compatibility remains platform-specific" `
            -Undo "N/A (BIOS setting)" `
            -Action {
                Write-Info "SAM cannot be set via PowerShell - BIOS required."
                Write-Blank
                Write-Host "  SAM BIOS GUIDE:" -ForegroundColor White
                Write-Info "  1.  Restart PC -> BIOS (DEL / F2)"
                Write-Info "  2.  Advanced -> PCI Subsystem -> Above 4G Decoding: ENABLED"
                Write-Info "  3.  Advanced -> PCI Subsystem -> Re-Size BAR Support: ENABLED (or Auto)"
                Write-Info "  4.  AMD specific: SAM / Smart Access Memory: ENABLED"
                Write-Info "  5.  Save + restart"
                Write-Info "  Verify: AMD Adrenalin -> System -> SmartAccess Memory: ON"
                if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] when done or to skip" }
                Complete-Step $PHASE 9 "ReBAR"
            } `
            -SkipAction { Skip-Step $PHASE 9 "ReBAR" }
    } elseif ($gpuInput -in @("1","2")) {
        $null = Invoke-TieredStep -Tier 2 -Title "Check NVIDIA Resizable BAR" `
            -Why "Checks whether Resizable BAR is reported and provides firmware guidance." `
            -Evidence "Support depends on the GPU, motherboard, firmware, driver, and application profile. No local benchmark is included." `
            -Caveat "Confirm support for the exact GPU and motherboard before changing firmware settings." `
            -Risk "SAFE" -Depth "CHECK" `
            -Improvement "Reports support state and provides firmware guidance" `
            -SideEffects "No automatic firmware change; compatibility remains platform-specific" `
            -Undo "N/A (BIOS setting)" `
            -Action {
                Write-Info "BIOS setting required - cannot be set via PowerShell."
                Write-Host "  ReBAR BIOS GUIDE:" -ForegroundColor White
                Write-Info "  1.  Restart PC -> BIOS (DEL / F2)"
                Write-Info "  2.  Advanced -> PCI Subsystem -> Above 4G Decoding: ENABLED"
                Write-Info "  3.  Advanced -> PCI Subsystem -> Re-Size BAR Support: ENABLED"
                Write-Info "  4.  Save + restart"
                Write-Info "  Verify: GPU-Z -> Advanced -> ReBAR: Yes"
                if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] when done or to skip" }
                Complete-Step $PHASE 9 "ReBAR"
            } `
            -SkipAction { Skip-Step $PHASE 9 "ReBAR" }
    } else {
        Write-Info "Intel Arc ReBAR state is not verified by this step; confirm it in firmware and vendor diagnostics."
        Skip-Step $PHASE 9 "ReBAR"
    }
}
