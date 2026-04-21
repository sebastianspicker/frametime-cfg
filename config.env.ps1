# ==============================================================================
#  config.env.ps1  —  Central Configuration · CS2 Optimization Suite
# ==============================================================================
#
#  SECURITY: This file is dot-sourced by every entry point (Run-Optimize, PostReboot-Setup,
#  SafeMode-DriverClean, Cleanup, FpsCap-Calculator, Verify-Settings, GUI). Dot-sourcing
#  executes all code in the caller's scope — if this file is tampered, arbitrary code runs
#  as Administrator. In Phase 2/3, this file is dot-sourced from C:\CS2_OPTIMIZE\config.env.ps1
#  (copied by Step 38).
#
#  Mitigations:
#    - C:\CS2_OPTIMIZE\ is created by an admin process, inheriting C:\ ACLs (Users: Read+Execute)
#    - Source repo directory requires write access to modify (standard Git checkout)
#    - No runtime integrity check is performed (accepted risk — if attacker has admin write
#      access to C:\CS2_OPTIMIZE\, the system is already compromised at that privilege level)
#

$CFG_Version        = "v2.1"
$CFG_WorkDir        = "C:\CS2_OPTIMIZE"
$CFG_LogDir         = "$CFG_WorkDir\Logs"
$CFG_LogFile        = "$CFG_LogDir\optimize_current.log"
$CFG_StateFile      = "$CFG_WorkDir\state.json"
$CFG_ProgressFile   = "$CFG_WorkDir\progress.json"
$CFG_LatencyHistoryFile = "$CFG_WorkDir\latency_history.json"
$CFG_LogMaxFiles    = 5
$CFG_BackupMaxVersions = 3
# Bypass is the default because the suite runs locally and is already admin-elevated.
# Harden further with RemoteSigned or AllSigned if you manage signed local scripts.
$CFG_RunOnceExecutionPolicy = "Bypass"

# ── Device Class GUIDs ───────────────────────────────────────────────────────
$CFG_GUID_Display   = "{4d36e968-e325-11ce-bfc1-08002be10318}"   # Display adapters (GPU)
$CFG_GUID_Network   = "{4d36e972-e325-11ce-bfc1-08002be10318}"   # Network adapters (NIC)

# ── Tool directories removed — all optimizations are now native PowerShell ────

# ── Benchmark Maps ─────────────────────────────────────────────────────────────
$CFG_Benchmark_Dust2   = "https://steamcommunity.com/sharedfiles/filedetails/?id=3240880604"
$CFG_Benchmark_Inferno = "https://steamcommunity.com/sharedfiles/filedetails/?id=2932674700"
$CFG_Benchmark_Ancient = "https://steamcommunity.com/sharedfiles/filedetails/?id=3472126051"

# ── FPS Cap ────────────────────────────────────────────────────────────────────
$CFG_FpsCap_Percent = 0.09
$CFG_FpsCap_Min     = 60

# ── Startup Validation ───────────────────────────────────────────────────────
# Validate $CFG_FpsCap_Percent is in a sane range (0.01 to 0.50 = 1% to 50%)
if ($CFG_FpsCap_Percent -lt 0.01 -or $CFG_FpsCap_Percent -gt 0.50) {
    Write-Warning "config.env.ps1: CFG_FpsCap_Percent ($CFG_FpsCap_Percent) is outside valid range (0.01-0.50). Resetting to 0.09."
    $CFG_FpsCap_Percent = 0.09
}
if ($CFG_FpsCap_Min -lt 30 -or $CFG_FpsCap_Min -gt 500) {
    Write-Warning "config.env.ps1: CFG_FpsCap_Min ($CFG_FpsCap_Min) is outside valid range (30-500). Resetting to 60."
    $CFG_FpsCap_Min = 60
}

