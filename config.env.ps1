# ==============================================================================
#  config.env.ps1  -  Central Configuration · frametime.cfg
# ==============================================================================
#
#  SECURITY: This file is dot-sourced by every entry point (Run-Optimize, PostReboot-Setup,
#  SafeMode-DriverClean, Cleanup, FpsCap-Calculator, Verify-Settings, GUI). Dot-sourcing
#  executes all code in the caller's scope - if this file is tampered, arbitrary code runs
#  as Administrator. In Phase 2/3, this file is dot-sourced from the published
#  immutable C:\FRAMETIME_CFG\runtime-generations\<id> payload created by Step 38.
#
#  Mitigations:
#    - C:\FRAMETIME_CFG\ is hardened to an Administrators/SYSTEM-only ACL before use
#    - Source repo directory requires write access to modify (standard Git checkout)
#    - A fixed payload file set is staged uniquely, SHA-256-manifest validated, and
#      published only after an exact-set verification; Phase 2/3 validate that manifest
#      before administrator checks or helper loading
#

$CFG_Version        = "v3.0.0-alpha.1"
$CFG_WorkDir        = "C:\FRAMETIME_CFG"
$CFG_LogDir         = "$CFG_WorkDir\Logs"
$CFG_LogFile        = "$CFG_LogDir\frametime_current.log"
$CFG_StateFile      = "$CFG_WorkDir\state.json"
$CFG_ProgressFile   = "$CFG_WorkDir\progress.json"
$CFG_LatencyHistoryFile = "$CFG_WorkDir\latency_history.json"
$CFG_LogMaxFiles    = 5
# Bypass is the default because the suite runs locally and is already admin-elevated.
# Harden further with RemoteSigned or AllSigned if you manage signed local scripts.
$CFG_RunOnceExecutionPolicy = "Bypass"

# ── Device Class GUIDs ───────────────────────────────────────────────────────
$CFG_GUID_Display   = "{4d36e968-e325-11ce-bfc1-08002be10318}"   # Display adapters (GPU)
$CFG_GUID_Network   = "{4d36e972-e325-11ce-bfc1-08002be10318}"   # Network adapters (NIC)

# No third-party optimization-tool directory is configured. The workflow still
# invokes Windows utilities and, when selected, the NVIDIA installer.

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
# wireless controllers - re-enable if using Xbox wireless peripherals.
$CFG_XboxServices = @("XblAuthManager", "XblGameSave", "XboxNetApiSvc", "XboxGipSvc")

# ── Virtual/VPN Adapter Filter ─────────────────────────────────────────────────
# Regex for -notmatch on InterfaceDescription. Filters virtual switches, VPN
# tunnels, and Bluetooth PAN from DNS and NIC operations. Each VPN product is
# listed explicitly - no bare "VPN" pattern (could false-match e.g. "Killer VPN-capable").
# Add your VPN adapter name here if it's not already listed.
$CFG_VirtualAdapterFilter = "Loopback|Virtual|Hyper-V|Bluetooth|TAP-Windows|WireGuard|Tailscale|OpenVPN|Cisco AnyConnect|Juniper|Fortinet|vEthernet|Docker|Mullvad|NordLynx|ProtonVPN|SoftEther|GlobalProtect|Pulse Secure"

# ── NIC Tweaks ─────────────────────────────────────────────────────────────────
# InterruptModeration uses the driver label "Medium". Vendor drivers can map
# labels differently, so the live step reports unsupported properties and the
# user should compare both states on the target adapter.
#
# DisplayName varies by vendor:
#   Intel:   "EEE", "FlowControl", "InterruptModeration", "ReceiveBuffers", "TransmitBuffers"
#   Realtek: "Energy Efficient Ethernet", "Flow Control", "Interrupt Moderation", "Receive Buffers", "Transmit Buffers"
# The application layer (Optimize-Hardware.ps1) tries each alternate name if the primary fails.
#
# Buffer sizing uses 512 by default and 2048 for a detected link speed of at
# least 5 Gbit/s. This is a repository heuristic, not a measured universal value.
$CFG_NIC_Tweaks = @{
    "EEE"                 = "Disabled"
    "FlowControl"         = "Disabled"
    "InterruptModeration" = "Medium"
    "ReceiveBuffers"      = "512"
    "TransmitBuffers"     = "512"
}
# Realtek uses space-separated DisplayNames - mapped from Intel-style names at runtime
$CFG_NIC_Tweaks_AltNames = @{
    "EEE"                 = "Energy Efficient Ethernet"
    "FlowControl"         = "Flow Control"
    "InterruptModeration" = "Interrupt Moderation"
    "ReceiveBuffers"      = "Receive Buffers"
    "TransmitBuffers"     = "Transmit Buffers"
}

# ── DNS Servers ──────────────────────────────────────────────────────────────
$CFG_DNS_Cloudflare = @("1.1.1.1", "1.0.0.1")
$CFG_DNS_Google     = @("8.8.8.8", "8.8.4.4")
$CFG_LatencyTargetsFile = if ($PSScriptRoot) { Join-Path $PSScriptRoot "cfgs\valve-latency-targets.json" } else { ".\cfgs\valve-latency-targets.json" }

