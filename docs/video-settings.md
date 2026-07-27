# Video Settings, Autoexec, and Launch Options

> Covers Phase 1 Step 34, Phase 3 Steps 4–6, and `docs/video.txt`.

CS2 has three configuration systems that can override or complement each other.
This document records the repository defaults and the file or interface that
owns each value.

## Evidence boundary

The repository does not contain a controlled image-quality, frame-time, or input
latency dataset for the settings below. Values described as suite defaults are
implemented choices, not universal recommendations. Community measurements can
identify settings worth testing, but they do not establish a result for another
GPU, driver, display, resolution, map, or CS2 build.

---

## The Three Configuration Systems

### 1. `video.txt` - Graphics pipeline settings

Located at `<Steam>\userdata\<SteamID>\730\local\cfg\video.txt`.

This file is written by the CS2 engine when you change anything in the in-game Video settings menu. It stores settings that require GPU pipeline reconfiguration to change - display mode, resolution, MSAA sample count, shadow quality, texture filtering, HDR mode.

This file does not control settings that can be changed mid-session through
console commands, such as most HUD toggles.

Important: CS2 overwrites this file when you click "Apply" in the video settings menu. If you edit it manually, make sure CS2 is fully closed before editing, and don't click Apply in the menu afterward without updating your file.

### 2. `optimization.cfg` and the `autoexec.cfg` bootstrap

Step 34 writes the repository's console-variable values to
`<CS2>\game\csgo\cfg\optimization.cfg`. It preserves an existing
`autoexec.cfg` and appends `exec optimization.cfg` when that line is absent.

The documented launch-option example uses `+exec autoexec` to request the
bootstrap file at game startup. The current client can change accepted options,
so verify execution in the game console.

`optimization.cfg` contains the repository's network, input, audio, HUD, and
gameplay values. Resolution and display mode remain in `video.txt`.

Step 34 writes the suite's generated defaults to `optimization.cfg` and appends `exec optimization.cfg` to the user's `autoexec.cfg`. Optional standalone CFGs such as `net_stable`, `debug_hud`, and `audio_lowlatency_025` are copied into the same CS2 `cfg` directory, but they are not auto-executed.

### 3. Launch options - Engine initialization flags

Set in Steam → Library → CS2 → Properties → Launch Options. The repository
prints and copies `-console +exec autoexec` as its minimal example. It does not
write Steam launch options automatically.

---

## Suite video baseline

In the desktop interface, `Auto` selects the `HIGH` preset when an NVIDIA
driver is detected and `MID` otherwise. This is a vendor heuristic, not hardware
performance detection. Select a preset explicitly when that mapping is not
appropriate for the target system.

### Display mode: fullscreen

```
"setting.fullscreen"    "1"
"setting.nowindowborder" "0"
```

The repository selects fullscreen rather than borderless windowed mode. Windows
presentation behavior depends on the Windows build, driver, overlays, and
Fullscreen Optimizations. Phase 1 Steps 4 and 26 apply the repository's related
per-application compatibility values. The repository does not assign a fixed
latency difference to these modes.

### Anti-aliasing

```
"setting.msaa_samples"  "4"  // HIGH and MID tiers
"setting.msaa_samples"  "0"  // LOW tier (use CMAA2 instead)
"setting.r_csgo_cmaa_enable" "0"  // HIGH and MID (MSAA handles AA)
"setting.r_csgo_cmaa_enable" "1"  // LOW tier only
```

The HIGH and MID files select 4x MSAA. LOW disables MSAA and enables CMAA2 to
reduce GPU work. Anti-aliasing cost and frame-time behavior vary by GPU and
resolution, so compare the available modes on the target system.

### Ambient occlusion: off

```
"setting.r_aoproxy_enable"  "0"
```

Ambient occlusion adds contact shading and consumes GPU resources. The suite
disables it to prioritize frame budget over that visual effect. No universal
cost is claimed.

