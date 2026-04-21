# Video Settings, Autoexec, and Launch Options — Deep Dive

> Covers Phase 1 Step 34, Phase 3 Steps 4–6, and `docs/video.txt`.

CS2 has three distinct configuration systems that interact in non-obvious ways. Understanding what each system controls — and why they're separate — prevents confusion when settings from one system appear to be ignored by another.

---

## The Three Configuration Systems

### 1. `video.txt` — Graphics pipeline settings

Located at `<Steam>\userdata\<SteamID>\730\local\cfg\video.txt`.

This file is written by the CS2 engine when you change anything in the in-game Video settings menu. It stores settings that require GPU pipeline reconfiguration to change — display mode, resolution, MSAA sample count, shadow quality, texture filtering, HDR mode.

**What you cannot control here:** Anything that can be changed mid-session via console commands (mostly shader-level settings and HUD toggles).

**Important:** CS2 overwrites this file when you click "Apply" in the video settings menu. If you edit it manually, make sure CS2 is fully closed before editing, and don't click Apply in the menu afterward without updating your file.

### 2. `autoexec.cfg` — Console variable overrides

Located at `<Steam>\userdata\<SteamID>\730\local\cfg\autoexec.cfg`.

Executed by the engine via `+exec autoexec` in launch options. This is CS2's scripting layer — every setting here is a console variable that can be changed at runtime. The autoexec runs on every map load.

**What belongs here:** Anything that can be set via the console — network settings, input settings, audio settings, HUD settings, gameplay preferences. Resolution and display mode do not belong here (they live in `video.txt`).

### 3. Launch options — Engine initialization flags

Set in Steam → Library → CS2 → Properties → Launch Options.

These flags are processed by the engine before any config files load. Use them for things that cannot be changed after engine initialization — like disabling the intro video, enabling the developer console, or toggling NVIDIA Reflex.

---

## Video Settings — 2026 Competitive Baseline

Based on: ThourCS2 benchmarks (driver 581.08), prosettings.net aggregation of 866 professional CS2 players, and Blur Busters forum testing.

### Display Mode: Fullscreen (Exclusive)

```
"setting.fullscreen"    "1"
"setting.nowindowborder" "0"
```

The fundamental choice in CS2: exclusive fullscreen vs. borderless windowed (which Windows presents as fullscreen optimization "on"). The difference is the Desktop Window Manager.

In **borderless windowed mode**, your game frames pass through DWM (Desktop Window Manager) before being presented to the display. DWM composites the game frame together with your taskbar, notification overlays, and Alt-Tab thumbnails. This compositing adds 1–3ms latency and prevents the GPU from maintaining exclusive access to the display's frame buffer.

In **exclusive fullscreen**, the game bypasses DWM entirely. The GPU writes directly to the display's front buffer. No compositing overhead, no DWM latency.

**Phase 1 Step 4** (`DisableFullscreenOptimizations`) reinforces this. Windows 10/11 introduced "fullscreen optimizations" that silently convert exclusive fullscreen requests into borderless windowed mode under certain conditions (when overlays are detected, when certain window event hooks are registered). The Step 4 registry change tells Windows to respect the game's exclusive fullscreen request and not convert it.

**Step 26** (`GameConfigStore FSE`) adds a second registry entry that configures the fullscreen exclusive state from the GameConfigStore side, belt-and-suspenders with Step 4.

### MSAA — 4x is Better Than None

```
"setting.msaa_samples"  "4"  // HIGH and MID tiers
"setting.msaa_samples"  "0"  // LOW tier (use CMAA2 instead)
"setting.r_csgo_cmaa_enable" "0"  // HIGH and MID (MSAA handles AA)
"setting.r_csgo_cmaa_enable" "1"  // LOW tier only
```

Counter-intuitive finding from some community benchmarks: 4x MSAA can produce better 1% lows than no AA on some systems.

The explanation: without anti-aliasing, the rasterizer must render more complex geometry per-pixel at edges (sub-pixel accuracy issues cause more branching in edge shaders). MSAA stabilizes this by resolving coverage at 4 samples per pixel — the rasterization pipeline becomes more predictable, reducing per-frame GPU work variance. The result is more consistent frametimes.

**8x MSAA** costs ~18% additional average FPS without meaningful visual improvement over 4x. Not recommended for competitive.