# ── Shader Cache Paths ─────────────────────────────────────────────────────────
$CFG_ShaderCache_Paths = @(
    "${env:ProgramFiles(x86)}\Steam\steamapps\shadercache\730",
    "$env:ProgramFiles\Steam\steamapps\shadercache\730",
    "D:\Steam\steamapps\shadercache\730",
    "E:\Steam\steamapps\shadercache\730",
    "F:\Steam\steamapps\shadercache\730"
)
$CFG_NV_ShaderCache = "$env:LOCALAPPDATA\NVIDIA\DXCache"
$CFG_NV_GLCache     = "$env:LOCALAPPDATA\NVIDIA\GLCache"
$CFG_DX_ShaderCache = "$env:LOCALAPPDATA\D3DSCache"

# ── Autostart ──────────────────────────────────────────────────────────────────
$CFG_Autostart_Remove = @(
    "OneDrive","Spotify","Discord","Teams","Skype",
    "AdobeUpdater","AdobeGCInvoker","CCleaner",
    "Dropbox","GoogleDriveFS","EpicGamesLauncher",
    "RTSS"
)

# ── Services to disable ───────────────────────────────────────────────────────
# Xbox services: background auth/sync/networking. XboxGipSvc controls Xbox
# wireless controllers — re-enable if using Xbox wireless peripherals.
$CFG_XboxServices = @("XblAuthManager", "XblGameSave", "XboxNetApiSvc", "XboxGipSvc")

# ── Virtual/VPN Adapter Filter ─────────────────────────────────────────────────
# Regex for -notmatch on InterfaceDescription. Filters virtual switches, VPN
# tunnels, and Bluetooth PAN from DNS and NIC operations. Each VPN product is
# listed explicitly — no bare "VPN" pattern (could false-match e.g. "Killer VPN-capable").
# Add your VPN adapter name here if it's not already listed.
$CFG_VirtualAdapterFilter = "Loopback|Virtual|Hyper-V|Bluetooth|TAP-Windows|WireGuard|Tailscale|OpenVPN|Cisco AnyConnect|Juniper|Fortinet|vEthernet|Docker|Mullvad|NordLynx|ProtonVPN|SoftEther|GlobalProtect|Pulse Secure"

# ── NIC Tweaks ─────────────────────────────────────────────────────────────────
# InterruptModeration: "Medium" not "Disabled".
#   djdallmann empirical test (Intel Gigabit CT): Medium produced the lowest DPC latency
#   variance. "Disabled" means every arriving packet fires an interrupt — under background
#   network traffic this creates an interrupt storm that increases DPC jitter rather than
#   reducing it. Medium coalesces packets within a short window (~50-100µs) which is
#   imperceptible for CS2's 128 tick rate but prevents burst-mode interrupt flooding.
#   Predictable DPC scheduling > theoretically lower single-packet latency.
#
# DisplayName varies by vendor:
#   Intel:   "EEE", "FlowControl", "InterruptModeration", "ReceiveBuffers", "TransmitBuffers"
#   Realtek: "Energy Efficient Ethernet", "Flow Control", "Interrupt Moderation", "Receive Buffers", "Transmit Buffers"
# The application layer (Optimize-Hardware.ps1) tries each alternate name if the primary fails.
#
# Buffer sizing: 512 is appropriate for 1G/2.5G. 5 GbE NICs (RTL8126) benefit from 2048.
# Detected at runtime in Optimize-Hardware.ps1 based on link speed.
$CFG_NIC_Tweaks = @{
    "EEE"                 = "Disabled"
    "FlowControl"         = "Disabled"
    "InterruptModeration" = "Medium"
    "ReceiveBuffers"      = "512"
    "TransmitBuffers"     = "512"
}
# Realtek uses space-separated DisplayNames — mapped from Intel-style names at runtime
$CFG_NIC_Tweaks_AltNames = @{
    "EEE"                 = "Energy Efficient Ethernet"
    "FlowControl"         = "Flow Control"
    "InterruptModeration" = "Interrupt Moderation"
    "ReceiveBuffers"      = "Receive Buffers"
    "TransmitBuffers"     = "Transmit Buffers"
}