### Shadows: On, Quality Tiered

```
"setting.csm_enabled"               "1"   // Dynamic shadows from map lights
```

Dynamic player shadows can reveal a position before the player model is fully
visible. The suite therefore keeps the shadow system enabled and varies quality
by tier. LOW lowers quality rather than disabling the system.

### Texture Filtering: 16x Anisotropic When FPS Allows

```
"setting.r_texturefilteringquality"  "5"  // 16x AF (HIGH and MID)
"setting.r_texturefilteringquality"  "0"  // Bilinear (LOW only, extreme FPS budget)
```

Anisotropic filtering improves texture detail at oblique viewing angles. HIGH
and MID select 16x. LOW uses bilinear filtering as a GPU-budget choice. Measure
the cost on older or constrained hardware rather than assuming it is zero.

### HDR: Performance Suite Default

```
"setting.sc_hdr_enabled_override"  "3"  // Performance (all tiers)
```

This is the suite's tone-mapping default. Compare Performance and Quality on the
target display; the repository does not claim a fixed performance difference.

### FidelityFX Super Resolution: Off By Default

```
"setting.r_csgo_fsr_upsample"  "0"
```

FSR renders at a lower internal resolution and reconstructs the output. The
suite disables it by default to retain native-resolution image characteristics.
Users who are GPU-limited should compare FSR and explicit lower resolutions for
performance and image quality.

### NVIDIA Reflex

```
"setting.r_low_latency"  "1"  // Enabled (default recommendation)
// Or: launch with -noreflex (contested test path)
```

