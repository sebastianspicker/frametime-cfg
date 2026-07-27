# ==============================================================================
#  Optimize-GameConfig.ps1  -  Steps 34-38: Autoexec, Chipset, Visual Effects,
#                                Services, Safe Mode Preparation
# ==============================================================================

# ══════════════════════════════════════════════════════════════════════════════
# STEP 34 - OPTIMIZATION.CFG GENERATOR  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 34) {
    Write-Section "Step 34 - optimization.cfg Generator"
    $null = Invoke-TieredStep -Tier 2 -Title "Generate optimization.cfg and update the autoexec bootstrap" `
        -Why "Generates the 73 repository-defined CVars across ten configuration categories, writes optimization.cfg, and adds one exec line to autoexec.cfg." `
        -Evidence "The generated key count, file paths, backup behavior, and append behavior are covered by tests. The repository does not include a benchmark for each CVar." `
        -Caveat "Game updates can remove or change CVars. Validate console output after updates. The SDR, audio, gamma, and display defaults may need local adjustment." `
        -Risk "SAFE" -Depth "APP" `
        -Improvement "Creates a deterministic repository configuration that can be reviewed and removed" `
        -SideEffects "optimization.cfg overrides same-named earlier CVars. Audio, weapon-switch, routing, display, and other behavior can change." `
        -Undo "Remove 'exec optimization.cfg' from autoexec.cfg, or delete optimization.cfg from game\csgo\cfg\" `
        -Action {
            # Build effective autoexec from config defaults. Thread-pool forcing is
            # intentionally omitted; keep it user/benchmark-driven.
            $effectiveAutoexec = [ordered]@{}
            foreach ($kv in $CFG_CS2_Autoexec.GetEnumerator()) { $effectiveAutoexec[$kv.Key] = $kv.Value }

            $cs2Path = Get-CS2InstallPath
            if (-not $cs2Path) {
                Write-Warn "CS2 not found. Manual: create game\csgo\cfg\optimization.cfg with:"
                foreach ($kv in $effectiveAutoexec.GetEnumerator()) {
                    Write-Sub "$($kv.Key) $($kv.Value)"
                }
                Write-Info "Then add 'exec optimization.cfg' as the last line of autoexec.cfg."
                Complete-Step $PHASE 34 "Autoexec"
            } else {
                $cfgDir       = "$cs2Path\game\csgo\cfg"
                $autoexecPath = "$cfgDir\autoexec.cfg"
                $optPath      = "$cfgDir\optimization.cfg"
                if (-not $SCRIPT:DryRun) { Ensure-Dir $cfgDir }

                # ── Read existing autoexec.cfg (read-only - we only append one line later) ──
                $existingLines = @()
                $existingKeys  = @{}
                if (Test-Path $autoexecPath) {
                    $existingLines = @(Get-Content $autoexecPath -Encoding UTF8)
                    foreach ($line in $existingLines) {
                        # Skip blank lines, comments (// ...), and command lines (not CVar assignments)
                        if ($line -match '^\s*(//|$)') { continue }
                        # Source 2 commands: bind/unbind/bindtoggle (key bindings), alias (macros),
                        # exec (run cfg), toggle/incrementvar (CVar manipulation), echo/say/say_team (output),
                        # host_writeconfig (save), clear (console), setinfo (user info strings),
                        # unbindall (reset bindings), +/- prefix (hold/release actions like +jump).
                        # Longer keywords before shorter (unbindall before unbind, bindtoggle before bind)
                        # so the \b word-boundary check works correctly on shorter-prefix matches.
                        if ($line -match '^\s*(\+|-|exec|bindtoggle|bind|unbindall|unbind|alias|toggle|incrementvar|echo|host_writeconfig|clear|say_team|say|setinfo)\b') { continue }
                        if ($line -match '^\s*(\S+)\s+(.+)$') {
                            # Strip inline comments (CS2 treats // as comment delimiter)
                            $val = $Matches[2] -replace '\s*//.*$', ''
                            $existingKeys[$Matches[1]] = $val.Trim()
                        }
                    }
                    Write-Info "autoexec.cfg: $($existingLines.Count) lines, $($existingKeys.Count) CVars detected."
                } else {
                    Write-Info "No autoexec.cfg found - will create a minimal one with exec line."
                }

                # ── Compare current autoexec vs. what optimization.cfg will contain ──────
                $matching  = [System.Collections.Generic.List[string]]::new()
                $differing = [System.Collections.Generic.List[hashtable]]::new()
                $newKeys   = [System.Collections.Generic.List[string]]::new()
                foreach ($kv in $effectiveAutoexec.GetEnumerator()) {
                    if (-not $existingKeys.ContainsKey($kv.Key)) {
                        $newKeys.Add($kv.Key)
                    } elseif ($existingKeys[$kv.Key] -ne $kv.Value) {
                        $differing.Add(@{ Key=$kv.Key; Current=$existingKeys[$kv.Key]; Recommended=$kv.Value })
                    } else {
                        $matching.Add($kv.Key)
                    }
                }

                # ── Status summary ─────────────────────────────────────────────────────────
                Write-Blank
                Write-Host "  YOUR AUTOEXEC vs. OPTIMIZATION.CFG:" -ForegroundColor White
                Write-Host "  ─────────────────────────────────────────────────────────────" -ForegroundColor DarkGray
                Write-Host "  $([char]0x2713)  Already at recommended value:  $($matching.Count) CVars" -ForegroundColor Green
                Write-Host "  !  Conflicts (opt.cfg overrides):    $($differing.Count) CVars" -ForegroundColor Yellow
                Write-Host "  +  New (not in your autoexec):       $($newKeys.Count) CVars" -ForegroundColor Cyan
                Write-Host "  ─────────────────────────────────────────────────────────────" -ForegroundColor DarkGray

                # ── Show conflicts ─────────────────────────────────────────────────────────
                # optimization.cfg is exec'd at the END of autoexec.cfg, so its values win.
                if ($differing.Count -gt 0) {
                    Write-Blank
                    Write-Host "  CONFLICTS - optimization.cfg runs after autoexec.cfg, so these" -ForegroundColor Yellow
                    Write-Host "  values in optimization.cfg override your current autoexec settings:" -ForegroundColor Yellow
                    Write-Blank
                    foreach ($d in $differing) {
                        Write-Host "    $($d.Key)" -ForegroundColor White
                        Write-Host "      autoexec (yours):   $($d.Current)" -ForegroundColor DarkYellow
                        Write-Host "      optimization.cfg:   $($d.Recommended)" -ForegroundColor Green
                    }
                    Write-Blank
                    Write-Info "To keep your own value: remove that key from optimization.cfg after install."
                }

                # ── Optionally show full optimization.cfg contents ─────────────────────────
                Write-Blank
                $showAll = if ($SCRIPT:DryRun -or (Test-YoloProfile)) { "n" } else { Read-Host "  Show full optimization.cfg ($($effectiveAutoexec.Count) CVars)? [y/N]" }
                if ($showAll -match "^[yY]$") {
                    Write-Blank
                    foreach ($kv in $effectiveAutoexec.GetEnumerator()) {
                        $marker = if ($matching.Contains($kv.Key))  { [char]0x2713 }
                                  elseif ($newKeys.Contains($kv.Key)) { "+" }
                                  else { "!" }
                        $color  = switch ($marker) {
                            { $_ -eq [char]0x2713 } { "DarkGreen" }
                            "+"                     { "Cyan" }
                            default                 { "Yellow" }
                        }
                        Write-Host "    $marker $($kv.Key) $($kv.Value)" -ForegroundColor $color
                    }
                    Write-Blank
                }

                # ── Write files ────────────────────────────────────────────────────────────
                $proceed = if ($SCRIPT:DryRun -or (Test-YoloProfile)) { "Y" } else { Read-Host "  Write optimization.cfg + add 'exec optimization.cfg' to autoexec? [Y/n]" }
                if ($proceed -notmatch "^[nN]$") {

                    # Write optimization.cfg - full clean write each run
                    $optLines = @(
                        "// frametime.cfg - optimization.cfg",
                        "// Generated: $(Get-Date -Format 'yyyy-MM-dd HH:mm')",
                        "// exec'd from the end of autoexec.cfg - overrides earlier same-named CVars.",
                        "// To revert one setting: remove or comment its line here.",
                        "// To revert all:         remove 'exec optimization.cfg' from autoexec.cfg.",
                        "//",
                        "// Optional standalone CFGs (also in game\csgo\cfg\, use from console as needed):",
                        "//   exec net_stable     - baseline / reset (stable wired/fiber)",
                        "//   exec net_highping   - 60ms+ ping, stable route",
                        "//   exec net_unstable   - jitter + loss, ping OK (Wi-Fi / 4G)",
                        "//   exec net_bad        - high ping + jitter/loss (satellite / mobile)",
                        "//   exec debug_hud      - temporary telemetry and network diagnostics",
                        "//   exec debug_hud_off  - reset diagnostic telemetry to quiet defaults",
                        "//   exec audio_stable   - suite audio buffer default / reset",
                        "//   exec audio_lowlatency_025 - experimental lower audio buffer",
                        "//   exec audio_lowlatency_001 - aggressive lower audio buffer",
                        ""
                    )
                    foreach ($kv in $effectiveAutoexec.GetEnumerator()) {
                        $optLines += "$($kv.Key) $($kv.Value)"
                    }
                    if (-not $SCRIPT:DryRun) {
                        # Back up existing optimization.cfg before overwriting (only if no backup exists yet)
                        if ((Test-Path $optPath) -and -not (Test-Path "$optPath.bak")) { Copy-Item $optPath "$optPath.bak" -Force }
                        # Use BOM-less UTF-8 - PS 5.1's -Encoding UTF8 adds a BOM (EF BB BF)
                        # which can corrupt the first line if Source 2 doesn't skip it.
                        [System.IO.File]::WriteAllLines($optPath, $optLines, [System.Text.UTF8Encoding]::new($false))
                        Write-OK "optimization.cfg written: $optPath  ($($effectiveAutoexec.Count) CVars)"
                    } else {
                        Write-Host "  [DRY-RUN] Would write: $optPath  ($($effectiveAutoexec.Count) CVars)" -ForegroundColor Magenta
                    }

                    # Append 'exec optimization.cfg' to autoexec.cfg - the only touch to the user's file
                    $execLine   = "exec optimization.cfg"
                    $alreadyHas = $existingLines | Where-Object { $_ -imatch '^\s*exec\s+optimization\.cfg\s*($|//)' }
                    if (-not $alreadyHas) {
                        if ($existingLines.Count -gt 0) {
                            if ($existingLines[-1].Trim() -ne "") { $existingLines += "" }
                            $existingLines += $execLine
                            if (-not $SCRIPT:DryRun) {
                                [System.IO.File]::WriteAllLines($autoexecPath, $existingLines, [System.Text.UTF8Encoding]::new($false))
                                Write-OK "autoexec.cfg: appended '$execLine'  (only change to your file)"
                            } else {
                                Write-Host "  [DRY-RUN] Would append '$execLine' to autoexec.cfg" -ForegroundColor Magenta
                            }
                        } else {
                            # No existing autoexec - create a minimal stub
                            if (-not $SCRIPT:DryRun) {
                                $stubLines = @("// Your CS2 autoexec - add personal CVars above the exec line.", "", $execLine)
                                [System.IO.File]::WriteAllLines($autoexecPath, $stubLines, [System.Text.UTF8Encoding]::new($false))
                                Write-OK "autoexec.cfg created (stub with exec line - add your own CVars above it)."
                            } else {
                                Write-Host "  [DRY-RUN] Would create autoexec.cfg with exec stub" -ForegroundColor Magenta
                            }
                        }
                    } else {
                        Write-OK "autoexec.cfg already has '$execLine' - no change to your file."
                    }

                    Write-Blank
                    Write-Info "Your autoexec.cfg is untouched except for the exec line at the end."
                    Write-Info "To revert all:       remove '$execLine' from autoexec.cfg."
                    Write-Info "To revert one CVar:  remove its line from optimization.cfg."

                    # ── Deploy optional standalone CFGs ───────────────────────────────────────
                    # These are standalone cfgs for network conditions, diagnostics, and
                    # audio-buffer experiments.
                    # They are NOT exec'd automatically - user calls them from console as needed.
                    $optionalCfgs = @(
                        "net_stable.cfg",
                        "net_highping.cfg",
                        "net_unstable.cfg",
                        "net_bad.cfg",
                        "debug_hud.cfg",
                        "debug_hud_off.cfg",
                        "audio_stable.cfg",
                        "audio_lowlatency_025.cfg",
                        "audio_lowlatency_001.cfg"
                    )
                    $cfgSourceDir = "$ScriptRoot\cfgs"
                    $optionalCfgsDeployed = 0
                    if (Test-Path $cfgSourceDir) {
                        foreach ($cfgFile in $optionalCfgs) {
                            $src  = "$cfgSourceDir\$cfgFile"
                            $dest = "$cfgDir\$cfgFile"
                            if (Test-Path $src) {
                                if (-not $SCRIPT:DryRun) {
                                    Copy-Item $src $dest -Force
                                    $optionalCfgsDeployed++
                                } else {
                                    Write-Host "  [DRY-RUN] Would copy: $cfgFile -> $cfgDir\" -ForegroundColor Magenta
                                }
                            } else {
                                Write-DebugLog "Optional CFG not found at source: $src"
                            }
                        }
                        if ($optionalCfgsDeployed -gt 0) {
                            Write-OK "$optionalCfgsDeployed optional CFGs deployed to game\csgo\cfg\"
                        }
                    } else {
                        Write-DebugLog "cfgs\ directory not found at: $cfgSourceDir - optional CFGs not deployed."
                    }

                    # ── Optional CFG usage guide ─────────────────────────────────────────────
                    if ($optionalCfgsDeployed -gt 0 -or ($SCRIPT:DryRun -and (Test-Path $cfgSourceDir))) {
                        Write-Blank
                        Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Cyan
                        Write-Host "  │  OPTIONAL CFGs (console commands, use manually as needed)   │" -ForegroundColor Cyan
                        Write-Host "  │                                                              │" -ForegroundColor Cyan
                        Write-Host "  │  exec net_stable     Baseline - stable wired/fiber          │" -ForegroundColor Green
                        Write-Host "  │  exec net_highping   60ms+ ping, stable route               │" -ForegroundColor Yellow
                        Write-Host "  │  exec net_unstable   Jitter + loss, ping OK (Wi-Fi/4G)      │" -ForegroundColor Yellow
                        Write-Host "  │  exec net_bad        High ping + jitter/loss (satellite/    │" -ForegroundColor Red
                        Write-Host "  │                      mobile roaming / hotel Wi-Fi)           │" -ForegroundColor Red
                        Write-Host "  │  exec debug_hud      Show telemetry + print net diagnostics │" -ForegroundColor Yellow
                        Write-Host "  │  exec debug_hud_off  Reset telemetry to quiet defaults      │" -ForegroundColor Green
                        Write-Host "  │  exec audio_stable   Reset to suite audio default           │" -ForegroundColor Green
                        Write-Host "  │  exec audio_lowlatency_025  Experimental lower audio buffer │" -ForegroundColor Yellow
                        Write-Host "  │  exec audio_lowlatency_001  Aggressive audio experiment     │" -ForegroundColor Red
                        Write-Host "  │                                                              │" -ForegroundColor Cyan
                        Write-Host "  │  Each cfg prints a confirmation line when loaded.           │" -ForegroundColor DarkGray
                        Write-Host "  │  Reset with 'exec net_stable' on a stable connection.       │" -ForegroundColor DarkGray
                        Write-Host "  │  Audio experiments reset with 'exec audio_stable'.          │" -ForegroundColor DarkGray
                        Write-Host "  │  These are not auto-executed.                               │" -ForegroundColor DarkGray
                        Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Cyan
                    }
                } else {
                    Write-Info "Skipped - run this step again to generate optimization.cfg."
                }
            }

            # ── Steam Cloud sync warning ──────────────────────────────────────
            # Steam Cloud behavior can affect synchronized configuration files.
            # The repository does not assume that every account syncs this path.
            Write-Blank
            $cloudLooksEnabled = $false
            try {
                $steamPath = Get-SteamPath
                if ($steamPath -and (Test-Path "$steamPath\userdata")) {
                    # localconfig.vdf is a per-user Valve Data Format file storing
                    # per-app Steam settings. CS2 App ID = 730.
                    $lcPath = Get-ChildItem "$steamPath\userdata\*\config\localconfig.vdf" `
                        -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1 -ExpandProperty FullName
                    if ($lcPath -and (Test-Path $lcPath)) {
                        $lc = Get-Content $lcPath -Raw -Encoding UTF8 -ErrorAction SilentlyContinue
                        if ([string]::IsNullOrWhiteSpace($lc)) { $lc = $null }
                    }
                    if ($lc) {
                        # VDF is nested text with recursive braces. A flat regex like
                        # [^}]* fails on nested sub-objects. Instead, find the "730" key
                        # and then manually walk braces to extract its block.
                        # Find "730" as a VDF key (preceded by whitespace), not as a value
                        $idx730 = -1
                        $searchFrom = 0
                        while ($searchFrom -lt $lc.Length) {
                            $candidate = $lc.IndexOf('"730"', $searchFrom)
                            if ($candidate -lt 0) { break }
                            # VDF keys are preceded by whitespace/newline and followed by whitespace then '{';
                            # values are also preceded by whitespace, so also check what follows
                            $before = if ($candidate -gt 0) { $lc[$candidate - 1] } else { "`n" }
                            if ($before -match '[\s\n\r{]') {
                                # Verify next non-whitespace after "730" is '{' (key), not '"' (value)
                                $afterToken = $lc.Substring($candidate + 5).TrimStart()
                                if ($afterToken.Length -gt 0 -and $afterToken[0] -eq '{') {
                                    $idx730 = $candidate; break
                                }
                            }
                            $searchFrom = $candidate + 5
                        }
                        if ($idx730 -ge 0) {
                            $braceStart = $lc.IndexOf('{', $idx730)
                            if ($braceStart -ge 0) {
                                $depth = 0
                                $braceEnd = -1
                                for ($ci = $braceStart; $ci -lt $lc.Length; $ci++) {
                                    if ($lc[$ci] -eq '{') { $depth++ }
                                    elseif ($lc[$ci] -eq '}') { $depth--; if ($depth -eq 0) { $braceEnd = $ci; break } }
                                }
                                if ($braceEnd -gt $braceStart) {
                                    $block730 = $lc.Substring($braceStart, $braceEnd - $braceStart + 1)
                                    if ($block730 -match '"cloud_enabled"\s+"1"') {
                                        $cloudLooksEnabled = $true
                                    }
                                }
                            }
                        }
                    }
                }
            } catch { Write-DebugLog "Steam Cloud check failed: $_" }

            $cloudColor = if ($cloudLooksEnabled) { "Red" } else { "Yellow" }
            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor $cloudColor
            Write-Host "  │  STEAM CLOUD - ACTION REQUIRED                               │" -ForegroundColor $cloudColor
            Write-Host "  │                                                              │" -ForegroundColor $cloudColor
            if ($cloudLooksEnabled) {
                Write-Host "  │  ⚠  Steam Cloud sync for CS2 is ENABLED on this account.    │" -ForegroundColor Red
                Write-Host "  │  Cloud sync may replace local configuration content.       │" -ForegroundColor Red
            } else {
                Write-Host "  │  Steam Cloud sync may overwrite autoexec.cfg on next Steam   │" -ForegroundColor White
                Write-Host "  │  launch, replacing the autoexec bootstrap written above.    │" -ForegroundColor White
            }
            Write-Host "  │                                                              │" -ForegroundColor $cloudColor
            Write-Host "  │  Disable Cloud sync for CS2 game files only:                │" -ForegroundColor White
            Write-Host "  │  Steam Library -> CS2 right-click -> Properties              │" -ForegroundColor DarkGray
            Write-Host "  │  -> General -> uncheck 'Keep saves in the Steam Cloud'       │" -ForegroundColor DarkGray
            Write-Host "  │  (Only disables cloud for CS2 - other games stay synced)    │" -ForegroundColor DarkGray
            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor $cloudColor
            if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] after disabling Cloud sync for CS2" }

            # ── Launch Options Guide ──────────────────────────────────────
            Write-Blank
            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Cyan
            Write-Host "  │  CS2 LAUNCH OPTIONS (Steam -> CS2 -> Properties)            │" -ForegroundColor Cyan
            Write-Host "  │                                                              │" -ForegroundColor Cyan
            Write-Host "  │  REPOSITORY EXAMPLE (verify with the current client):       │" -ForegroundColor White
            Write-Host "  │  -console        Requests the developer console at startup  │" -ForegroundColor Green
            Write-Host "  │  +exec autoexec  Requests execution of autoexec.cfg         │" -ForegroundColor Green
            Write-Host "  │  OPTIONAL (conditional):                                    │" -ForegroundColor White
            Write-Host "  │  -fullscreen     Request exclusive fullscreen; compare     │" -ForegroundColor DarkGray
            Write-Host "  │                  presentation modes on the local system    │" -ForegroundColor DarkGray
            Write-Host "  │  -language english  Requests English text                  │" -ForegroundColor DarkGray
            Write-Host "  │                                                              │" -ForegroundColor Cyan
            Write-Host "  │  Minimal example string:                                    │" -ForegroundColor DarkGray
            Write-Host "  │  -console +exec autoexec                                    │" -ForegroundColor Green
            Write-Host "  │                                                              │" -ForegroundColor Cyan
            Write-Host "  │  NOT INCLUDED BY THIS REPOSITORY:                           │" -ForegroundColor Yellow
            Write-Host "  │  -novid         Legacy option; omitted from this example    │" -ForegroundColor Red
            Write-Host "  │  -threads N     Manual thread override; omitted             │" -ForegroundColor Red
            Write-Host "  │  -tickrate 128  Not used for CS2 matchmaking configuration │" -ForegroundColor Red
            Write-Host "  │  -nojoy         Can affect controller input; omitted        │" -ForegroundColor Red
            Write-Host "  │  -refresh Hz    Use Windows and in-game display settings    │" -ForegroundColor Red
            Write-Host "  │  -softparticlesdefaultoff  Source 1-era; omitted           │" -ForegroundColor Red
            Write-Host "  │  +cl_forcepreload 1  Current behavior is not validated     │" -ForegroundColor Red
            Write-Host "  │  +mat_queue_mode 2   Current behavior is not validated     │" -ForegroundColor Red
            Write-Host "  │  -vulkan        Windows compatibility is not validated      │" -ForegroundColor Red
            Write-Host "  │  -dxlevel N     Source 1-era option; omitted                │" -ForegroundColor Red
            Write-Host "  │  -high          Omitted; Phase 3 offers separate IFEO      │" -ForegroundColor Red
            Write-Host "  │                 priority policy with its own tradeoffs      │" -ForegroundColor Red
            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Cyan
            Write-Blank
            "-console +exec autoexec" | Set-ClipboardSafe
            Write-OK "Launch options string copied to clipboard: -console +exec autoexec"
            Write-Info "Set refresh rate in Windows Advanced display settings and CS2 video settings, not launch options."
            if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] when launch options are set in Steam" }

            Complete-Step $PHASE 34 "Autoexec"
        } `
        -SkipAction { Skip-Step $PHASE 34 "Autoexec" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 35 - CHIPSET DRIVER CHECK  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 35) {
    Write-Section "Step 35 - Chipset Driver Check"
    $null = Invoke-TieredStep -Tier 2 -Title "Check chipset driver and show update link" `
        -Why "Reports the detected chipset-driver information and opens the vendor download page for manual review." `
        -Evidence "This is a read-only inventory and manual link step. The repository includes no driver-version performance matrix." `
        -Caveat "A chipset update can change device behavior and may require a restart. Install only a package intended for the detected platform." `
        -Risk "SAFE" -Depth "CHECK" `
        -Improvement "Provides chipset-driver inventory and the matching vendor download page" `
        -SideEffects "No automatic driver change; a manually installed package can change device behavior" `
        -Undo "N/A (manual download)" `
        -Action {
            $vendor = Get-ChipsetVendor
            Write-Info "CPU manufacturer: $vendor"

            try {
                $chipsetDrv = Get-CimInstance Win32_PnPSignedDriver |
                    Where-Object { $_.DeviceClass -eq "SYSTEM" -and $_.DeviceName -match "Chipset|SMBus|PCI" } |
                    Select-Object -First 1
                if ($chipsetDrv) {
                    Write-Info "Chipset driver: $($chipsetDrv.DeviceName)"
                    Write-Info "Version:        $($chipsetDrv.DriverVersion)"
                    Write-Info "Date:           $($chipsetDrv.DriverDate)"
                }
            } catch { Write-DebugLog "Chipset driver info not readable." }

            $url = switch ($vendor) {
                "AMD"   { $CFG_URL_AMD_Chipset }
                "Intel" { $CFG_URL_Intel_Chipset }
                default { $null }
            }

            if ($url) {
                Write-Blank
                Write-Info "Download: $url"
                $url | Set-ClipboardSafe
                Write-OK "URL copied to clipboard."
                if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) {
                    $r = Read-Host "  Open in browser? [y/N]"
                    if ($r -match "^[jJyY]$") {
                        if ($url -match '^https://(www\.)?(amd|intel)\.com/') {
                            Start-Process $url
                        } else {
                            Write-Warn "URL validation failed - open manually: $url"
                        }
                    }
                }
            } else {
                Write-Warn "CPU manufacturer not recognized - check manually."
            }
            Complete-Step $PHASE 35 "ChipsetDriver"
        } `
        -SkipAction { Skip-Step $PHASE 35 "ChipsetDriver" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 36 - VISUAL EFFECTS BEST PERFORMANCE  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 36) {
    Write-Section "Step 36 - Visual Effects + Auto HDR"
    $null = Invoke-TieredStep -Tier 3 -Title "Visual effects 'Best Performance' + Win11 Auto HDR off" `
        -Why "Changes the Windows visual-effects policy and disables Auto HDR for the current user." `
        -Evidence "Windows exposes separate controls for visual effects and Auto HDR. This repository includes no isolated performance benchmark for the combined step." `
        -Caveat "The desktop appearance changes. Auto HDR applies only on supported Windows and display configurations." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Applies the three documented Windows policy changes" `
        -SideEffects "Window animations and other visual effects change. Auto HDR is disabled for the current user." `
        -Undo "Restore VisualFXSetting, UserPreferencesMask, FontSmoothing, and AutoHDREnabled from backup" `
        -Action {
            # ── Visual Effects ─────────────────────────────────────────────────
            # VisualFXSetting=2 tells the System Properties dialog "Best Performance" is selected.
            # UserPreferencesMask is the actual bitmap controlling individual visual effects.
            # Setting both ensures immediate effect without requiring a separate logon cycle.
            Set-RegistryValue "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects" `
                "VisualFXSetting" 2 "DWord" "Best Performance"
            # UserPreferencesMask: 90 12 03 80 10 00 00 00 = "Best Performance" with font smoothing ON
            # Byte 2 = 0x03 (instead of 0x01) preserves ClearType/font smoothing - text remains readable.
            # Without this, "Best Performance" disables ClearType, making text aliased and hard to read.
            $upmPath = "HKCU:\Control Panel\Desktop"
            Set-RegistryValue $upmPath "UserPreferencesMask" ([byte[]](0x90,0x12,0x03,0x80,0x10,0x00,0x00,0x00)) "Binary" "Best Performance + ClearType preserved"
            # FontSmoothing=2 is the string value that enables ClearType at the GDI level
            Set-RegistryValue $upmPath "FontSmoothing" "2" "String" "ClearType font smoothing enabled"
            Write-ActionOK "Visual effects: Best Performance (ClearType font smoothing preserved)."

            # ── Win11 Auto HDR disable ─────────────────────────────────────────
            # Auto HDR applies tone-mapping post-processing to SDR games in Win11.
            # Changes tone mapping and image presentation. The repository does
            # not contain an image-quality or frame-time comparison for CS2.
            # Only relevant on Windows 11 with an HDR-capable display. Other systems may ignore it.
            $videoSettings = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\VideoSettings"
            Set-RegistryValue $videoSettings "AutoHDREnabled" 0 "DWord" "Win11 Auto HDR disabled"
            Write-ActionOK "Win11 Auto HDR policy: disabled for CS2."

            Write-Info "Undo: restore the Step 36 registry values through Recovery."
            Complete-Step $PHASE 36 "VisualEffects"
        } `
        -SkipAction { Skip-Step $PHASE 36 "VisualEffects" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 37 - SYSMAIN + WINDOWS SEARCH DISABLE  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 37) {
    Write-Section "Step 37 - Disable SysMain + Windows Search"
    $null = Invoke-TieredStep -Tier 3 -Title "Disable SysMain, Windows Search, QWAVE + Xbox services" `
        -Why "Disables the selected prefetch, indexing, multimedia-networking, and Xbox services." `
        -Evidence "These Windows services provide prefetch, indexing, multimedia networking, Xbox authentication, synchronization, or accessory support. The repository includes no isolated performance benchmark for disabling them." `
        -Caveat "Search and application launch behavior can regress. qWave-dependent applications lose related QoS behavior. Xbox authentication, cloud saves, multiplayer, and accessories can stop working. Do not apply this group when those features are required." `
        -Risk "MODERATE" -Depth "SERVICE" `
        -Improvement "Stops the selected services until they are restored" `
        -SideEffects "Search, prefetch, multimedia QoS, Xbox Live, cloud-save, multiplayer, and Xbox accessory behavior can change." `
        -Undo "Use Recovery to restore each service's captured startup type and running state" `
        -Action {
            $serviceFailures = [System.Collections.Generic.List[string]]::new()
            if ($SCRIPT:DryRun) {
                $svcList = @("SysMain", "WSearch", "qWave") + $CFG_XboxServices
                Write-Host "  [DRY-RUN] Would disable: $($svcList -join ', ')" -ForegroundColor Magenta
            } else {
                # Capture and persist every present service before changing any
                # of them. A failed query or backup write blocks the whole group.
                $serviceStepTitle = "Disable SysMain + Search + QWAVE + Xbox"
                $serviceNames = @("SysMain", "WSearch", "qWave") + $CFG_XboxServices
                $presentServices = [System.Collections.Generic.List[string]]::new()
                foreach ($serviceName in $serviceNames) {
                    $service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
                    if (-not $service) { continue }
                    $presentServices.Add($serviceName) | Out-Null
                    $capture = Backup-ServiceState -ServiceName $serviceName `
                        -StepTitle $serviceStepTitle -PassThru
                    if (-not $capture -or -not $capture.Captured) {
                        $detail = if ($capture -and $capture.Message) { $capture.Message } else { 'No capture result was returned.' }
                        throw "Service changes blocked because '$serviceName' was not captured: $detail"
                    }
                }
                Flush-BackupBuffer
                $durableBackup = Get-BackupDataRaw
                foreach ($serviceName in $presentServices) {
                    $capturePersisted = @($durableBackup.entries | Where-Object {
                        $_.type -eq 'service' -and $_.step -eq $serviceStepTitle -and $_.name -eq $serviceName
                    }).Count -gt 0
                    if (-not $capturePersisted) {
                        throw "Service changes blocked because backup.json has no restore record for '$serviceName'."
                    }
                }
                # ── SysMain + WSearch (original) ─────────────────────────────────────
                try {
                    $smSvc = Get-Service "SysMain" -ErrorAction SilentlyContinue
                    if ($smSvc) {
                        Set-Service SysMain -StartupType Disabled -ErrorAction Stop
                        Stop-Service SysMain -Force -ErrorAction Stop
                        Write-OK "SysMain (Superfetch) disabled."
                    } else {
                        Write-Sub "SysMain: not present on this system (skipped)."
                    }
                } catch {
                    $serviceFailures.Add("SysMain")
                    Write-Warn "Could not disable SysMain: $_"
                }
                try {
                    $wsSvc = Get-Service "WSearch" -ErrorAction SilentlyContinue
                    if ($wsSvc) {
                        Set-Service WSearch -StartupType Disabled -ErrorAction Stop
                        Stop-Service WSearch -Force -ErrorAction Stop
                        Write-OK "Windows Search disabled."
                    } else {
                        Write-Sub "WSearch: not present on this system (skipped)."
                    }
                } catch {
                    $serviceFailures.Add("WSearch")
                    Write-Warn "Could not disable Windows Search: $_"
                }

                # ── qWave - Quality Windows Audio/Video Experience ───────────────────
                # qWave exposes multimedia QoS APIs. The separate CS2 policy in
                # Step 16 does not prove that other applications do not need it.
                try {
                    $qw = Get-Service "qWave" -ErrorAction SilentlyContinue
                    if ($qw) {
                        Set-Service qWave -StartupType Disabled -ErrorAction Stop
                        Stop-Service qWave -Force -ErrorAction Stop
                        Write-OK "qWave multimedia QoS service disabled."
                    } else {
                        Write-Sub "qWave: not present on this system (skipped)."
                    }
                } catch {
                    $serviceFailures.Add("qWave")
                    Write-Warn "Could not disable qWave: $_"
                }

                # ── Xbox services ────────────────────────────────────────────────────
                # Background network activity for Xbox Live auth, game save sync, networking.
                # NOTE: XboxGipSvc controls Xbox wireless accessories (controllers, headsets).
                # If you use Xbox wireless peripherals, skip this or re-enable XboxGipSvc.
                Write-Info "Xbox services: disabling background auth/sync/networking."
                Write-Host "  NOTE: Re-enable XboxGipSvc if you use Xbox wireless controller/headset." -ForegroundColor DarkYellow
                foreach ($svcName in $CFG_XboxServices) {
                    try {
                        $svc = Get-Service $svcName -ErrorAction SilentlyContinue
                        if ($svc) {
                            Set-Service $svcName -StartupType Disabled -ErrorAction Stop
                            Stop-Service $svcName -Force -ErrorAction Stop
                            Write-OK "$svcName disabled."
                        } else {
                            Write-Sub "${svcName}: not present on this system (skipped)."
                        }
                    } catch {
                        $serviceFailures.Add($svcName)
                        Write-Warn "Could not disable ${svcName}: $_"
                    }
                }
            }

            if ($serviceFailures.Count -gt 0) {
                throw "Required service changes failed: $($serviceFailures -join ', '). Step 37 was not completed."
            }
            Write-Info "Undo: use Recovery to restore captured service state."
            Complete-Step $PHASE 37 "SysMainSearch"
        } `
        -SkipAction { Skip-Step $PHASE 37 "SysMainSearch" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 38 - PREPARE SAFE MODE
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 38) {
    Write-Section "Step 38 - Activate Safe Mode + Register Phase 2"
    Write-TierBadge 1 "Safe Mode for GPU driver clean removal"
    Write-Info "GPU driver clean removal runs in Safe Mode - driver files are unlocked there."

    if ($SCRIPT:DryRun) {
        $phase2Transaction = Enable-Phase2SafeModeTransaction -SourceRoot $ScriptRoot -DestinationRoot $CFG_WorkDir -StatePath $CFG_StateFile -Why "Phase 1 Step 38 - Safe Mode for GPU driver clean"
        Write-Info $phase2Transaction.Message
        Write-Host "  [DRY-RUN] Would restart into Safe Mode only after all handoff verifications succeeded." -ForegroundColor Magenta
        Complete-Step $PHASE 38 "SafeMode (DRY-RUN preview)"
    } else {
        $phase2Transaction = Enable-Phase2SafeModeTransaction -SourceRoot $ScriptRoot -DestinationRoot $CFG_WorkDir -StatePath $CFG_StateFile -Why "Phase 1 Step 38 - Safe Mode for GPU driver clean"
        if ($phase2Transaction.Applied) {
            $SCRIPT:SafebootReady = $true
            Write-Blank
            Write-Host "  ╔══════════════════════════════════════════════════════════╗" -ForegroundColor Yellow
            Write-Host "  ║  NOTICE: Safe Mode boot flag has been set.              ║" -ForegroundColor Yellow
            Write-Host "  ║  The NEXT reboot will boot into Safe Mode.              ║" -ForegroundColor Yellow
            Write-Host "  ╚══════════════════════════════════════════════════════════╝" -ForegroundColor Yellow
            Complete-Step $PHASE 38 "SafeMode"
        } else {
            $SCRIPT:SafebootReady = $false
            Write-Blank
            Write-Host "  ╔══════════════════════════════════════════════════════════╗" -ForegroundColor Red
            Write-Host "  ║  ERROR: Safe Mode boot flag could NOT be set.           ║" -ForegroundColor Red
            Write-Host "  ║                                                         ║" -ForegroundColor Red
            Write-Host "  ║  The system will boot into Normal Mode on restart.      ║" -ForegroundColor Red
            Write-Host "  ║  Try manually from an elevated cmd.exe:                 ║" -ForegroundColor Red
            Write-Host "  ║    bcdedit /set {current} safeboot minimal              ║" -ForegroundColor Red
            Write-Host "  ║  Then restart to enter Safe Mode for Phase 2.           ║" -ForegroundColor Red
            Write-Host "  ╚══════════════════════════════════════════════════════════╝" -ForegroundColor Red
            Write-Err "$($phase2Transaction.Message) Step 38 is NOT marked complete (will retry on next run)."
        }
    }
}
