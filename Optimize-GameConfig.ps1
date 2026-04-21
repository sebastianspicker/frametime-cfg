# ==============================================================================
#  Optimize-GameConfig.ps1  —  Steps 34-38: Autoexec, Chipset, Visual Effects,
#                                Services, Safe Mode Preparation
# ==============================================================================

# ══════════════════════════════════════════════════════════════════════════════
# STEP 34 — AUTOEXEC.CFG GENERATOR  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 34) {
    Write-Section "Step 34 — Autoexec.cfg Generator"
    Invoke-TieredStep -Tier 2 -Title "Create/update CS2 autoexec.cfg + launch options guide" -EstimateKey "Autoexec CVars" `
        -Why "73 CVars across 8 categories. Network: rate 1000000, cl_net_buffer_ticks 0, mm_dedicated_search_maxping 80. Engine: fps_max 0 and engine_low_latency_sleep_after_client_tick 1 as the suite's low-latency baseline. Gameplay: cl_predict_body/head_shot_fx 0, cl_sniper_delay_unscope 0. HUD: 5x cl_hud_telemetry CVars=0 (replaces removed net_graph). Audio: speaker_config 1, snd_mixahead 0.05 as a stability-oriented suite default, snd_headphone_eq 0 (Natural — current pro preference trend), snd_spatialize_lerp 0. 8x music muting CVars, snd_voipvolume 0.5, voice_always_sample_mic 1. Mouse: m_rawinput 1 kept as a harmless documentation/forward-compatibility stub because CS2 already forces raw input; Step 29 remains the real Windows-side acceleration change. m_mouseaccel1/2/customaccel 0. Video: r_player_visibility_mode 1 as a suite default, mat_monitorgamma_tv_enabled 0. Intel hybrid: thread_pool_option 2 (auto-detected)." `
        -Evidence "ArminC-AutoExec (2025): rate 1000000 max (786432 = UI bug). Current CS2 convar surface: engine_low_latency_sleep_after_client_tick exists and references r_low_latency; the repo no longer treats fps_max 0 as its activation condition. cl_predict_body/head_shot_fx: CS2-native Valve CVars, default 0. snd_mixahead 0.05: community-preferred stability setting; current public convar data shows a much lower engine default. speaker_config 1 and snd_spatialize_lerp 0 remain part of the suite's headphone-focused spatial baseline, but the repo no longer claims a separate snd_use_hrtf toggle exists in the current public convar surface. snd_headphone_eq 0 (Natural): esportfire.com 30+ pro study, 2026 — 62.5% Natural vs 37.5% Crisp. r_player_visibility_mode 1 exists in the current convar surface. cl_hud_telemetry_* CVars: CS2 replacement for removed net_graph. m_rawinput 1 is retained only as a no-op documentation stub in current CS2 builds, while m_mouseaccel1/2/customaccel remain the active CS2-side acceleration guards. cl_autowepswitch 0: common competitive default." `
        -Caveat "net_client_steamdatagram_enable_override 1 forces Valve SDR — disable if your direct routing is already clean. snd_headphone_eq 0 (Natural) vs 1 (Crisp): Crisp boosts highs but can cause ear fatigue; change to 1 in config.env.ps1 if preferred. snd_spatialize_lerp 0 is a suite default for the current headphone-focused spatial path, not a Valve-documented competitive requirement. r_fullscreen_gamma 2.2 only applies in exclusive fullscreen. snd_tensecondwarning_volume kept at 0.1 (tactical cue). m_rawinput 1 is kept for documentation clarity and future compatibility, not because current CS2 needs it to enable raw input." `
        -Risk "SAFE" -Depth "APP" `
        -Improvement "73 CS2 CVars: network + hit prediction + CS2-side mouse acceleration guards + audio spatial defaults + music muting + video + HUD cleanup + gameplay (autowepswitch)" `
        -SideEffects "autoexec.cfg untouched except one appended line: 'exec optimization.cfg'. optimization.cfg overrides any same-named CVars appearing earlier in autoexec.cfg. snd_headphone_eq 0 changes EQ from Crisp to Natural. cl_autowepswitch 0 disables auto weapon pickup switch. Intel only: thread_pool_option 2 limits CS2 to P-cores. m_rawinput 1 remains in the generated file as a harmless no-op/documentation stub in current CS2 builds." `
        -Undo "Remove 'exec optimization.cfg' from autoexec.cfg, or delete optimization.cfg from game\csgo\cfg\" `
        -Action {
            # Build effective autoexec: start with config defaults, add hardware-specific CVars
            $effectiveAutoexec = [ordered]@{}
            foreach ($kv in $CFG_CS2_Autoexec.GetEnumerator()) { $effectiveAutoexec[$kv.Key] = $kv.Value }

            # Intel 12th gen+ hybrid: prefer P-cores for CS2 thread pool
            $intelHybridName = Get-IntelHybridCpuName
            if ($intelHybridName) {
                $effectiveAutoexec["thread_pool_option"] = "2"
                Write-Info "Intel hybrid CPU ($intelHybridName) — adding thread_pool_option 2 (prefer P-cores)"
            }

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
                Ensure-Dir $cfgDir

                # ── Read existing autoexec.cfg (read-only — we only append one line later) ──
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
                    Write-Info "No autoexec.cfg found — will create a minimal one with exec line."
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
                    Write-Host "  CONFLICTS — optimization.cfg runs after autoexec.cfg, so these" -ForegroundColor Yellow
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

                    # Write optimization.cfg — full clean write each run
                    $optLines = @(
                        "// CS2-Optimize Suite — optimization.cfg",
                        "// Generated: $(Get-Date -Format 'yyyy-MM-dd HH:mm')",
                        "// exec'd from the end of autoexec.cfg — overrides earlier same-named CVars.",
                        "// To revert one setting: remove or comment its line here.",
                        "// To revert all:         remove 'exec optimization.cfg' from autoexec.cfg.",
                        "//",
                        "// Network condition CFGs (also in game\csgo\cfg\, use from console as needed):",
                        "//   exec net_stable     — optimal / reset (stable wired/fiber)",
                        "//   exec net_highping   — 60ms+ ping, stable route",
                        "//   exec net_unstable   — jitter + loss, ping OK (Wi-Fi / 4G)",
                        "//   exec net_bad        — high ping + jitter/loss (satellite / mobile)",
                        ""
                    )
                    foreach ($kv in $effectiveAutoexec.GetEnumerator()) {
                        $optLines += "$($kv.Key) $($kv.Value)"
                    }
                    if (-not $SCRIPT:DryRun) {
                        # Back up existing optimization.cfg before overwriting (only if no backup exists yet)
                        if ((Test-Path $optPath) -and -not (Test-Path "$optPath.bak")) { Copy-Item $optPath "$optPath.bak" -Force }
                        # Use BOM-less UTF-8 — PS 5.1's -Encoding UTF8 adds a BOM (EF BB BF)
                        # which can corrupt the first line if Source 2 doesn't skip it.
                        [System.IO.File]::WriteAllLines($optPath, $optLines, [System.Text.UTF8Encoding]::new($false))
                        Write-OK "optimization.cfg written: $optPath  ($($effectiveAutoexec.Count) CVars)"
                    } else {
                        Write-Host "  [DRY-RUN] Would write: $optPath  ($($effectiveAutoexec.Count) CVars)" -ForegroundColor Magenta
                    }

                    # Append 'exec optimization.cfg' to autoexec.cfg — the only touch to the user's file
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
                            # No existing autoexec — create a minimal stub
                            if (-not $SCRIPT:DryRun) {
                                $stubLines = @("// Your CS2 autoexec — add personal CVars above the exec line.", "", $execLine)
                                [System.IO.File]::WriteAllLines($autoexecPath, $stubLines, [System.Text.UTF8Encoding]::new($false))
                                Write-OK "autoexec.cfg created (stub with exec line — add your own CVars above it)."
                            } else {
                                Write-Host "  [DRY-RUN] Would create autoexec.cfg with exec stub" -ForegroundColor Magenta
                            }
                        }
                    } else {
                        Write-OK "autoexec.cfg already has '$execLine' — no change to your file."
                    }

                    Write-Blank
                    Write-Info "Your autoexec.cfg is untouched except for the exec line at the end."
                    Write-Info "To revert all:       remove '$execLine' from autoexec.cfg."
                    Write-Info "To revert one CVar:  remove its line from optimization.cfg."

                    # ── Deploy network condition CFGs ──────────────────────────────────────────
                    # These are standalone cfgs for bad-connection scenarios.
                    # They are NOT exec'd automatically — user calls them from console as needed.
                    $netCfgs = @("net_stable.cfg", "net_highping.cfg", "net_unstable.cfg", "net_bad.cfg")
                    $cfgSourceDir = "$ScriptRoot\cfgs"
                    $netCfgsDeployed = 0
                    if (Test-Path $cfgSourceDir) {
                        foreach ($cfgFile in $netCfgs) {
                            $src  = "$cfgSourceDir\$cfgFile"
                            $dest = "$cfgDir\$cfgFile"
                            if (Test-Path $src) {
                                if (-not $SCRIPT:DryRun) {
                                    Copy-Item $src $dest -Force
                                    $netCfgsDeployed++
                                } else {
                                    Write-Host "  [DRY-RUN] Would copy: $cfgFile -> $cfgDir\" -ForegroundColor Magenta
                                }
                            } else {
                                Write-DebugLog "Network CFG not found at source: $src"
                            }
                        }
                        if ($netCfgsDeployed -gt 0) {
                            Write-OK "$netCfgsDeployed network condition CFGs deployed to game\csgo\cfg\"
                        }
                    } else {
                        Write-DebugLog "cfgs\ directory not found at: $cfgSourceDir — network CFGs not deployed."
                    }

                    # ── Network CFG usage guide ──────────────────────────────────────────────
                    if ($netCfgsDeployed -gt 0 -or ($SCRIPT:DryRun -and (Test-Path $cfgSourceDir))) {
                        Write-Blank
                        Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Cyan
                        Write-Host "  │  NETWORK CONDITION CFGs (console commands, use as needed)   │" -ForegroundColor Cyan
                        Write-Host "  │                                                              │" -ForegroundColor Cyan
                        Write-Host "  │  exec net_stable     Optimal — stable wired/fiber           │" -ForegroundColor Green
                        Write-Host "  │  exec net_highping   60ms+ ping, stable route               │" -ForegroundColor Yellow
                        Write-Host "  │  exec net_unstable   Jitter + loss, ping OK (Wi-Fi/4G)      │" -ForegroundColor Yellow
                        Write-Host "  │  exec net_bad        High ping + jitter/loss (satellite/    │" -ForegroundColor Red
                        Write-Host "  │                      mobile roaming / hotel Wi-Fi)           │" -ForegroundColor Red
                        Write-Host "  │                                                              │" -ForegroundColor Cyan
                        Write-Host "  │  Each cfg prints a confirmation line when loaded.           │" -ForegroundColor DarkGray
                        Write-Host "  │  Reset with 'exec net_stable' on a stable connection.       │" -ForegroundColor DarkGray
                        Write-Host "  │  These do NOT override optimization.cfg — they're additive. │" -ForegroundColor DarkGray
                        Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Cyan
                    }
                } else {
                    Write-Info "Skipped — run this step again to generate optimization.cfg."
                }
            }

            # ── Steam Cloud sync warning ──────────────────────────────────────
            # Steam Cloud can silently overwrite autoexec.cfg on next Steam launch,
            # restoring the old file and reverting all changes written above.
            # This is the most common cause of "settings didn't stick" reports.
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
            Write-Host "  │  STEAM CLOUD — ACTION REQUIRED                               │" -ForegroundColor $cloudColor
            Write-Host "  │                                                              │" -ForegroundColor $cloudColor
            if ($cloudLooksEnabled) {
                Write-Host "  │  ⚠  Steam Cloud sync for CS2 is ENABLED on this account.    │" -ForegroundColor Red
                Write-Host "  │  It WILL overwrite your autoexec.cfg on next Steam launch.  │" -ForegroundColor Red
            } else {
                Write-Host "  │  Steam Cloud sync may overwrite autoexec.cfg on next Steam   │" -ForegroundColor White
                Write-Host "  │  launch, reverting all changes written above.               │" -ForegroundColor White
            }
            Write-Host "  │                                                              │" -ForegroundColor $cloudColor
            Write-Host "  │  Disable Cloud sync for CS2 game files only:                │" -ForegroundColor White
            Write-Host "  │  Steam Library -> CS2 right-click -> Properties              │" -ForegroundColor DarkGray
            Write-Host "  │  -> General -> uncheck 'Keep saves in the Steam Cloud'       │" -ForegroundColor DarkGray
            Write-Host "  │  (Only disables cloud for CS2 — other games stay synced)    │" -ForegroundColor DarkGray
            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor $cloudColor
            if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] after disabling Cloud sync for CS2" }

            # ── Launch Options Guide ──────────────────────────────────────
            Write-Blank
            Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Cyan
            Write-Host "  │  CS2 LAUNCH OPTIONS (Steam -> CS2 -> Properties)            │" -ForegroundColor Cyan
            Write-Host "  │                                                              │" -ForegroundColor Cyan
            Write-Host "  │  RECOMMENDED (evidence-based):                              │" -ForegroundColor White
            Write-Host "  │  -console        Open developer console at startup          │" -ForegroundColor Green
            Write-Host "  │  +exec autoexec  Force autoexec.cfg execution on start      │" -ForegroundColor Green
            Write-Host "  │  -refresh [Hz]   Set display refresh rate (e.g. -refresh 144)│" -ForegroundColor Green
            Write-Host "  │                  Required in exclusive fullscreen mode       │" -ForegroundColor DarkGray
            Write-Host "  │                                                              │" -ForegroundColor Cyan
            Write-Host "  │  OPTIONAL (conditional):                                    │" -ForegroundColor White
            Write-Host "  │  -fullscreen     Exclusive fullscreen (lower latency than   │" -ForegroundColor DarkGray
            Write-Host "  │                  FSW on pre-HAGS systems; test both)        │" -ForegroundColor DarkGray
            Write-Host "  │  -language english  Force English text (non-English Windows)│" -ForegroundColor DarkGray
            Write-Host "  │                                                              │" -ForegroundColor Cyan
            Write-Host "  │  Minimal example string:                                    │" -ForegroundColor DarkGray
            Write-Host "  │  -console +exec autoexec                                    │" -ForegroundColor Green
            Write-Host "  │                                                              │" -ForegroundColor Cyan
            Write-Host "  │  DO NOT USE (cargo-cult / harmful / removed in CS2):        │" -ForegroundColor Yellow
            Write-Host "  │  -novid         No-op in CS2 (no intro video to skip)     │" -ForegroundColor Red
            Write-Host "  │  -threads N     Valve warns: conflicts with engine threading│" -ForegroundColor Red
            Write-Host "  │  -tickrate 128  No-op in CS2 sub-tick (silently ignored)   │" -ForegroundColor Red
            Write-Host "  │  -nojoy         Useless in CS2 (no measurable impact)      │" -ForegroundColor Red
            Write-Host "  │  -softparticlesdefaultoff  Source 1 only, unparsed in CS2  │" -ForegroundColor Red
            Write-Host "  │  +cl_forcepreload 1  Source 1 only; causes VRAM spike      │" -ForegroundColor Red
            Write-Host "  │  +mat_queue_mode 2   Source 1 only; dead CVar in CS2       │" -ForegroundColor Red
            Write-Host "  │  -vulkan (Windows)   Unstable on Windows; Linux/Mac only   │" -ForegroundColor Red
            Write-Host "  │  -dxlevel N     Source 1 only; ignored by CS2              │" -ForegroundColor Red
            Write-Host "  │  -high          Use Phase 3 IFEO instead — persistent,     │" -ForegroundColor Red
            Write-Host "  │                 kernel-level, zero launch flag overhead     │" -ForegroundColor Red
            Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Cyan
            Write-Blank
            "-console +exec autoexec" | Set-ClipboardSafe
            Write-OK "Launch options string copied to clipboard: -console +exec autoexec"
            Write-Info "Add -refresh [Hz] with your monitor's refresh rate (e.g. -refresh 144)."
            if (-not $SCRIPT:DryRun -and -not (Test-YoloProfile)) { Read-Host "  [Enter] when launch options are set in Steam" }

            Complete-Step $PHASE 34 "Autoexec"
        } `
        -SkipAction { Skip-Step $PHASE 34 "Autoexec" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 35 — CHIPSET DRIVER CHECK  [T2]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 35) {
    Write-Section "Step 35 — Chipset Driver Check"
    Invoke-TieredStep -Tier 2 -Title "Check chipset driver and show update link" `
        -Why "Outdated chipset drivers can affect USB polling, PCIe latency and memory controller." `
        -Evidence "AMD chipset drivers contain CPPC2 fix (Ryzen). Intel RST/Chipset INF for PCIe timing." `
        -Caveat "Chipset update is rarely the cause of bad FPS. But: no downside, quick to do." `
        -Risk "SAFE" -Depth "CHECK" `
        -Improvement "Updated chipset may fix USB polling and PCIe latency" `
        -SideEffects "None — shows download link only" `
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
                            Write-Warn "URL validation failed — open manually: $url"
                        }
                    }
                }
            } else {
                Write-Warn "CPU manufacturer not recognized — check manually."
            }
            Complete-Step $PHASE 35 "ChipsetDriver"
        } `
        -SkipAction { Skip-Step $PHASE 35 "ChipsetDriver" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 36 — VISUAL EFFECTS BEST PERFORMANCE  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 36) {
    Write-Section "Step 36 — Visual Effects + Defender Exclusions + Auto HDR"
    Invoke-TieredStep -Tier 3 -Title "Visual effects 'Best Performance' + Defender CS2 exclusions + Win11 Auto HDR off" -EstimateKey "Visual Effects" `
        -Why "Visual effects: removes DWM animation/transparency overhead. Defender exclusions: eliminates real-time scan hooks on CS2 shader cache files — every cache read triggers a Defender intercept at kernel level; exclusions remove this for known-safe game paths without disabling protection. Win11 Auto HDR: applies HDR tone-mapping post-processing to SDR games — adds GPU overhead, and in CS2 can overbrighten window/lamp areas." `
        -Evidence "Defender exclusions: djdallmann GamingPCSetup, valleyofdoom PC-Tuning (add game exe + shader cache paths). Visual effects: community consensus, marginal. Auto HDR: Win11 feature — disable for competitive via registry AutoHDREnabled=0." `
        -Caveat "Defender exclusions do NOT disable Windows Defender — only skip scanning of specific trusted game paths. Desktop looks simpler after visual effects change. Auto HDR change only applies on Win11 HDR-capable displays." `
        -Risk "SAFE" -Depth "REGISTRY" `
        -Improvement "Defender exclusions: eliminates scan-time overhead on shader cache I/O. Visual effects: marginal desktop overhead reduction. Auto HDR: prevents unintended tone-mapping during competitive play." `
        -SideEffects "Desktop looks simpler. Defender will not scan added exclusion paths." `
        -Undo "Remove-MpPreference -ExclusionPath/Process; restore VisualFXSetting + AutoHDREnabled" `
        -Action {
            # ── Visual Effects ─────────────────────────────────────────────────
            # VisualFXSetting=2 tells the System Properties dialog "Best Performance" is selected.
            # UserPreferencesMask is the actual bitmap controlling individual visual effects.
            # Setting both ensures immediate effect without requiring a separate logon cycle.
            Set-RegistryValue "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects" `
                "VisualFXSetting" 2 "DWord" "Best Performance"
            # UserPreferencesMask: 90 12 03 80 10 00 00 00 = "Best Performance" with font smoothing ON
            # Byte 2 = 0x03 (instead of 0x01) preserves ClearType/font smoothing — text remains readable.
            # Without this, "Best Performance" disables ClearType, making text aliased and hard to read.
            $upmPath = "HKCU:\Control Panel\Desktop"
            Set-RegistryValue $upmPath "UserPreferencesMask" ([byte[]](0x90,0x12,0x03,0x80,0x10,0x00,0x00,0x00)) "Binary" "Best Performance + ClearType preserved"
            # FontSmoothing=2 is the string value that enables ClearType at the GDI level
            Set-RegistryValue $upmPath "FontSmoothing" "2" "String" "ClearType font smoothing enabled"
            Write-ActionOK "Visual effects: Best Performance (ClearType font smoothing preserved)."

            # ── Windows Defender CS2 Exclusions ───────────────────────────────
            # Exclude CS2 executable and shader cache directories from real-time scanning.
            # Uses Add-MpPreference (Windows Defender API) — does NOT disable protection.
            # djdallmann: "excludes NVIDIA DXCache, AMD shader cache, update datastore"
            if (-not (Get-Command Add-MpPreference -ErrorAction SilentlyContinue)) {
                Write-Warn "Windows Defender API not available — exclusions skipped. (This is normal on some debloated Windows builds.)"
            } else { try {
                # Exclusion paths — common Steam install locations + shader caches
                $excludePaths = @(
                    "${env:ProgramFiles(x86)}\Steam\steamapps\common\Counter-Strike Global Offensive",
                    "$env:ProgramFiles\Steam\steamapps\common\Counter-Strike Global Offensive",
                    "D:\Steam\steamapps\common\Counter-Strike Global Offensive",
                    "E:\Steam\steamapps\common\Counter-Strike Global Offensive",
                    "$env:LOCALAPPDATA\NVIDIA\DXCache",
                    "$env:LOCALAPPDATA\NVIDIA\GLCache",
                    "$env:LOCALAPPDATA\D3DSCache",
                    "${env:ProgramFiles(x86)}\Steam\steamapps\shadercache\730",
                    "$env:ProgramFiles\Steam\steamapps\shadercache\730"
                )
                # Add actual CS2 install path detected via Steam registry + libraryfolders.vdf
                $detectedCS2Path = Get-CS2InstallPath
                if ($detectedCS2Path -and ($detectedCS2Path -notin $excludePaths)) {
                    $excludePaths += $detectedCS2Path
                }
                # Filter to existing paths only — Add-MpPreference accepts non-existent paths but
                # this keeps the exclusion list clean and avoids confusing users.
                $addedPaths = @()
                $addedProcesses = @()
                foreach ($p in $excludePaths) {
                    if (Test-Path $p) {
                        if (-not $SCRIPT:DryRun) {
                            Add-MpPreference -ExclusionPath $p -ErrorAction SilentlyContinue
                            $addedPaths += $p
                        } else {
                            Write-Host "  [DRY-RUN] Would exclude: $p" -ForegroundColor Magenta
                        }
                    }
                }
                # Process exclusion for cs2.exe
                if (-not $SCRIPT:DryRun) {
                    Add-MpPreference -ExclusionProcess "cs2.exe" -ErrorAction SilentlyContinue
                    $addedProcesses += "cs2.exe"
                } else {
                    Write-Host "  [DRY-RUN] Would exclude process: cs2.exe" -ForegroundColor Magenta
                }
                # Backup the exclusions we just added so they can be removed on restore
                if (-not $SCRIPT:DryRun -and ($addedPaths.Count -gt 0 -or $addedProcesses.Count -gt 0)) {
                    Backup-DefenderExclusions -ExclusionPaths $addedPaths `
                        -ExclusionProcesses $addedProcesses -StepTitle $SCRIPT:CurrentStepTitle
                }
                Write-ActionOK "Defender: excluded cs2.exe + $($addedPaths.Count) game/cache paths from real-time scanning."
            } catch {
                Write-Warn "Defender exclusion error: $($_.Exception.Message)"
            } }

            # ── Win11 Auto HDR disable ─────────────────────────────────────────
            # Auto HDR applies tone-mapping post-processing to SDR games in Win11.
            # Adds GPU overhead; can overbrighten window/lamp areas in CS2 maps (competitive disadvantage).
            # Only relevant on Win11 + HDR-capable display. Harmless write on non-HDR/Win10 systems.
            $videoSettings = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\VideoSettings"
            Set-RegistryValue $videoSettings "AutoHDREnabled" 0 "DWord" "Win11 Auto HDR disabled"
            Write-ActionOK "Win11 Auto HDR: disabled (prevents tone-mapping overhead in CS2)."

            Write-Info "Undo: Remove-MpPreference to remove exclusions; restore VisualFXSetting=0 for 'Let Windows decide'."
            Complete-Step $PHASE 36 "VisualEffects"
        } `
        -SkipAction { Skip-Step $PHASE 36 "VisualEffects" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 37 — SYSMAIN + WINDOWS SEARCH DISABLE  [T3]
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 37) {
    Write-Section "Step 37 — Disable SysMain + Windows Search"
    Invoke-TieredStep -Tier 3 -Title "Disable SysMain, Windows Search, QWAVE + Xbox services" -EstimateKey "SysMain Disable" `
        -Why "SysMain prefetches apps to RAM -> disk IO under load. Windows Search indexes -> CPU/disk spikes. qWave (Quality Windows Audio/Video Experience): makes periodic network QoS probes that generate UDP DPC activity — redundant since Step 16 already implements DSCP EF=46 for CS2. qWave's probing adds background DPC noise. Xbox services (XblAuthManager, XblGameSave, XboxNetApiSvc, XboxGipSvc): these services maintain persistent connections to Xbox Live, sync game saves, and handle Xbox wireless accessories — all unnecessary for CS2 on systems not using Xbox Game Pass or Xbox wireless controllers." `
        -Evidence "T3: Community consensus, contested on SSD systems. Measurable on HDD. On NVMe: minimal. qWave: Windows QoS service that probes network for multimedia QoS — redundant with per-packet DSCP implementation in Step 16. Xbox services: background network activity and periodic I/O from authentication/sync calls." `
        -Caveat "Windows Search becomes slower. SysMain benefit lost (app startup time). Only recommended on SSD. qWave: disabling may affect WMP/WMV hardware acceleration on some configurations. Xbox services: DO NOT disable if you use Xbox Game Pass, Windows Store games, or Xbox wireless controllers (XboxGipSvc). Re-enable any service that breaks functionality." `
        -Risk "MODERATE" -Depth "SERVICE" `
        -Improvement "Eliminates disk IO spikes from prefetching; removes qWave periodic UDP DPC noise; eliminates Xbox background network activity" `
        -SideEffects "Slower app startup times. Windows Search much slower. qWave: WMP multimedia features may degrade. Xbox: Game Pass / Xbox wireless accessories disabled." `
        -Undo "Set-Service SysMain/WSearch/qWave/XblAuthManager/XblGameSave/XboxNetApiSvc/XboxGipSvc -StartupType Manual (or Automatic for SysMain/WSearch)" `
        -Action {
            if ($SCRIPT:DryRun) {
                $svcList = @("SysMain", "WSearch", "qWave") + $CFG_XboxServices
                Write-Host "  [DRY-RUN] Would disable: $($svcList -join ', ')" -ForegroundColor Magenta
            } else {
                # Backup service state before modification
                Backup-ServiceState -ServiceName "SysMain"         -StepTitle "Disable SysMain + Search + QWAVE + Xbox"
                Backup-ServiceState -ServiceName "WSearch"         -StepTitle "Disable SysMain + Search + QWAVE + Xbox"
                Backup-ServiceState -ServiceName "qWave"           -StepTitle "Disable SysMain + Search + QWAVE + Xbox"
                foreach ($xSvc in $CFG_XboxServices) {
                    Backup-ServiceState -ServiceName $xSvc -StepTitle "Disable SysMain + Search + QWAVE + Xbox"
                }
                # ── SysMain + WSearch (original) ─────────────────────────────────────
                try {
                    $smSvc = Get-Service "SysMain" -ErrorAction SilentlyContinue
                    if ($smSvc) {
                        Set-Service SysMain -StartupType Disabled -ErrorAction Stop
                        Stop-Service SysMain -Force -ErrorAction SilentlyContinue
                        Write-OK "SysMain (Superfetch) disabled."
                    } else {
                        Write-Sub "SysMain: not present on this system (skipped)."
                    }
                } catch { Write-Warn "Could not disable SysMain: $_" }
                try {
                    $wsSvc = Get-Service "WSearch" -ErrorAction SilentlyContinue
                    if ($wsSvc) {
                        Set-Service WSearch -StartupType Disabled -ErrorAction Stop
                        Stop-Service WSearch -Force -ErrorAction SilentlyContinue
                        Write-OK "Windows Search disabled."
                    } else {
                        Write-Sub "WSearch: not present on this system (skipped)."
                    }
                } catch { Write-Warn "Could not disable Windows Search: $_" }

                # ── qWave — Quality Windows Audio/Video Experience ───────────────────
                # Makes periodic network QoS probes that generate UDP DPC noise.
                # Redundant since Step 16 already handles CS2 QoS via DSCP EF=46.
                try {
                    $qw = Get-Service "qWave" -ErrorAction SilentlyContinue
                    if ($qw) {
                        Set-Service qWave -StartupType Disabled -ErrorAction Stop
                        Stop-Service qWave -Force -ErrorAction SilentlyContinue
                        Write-OK "qWave (QoS network probe service) disabled."
                    } else {
                        Write-Sub "qWave: not present on this system (skipped)."
                    }
                } catch { Write-Warn "Could not disable qWave: $_" }

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
                            Stop-Service $svcName -Force -ErrorAction SilentlyContinue
                            Write-OK "$svcName disabled."
                        } else {
                            Write-Sub "${svcName}: not present on this system (skipped)."
                        }
                    } catch { Write-Warn "Could not disable ${svcName}: $_" }
                }
            }
            # ── Memory compression awareness ──────────────────────────────────────
            # Windows Memory Manager can compress idle pages in RAM to free space.
            # Disabling (Disable-MMAgent -mc) removes compression CPU overhead but means
            # Windows goes to the pagefile sooner under memory pressure — a worse outcome
            # on 16GB systems. Not applied automatically; informational for low-RAM setups.
            try {
                $mm = Get-MMAgent -ErrorAction SilentlyContinue
                if ($mm) {
                    $ramGB = try { (Get-RamInfo).TotalGB } catch { 0 }
                    if (-not $mm.MemoryCompression) {
                        Write-Sub "Memory compression: already disabled."
                    } elseif ($ramGB -gt 0 -and $ramGB -le 16) {
                        Write-Blank
                        Write-Info "Memory compression: ENABLED — on ${ramGB} GB RAM this may cause CPU spikes"
                        Write-Info "  under high load (CS2 + Discord + browser can fill 16 GB)."
                        Write-Info "  Disable if CPU spikes appear during play: Disable-MMAgent -mc"
                        Write-Info "  Re-enable: Enable-MMAgent -mc  (requires admin PowerShell + reboot)"
                    }
                    # 32GB+: compression rarely triggers — no advisory needed
                }
            } catch { Write-DebugLog "MMAgent check failed." }

            Write-Info "Undo: Set-Service <name> -StartupType Manual (or Automatic for SysMain/WSearch)"
            Complete-Step $PHASE 37 "SysMainSearch"
        } `
        -SkipAction { Skip-Step $PHASE 37 "SysMainSearch" }
}

# ══════════════════════════════════════════════════════════════════════════════
# STEP 38 — PREPARE SAFE MODE
# ══════════════════════════════════════════════════════════════════════════════
if ($startStep -le 38) {
    Write-Section "Step 38 — Activate Safe Mode + Register Phase 2"
    Write-TierBadge 1 "Safe Mode for GPU driver clean removal"
    Write-Info "GPU driver clean removal runs in Safe Mode — driver files are unlocked there."

    if ($SCRIPT:DryRun) {
        Write-Host "  [DRY-RUN] Would copy scripts to $CFG_WorkDir, set RunOnce, and boot into Safe Mode" -ForegroundColor Magenta
        Complete-Step $PHASE 38 "SafeMode (DRY-RUN preview)"
    } else {
        Copy-PhaseRuntimePayload -SourceRoot $ScriptRoot -DestinationRoot $CFG_WorkDir

        Set-RunOnce "CS2_Phase2" "$CFG_WorkDir\SafeMode-DriverClean.ps1" -SafeMode

        # Use explicit {current} identifier — some Windows builds ignore the implicit default.
        # Capture exit code before piping (| Out-String resets $LASTEXITCODE in PS 5.1).
        $bcdOut = bcdedit /set "{current}" safeboot minimal 2>&1
        $bcdExit = $LASTEXITCODE
        Write-DebugLog "bcdedit /set {current} safeboot minimal — exit $bcdExit — $($bcdOut | Out-String)"

        # If first attempt failed, retry without {current}
        if ($bcdExit -ne 0) {
            Write-Warn "bcdedit /set exited with code $bcdExit — retrying without {current}..."
            $bcdOut = bcdedit /set safeboot minimal 2>&1
            $bcdExit = $LASTEXITCODE
            Write-DebugLog "bcdedit retry exit $bcdExit — $($bcdOut | Out-String)"
        }

        # Trust bcdedit exit code 0 as success. Only use Test-BootConfigSet as a
        # fallback when bcdedit reports failure (exit code might be wrong but BCD
        # was actually written — e.g. bcdedit returns 0 on "already set").
        $safebootOk = ($bcdExit -eq 0) -or (Test-BootConfigSet "safeboot")

        if ($safebootOk) {
            Set-Phase1SafeModeReadyFlag -Path $CFG_StateFile | Out-Null
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
            Write-Err "bcdedit /set safeboot minimal failed — step 38 NOT marked complete (will retry on next run)."
        }
    }
}