**CMAA2** (Conservative Morphological Anti-Aliasing 2, Intel's algorithm) is CS2's "free" AA option: post-process edge smoothing with near-zero GPU cost. On LOW tier (where MSAA is disabled for FPS budget reasons), CMAA2 provides some visual quality improvement without FPS cost.

### Ambient Occlusion: Always Off

```
"setting.r_aoproxy_enable"  "0"
```

AO adds realistic contact shadowing where objects meet surfaces. In CS2 this costs 10–15% GPU time (ThourCS2 measurement). The visual improvement is entirely cosmetic — AO on floors and walls provides no competitive information. Turn it off.

### Shadows: On, Medium Quality

```
"setting.csm_enabled"               "1"   // Dynamic shadows from map lights
"setting.shadow_quality_very_high"  "0"   // NOT high quality
```

The one setting where "lower = better" is incorrect for competitive play:

**Why shadows matter competitively:** Player shadows cast through doorways and onto floor surfaces reveal enemy positions before the player model becomes visible. A player peeking through a doorway with a light source behind them casts their shadow first. Disabling shadows removes this information.

**Why not High quality:** High shadow quality enables cascaded shadow maps with higher sample counts — visually prettier, but Shadow Quality = Medium already enables player shadows without the full cascade. Medium is the sweet spot.

**Why LOW tier doesn't use Low:** Even on LOW tier, player shadows are enabled. Removing them is a competitive disadvantage that outweighs the minor FPS gain.

### Texture Filtering: Always 16x Anisotropic

```
"setting.r_texturefilteringquality"  "5"  // 16x AF (HIGH and MID)
"setting.r_texturefilteringquality"  "0"  // Bilinear (LOW only, extreme FPS budget)
```

Since NVIDIA Pascal (2016) and AMD GCN 3rd gen (2015), the hardware anisotropic filtering unit runs essentially for free. The GPU has a dedicated AF unit that operates in parallel with the main rendering pipeline — it does not consume shader execution units. The FPS difference between 16x AF and bilinear is less than 1% on any GPU from the last decade.

What 16x AF does: sharpens textures viewed at oblique angles (floors, walls, the ground between cover). Without AF, floors and walls become blurry at shallow viewing angles — blurry surfaces where enemies might be hiding.

### HDR: Performance for All Tiers

```
"setting.sc_hdr_enabled_override"  "3"  // Performance (all tiers)
```

CS2's HDR setting is a tone-mapping preference, not a resolution or sample count change. The FPS difference between Quality and Performance is less than 1%.

**Why Performance, not Quality?** This is the suite's current default based on community benchmarking and player preference. Some benchmarkers report that HDR Quality washes out high-contrast lit areas, but this is not a Valve-documented competitive rule. Benchmark and compare visually on your own display.

### FidelityFX Super Resolution: Always Off

```
"setting.r_csgo_fsr_upsample"  "0"
```

FSR in CS2 upscales from a lower internal resolution using temporal reconstruction. The reconstruction algorithm blurs thin geometry — enemy model edges at distance, crosshairs, thin cover objects. The blurring is most pronounced at movement, which is exactly when you're aiming at moving enemies.

If you need more FPS, lower the resolution explicitly rather than using FSR. Explicit lower resolution is predictable; FSR-induced blur is not.

### NVIDIA Reflex: See the Dedicated Section

```
"setting.r_low_latency"  "1"  // Enabled (default recommendation)
// Or: launch with -noreflex (community meta — contested)
```

See [The Reflex Controversy](../README.md#the-reflex-controversy). The suite presents both options and lets you choose. Benchmark both configurations on your system.

### Resolution: The Largest Single FPS Factor

Resolution has more FPS impact than all other video settings combined. The relationship is roughly quadratic — halving the linear resolution approximately quadruples the pixel throughput reduction.

| Resolution | Relative Pixels | Typical FPS vs. 1080p |
|------------|----------------|----------------------|
| 1920×1080 | 100% (baseline) | Baseline |
| 1280×960 (4:3) | 59% | +40–60% |
| 1024×768 (4:3) | 38% | +60–80% |
| 1280×1080 (stretched 4:3) | 66% | +30–50% |

**Why pros use 4:3 stretched:** Stretching a 4:3 resolution to a 16:9 display makes character models appear wider — their horizontal hit box (what the player perceives) is larger relative to their height. Hitting enemies is geometrically easier when the target is wider. 80% of professional CS2 players use stretched 4:3 (prosettings.net aggregation).

The tradeoff: reduced horizontal FOV (you see less of the scene on the sides). This is a skill-weighted tradeoff — players who prioritize aim over awareness prefer 4:3 stretched.

---

## Autoexec.cfg — 73 CVars (10 Categories)

The autoexec is generated and written by Step 34. Here's the rationale for each category.

### Network (11 CVars)

**`rate 1000000`** — Maximum network bandwidth for receiving game state updates. CS2 servers send denser updates than CS:GO — this removes any artificial bandwidth cap.

**`cl_net_buffer_ticks 0`**, **`cl_net_buffer_ticks_use_interp 1`**, **`cl_tickpacket_desired_queuelength 0`** — Minimize network-side buffering. `cl_net_buffer_ticks 0` disables the receive tick buffer (immediate rendering). `cl_net_buffer_ticks_use_interp 1` ties any remaining buffering to the interpolation window. `cl_tickpacket_desired_queuelength 0` removes the packet queue target. For unstable connections, the `cfgs/` network condition configs adjust these values.

**`cl_interp_ratio 1`**, **`cl_interp 0`**, **`cl_updaterate 128`** — Deprecated in CS2 Source 2. The sub-tick system handles interpolation differently; `cl_net_buffer_ticks` is the actual control. These are kept as harmless belt-and-suspenders (silently ignored by the engine but cause no harm if set).

**`mm_dedicated_search_maxping 80`** and **`mm_session_search_qos_timeout 20`** — Maximum matchmaking ping and QoS probe timeout. 80ms works for most regions (EU players can lower to 40; SEA players may need 80–150). Value 40 used by some guides is too aggressive in low-server-density regions.

**`cl_timeout 30`** — Seconds before the client disconnects from an unresponsive server. Default is appropriate for stable connections; the `cfgs/net_unstable` and `cfgs/net_bad` configs raise this to 60.

**`net_client_steamdatagram_enable_override 1`** — Routes game traffic through Valve's SDR (Steam Datagram Relay) backbone, bypassing congested ISP routes. Measurably improves delivery for players with poor direct routing (visible as ping variance, not just absolute ping). Neutral on already-optimal routes.

### Engine / FPS (5 CVars)

**`engine_low_latency_sleep_after_client_tick 1`** — Tells the engine to sleep immediately after processing a client tick, rather than spinning the CPU. This allows the CPU to service other threads (audio, OS tasks) in the gap between ticks, reducing the chance of frametime spikes from CPU resource contention.

**`engine_no_focus_sleep 0`** — When CS2 is not the foreground window, it normally throttles to 60 FPS. Setting 0 disables this throttle entirely, so the game loop continues at full speed even when Alt-Tabbed. Useful if you switch tabs to look up callouts and want the game to keep running.

**`fps_max 0`** — Removes the in-game FPS cap. Your actual cap comes from the NVCP frame rate limiter, which provides lower latency and more consistent frametimes than the in-game limiter. Set this after NVCP cap is configured.

**`fps_max_ui 200`** and **`fps_max_tools 144`** — Cap the FPS in menus and tools/editor modes respectively. The main menu doesn't need 400+ FPS and it reduces GPU/power noise while navigating settings.

### Gameplay (17 CVars)

**`cl_predict_body_shot_fx 0`** and **`cl_predict_head_shot_fx 0`** — Disable client-side hit effect prediction. Per a 120-player study by ThourCS2 (2025), 95% of professionals keep these off — predicted effects (rendered before server confirmation) cause phantom dink visuals that trigger fatal target-switching errors. Value `1` (on) is the CS2 default and was previously recommended; the community reversed course based on this data.

**`cl_predict_kill_ragdolls 0`** and **`cl_disable_ragdolls 1`** — Disable ragdoll simulation entirely. Ragdoll physics are CPU-intensive mid-firefight; corpse positions provide no actionable information after a kill. The first CVar disables predicted ragdolls; the second disables them on server confirmation too.

**`cl_sniper_delay_unscope 0`** — Removes the delay before the AWP scope clears after firing. Faster visual feedback on bolt-action sniper mechanics. Standard competitive setting.

**`cl_sniper_show_inaccuracy 0`** and **`cl_crosshair_sniper_show_normal_inaccuracy 0`** — Disable scope bloom indicator and standing inaccuracy blur overlay. These visual cues train you to wait for the indicator rather than trusting timing; experienced players track shot timing internally.

**`r_drawtracers_firstperson 0`** — Hides bullet tracers in first-person view. Tracers can briefly obscure the target model during rapid fire.

**`gameinstructor_enable 0`** and **`con_enable 1`** — Disables tutorial overlays; enables the developer console (allows mid-session CVar changes without relaunch).

**`cl_autowepswitch 0`** — Disables automatic weapon switch when picking up a new weapon. Picking up a weapon during a firefight should not interrupt your current action without explicit input.

**`cl_silencer_mode 0`** — Prevents accidental silencer attachment/detachment on the M4A1-S and USP-S. The detach animation costs ~1 second; `0` locks the silencer state.

**`cl_dm_buyrandomweapons 0`** — In Deathmatch mode, lets you choose your weapon rather than receiving a random one.

**`cl_join_advertise 2`** — Allows friends to see and join your server (community/practice servers).

**`lobby_default_privacy_bits2 0`** — Sets lobby to Open (friends can join), preventing accidental private lobby lock.

**`option_duck_method 0`** and **`option_speed_method 0`** — Crouch and walk in toggle mode rather than hold mode.

### HUD / QoL (7 CVars)

**`cl_compass_enabled 0`** — Hides the compass overlay. The minimap provides the same directional information; the compass is visual noise.

**`cl_show_clan_in_death_notice 0`** — Removes clan tag from the kill feed. Cleaner read during multi-kill sequences.

**`cl_weapon_selection_rarity_color 0`** — Removes skin rarity color glow from the weapon selection HUD. Cosmetic noise; removes a frame of visual processing on weapon switch.

**`cl_use_opens_buy_menu 0`** — Prevents the Use key (`E`) from opening the buy menu. Eliminates accidental buy menu triggers during movement near the spawn zone.

**`cl_buywheel_nonumberpurchasing 1`** — Prevents number keys from purchasing in the buy zone. Avoids unintended purchases when pressing key binds near a buy zone.

**`cl_spec_show_bindings 0`** — Hides the spectator control hint overlay when dead. Removes a persistent distraction during death camera.

**`viewmodel_recoil 0`** — Disables weapon kick animation during spray. The animation has no gameplay function; removing it reduces visual distraction during full-auto fire.

### Privacy / Anti-distraction (5 CVars)

**`cl_invites_only_mainmenu 1`** and **`cl_invites_only_friends 1`** — Block invite popups during active matches and limit invites to friends only. Popup overlays can appear mid-round and cause visual/input disruption.

**`cl_embedded_stream_audio_volume 0`** — Mutes embedded Twitch/event stream audio in the CS2 overlay. Zero competitive use.

**`tv_nochat 1`** — Mutes GOTV spectator chat, which can appear in the chat window during streamed matches.

**`snd_mute_mvp_music_live_players 1`** — Mutes MVP music while any players are still alive. The MVP anthem in competitive contexts plays while the round transitions; muting it keeps audio focused on live-round information.

### HUD Telemetry (5 CVars)

CS2 removed `net_graph` from CS:GO. The replacement is five separate CVars:

```
cl_hud_telemetry_frametime_show 0
cl_hud_telemetry_ping_show 0
cl_hud_telemetry_net_misdelivery_show 0
cl_hud_telemetry_net_quality_graph_show 0
cl_hud_telemetry_serverrecvmargin_graph_show 0
```

All set to 0 (hidden) in the autoexec. Show them individually if you're diagnosing specific issues.

### Audio Spatial + System (8 CVars)

**`speaker_config 1`** — Sets the speaker configuration to Headphones. The suite treats this as the current headphone-focused spatial baseline.

The repo no longer documents a separate `snd_use_hrtf` toggle as an active CS2 CVar, because it does not appear in the current public convar dump. Older guidance in the CS2 community often referenced it, but this repo now aligns to the currently visible convar surface.

**`snd_headphone_eq 0`** — Sets the EQ preset to "Natural" (0). Current pro-settings aggregation trends still lean Natural over Crisp, but this is a community preference rather than a Valve recommendation.

**`snd_spatialize_lerp 0`** — Controls interpolation smoothness for spatialized audio positions. The suite uses `0` as its current headphone-focused spatial default. Treat this as benchmark/listening-dependent, not as a hard competitive law.

**`snd_mixahead 0.05`** — Audio buffer size in seconds (50ms). The suite uses this as a conservative, stability-oriented default. Some guides push much lower values, but those can be less tolerant of scheduling jitter. The current public convar dump shows a much lower engine default than older guides documented, so this repo now treats `0.05` as a suite choice, not as the Valve default.

**`snd_mute_losefocus 0`** — Keeps audio playing when CS2 is not the foreground window. Useful for hearing game sounds while Alt-Tabbed to a reference image or map.

### Audio Music Muting (8 CVars)

All music categories are set to volume 0 — main menu, round start, round end, action music, MVP anthem, map objective music, death camera — except the 10-second warning which is kept at 0.1 (audible but not obnoxious). This is a widely used competitive preference, not a vendor-documented requirement.

### Mouse (4 CVars)

**`m_rawinput 1`** — Kept in the generated config as a harmless documentation and forward-compatibility stub. Current CS2 builds already force raw input on, so this line is effectively a no-op today.

The real active protection is the combination of Step 29's Windows-level acceleration disable plus **`m_mouseaccel1 0`**, **`m_mouseaccel2 0`**, and **`m_customaccel 0`** inside CS2. `m_rawinput 1` stays in the file mainly so the intent is explicit if Valve ever changes that behavior again.

**`m_mouseaccel1 0`**, **`m_mouseaccel2 0`**, **`m_customaccel 0`** — Belt-and-suspenders mouse acceleration disables within CS2's own input processing (separate from Windows-level settings).

### Video (3 CVars)

**`r_player_visibility_mode 1`** — Enables "Boost Player Contrast." This is the suite's current default because the CVar exists, the feature is real, and many players prefer it for readability. Any claim that it improves 1% lows should be treated as benchmark-dependent rather than settled fact.

**`r_fullscreen_gamma 2.2`** — Standard gamma for most monitors (sRGB). If your monitor is calibrated to a different gamma, adjust this value.

**`mat_monitorgamma_tv_enabled 0`** — Disables TV gamma correction (which maps to ~2.2 but with black crush). On PC monitors, this should be 0.

---

## Launch Options

```
-console +exec autoexec
```

**`-console`** — Enables the developer console at launch (same as `con_enable 1` in autoexec, but also enables the console before any config loads).

**`+exec autoexec`** — Executes your autoexec.cfg on game launch. This is how the 73 CVars above take effect.

### -noreflex (optional)

```
-console -noreflex +exec autoexec
```

This disables NVIDIA Reflex entirely at the engine level. See [The Reflex Controversy](../README.md#the-reflex-controversy) for the full debate. The suite asks you to choose — it is not the default.

### What we deliberately excluded

**`-novid`** — No-op in CS2. There is no intro video to skip. The flag is parsed and silently ignored.

**`-threads N`** — Valve explicitly warns against this. Source 2 manages its own thread pool. Manually specifying thread count can cause instability, crashes, or performance reduction. Removed from older versions of this guide.

**`-tickrate 128`** — CS2's sub-tick architecture means fixed tickrate parameters are silently ignored. The flag is parsed and discarded.

**`-nojoy`** — Removes joystick support. In CS2, the joystick subsystem is minimal. Any freed RAM is single-digit MB. No measurable impact.

**`-high`** — Sets process priority to High but resets to Normal on exit. IFEO PerfOptions (Step 10) is strictly superior: applied at kernel level at process creation, persistent across every launch.

**`-softparticlesdefaultoff`** — Source 1 launch option. Not parsed by CS2's Source 2 engine.

---

## The Intel Hybrid Thread Pool Exception

For Intel 12th gen+ CPUs (Alder Lake, Raptor Lake), the autoexec generator adds one additional CVar:

```
thread_pool_option 2
```

This remains a suite heuristic for Intel hybrid CPUs, not a Valve-documented recommendation. It is auto-detected by matching `Win32_Processor.Name` against `1[2-9]\d{3}[A-Z]` (12th-19th gen series).

---

## How to Verify Your Configuration

### Checking autoexec is loaded

Open the CS2 console and type:
```
m_rawinput
```
Should return `m_rawinput = 1`. If it returns 0, your autoexec isn't executing. Check that launch options contain `+exec autoexec` and that the file exists at the correct path.

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

In the CS2 video settings menu, your settings should match what's in `video.txt`. If they don't, the file may not have been read — ensure CS2 was fully closed before the file was written.

To find your video.txt path:
```powershell
(Get-ItemProperty "HKCU:\Software\Valve\Steam" -Name "SteamPath").SteamPath
# Then: <SteamPath>\userdata\<SteamID>\730\local\cfg\video.txt
```