The suite presents the in-game setting and an optional `-noreflex` comparison
path. It does not claim one state is universal. See
[Known evidence gaps](debunked.md#known-evidence-gaps).

### Resolution and aspect ratio

Lower resolutions reduce the number of rendered pixels, but the resulting frame
rate change depends on whether the workload is GPU-limited. A stretched 4:3
image makes models appear wider on a 16:9 display while reducing horizontal
field of view. Resolution and aspect ratio are user choices; the suite does not
claim one is easier to aim with.

---

## Generated `optimization.cfg` settings

Step 34 generates these values in `optimization.cfg`. The user's
`autoexec.cfg` contains only the bootstrap line added by the suite unless the
user already placed other content there.

### Network (11 CVars)

`rate 1000000` sets the repository's receive-bandwidth value. Servers can impose
their own limits.

`cl_net_buffer_ticks 0`, `cl_net_buffer_ticks_use_interp 1`, and
`cl_tickpacket_desired_queuelength 0` are the suite's stable-route defaults. The
latter two have limited public semantics. Treat them as experimental. The
network condition files adjust these values for comparison.

`cl_interp_ratio 1`, `cl_interp 0`, and `cl_updaterate 128` are legacy controls.
Current Source 2 builds may ignore them. Their presence is not evidence that the
engine applies fixed-rate interpolation.

`mm_dedicated_search_maxping 40` and `mm_session_search_qos_timeout 20` set the
suite's matchmaking thresholds. Raise the maximum ping if matchmaking cannot
find an acceptable server in the user's region.

`cl_timeout 30` sets the disconnect timeout in seconds. The unstable profiles
raise it to 60.

`net_client_steamdatagram_enable_override 1` requests Valve SDR routing. The
resulting route can be better or worse than another available path. Validate it
on the target connection.

### Engine / FPS (5 CVars)

`engine_low_latency_sleep_after_client_tick 1` is an implemented suite default.
Verify that the current client accepts the CVar; the repository does not contain
an isolated timing trace for it.

`engine_no_focus_sleep 0` disables the repository's background sleep request.
This can increase resource use while the game is not focused.

`fps_max 0` removes the in-game FPS cap. If a driver-level or external cap is
used, verify that it is active before relying on this value.

`fps_max_ui 200` and `fps_max_tools 144` limit rendering in menus and tools.

### Gameplay (17 CVars)

`cl_predict_body_shot_fx 0` and `cl_predict_head_shot_fx 0` disable
client-predicted hit effects. The suite uses server-confirmed feedback as a user
preference; it does not claim a performance benefit.

`cl_predict_kill_ragdolls 0` and `cl_disable_ragdolls 1` request fewer ragdoll
effects. The first controls predicted ragdolls and the second controls confirmed
ragdolls. This changes visual presentation.

`cl_sniper_delay_unscope 0` selects the suite's unscope timing preference.

`cl_sniper_show_inaccuracy 0` and
`cl_crosshair_sniper_show_normal_inaccuracy 0` hide the corresponding scope
indicators.

`r_drawtracers_firstperson 0` hides first-person bullet tracers.

`gameinstructor_enable 0` disables tutorial overlays. `con_enable 1` enables the
developer console for mid-session CVar changes.

`cl_autowepswitch 0` disables automatic weapon selection when picking up a new
weapon.

`cl_silencer_mode 0` prevents silencer attachment and detachment through the
normal input path.

`cl_dm_buyrandomweapons 0` disables random weapon selection in Deathmatch.

`cl_join_advertise 2` exposes a joinable community or practice session to
friends.

`lobby_default_privacy_bits2 0` selects the repository's open-lobby default.

`option_duck_method 0` and `option_speed_method 0` select hold-to-crouch and
hold-to-walk. Value `1` selects toggle behavior.

### HUD / QoL (7 CVars)

`cl_compass_enabled 0` hides the compass overlay.

`cl_show_clan_in_death_notice 0` hides clan tags in the kill feed.

`cl_weapon_selection_rarity_color 0` removes the rarity-color presentation from
the weapon-selection HUD. This is a visual preference, not a measured
performance setting.

`cl_use_opens_buy_menu 0` prevents the Use key from opening the buy menu.

`cl_buywheel_nonumberpurchasing 1` prevents number keys from purchasing through
the buy wheel.

`cl_spec_show_bindings 0` hides spectator control hints.

`viewmodel_recoil 0` requests the repository's reduced view-model recoil
presentation.

### Privacy / Anti-distraction (5 CVars)

`cl_invites_only_mainmenu 1` limits invite presentation to the main menu.
`cl_invites_only_friends 1` limits invites to friends.

`cl_embedded_stream_audio_volume 0` mutes embedded event-stream audio in the CS2
interface.

`tv_nochat 1` hides GOTV spectator chat.

`snd_mute_mvp_music_live_players 1` mutes MVP music while any players remain
alive.

### HUD Telemetry (5 CVars)

The repository uses five CS2 telemetry CVars instead of the former `net_graph`
workflow:

```
cl_hud_telemetry_frametime_show 0
cl_hud_telemetry_ping_show 0
cl_hud_telemetry_net_misdelivery_show 0
cl_hud_telemetry_net_quality_graph_show 0
cl_hud_telemetry_serverrecvmargin_graph_show 0
```

All are set to 0 (hidden) in `optimization.cfg`. Show them individually when
diagnosing a specific issue.

For a temporary full diagnostic overlay and console snapshot, use the optional Step 34 CFGs:

```text
exec debug_hud
exec debug_hud_off
```

`debug_hud` enables the telemetry overlay and prints `net_print_sdr_ping_times`, `net_status`, and `cl_ticktiming print detail`. `debug_hud_off` returns telemetry visibility to the suite's quiet defaults.

### Audio Spatial + System (8 CVars)

`speaker_config 1` sets the suite's headphone configuration.

The repository does not write a separate `snd_use_hrtf` toggle because that
name is absent from its checked convar reference.

`snd_headphone_eq 0` selects the Natural EQ option as the suite default.

`snd_spatialize_lerp 0` is the suite's spatial-audio default. Public semantics
are limited, so treat it as listening-dependent.

`snd_mixahead 0.05` is the suite's stability-oriented mixer-buffer default. It
is not described as Valve's default and can have a latency tradeoff.

Step 34 also deploys optional audio CFGs for manual listen-and-benchmark testing:

```text
exec audio_lowlatency_025
exec audio_lowlatency_001
exec audio_stable
```

The low-latency CFGs keep `snd_autodetect_latency 1` and change only `snd_mixahead`. Revert with `exec audio_stable` if you hear crackle, dropouts, delayed cues, missing sounds, or audio/game desync.

`snd_mute_losefocus 0` keeps game audio active when CS2 loses focus.

### Audio Music Muting (8 CVars)

The suite mutes main-menu, round-start, round-end, action, MVP, map-objective,
and death-camera music. It retains the 10-second warning at `0.1`. These are user
preferences rather than vendor recommendations.

### Mouse (4 CVars)

`m_rawinput 1` is retained as a compatibility and intent marker. The current
client may ignore it.

Step 29 also applies the Windows-side acceleration setting. `optimization.cfg` includes
`m_mouseaccel1 0`, `m_mouseaccel2 0`, and `m_customaccel 0` as game-side
compatibility values.

These three compatibility values request disabled game-side mouse acceleration.
The current client may ignore them.

### Video (3 CVars)

`r_player_visibility_mode 1` enables Boost Player Contrast as a readability
preference. No frame-performance result is claimed.

`r_fullscreen_gamma 2.2` is the suite baseline. Adjust it for the target display
and viewing conditions.

`mat_monitorgamma_tv_enabled 0` disables the TV gamma mode. Confirm the result
on the target display.

---

## Launch Options

```
-console +exec autoexec
```

`-console` requests the developer console at launch. The generated
`optimization.cfg` also contains `con_enable 1`.

`+exec autoexec` requests execution of `autoexec.cfg` when the game launches.

### -noreflex (optional)

```
-console -noreflex +exec autoexec
```

This requests that NVIDIA Reflex be disabled. It is an optional comparison path,
not the suite default. See [Known evidence gaps](debunked.md#known-evidence-gaps).

### What we deliberately excluded

`-novid` is legacy guidance and is not included by the suite.

`-threads N` manually forces thread-pool behavior and is not included by the
suite.

`-tickrate 128` is not used because Source 2 subtick input processing is not
configured through this legacy launch option.

`-nojoy` is not included because the repository has no evidence of a useful
effect and it can remove controller support.

`-high` is not included. The suite uses persistent IFEO PerfOptions in Step 10
instead of a launch flag.

`-softparticlesdefaultoff` is a Source 1-era launch option. The repository omits
it and does not claim a current CS2 effect.

---

## Thread Pool Policy

The suite does not add `thread_pool_option`. Leave Source 2 thread scheduling at
its default unless a controlled test on the target CPU justifies an override.

---

## How to Verify Your Configuration

### Checking autoexec is loaded

Open the CS2 console and type:
```
m_rawinput
```
Compare the returned value with the checked configuration. If it differs, check
that launch options contain `+exec autoexec`, `autoexec.cfg` contains
`exec optimization.cfg`, and both files exist at the expected path. A current
client may ignore legacy CVars, so also inspect console warnings.

### Checking headphone-mode audio settings

```
speaker_config
```
Should return `speaker_config = 1` (Headphones). You can also inspect:
```
snd_spatialize_lerp
snd_headphone_eq
snd_mixahead
```
to confirm the repo's current suite defaults were written.

### Checking video.txt is being read

In the CS2 video settings menu, your settings should match what's in `video.txt`. If they don't, the file may not have been read - ensure CS2 was fully closed before the file was written.

To find your video.txt path:
```powershell
(Get-ItemProperty "HKCU:\Software\Valve\Steam" -Name "SteamPath").SteamPath
# Then: <SteamPath>\userdata\<SteamID>\730\local\cfg\video.txt
```
