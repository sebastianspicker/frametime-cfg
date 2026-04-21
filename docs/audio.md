# Audio Optimization — Deep Dive

> Covers Phase 1 Step 33 and the audio CVars in Phase 1 Step 34 (`$CFG_CS2_Autoexec`).

CS2's audio is a Steam Audio-based pipeline running on top of Windows WASAPI. The optimization path has two layers: the Windows audio subsystem configuration (Step 33), and the CS2-side CVars written to autoexec.cfg (Step 34).

---

## The Headphone Spatial Baseline

Head-Related Transfer Function (HRTF) is the mechanism by which the audio engine simulates 3D sound positioning using stereo headphones. Instead of simple left/right panning, HRTF applies per-frequency filtering that mimics how sound waves interact with the shape of a human head and ears, creating the perception of sounds coming from specific directions including above, below, and behind.

For competitive CS2, accurate directional audio — hearing whether footsteps are above you, in front, or behind — is directly relevant to gameplay. HRTF is not a "premium feature"; it is the correct mode for headphone users.

### The current suite baseline

**1. `speaker_config "1"` — Headphones mode**

`speaker_config` tells the mixer which speaker configuration to target. Value `1` is Headphones (2-channel stereo). The suite keeps this as the current headphone-focused baseline.

The repo no longer documents a separate `snd_use_hrtf` toggle as an active CS2 CVar, because it does not appear in the current public convar dump.

**2. `snd_spatialize_lerp "0"` — Suite default**

Controls how quickly the HRTF filter updates as a sound source moves. Value `0.5` (default) interpolates between HRTF positions over time, smoothing transitions but introducing a subtle smearing of the 3D position during movement. Value `0` snaps to the correct HRTF position immediately.

The suite uses `0` as its current headphone-focused spatial default. Treat this as a community-preferred setting rather than as a Valve requirement.

62.5% of professional CS2 players use `snd_spatialize_lerp 0` (source: esportfire.com pro settings study, 2026).

**3. `snd_steamaudio_enable_perspective_correction "1"`**

Enables perspective correction in the Steam Audio engine — adjusts HRTF filtering based on listener head orientation rather than using a fixed forward-facing response. This makes sounds that pass across the listener's field of view track more accurately.

---

## `snd_headphone_eq`

The headphone EQ setting applies a frequency response curve to the final audio output before it reaches WASAPI.

| Value | Name | Effect |
|-------|------|--------|
| `0` | Natural | Flat curve — no EQ applied. Output is what Steam Audio intended. |
| `1` | Crisp | Boosts high frequencies (footstep transients, door sounds) and slightly reduces bass. |

**Which to choose:** 62.5% of pro players use Natural (0), 37.5% use Crisp (1), per the 2026 esportfire.com study. Crisp is preferred by players who want footsteps more prominent in the mix. Natural is preferred by players who find Crisp causes ear fatigue in long sessions. The suite defaults to Natural (0) — change to `1` in `config.env.ps1` if preferred.

---

## `snd_mixahead`

Controls how many seconds ahead the audio mixer prepares audio buffers. This is the audio equivalent of a frame buffer — it trades latency for stability.

**Why the suite default is `"0.05"` and not lower:**

The minimum value of `0.001` (1ms) was previously recommended by some guides. At 1ms, the audio buffer contains approximately 48 samples at 48 kHz. The Windows scheduler's minimum quantum on a 15.6ms timer (default timer resolution) is larger than 1ms — meaning the audio buffer can be exhausted before the next scheduler tick allows the audio thread to refill it, causing dropout (clicking, crackling) under any scheduling pressure (Windows Update activity, antivirus scan, brief CPU spike).

After Step 28, the registry setting only allows applications to request a lower timer resolution; it does not force a permanent global 1ms timer. The suite still uses `0.05` (50ms) as a conservative, community-preferred default because it is tolerant of CPU scheduling jitter.

The 50ms buffer does not affect the timing of sound events relative to gameplay. The audio thread keeps the buffer filled and the sound engine submits events to the buffer in real-time. What you hear is current; the buffer is a safety margin, not a delay.

---

## Music Muting

Eight CVars mute music that provides no competitive information and consumes audio processing capacity:

| CVar | Value | What it mutes |
|------|-------|---------------|
| `snd_menu_music_volume` | `0` | Main menu music |
| `snd_roundstart_volume` | `0` | Round start sting |
| `snd_roundend_volume` | `0` | Round end music |
| `snd_roundaction_volume` | `0` | Action phase music |
| `snd_mvp_volume` | `0` | MVP music |
| `snd_mapobjective_volume` | `0` | Map objective music |
| `snd_tensecondwarning_volume` | `0.1` | 10-second bomb timer warning |
| `snd_deathcamera_volume` | `0` | Death camera music |

The 10-second bomb timer warning is intentionally kept at `0.1` (not `0`). The audio cue at 10 seconds is an audible tactical cue — hearing it while looking at a different part of the map is relevant game information. All other music provides no tactical information.

---

## Windows Audio — Audio Ducking Disable

`UserDuckingPreference = 3` in `HKCU:\Software\Microsoft\Multimedia\Audio`

Windows "Communications" audio ducking automatically lowers other audio streams when a communication application (Discord, Teams, Windows voice call) is detected as active. This causes game audio to drop by ~50% when Discord voice is active during CS2 play.

Value `3` = Do Nothing — disables automatic ducking entirely. CS2 audio plays at full volume regardless of communication app state.

This is a Windows system setting, not a CS2 CVar. It's applied in Step 33 rather than Step 34.

---

## Voice CVars

**`voice_always_sample_mic "1"`** — Keeps the microphone pre-sampled at all times. Without this, activating the microphone has a brief startup latency while the audio engine initializes the capture path. With this enabled, the capture path is always warm, eliminating the startup delay.

**`snd_voipvolume "0.5"`** — Sets incoming voice chat volume to 50%. The default is often too loud relative to game audio in competitive scenarios. Adjust to taste.

---

## `snd_mute_losefocus`

**`snd_mute_losefocus "0"`** — By default, CS2 mutes audio when the game window loses focus. Setting `0` keeps audio playing. This is relevant during alt-tab moments (checking Discord, browser) — you continue hearing the round's audio, including countdown timers and bomb plants.

---

## Exclusive Mode — Why the Suite Does Not Force It

Windows WASAPI exclusive mode gives the audio application direct hardware access, bypassing the Windows Audio Session API mixer. In theory, lower latency. In practice, for CS2:

- Exclusive mode conflicts with other audio applications (Discord, system sounds)
- Steam Audio's HRTF processing already runs in the driver's shared mode path efficiently
- The latency benefit (1–3ms) is below the threshold of human audio-visual perception in gameplay

The suite does not force exclusive mode. It focuses on `snd_mixahead` (chosen buffer size), the current headphone/spatial defaults, and audio ducking (correct volume behavior).