# ── Timer Resolution ─────────────────────────────────────────────────────────
$CFG_TimerResolution_Desired = 5000        # 0.5ms in 100ns units

# ── DNS Servers ──────────────────────────────────────────────────────────────
$CFG_DNS_Cloudflare = @("1.1.1.1", "1.0.0.1")
$CFG_DNS_Google     = @("8.8.8.8", "8.8.4.4")
$CFG_LatencyTargetsFile = if ($PSScriptRoot) { Join-Path $PSScriptRoot "cfgs\valve-latency-targets.json" } else { ".\cfgs\valve-latency-targets.json" }

# ── CS2 Autoexec Defaults ────────────────────────────────────────────────────
# Notes:
#   rate 1000000      — actual CS2 max; 786432 shows "Extremely restricted" in UI (display bug).
#   cl_net_buffer_ticks 0 — authoritative interp control in CS2; cl_interp_ratio/cl_interp are
#     belt-and-suspenders. engine_low_latency_sleep_after_client_tick remains in the current
#     public convar surface and is documented as interacting with r_low_latency; the repo keeps
#     it enabled as a suite default without claiming fps_max 0 is what activates it.
#   fps_max 0 — uncapped default. If you prefer a cap, set it explicitly after benchmarking or
#     via the FPS Cap Calculator guidance.
#   net_client_steamdatagram_enable_override 1 — forces Valve SDR routing (helps most regions).
#     Set to 0 here if your direct connection is already clean/low-latency.
#   speaker_config 1 — Headphones mode. The repo treats this as the current headphone-focused
#     spatial baseline, but does not claim a separate snd_use_hrtf toggle exists in the current
#     public convar surface.
#   snd_headphone_eq 0 — Natural (unprocessed). 2026 pro study (esportfire.com, 30+ players):
#     62.5% Natural, 37.5% Crisp. Crisp (1) boosts 2-4kHz highs for footstep clarity but
#     causes ear fatigue over long sessions. Change to 1 to prefer Crisp.
#   snd_spatialize_lerp 0 — current suite default for the headphone-focused spatial path.
#     This is a community-preferred setting, not a Valve-documented competitive requirement.
#   snd_mixahead 0.05 — conservative community-preferred audio buffer. The current public
#     convar dump shows a much lower engine default, so the repo treats 0.05 as a stability-
#     oriented suite default rather than as Valve's current default.
#   mm_dedicated_search_maxping 80 — tune by region: EU → 40, SEA → 80-150.
#   r_fullscreen_gamma 2.2 — exclusive fullscreen only (no-op in fullscreen windowed).
#     Competitive players use 1.6-1.8 to brighten dark corners. 2.2 = system default.
#   r_player_visibility_mode 1 — Boost Player Contrast. Community benchmarking reports low
#     overhead on many systems, but the repo now treats it as a suite default rather than a
#     universal rule.
#   m_rawinput 1 — kept only as a harmless documentation/forward-compatibility stub.
#     Current CS2 builds force raw input on already, so this line is a no-op today.
#     Step 29 handles the Windows-side pointer-acceleration setting; m_mouseaccel1/2/customaccel 0
#     remain the active CS2-side acceleration guards inside the generated config.
$CFG_CS2_Autoexec = [ordered]@{
    # ── Network / Interpolation ────────────────────────────────────────────
    # NOTE: cl_interp_ratio, cl_interp, cl_updaterate are deprecated in CS2 Source 2.
    # The subtick system handles interpolation differently; cl_net_buffer_ticks is the
    # actual control. These are kept as belt-and-suspenders (harmless no-ops).
    "cl_interp_ratio"                              = "1"
    "cl_interp"                                    = "0"
    "cl_updaterate"                                = "128"
    "rate"                                         = "1000000"
    "cl_net_buffer_ticks"                          = "0"
    "cl_net_buffer_ticks_use_interp"               = "1"
    "cl_tickpacket_desired_queuelength"            = "0"
    "mm_dedicated_search_maxping"                  = "80"
    "mm_session_search_qos_timeout"                = "20"
    "cl_timeout"                                   = "30"
    "net_client_steamdatagram_enable_override"     = "1"
    # ── Engine / FPS ──────────────────────────────────────────────────────
    "engine_low_latency_sleep_after_client_tick"   = "1"     # Current convar exists; kept as a suite default without claiming fps_max 0 activates it
    "engine_no_focus_sleep"                        = "0"
    "fps_max"                                      = "0"     # Uncapped default — use FPS Cap Calculator to set optimal cap (enables low_latency_sleep above)
    "fps_max_ui"                                   = "200"
    "fps_max_tools"                                = "144"
    # ── Gameplay ──────────────────────────────────────────────────────────
    "cl_predict_body_shot_fx"                      = "0"     # OFF — 95% pro consensus (ThourCS2 120-player study)
    "cl_predict_head_shot_fx"                      = "0"     # OFF — phantom dinks cause fatal target-switching errors
    "cl_predict_kill_ragdolls"                     = "0"
    "cl_disable_ragdolls"                          = "1"     # No corpse physics (CPU savings + visual clarity)
    "cl_sniper_delay_unscope"                      = "0"
    "cl_sniper_show_inaccuracy"                    = "0"     # Oct 2025 — disable scope bloom indicator
    "cl_crosshair_sniper_show_normal_inaccuracy"   = "0"     # Crisp scope crosshair (no standing inaccuracy blur)
    "r_drawtracers_firstperson"                    = "0"
    "gameinstructor_enable"                        = "0"
    "con_enable"                                   = "1"
    "option_duck_method"                           = "0"
    "option_speed_method"                          = "0"
    "lobby_default_privacy_bits2"                  = "0"
    "cl_autowepswitch"                             = "0"
    "cl_silencer_mode"                             = "0"     # Prevent accidental silencer detach
    "cl_dm_buyrandomweapons"                       = "0"     # Pick own weapon in DM
    "cl_join_advertise"                            = "2"     # Friends can see/join your server
    # ── HUD / QoL ────────────────────────────────────────────────────────
    "cl_compass_enabled"                           = "0"     # Hide compass (radar sufficient)
    "cl_show_clan_in_death_notice"                 = "0"     # Cleaner kill feed
    "cl_weapon_selection_rarity_color"             = "0"     # No skin rarity glow on weapon icons
    "cl_use_opens_buy_menu"                        = "0"     # Prevent E key opening buy menu
    "cl_buywheel_nonumberpurchasing"               = "1"     # Number keys won't buy in buy zone
    "cl_spec_show_bindings"                        = "0"     # Hide spectator control hints
    "viewmodel_recoil"                             = "0"     # No weapon kick animation during spray
    # ── Privacy / Anti-distraction ───────────────────────────────────────
    "cl_invites_only_mainmenu"                     = "1"     # Block invite popups during matches
    "cl_invites_only_friends"                      = "1"     # Only accept invites from friends
    "cl_embedded_stream_audio_volume"              = "0"     # Mute embedded Twitch/event audio
    "tv_nochat"                                    = "1"     # Mute GOTV spectator chat
    "snd_mute_mvp_music_live_players"              = "1"     # Mute MVP music while players alive
    # ── HUD / Telemetry (CS2-native; replaces removed net_graph) ──────────
    "cl_hud_telemetry_frametime_show"              = "0"
    "cl_hud_telemetry_ping_show"                   = "0"
    "cl_hud_telemetry_net_misdelivery_show"        = "0"
    "cl_hud_telemetry_net_quality_graph_show"      = "0"
    "cl_hud_telemetry_serverrecvmargin_graph_show" = "0"
    # ── Audio — Spatial / System ───────────────────────────────────────────
    "speaker_config"                               = "1"
    "snd_mixahead"                                 = "0.05"
    "snd_headphone_eq"                             = "0"
    "snd_spatialize_lerp"                          = "0"
    "snd_steamaudio_enable_perspective_correction" = "1"
    "voice_always_sample_mic"                      = "1"
    "snd_mute_losefocus"                           = "0"
    "snd_voipvolume"                               = "0.5"
    # ── Audio — Music muting (zero competitive downside) ──────────────────
    "snd_menumusic_volume"                         = "0"
    "snd_roundstart_volume"                        = "0"
    "snd_roundend_volume"                          = "0"
    "snd_roundaction_volume"                       = "0"
    "snd_mvp_volume"                               = "0"
    "snd_mapobjective_volume"                      = "0"
    "snd_tensecondwarning_volume"                  = "0.1"
    "snd_deathcamera_volume"                       = "0"
    # ── Mouse — raw input (bypass Windows pointer processing) ─────────────
    # NOTE: m_rawinput is a no-op in CS2 — raw input is always forced on and cannot be disabled.
    # Kept as belt-and-suspenders for documentation clarity and forward-compatibility.
    # m_mouseaccel1/2: disable CS2-engine acceleration thresholds (belt-and-suspenders with Step 29).
    # m_customaccel: disable any custom acceleration curve.
    "m_rawinput"                                   = "1"
    "m_mouseaccel1"                                = "0"
    "m_mouseaccel2"                                = "0"
    "m_customaccel"                                = "0"
    # ── Video — autoexec-settable (remainder is video.txt / in-game menu) ─
    "r_player_visibility_mode"                     = "1"
    "r_fullscreen_gamma"                           = "2.2"
    "mat_monitorgamma_tv_enabled"                  = "0"
}

