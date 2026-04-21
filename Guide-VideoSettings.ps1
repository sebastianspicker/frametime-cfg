# ==============================================================================
#  Guide-VideoSettings.ps1  —  CS2 Video Settings Guide (Feb 2026 community meta)
# ==============================================================================

function Show-CS2SettingsGuide {
    param(
        [int] $fpsCap,
        [int] $avgFps,
        [string] $gpuInput
    )

    # DRY-RUN: skip interactive guide entirely — it has multiple Read-Host loops
    if ($SCRIPT:DryRun) {
        Write-Host "  [DRY-RUN] Would show CS2 video settings guide (interactive)" -ForegroundColor Magenta
        return
    }

    # ── REFLEX DECISION (conflicting data) ─────────────────────────────────
    Write-Blank
    Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Yellow
    Write-Host "  │  NVIDIA REFLEX — CONFLICTING DATA                          │" -ForegroundColor Yellow
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │  Two supported positions:                                   │" -ForegroundColor Yellow
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │  [A] -noreflex + NVCP Low Latency Ultra                     │" -ForegroundColor Cyan
    Write-Host "  │      Community meta since Jan 2025 (Blur Busters, ThourCS2) │" -ForegroundColor DarkGray
    Write-Host "  │      More stable 1% lows in multiple tests.                │" -ForegroundColor DarkGray
    Write-Host "  │      CAVEAT: @CS2Kitchen proved that CapFrameX with        │" -ForegroundColor DarkYellow
    Write-Host "  │      PresentMon 2.2+ delivers false lows data when         │" -ForegroundColor DarkYellow
    Write-Host "  │      Reflex off + NVCP cap active. Boost possibly a        │" -ForegroundColor DarkYellow
    Write-Host "  │      measurement artifact.                                  │" -ForegroundColor DarkYellow
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │  [B] Reflex ON (Enabled — 0% of pros use Boost)            │" -ForegroundColor Green
    Write-Host "  │      ThourCS2 benchmark (NVIDIA Drv 581.08):               │" -ForegroundColor DarkGray
    Write-Host "  │      3-4 ms less input lag on high-end, up to 15 ms on     │" -ForegroundColor DarkGray
    Write-Host "  │      low-end. Nearly no 1%-low difference (+-0.5%).        │" -ForegroundColor DarkGray
    Write-Host "  │      Valve + NVIDIA recommend Reflex ON.                   │" -ForegroundColor DarkGray
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │      IMPORTANT (Jan 2026, Blur Busters):                   │" -ForegroundColor DarkYellow
    Write-Host "  │      Reflex is non-functional in HW:FLIP mode (default).   │" -ForegroundColor DarkYellow
    Write-Host "  │      FIX: Right-click cs2.exe -> Properties -> Compat ->   │" -ForegroundColor DarkYellow
    Write-Host "  │      'Disable fullscreen optimizations'. This switches to  │" -ForegroundColor DarkYellow
    Write-Host "  │      HW:Legacy Flip where Reflex works correctly.          │" -ForegroundColor DarkYellow
    Write-Host "  │                                                              │" -ForegroundColor Yellow
    Write-Host "  │  CONCLUSION: Test both with CapFrameX. Use whatever        │" -ForegroundColor White
    Write-Host "  │  feels better to you. There is no definitive scientific    │" -ForegroundColor White
    Write-Host "  │  proof for either side.                                     │" -ForegroundColor White
    Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Yellow
    Write-Blank

    Write-Host "  Which launch option do you want in your clipboard?" -ForegroundColor White
    Write-Host "  [1]  -noreflex  (community meta, more 1% lows in tests)" -ForegroundColor Cyan
    Write-Host "  [2]  No -noreflex  (Reflex ON in-game, less input lag)" -ForegroundColor Green
    Write-Host "  [3]  Show both, I'll decide myself" -ForegroundColor DarkGray
    do { $reflexChoice = Read-Host "  [1/2/3]" } while ($reflexChoice -notin @("1","2","3"))

    $reflexFlag = switch ($reflexChoice) { "1" {"-noreflex "} "2" {""} "3" {""} }
    $launchOpts = "-console $($reflexFlag)+exec autoexec".Trim()

    Write-Blank
    Write-Host "  LAUNCH OPTIONS:" -ForegroundColor White
    Write-Info "  Steam -> CS2 -> Right-click -> Properties -> General -> Launch Options"
    Write-Blank
    Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Green
    Write-Host "  │  $($launchOpts.PadRight(60))│" -ForegroundColor Green
    Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Green
    $launchOpts | Set-ClipboardSafe
    Write-OK "Copied to clipboard."

    if ($reflexChoice -eq "3") {
        Write-Blank
        Write-Host "  Both options for testing:" -ForegroundColor White
        Write-Host "  [A] -console -noreflex +exec autoexec" -ForegroundColor Cyan
        Write-Host "  [B] -console +exec autoexec  (then Reflex ON in-game)" -ForegroundColor Green
    }

    Write-Blank
    Write-Host "  Parameter explanation:" -ForegroundColor White
    Write-Info "  -console               Open developer console at startup"
    if ($reflexFlag) {
        Write-Info "  -noreflex              Disables Reflex completely (including in-game UI)"
        Write-Info "                         -> Use NVCP Low Latency Mode Ultra instead"
    } else {
        Write-Info "  (no -noreflex)         Set Reflex in-game to 'Enabled' or 'Enabled+Boost'"
    }
    Write-Info "  +exec autoexec         Loads autoexec.cfg on start (also auto-loaded by CS2)"
    Write-Info "  fps_max                Set via autoexec.cfg (default 0 = uncapped)"
    Write-Info "  NOTE: -threads N is cargo-cult — Valve warns against it (omitted deliberately)"

    # ── NVIDIA CP SETTINGS ───────────────────────────────────────────────
    if ($gpuInput -in @("1","2")) {
        Write-Blank
        Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor Cyan
        Write-Host "  │  NVIDIA CONTROL PANEL  ->  3D Settings  ->  CS2/cs2.exe    │" -ForegroundColor Cyan
        Write-Host "  │                                                              │" -ForegroundColor Cyan
        Write-Host "  │  Shader Cache Size   -> Unlimited      [widely recommended] │" -ForegroundColor Green
        if ($fpsCap -gt 0) {
            $capStr = "Max Frame Rate         -> $fpsCap (avg $avgFps - 9%)"
            Write-Host "  │  $($capStr.PadRight(60))│" -ForegroundColor Green
            Write-Host "  │                                     [benchmark-derived cap] │" -ForegroundColor Green
        } else {
            Write-Host "  │  Max Frame Rate       -> set after benchmark  [benchmark]  │" -ForegroundColor Yellow
        }
        Write-Host "  │  Power Management     -> Prefer Maximum Perf. [low-latency] │" -ForegroundColor Yellow
        if ($reflexFlag) {
            Write-Host "  │  Low Latency Mode     -> Ultra              [heuristic]    │" -ForegroundColor Yellow
        } else {
            Write-Host "  │  Low Latency Mode     -> Off (Reflex handles) [heuristic]  │" -ForegroundColor Yellow
        }
        Write-Host "  │  Vertical Sync        -> Off         [common low-latency]   │" -ForegroundColor DarkGray
        if ($gpuInput -eq "1") {
            Write-Host "  │                                                              │" -ForegroundColor Cyan
            Write-Host "  │  RTX 5000: Scaling -> MONITOR (not GPU)                    │" -ForegroundColor Yellow
        }
        Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor Cyan
        if ($fpsCap -gt 0) { "$fpsCap" | Set-ClipboardSafe; Write-OK "FPS cap $fpsCap copied to clipboard again." }
    }

    # ── WINDOWS 11: Optimizations for windowed games ─────────────────────
    $buildProps = Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion" `
        -Name "CurrentBuildNumber" -ErrorAction SilentlyContinue
    $buildRaw = if ($buildProps) { $buildProps.CurrentBuildNumber } else { $null }
    $build = 0
    if ($buildRaw) { try { $build = [int]$buildRaw } catch { $build = 0 } }
    if ($build -ge 22000) {
        Write-Blank
        Write-Host "  ┌──────────────────────────────────────────────────────────────┐" -ForegroundColor DarkCyan
        Write-Host "  │  WINDOWS 11: Optimizations for windowed games              │" -ForegroundColor DarkCyan
        Write-Host "  │                                                              │" -ForegroundColor DarkCyan
        Write-Host "  │  Settings -> Display -> Graphics -> 'Optimizations for       │" -ForegroundColor White
        Write-Host "  │  windowed games' -> ON                                       │" -ForegroundColor White
        Write-Host "  │                                                              │" -ForegroundColor DarkCyan
        Write-Host "  │  Moves DX10/11 from legacy blt to flip-model in windowed    │" -ForegroundColor DarkGray
        Write-Host "  │  mode. Reduces frame latency for borderless/windowed.       │" -ForegroundColor DarkGray
        Write-Host "  │  No effect in exclusive fullscreen.                         │" -ForegroundColor DarkGray
        Write-Host "  └──────────────────────────────────────────────────────────────┘" -ForegroundColor DarkCyan
    }

    # ── HARDWARE TIER DETECTION ──────────────────────────────────────────
    Write-Blank
    Write-Host "  YOUR PC TIER (for settings recommendations):" -ForegroundColor White
    Write-Host "  [1]  LOW-END     GTX 1650/1660, RX 580/5500, <150 avg FPS" -ForegroundColor DarkGray
    Write-Host "  [2]  MID-RANGE   RTX 3060/4060, RX 6700/7600, 150-300 avg FPS" -ForegroundColor Yellow
    Write-Host "  [3]  HIGH-END    RTX 4070+/5070+, RX 7800+, 300+ avg FPS" -ForegroundColor Green
    do { $tierChoice = Read-Host "  [1/2/3]" } while ($tierChoice -notin @("1","2","3"))
    $pcTier = switch ($tierChoice) { "1" {"LOW"} "2" {"MID"} "3" {"HIGH"} }

    Write-Blank
    Write-Host "  YOUR RESOLUTION:" -ForegroundColor White
    Write-Host "  [1]  1280x960 / 1024x768  (4:3 stretched — ~80% pros)" -ForegroundColor Cyan
    Write-Host "  [2]  1920x1080            (16:9 native — more FOV)" -ForegroundColor White
    Write-Host "  [3]  2560x1440            (1440p — visual quality)" -ForegroundColor DarkGray
    Write-Host "  [4]  Other" -ForegroundColor DarkGray
    do { $resChoice = Read-Host "  [1/2/3/4]" } while ($resChoice -notin @("1","2","3","4"))

    $resLabel = switch ($resChoice) { "1" {"4:3 stretched"} "2" {"1080p"} "3" {"1440p"} "4" {"custom"} }
    # Pixel dimensions + aspect ratio mode for video.txt write (populated below for "4" custom)
    $resMap = switch ($resChoice) {
        "1" { @{ w="1280"; h="960";  ar="1" } }   # 4:3 stretched
        "2" { @{ w="1920"; h="1080"; ar="0" } }   # 1080p
        "3" { @{ w="2560"; h="1440"; ar="0" } }   # 1440p
        "4" { @{ w=$null;  h=$null;  ar="0" } }   # custom — filled from existing file
    }

    # ── VIDEO SETTINGS — FEB 2026 META ───────────────────────────────────
    Write-Blank
    Write-Host "  ╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
    Write-Host "  ║  CS2 VIDEO SETTINGS — FEB 2026 META                        ║" -ForegroundColor Cyan
    Write-Host "  ║  Tailored for: $($pcTier.PadRight(9)) GPU  ·  $($resLabel.PadRight(14))                ║" -ForegroundColor Cyan
    Write-Host "  ║  Sources: community benchmarks, surveys, Blur Busters meta ║" -ForegroundColor DarkGray
    Write-Host "  ╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
    Write-Blank

    # Hardware tier examples
    Write-Host "  HARDWARE EXAMPLES:" -ForegroundColor DarkGray
    switch ($pcTier) {
        "LOW"  { Write-Host "  Your tier: GTX 1650/1660, RX 580/5500 XT, i5-10400, Ryzen 5 3600" -ForegroundColor DarkGray }
        "MID"  { Write-Host "  Your tier: RTX 3060/4060, RX 6700XT/7600, i5-12600K, Ryzen 5 7600X" -ForegroundColor DarkGray }
        "HIGH" { Write-Host "  Your tier: RTX 4070+/5070+, RX 7800XT+, i7-14700K, 9800X3D" -ForegroundColor DarkGray }
    }
    Write-Blank

    # Determine recommended values per tier
    $msaa       = switch ($pcTier) { "LOW" {"None + CMAA2"}    "MID" {"4x (sweet spot)"}  "HIGH" {"4x (community bench favorite)"} }
    $shadows    = switch ($pcTier) { "LOW" {"Low"}             "MID" {"Medium"}            "HIGH" {"Medium"} }
    $dynShadows = switch ($pcTier) { "LOW" {"Sun Only"}        "MID" {"All"}               "HIGH" {"All"} }
    $shaderDet  = switch ($pcTier) { "LOW" {"Low"}             "MID" {"Low"}               "HIGH" {"High"} }
    $texFilter  = switch ($pcTier) { "LOW" {"Bilinear"}        "MID" {"16x Anisotropic"}   "HIGH" {"16x Anisotropic"} }
    $modelTex   = switch ($pcTier) { "LOW" {"Low"}             "MID" {"Medium"}            "HIGH" {"Medium"} }
    $particle   = switch ($pcTier) { "LOW" {"Low"}             "MID" {"Low"}               "HIGH" {"Low"} }
    $hdr        = switch ($pcTier) { "LOW" {"Performance"}     "MID" {"Performance"}       "HIGH" {"Performance"} }

    $msaaNote   = switch ($pcTier) {
        "LOW"  {"CMAA2: post-process, near-zero FPS cost."}
        "MID"  {"ThourCS2: 4x = better 1% lows than None. -12% avg."}
        "HIGH" {"ThourCS2 community benches: 4x better lows. 8x: -18%, no benefit."}
    }

    Write-Host "  [BENCHMARK-BACKED / COMMON DEFAULTS]" -ForegroundColor Green
    Write-Host @"
  ┌──────────────────────────────────────────────────────────────────────┐
  │ Setting                  │ Value          │ Proof / Source            │
  ├──────────────────────────┼────────────────┼──────────────────────────┤
  │ Display Mode             │ Fullscreen     │ Measurable: lowest       │
  │                          │                │ input lag + best lows    │
  ├──────────────────────────┼────────────────┼──────────────────────────┤
  │ Boost Player Contrast    │ ON             │ ThourCS2: +5% 1% lows    │
  │                          │                │ r_player_visibility_mode │
  │                          │                │ 1 set in autoexec        │
  ├──────────────────────────┼────────────────┼──────────────────────────┤
  │ Ambient Occlusion        │ OFF            │ ~30 FPS cost (multiple   │
  │                          │                │ tests). No comp. adv.    │
  ├──────────────────────────┼────────────────┼──────────────────────────┤
  │ HDR (light shader)       │ Performance    │ Quality can wash out     │
  │                          │                │ window/sun areas on maps │
  ├──────────────────────────┼────────────────┼──────────────────────────┤
  │ FidelityFX Super Res.    │ OFF            │ Blur harms enemy         │
  │                          │                │ recognition. Consensus   │
  ├──────────────────────────┼────────────────┼──────────────────────────┤
  │ Motion Blur              │ N/A (removed)  │ Disabled engine-wide by  │
  │                          │                │ Valve (AMD GPU bug fix)  │
  ├──────────────────────────┼────────────────┼──────────────────────────┤
  │ V-Sync                   │ OFF            │ Universal, clear         │
  └──────────────────────────┴────────────────┴──────────────────────────┘
  Tip: see docs/video.txt for a copy-ready 2026 meta video.txt example.
"@ -ForegroundColor Green

    Write-Blank
    Write-Host "  [RECOMMENDED FOR YOUR TIER: $pcTier]" -ForegroundColor Cyan
    Write-Host "  ┌──────────────────────────────────────────────────────────────────────┐" -ForegroundColor Cyan
    Write-Host "  │ Setting                  │ Your Value     │ Reason                   │" -ForegroundColor Cyan
    Write-Host "  ├──────────────────────────┼────────────────┼──────────────────────────┤" -ForegroundColor Cyan
    Write-Host "  │ Resolution               │ $($resLabel.PadRight(14)) │ $(if($resChoice -eq '1'){'~80% pros. Wider models.'}elseif($resChoice -eq '2'){'More FOV. Clear image.'}elseif($resChoice -eq '3'){'Visual quality. GPU-heavy.'}else{'Your preference.'})$((' ' * [math]::Max(0,24 - $(if($resChoice -eq '1'){'~80% pros. Wider models.'}elseif($resChoice -eq '2'){'More FOV. Clear image.'}elseif($resChoice -eq '3'){'Visual quality. GPU-heavy.'}else{'Your preference.'}).Length)))  │" -ForegroundColor White
    Write-Host "  │ MSAA                     │ $($msaa.PadRight(14)) │ $(if ($msaaNote.Length -gt 24) { $msaaNote.Substring(0, 21) + '...' } else { $msaaNote.PadRight(24) })  │" -ForegroundColor White
    Write-Host "  │ Global Shadow Quality    │ $($shadows.PadRight(14)) │ $(if($pcTier -eq 'LOW'){'Saves ~15 FPS.          '}else{'Low=disadvantage (Nuke).  '})│" -ForegroundColor White
    Write-Host "  │ Dynamic Shadows          │ $($dynShadows.PadRight(14)) │ $(if($pcTier -eq 'LOW'){'Sun Only saves FPS.      '}else{'Player shadows visible.   '})│" -ForegroundColor White
    Write-Host "  │ Shader Detail            │ $($shaderDet.PadRight(14)) │ $(if($pcTier -eq 'LOW'){'Saves FPS. Visual loss.  '}else{'Cleaner shadows. Low cost.'})│" -ForegroundColor White
    Write-Host "  │ Texture Filtering        │ $($texFilter.PadRight(14)) │ $(if($pcTier -eq 'LOW'){'Saves ~5 FPS.            '}else{'Nearly no FPS cost.       '})│" -ForegroundColor White
    Write-Host "  │ Model / Texture Detail   │ $($modelTex.PadRight(14)) │ No FPS difference.        │" -ForegroundColor White
    Write-Host "  │ Particle Detail          │ $($particle.PadRight(14)) │ Community consensus.      │" -ForegroundColor White
    Write-Host "  │ HDR                      │ $($hdr.PadRight(14)) │ Quality: washes out sun.  │" -ForegroundColor White
    Write-Host "  └──────────────────────────┴────────────────┴──────────────────────────────┘" -ForegroundColor Cyan

    if ($pcTier -eq "LOW") {
        Write-Blank
        Write-Host "  LOW-END TIPS:" -ForegroundColor Yellow
        Write-Host "  -> 4:3 stretched (1280x960) gives ~25-40% more FPS than 1080p" -ForegroundColor White
        Write-Host "  -> If <100 FPS: use MSAA None, Shadow Low, Shader Low" -ForegroundColor White
        Write-Host "  -> Consider FSR if GPU-bound (check GPU usage with Task Manager)" -ForegroundColor White
        Write-Host "  -> CS2 is CPU-bound: if CPU at 100% and GPU at 60%, lower settings won't help" -ForegroundColor DarkYellow
    } elseif ($pcTier -eq "MID") {
        Write-Blank
        Write-Host "  MID-RANGE TIPS:" -ForegroundColor Yellow
        Write-Host "  -> MSAA 4x is worth the cost: ThourCS2 showed better 1% lows than None" -ForegroundColor White
        Write-Host "  -> At 1080p these settings should give 200-350 FPS" -ForegroundColor White
        Write-Host "  -> If GPU-bound: 4:3 stretched frees ~30% GPU headroom" -ForegroundColor White
        Write-Host "  -> 8 GB VRAM GPUs (4060/7600): restart CS2 every 2-3h (VRAM leak)" -ForegroundColor DarkYellow
    } else {
        Write-Blank
        Write-Host "  HIGH-END TIPS:" -ForegroundColor Yellow
        Write-Host "  -> CPU is your bottleneck, not GPU. Higher settings won't cost FPS." -ForegroundColor White
        Write-Host "  -> MSAA 4x: community benches favor it for 1% lows. Don't go 8x (-18% for little return)." -ForegroundColor White
        Write-Host "  -> At 1080p you should see 400+ FPS. At 1440p: 300+ FPS." -ForegroundColor White
        Write-Host "  -> FPS cap is MORE important for you: high FPS = more frametime variance" -ForegroundColor DarkYellow
    }

    Write-Blank
    Write-Host "  IMPORTANT FOR BENCHMARKS:" -ForegroundColor White
    Write-Host "  Apply these settings BEFORE running benchmarks. Different settings" -ForegroundColor DarkGray
    Write-Host "  = different results. For comparable before/after measurements:" -ForegroundColor DarkGray
    Write-Host "  -> Set these video settings first, then run baseline benchmark." -ForegroundColor White
    Write-Host "  -> Never change settings between baseline and post-optimization benchmark." -ForegroundColor White

    Write-Blank
    Write-Host "  HONEST LIMITATION:" -ForegroundColor DarkYellow
    Write-Host @"
  CS2 has structurally poor frame pacing due to the Source 2 engine.
  Games with much higher graphics load (Apex, The Finals, Warzone)
  deliver consistently better 1% lows than CS2. This is not a
  hardware problem — it's a Valve engine problem. No setting or
  tweak fixes this fundamentally.

  What actually helps (priority by evidence):
  1.  CPU with high single-core performance  (9800X3D = current #1)
  2.  RAM at rated speed (XMP/EXPO) — CS2 effect unclear, generally wise
  3.  FPS cap via NVCP  (measurable, reproduced)
  4.  Clean driver install (native removal + reinstall)
  5.  Everything else: system hygiene or community consensus without hard proof
"@ -ForegroundColor DarkGray

    # ── VIDEO.TXT — AUTOMATED WRITE ───────────────────────────────────────────
    Write-Blank
    Write-Host "  ──────────────────────────────────────────────────────────────" -ForegroundColor DarkGray
    Write-Host "  VIDEO.TXT — AUTOMATIC WRITE" -ForegroundColor Cyan
    Write-Host "  Path: <Steam>\userdata\<SteamID>\730\local\cfg\video.txt" -ForegroundColor DarkGray
    Write-Host "  ──────────────────────────────────────────────────────────────" -ForegroundColor DarkGray
    Write-Blank

    # ── Locate video.txt ──────────────────────────────────────────────────────
    $videoTxtPath = $null
    $videoTxtDir  = $null
    try {
        $steamPath = Get-SteamPath
        if ($steamPath -and (Test-Path "$steamPath\userdata")) {
            # Find the most recently touched video.txt across all Steam accounts
            $found = Get-ChildItem "$steamPath\userdata\*\730\local\cfg\video.txt" `
                -ErrorAction SilentlyContinue |
                Sort-Object LastWriteTime -Descending | Select-Object -First 1
            if ($found) {
                $videoTxtPath = $found.FullName
                $videoTxtDir  = $found.DirectoryName
            } else {
                # No video.txt yet — target the most recently modified Steam account
                $userDir = Get-ChildItem "$steamPath\userdata" -Directory `
                    -ErrorAction SilentlyContinue |
                    Sort-Object LastWriteTime -Descending | Select-Object -First 1
                if ($userDir) {
                    $videoTxtDir  = "$($userDir.FullName)\730\local\cfg"
                    $videoTxtPath = "$videoTxtDir\video.txt"
                }
            }
        }
    } catch { Write-DebugLog "video.txt path detection failed: $_" }

    # ── Parse existing video.txt ──────────────────────────────────────────────
    $existingVideoKeys  = @{}
    $existingVideoLines = @()
    $videoExists = $videoTxtPath -and (Test-Path $videoTxtPath)
    if ($videoExists) {
        $existingVideoLines = @(Get-Content $videoTxtPath -Encoding UTF8 -ErrorAction SilentlyContinue)
        foreach ($line in $existingVideoLines) {
            # VDF: "key" "value"  — skip comments and structural lines
            if ($line -match '^\s*"([^"]+)"\s+"([^"]*)"') {
                $existingVideoKeys[$Matches[1]] = $Matches[2]
            }
        }
        Write-Info "video.txt: $videoTxtPath"
        Write-Info "  $($existingVideoLines.Count) lines, $($existingVideoKeys.Count) settings parsed."
    } else {
        if ($videoTxtPath) {
            Write-Info "No video.txt found — will create at:"
            Write-Sub "  $videoTxtPath"
        } else {
            Write-Warn "Steam path not found — cannot locate video.txt automatically."
        }
    }

    # ── Preserve personal settings from existing file ─────────────────────────
    # Refresh rate and brightness are hardware/preference-specific — keep current
    # values instead of overriding them with our defaults.
    $currentHz = if ($existingVideoKeys.ContainsKey("setting.refreshrate_numerator")) {
                     $existingVideoKeys["setting.refreshrate_numerator"]
                 } else { $null }
    $currentBrightness = if ($existingVideoKeys.ContainsKey("setting.brightness")) {
                             $existingVideoKeys["setting.brightness"]
                         } else { "0.000000" }

    Write-Blank
    if ($currentHz) {
        Write-Info "Refresh rate from current video.txt: ${currentHz} Hz"
        $hzIn = Read-Host "  Keep ${currentHz} Hz? [Enter] or type new value (e.g. 144, 240, 360)"
        if ($hzIn.Trim() -match '^\d+$' -and [int]$hzIn.Trim() -ge 30 -and [int]$hzIn.Trim() -le 500) { $currentHz = $hzIn.Trim() }
    } else {
        $hzIn = Read-Host "  Monitor refresh rate Hz? [Enter = 240, or type: 60, 144, 165, 240, 360]"
        $currentHz = if ($hzIn.Trim() -match '^\d+$' -and [int]$hzIn.Trim() -ge 30 -and [int]$hzIn.Trim() -le 500) { $hzIn.Trim() } else { "240" }
    }

    # For "Other" resolution: preserve from existing file rather than writing blank values
    if ($resChoice -eq "4") {
        $resMap.w  = if ($existingVideoKeys.ContainsKey("setting.defaultres"))       { $existingVideoKeys["setting.defaultres"] }       else { "1920" }
        $resMap.h  = if ($existingVideoKeys.ContainsKey("setting.defaultresheight")) { $existingVideoKeys["setting.defaultresheight"] } else { "1080" }
        $resMap.ar = if ($existingVideoKeys.ContainsKey("setting.aspectratiomode"))  { $existingVideoKeys["setting.aspectratiomode"] }  else { "0" }
        Write-Info "Custom resolution preserved from current file: $($resMap.w)x$($resMap.h)  (AR mode $($resMap.ar))"
    }

    $reflexVideoVal = if ($reflexFlag) { "0" } else { "1" }

    # ── Build recommended config (tier + user choices) ────────────────────────
    $rec_msaa       = switch ($pcTier) { "LOW" {"0"}    "MID" {"4"}    "HIGH" {"4"} }
    $rec_cascades   = switch ($pcTier) { "LOW" {"2"}    "MID" {"3"}    "HIGH" {"3"} }
    $rec_shadowTex  = switch ($pcTier) { "LOW" {"256"}  "MID" {"512"}  "HIGH" {"512"} }
    $rec_dynShadows = switch ($pcTier) { "LOW" {"0"}    "MID" {"1"}    "HIGH" {"1"} }
    $rec_shaderQ    = switch ($pcTier) { "LOW" {"0"}    "MID" {"0"}    "HIGH" {"1"} }
    $rec_texFilter  = switch ($pcTier) { "LOW" {"0"}    "MID" {"5"}    "HIGH" {"5"} }
    $rec_charDecal  = switch ($pcTier) { "LOW" {"256"}  "MID" {"512"}  "HIGH" {"512"} }
    $rec_texStream  = switch ($pcTier) { "LOW" {"256"}  "MID" {"512"}  "HIGH" {"1024"} }

    $videoRecommended = [ordered]@{
        "setting.fullscreen"                               = "1"
        "setting.nowindowborder"                           = "0"
        "setting.coop_fullscreen"                          = "0"
        "setting.defaultres"                               = $resMap.w
        "setting.defaultresheight"                         = $resMap.h
        "setting.aspectratiomode"                          = $resMap.ar
        "setting.refreshrate_numerator"                    = $currentHz
        "setting.refreshrate_denominator"                  = "1"
        "setting.brightness"                               = $currentBrightness
        "setting.mat_vsync"                                = "0"
        "setting.msaa_samples"                             = $rec_msaa
        "setting.r_csgo_cmaa_enable"                       = $(if ($pcTier -eq "LOW") { "1" } else { "0" })  # CMAA2 on LOW (free AA when msaa=0)
        "setting.r_csgo_fsr_upsample"                      = "0"
        "setting.mat_viewportscale"                        = "1.000000"
        "setting.r_low_latency"                            = $reflexVideoVal
        "setting.csm_enabled"                              = "1"
        "setting.csm_max_num_cascades_override"            = $rec_cascades
        "setting.lb_csm_override_staticgeo_cascades_value" = "2"
        "setting.lb_shadow_texture_width_override"         = $rec_shadowTex
        "setting.lb_shadow_texture_height_override"        = $rec_shadowTex
        "setting.videocfg_dynamic_shadows"                 = $rec_dynShadows
        "setting.csm_viewmodel_shadows"                    = "0"
        "setting.r_particle_shadows"                       = "0"
        "setting.shaderquality"                            = $rec_shaderQ
        "setting.r_texturefilteringquality"                = $rec_texFilter
        "setting.r_character_decal_resolution"             = $rec_charDecal
        "setting.r_texture_stream_max_resolution"          = $rec_texStream
        "setting.cpu_level"                                = "2"
        "setting.gpu_level"                                = "3"
        "setting.gpu_mem_level"                            = "2"
        "setting.mem_level"                                = "2"
        "setting.r_particle_max_detail_level"              = "0"
        "setting.r_aoproxy_enable"                         = "0"
        "setting.r_aoproxy_min_dist"                       = "0"
        "setting.r_ssao"                                   = "0"
        "setting.sc_hdr_enabled_override"                  = "3"
    }

    # ── Compare current vs. recommended ──────────────────────────────────────
    $vMatching  = [System.Collections.Generic.List[string]]::new()
    $vDiffering = [System.Collections.Generic.List[hashtable]]::new()
    $vNewKeys   = [System.Collections.Generic.List[string]]::new()
    foreach ($kv in $videoRecommended.GetEnumerator()) {
        if (-not $existingVideoKeys.ContainsKey($kv.Key)) {
            $vNewKeys.Add($kv.Key)
        } elseif ($existingVideoKeys[$kv.Key] -ne $kv.Value) {
            $vDiffering.Add(@{ Key=$kv.Key; Current=$existingVideoKeys[$kv.Key]; Recommended=$kv.Value })
        } else {
            $vMatching.Add($kv.Key)
        }
    }

    # ── Summary table ─────────────────────────────────────────────────────────
    Write-Blank
    Write-Host "  YOUR VIDEO.TXT vs. OPTIMIZED:" -ForegroundColor White
    Write-Host "  ─────────────────────────────────────────────────────────────" -ForegroundColor DarkGray
    Write-Host "  $([char]0x2713)  Already at recommended value:   $($vMatching.Count) settings" -ForegroundColor Green
    Write-Host "  !  Will be changed:                $($vDiffering.Count) settings" -ForegroundColor Yellow
    Write-Host "  +  New (not in current video.txt): $($vNewKeys.Count) settings" -ForegroundColor Cyan
    Write-Host "  ─────────────────────────────────────────────────────────────" -ForegroundColor DarkGray

    if ($vDiffering.Count -gt 0) {
        Write-Blank
        Write-Host "  CHANGES:" -ForegroundColor Yellow
        Write-Blank
        foreach ($d in $vDiffering) {
            Write-Host "    $($d.Key)" -ForegroundColor White
            Write-Host "      Current:   $($d.Current)" -ForegroundColor DarkYellow
            Write-Host "      Optimized: $($d.Recommended)" -ForegroundColor Green
        }
    }

    # ── Optional full preview ─────────────────────────────────────────────────
    Write-Blank
    $showVAll = Read-Host "  Show full optimized video.txt ($($videoRecommended.Count) settings)? [y/N]"
    if ($showVAll -match "^[yY]$") {
        Write-Blank
        foreach ($kv in $videoRecommended.GetEnumerator()) {
            $marker = if ($vMatching.Contains($kv.Key))  { [char]0x2713 }
                      elseif ($vNewKeys.Contains($kv.Key)) { "+" }
                      else { "!" }
            $color = switch ($marker) {
                { $_ -eq [char]0x2713 } { "DarkGreen" }
                "+"                     { "Cyan" }
                default                 { "Yellow" }
            }
            Write-Host "    $marker  $($kv.Key)  $($kv.Value)" -ForegroundColor $color
        }
        Write-Blank
    }

    # ── Write ─────────────────────────────────────────────────────────────────
    if ($videoTxtPath) {
        $vProceed = Read-Host "  Rename video.txt → video.txt.bak + write optimized? [Y/n]"
        if ($vProceed -notmatch "^[nN]$") {
            $bakPath = $null
            if ($videoExists) {
                $bakPath = "$videoTxtPath.bak"
                if (-not $SCRIPT:DryRun) {
                    # Preserve the very first original as .bak.orig
                    if (-not (Test-Path "$videoTxtPath.bak.orig") -and (Test-Path "$videoTxtPath.bak")) {
                        Move-Item "$videoTxtPath.bak" "$videoTxtPath.bak.orig" -Force
                    }
                    Move-Item $videoTxtPath $bakPath -Force
                    Write-OK "Renamed: video.txt  →  video.txt.bak"
                } else {
                    Write-Host "  [DRY-RUN] Would rename: $videoTxtPath  →  $bakPath" -ForegroundColor Magenta
                }
            }

            # Build VDF output — key padded to 52 chars for readability
            $vLines = @(
                '"VideoConfig"',
                '{',
                "    // CS2-Optimize Suite — $(Get-Date -Format 'yyyy-MM-dd HH:mm')",
                "    // Tier: $pcTier  |  $($resMap.w)x$($resMap.h)  |  ${currentHz}Hz  |  Reflex: $(if ($reflexFlag) {'OFF (-noreflex)'} else {'ON'})",
                "    // Original backed up as video.txt.bak",
                ""
            )
            foreach ($kv in $videoRecommended.GetEnumerator()) {
                $keyStr = "`"$($kv.Key)`""
                $vLines += "    $($keyStr.PadRight(52))  `"$($kv.Value)`""
            }
            $vLines += "}"

            if (-not $SCRIPT:DryRun) {
                if (-not (Test-Path $videoTxtDir)) {
                    New-Item -ItemType Directory -Path $videoTxtDir -Force -ErrorAction SilentlyContinue | Out-Null
                }
                # Use BOM-less UTF-8 — PS 5.1's -Encoding UTF8 adds BOM which Valve VDF parsers may reject
                try {
                    [System.IO.File]::WriteAllLines($videoTxtPath, $vLines, [System.Text.UTF8Encoding]::new($false))
                } catch {
                    Write-Warn "Failed to write video.txt: $_"
                    if ($bakPath -and (Test-Path $bakPath)) {
                        Move-Item $bakPath $videoTxtPath -Force
                        Write-Info "Restored original video.txt from backup."
                    }
                    return
                }
                Write-OK "video.txt written: $videoTxtPath"
                Write-Info "CS2 must be fully closed for the new file to take effect on next launch."
                Write-Info "To revert: rename video.txt.bak back to video.txt (delete current video.txt first)."
            } else {
                Write-Host "  [DRY-RUN] Would write: $videoTxtPath" -ForegroundColor Magenta
            }
        } else {
            Write-Info "Skipped — video.txt unchanged."
        }
    } else {
        Write-Warn "Could not locate video.txt path automatically."
        Write-Info "Set manually: <Steam>\userdata\<SteamID>\730\local\cfg\video.txt"
        Write-Info "See docs\video.txt for the annotated template."
    }
}