# ── CS2 generated CFG defaults ───────────────────────────────────────────────
# Notes:
#   rate 1000000      - repository receive-bandwidth value; server and client limits can change.
#   cl_net_buffer_ticks 0 - repository stable-route default. cl_interp_ratio/cl_interp
#     are legacy compatibility entries. cl_net_buffer_ticks_use_interp and
#     cl_tickpacket_desired_queuelength have limited public semantics and are
#     treated as experimental.
#   engine_low_latency_sleep_after_client_tick remains in the current public convar surface and
#     is documented as interacting with r_low_latency; the repo keeps it enabled as a suite
#     default without claiming fps_max 0 is what activates it.
#   fps_max 0 - uncapped default. If you prefer a cap, set it explicitly after benchmarking or
#     via the FPS Cap Calculator guidance.
#   net_client_steamdatagram_enable_override 1 - requests Valve SDR routing and
#     can change the selected network path. Compare 0 and 1 locally.
#   speaker_config 1 - Headphones mode. The repo treats this as the current headphone-focused
#     spatial baseline, but does not claim a separate snd_use_hrtf toggle exists in the current
#     public convar surface.
#   snd_headphone_eq 0 - Natural is the repository default. Change to 1 for the
#     Crisp equalizer if preferred, then compare audibility and listening comfort.
#   snd_spatialize_lerp 0 - repository default for the headphone-focused spatial path.
#     It is not documented here as a Valve competitive requirement.
#   snd_mixahead 0.05 - repository audio-buffer default. It differs from observed
#     engine defaults and must be validated after game updates.
#   mm_dedicated_search_maxping 40 is the repository EU baseline; raise to 80-150
#     in low-server-density regions.
#   r_fullscreen_gamma 2.2 - repository default for exclusive fullscreen. Verify
#     whether the value is active in the selected display mode.
#   r_player_visibility_mode 1 - repository Boost Player Contrast preference.
#     The repository contains no benchmark establishing a universal cost.
#   m_rawinput 1 - retained as a legacy compatibility and intent marker.
#     A current client may ignore unsupported legacy CVars.
#     Step 29 handles the Windows-side pointer-acceleration setting. The generated
#     config retains m_mouseaccel1/2/customaccel 0 as legacy intent markers that
#     a current client can ignore.
$CFG_CS2_Autoexec = [ordered]@{
    # ── Network / Interpolation ────────────────────────────────────────────
    # NOTE: cl_interp_ratio, cl_interp, cl_updaterate are deprecated in CS2 Source 2.
    # Current behavior must be checked in the game console. These legacy entries
    # are retained for compatibility review.
    "cl_interp_ratio"                              = "1"
    "cl_interp"                                    = "0"
    "cl_updaterate"                                = "128"
    "rate"                                         = "1000000"
    "cl_net_buffer_ticks"                          = "0"
    "cl_net_buffer_ticks_use_interp"               = "1"
    "cl_tickpacket_desired_queuelength"            = "0"
    "mm_dedicated_search_maxping"                  = "40"
    "mm_session_search_qos_timeout"                = "20"
    "cl_timeout"                                   = "30"
    "net_client_steamdatagram_enable_override"     = "1"
    # ── Engine / FPS ──────────────────────────────────────────────────────
    "engine_low_latency_sleep_after_client_tick"   = "1"     # Current convar exists; kept as a suite default without claiming fps_max 0 activates it
    "engine_no_focus_sleep"                        = "0"
    "fps_max"                                      = "0"     # Uncapped default - use FPS Cap Calculator/NVCP if a cap improves frametimes
    "fps_max_ui"                                   = "200"
    "fps_max_tools"                                = "144"
    # ── Gameplay ──────────────────────────────────────────────────────────
    "cl_predict_body_shot_fx"                      = "0"     # OFF - repository default
    "cl_predict_head_shot_fx"                      = "0"     # OFF - repository default
    "cl_predict_kill_ragdolls"                     = "0"
    "cl_disable_ragdolls"                          = "1"     # Disable corpse physics
    "cl_sniper_delay_unscope"                      = "0"
    "cl_sniper_show_inaccuracy"                    = "0"     # Disable scope inaccuracy indicator
    "cl_crosshair_sniper_show_normal_inaccuracy"   = "0"     # Disable standing-inaccuracy blur
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
    # ── Audio - Spatial / System ───────────────────────────────────────────
    "speaker_config"                               = "1"
    "snd_mixahead"                                 = "0.05"
    "snd_headphone_eq"                             = "0"
    "snd_spatialize_lerp"                          = "0"
    "snd_steamaudio_enable_perspective_correction" = "1"
    "voice_always_sample_mic"                      = "1"
    "snd_mute_losefocus"                           = "0"
    "snd_voipvolume"                               = "0.5"
    # ── Audio - Repository music-volume preferences ───────────────────────
    "snd_menumusic_volume"                         = "0"
    "snd_roundstart_volume"                        = "0"
    "snd_roundend_volume"                          = "0"
    "snd_roundaction_volume"                       = "0"
    "snd_mvp_volume"                               = "0"
    "snd_mapobjective_volume"                      = "0"
    "snd_tensecondwarning_volume"                  = "0.1"
    "snd_deathcamera_volume"                       = "0"
    # ── Mouse - Compatibility values ──────────────────────────────────────
    # These legacy input CVars are retained as intent markers. Current client
    # behavior must be checked because unsupported CVars may be ignored.
    "m_rawinput"                                   = "1"
    "m_mouseaccel1"                                = "0"
    "m_mouseaccel2"                                = "0"
    "m_customaccel"                                = "0"
    # ── Video - console-settable (remainder is video.txt / in-game menu) ──
    "r_player_visibility_mode"                     = "1"
    "r_fullscreen_gamma"                           = "2.2"
    "mat_monitorgamma_tv_enabled"                  = "0"
}

# ── Chipset Driver URLs ──────────────────────────────────────────────────────
$CFG_URL_AMD_Chipset   = "https://www.amd.com/en/support/download/drivers.html"
$CFG_URL_Intel_Chipset = "https://www.intel.com/content/www/us/en/download/19347/chipset-inf-utility.html"