# ── Chipset Driver URLs ──────────────────────────────────────────────────────
$CFG_URL_AMD_Chipset   = "https://www.amd.com/en/support/download/drivers.html"
$CFG_URL_Intel_Chipset = "https://www.intel.com/content/www/us/en/download/19347/chipset-inf-utility.html"
# Process Lasso — kept as alternative reference (native IFEO replaces it in Step 10)
$CFG_URL_ProcessLasso  = "https://bitsum.com/processlasso/"

# ── Estimated Improvement Ranges ────────────────────────────────────────
# P1LowMin/P1LowMax = estimated % improvement in 1% lows (main metric)
# AvgMin/AvgMax      = estimated % improvement in avg FPS
# Confidence = how well-measured the estimate is (HIGH, MEDIUM, LOW)
# These are used to compare estimates vs. actual benchmark results.
$CFG_ImprovementEstimates = @{
    "Clear Shader Cache"         = @{ P1LowMin=0;  P1LowMax=5;  AvgMin=0; AvgMax=1;  Confidence="HIGH";   Note="Only if stale shaders exist" }
    "Fullscreen Optimizations"   = @{ P1LowMin=1;  P1LowMax=5;  AvgMin=0; AvgMax=2;  Confidence="HIGH";   Note="FSE prevents DWM compositing overhead" }
    "CS2 Optimized Power Plan"   = @{ P1LowMin=2;  P1LowMax=8;  AvgMin=1; AvgMax=5;  Confidence="HIGH";   Note="Native tiered plan: T1 safe, T2 RECOMMENDED+, T3 COMPETITIVE+ (CPU vendor-aware)" }
    "Game DVR / Game Bar Off"    = @{ P1LowMin=0;  P1LowMax=3;  AvgMin=0; AvgMax=2;  Confidence="MEDIUM"; Note="Background recording can steal GPU time" }
    "Mouse Acceleration Off"     = @{ P1LowMin=0;  P1LowMax=0;  AvgMin=0; AvgMax=0;  Confidence="HIGH";   Note="Input consistency, not FPS" }
    "Disable Overlays"           = @{ P1LowMin=0;  P1LowMax=3;  AvgMin=0; AvgMax=2;  Confidence="MEDIUM"; Note="Overlay rendering overhead" }
    # The following entry is referenced by step-catalog.ps1 for GUI display
    # but is not directly used as -EstimateKey in Invoke-TieredStep calls.
    "FPS Cap"                    = @{ P1LowMin=5;  P1LowMax=20; AvgMin=-9;AvgMax=-9; Confidence="HIGH";   Note="Stabilizes frametimes. Avg drops by cap." }
    "Timer Resolution"           = @{ P1LowMin=0;  P1LowMax=2;  AvgMin=0; AvgMax=0;  Confidence="MEDIUM"; Note="More precise system timer" }
    "HAGS Toggle"                = @{ P1LowMin=-3; P1LowMax=5;  AvgMin=-2;AvgMax=3;  Confidence="MEDIUM"; Note="Suite default favors ON on newer GPUs based on current community benchmarking; benchmark both on your own system" }
    "NIC Tweaks"                 = @{ P1LowMin=0;  P1LowMax=3;  AvgMin=0; AvgMax=0;  Confidence="LOW";    Note="Only if LatencyMon shows NIC DPC" }
    "MSI Interrupts"             = @{ P1LowMin=0;  P1LowMax=5;  AvgMin=0; AvgMax=1;  Confidence="MEDIUM"; Note="Reduces DPC latency" }
    # The following entry is referenced by step-catalog.ps1 for GUI display
    # but is not directly used as -EstimateKey in Invoke-TieredStep calls.
    "Clean Driver Install"       = @{ P1LowMin=2;  P1LowMax=10; AvgMin=0; AvgMax=5;  Confidence="HIGH";   Note="Bloat-free driver" }
    "Autoexec CVars"             = @{ P1LowMin=0;  P1LowMax=2;  AvgMin=0; AvgMax=0;  Confidence="MEDIUM"; Note="73 CVars: network, engine latency sleep, CS2-side mouse acceleration guards, audio spatial defaults, music mute, video, gameplay" }
    "SysMain Disable"            = @{ P1LowMin=0;  P1LowMax=3;  AvgMin=0; AvgMax=1;  Confidence="LOW";    Note="Only on HDD or low RAM" }
    "Debloat"                    = @{ P1LowMin=0;  P1LowMax=2;  AvgMin=0; AvgMax=1;  Confidence="LOW";    Note="Fewer background processes" }
    "Visual Effects"             = @{ P1LowMin=0;  P1LowMax=1;  AvgMin=0; AvgMax=1;  Confidence="LOW";    Note="DWM overhead reduction" }
    "VBS/Core Isolation Off"     = @{ P1LowMin=2;  P1LowMax=8;  AvgMin=1; AvgMax=5;  Confidence="MEDIUM"; Note="Removes hypervisor overhead on OEM Win11 — 5-15% CPU cost. Skip if FACEIT/Vanguard." }
    # NetworkThrottlingIndex is intentionally NOT set by the suite.
    # djdallmann xperf analysis found 0xFFFFFFFF increases NDIS.sys DPC latency vs. default (10).
    # "NetworkThrottlingIndex"   = @{ ... } — deliberately omitted; default value 10 is correct.
    "Win32PrioritySeparation"    = @{ P1LowMin=1;  P1LowMax=4;  AvgMin=0; AvgMax=1;  Confidence="HIGH";   Note="0x2A: fixed quantum, max foreground boost — 2025 Blur Busters + Overclock.net showed better 1% lows vs 0x26 variable" }
    "HiberbootEnabled=0"         = @{ P1LowMin=0;  P1LowMax=0;  AvgMin=0; AvgMax=0;  Confidence="HIGH";   Note="Not FPS — enables MSI interrupt registry changes to persist across shutdown/restart" }
}

# ── NVIDIA Profile Estimated Improvement Ranges ─────────────────────────────
# Reference-only: used in step-catalog.ps1 for GUI display, not aggregated in improvement estimates
# Estimates for individual DRS setting clusters in nvidia-profile.ps1
# Applied as a group in Phase 3 Step 4 (T3, COMPETITIVE+ only)
# Source: NvApiDriverSettings.h enum decode + Orbmu2k/nvidiaProfileInspector
$CFG_NvidiaProfileEstimates = @{
    "NV — Power Management (Prefer Max)"    = @{ P1LowMin=3;  P1LowMax=8;  AvgMin=2; AvgMax=5;  Confidence="HIGH";   Note="GPU P-state locked to max; prevents clock dip during momentary load troughs" }
    "NV — Max Pre-rendered Frames = 1"      = @{ P1LowMin=2;  P1LowMax=5;  AvgMin=0; AvgMax=0;  Confidence="HIGH";   Note="Minimum render queue; improves 1% lows without changing avg. Marginal at <144 FPS" }
    "NV — Threaded Optimization (Force On)" = @{ P1LowMin=1;  P1LowMax=3;  AvgMin=1; AvgMax=3;  Confidence="MEDIUM"; Note="Force On (default: Auto). Offloads API calls to CPU threads parallel to GPU" }
    "NV — Texture Filtering (High Perf)"   = @{ P1LowMin=1;  P1LowMax=3;  AvgMin=1; AvgMax=2;  Confidence="MEDIUM"; Note="Max driver-side quality reduction. Trades AF/trilinear quality for GPU bandwidth" }
    "NV — VSync Force Off"                  = @{ P1LowMin=0;  P1LowMax=0;  AvgMin=0; AvgMax=0;  Confidence="HIGH";   Note="Only adds FPS if VSync was previously on. CS2 default is off — redundant safety net" }
    "NV — Shader Cache 10 GB"               = @{ P1LowMin=1;  P1LowMax=5;  AvgMin=0; AvgMax=1;  Confidence="MEDIUM"; Note="Larger cache reduces recompilation stalls. Measurable only on first session per map. 2026 consensus: 10GB+" }
    "NV — All VRR/G-SYNC Disabled"         = @{ P1LowMin=0;  P1LowMax=2;  AvgMin=0; AvgMax=1;  Confidence="LOW";    Note="Eliminates VRR processing overhead. EXCEPTION: G-SYNC Pulsar (Jan 2026) monitors — Pulsar combines VRR + ULMB 2 strobing; owners may want VRR ON + Reflex ON for the lowest motion blur at high FPS" }
    "NV — FXAA Off (double gate)"           = @{ P1LowMin=0;  P1LowMax=1;  AvgMin=0; AvgMax=1;  Confidence="MEDIUM"; Note="CS2 doesn't use FXAA but the gate blocks driver injection. Small safety margin" }
    "NV — Ansel Disabled"                   = @{ P1LowMin=0;  P1LowMax=0;  AvgMin=0; AvgMax=0;  Confidence="HIGH";   Note="Zero overhead. Screenshot tool off; no DPC or API cost when inactive" }
    "NV — Antialiasing App Controlled"      = @{ P1LowMin=0;  P1LowMax=1;  AvgMin=0; AvgMax=1;  Confidence="LOW";    Note="Prevents driver AA injection. CS2 controls MSAA natively" }
    "NV — FRL NVCPL Cap"                    = @{ P1LowMin=5;  P1LowMax=20; AvgMin=-9;AvgMax=-9; Confidence="HIGH";   Note="Value 500 = effectively unlimited. Replaced by fpsCap calculator when set" }
    "NV — Smooth Motion APIs = 1"           = @{ P1LowMin=-5; P1LowMax=0;  AvgMin=0; AvgMax=10; Confidence="LOW";    Note="CAUTION: frame interpolation adds latency. Net-negative for competitive play. ps1 does not apply" }
    "NV — Optimus Discrete GPU Forced"      = @{ P1LowMin=0;  P1LowMax=5;  AvgMin=0; AvgMax=3;  Confidence="LOW";    Note="Laptop only. Desktop = zero impact. Forces dGPU for rendering" }
    "NV — Raytracing Off (DXR + Vulkan)"    = @{ P1LowMin=0;  P1LowMax=2;  AvgMin=0; AvgMax=2;  Confidence="MEDIUM"; Note="CS2 doesn't enable RT by default but the DRS gate prevents any accidental activation" }
}
