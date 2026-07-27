# Audio Configuration Deep Dive

> Covers Phase 1 Step 33 and the audio CVars in Phase 1 Step 34 (`$CFG_CS2_Autoexec`).

CS2 exposes Steam Audio-related console variables and uses the Windows audio
stack for output. The repository configures a Windows audio preference in Step
33 and writes CS2 audio CVars in Step 34.

## Evidence boundary

The repository does not contain controlled listening tests, device-latency
captures, or audio-thread traces. The settings below are implemented suite
defaults and user preferences, not demonstrated universal competitive
improvements. Audio behavior depends on the game build, output device, Windows
driver, sample rate, headphones, and listener. Test changes on the target setup
and restore the default if they introduce dropout, desynchronization, or poorer
positional cues.

---

## The Headphone Spatial Baseline

Head-Related Transfer Function (HRTF) is the mechanism by which the audio engine simulates 3D sound positioning using stereo headphones. Instead of simple left/right panning, HRTF applies per-frequency filtering that mimics how sound waves interact with the shape of a human head and ears, creating the perception of sounds coming from specific directions including above, below, and behind.

The suite keeps a headphone-focused spatial baseline. Treat it as a starting
point to evaluate, not a Valve requirement.

### Suite baseline

### `speaker_config "1"`

The repository uses value `1` for its headphone configuration.

The repository does not write a separate `snd_use_hrtf` toggle because that
name is not present in its checked convar reference.

### `snd_spatialize_lerp "0"`

The repository uses value `0` for its headphone-focused spatial baseline. Public
documentation for the exact runtime effect is limited, and community guidance
differs. Treat this as a listening-dependent, user-tunable choice.

### `snd_steamaudio_enable_perspective_correction "1"`

The repository enables the named perspective-correction feature as part of its
headphone baseline. It has no committed listening comparison in this repository.

---

## `snd_headphone_eq`

The headphone EQ setting applies a frequency response curve to the final audio output before it reaches WASAPI.

| Value | Name | Effect |
|-------|------|--------|
| `0` | Natural | Repository default. |
| `1` | Crisp | Alternate in-game EQ choice. |

The suite defaults to Natural. Change the value to `1` in `config.env.ps1` if
the alternate EQ is clearer or more comfortable on the target headphones.

---

## `snd_mixahead`

This value controls audio mixer buffering. Smaller values can reduce buffering
but leave less tolerance for scheduling or device delays.

The suite uses `0.05` as its stability-oriented default. It does not claim that
this value has no latency cost or that a lower value is safe on every device.
The optional files below allow a controlled local comparison without changing
the generated default.

### Experimental low audio latency CFGs

Step 34 also deploys three optional audio CFGs to `game\csgo\cfg\`. They are not executed automatically and do not change the generated `optimization.cfg`.

| CFG | Settings | Use |
|-----|----------|-----|
| `exec audio_stable` | `snd_autodetect_latency "1"`, `snd_mixahead "0.05"` | Suite default and reset path |
| `exec audio_lowlatency_025` | `snd_autodetect_latency "1"`, `snd_mixahead "0.025"` | Moderate latency experiment |
| `exec audio_lowlatency_001` | `snd_autodetect_latency "1"`, `snd_mixahead "0.001"` | Aggressive latency experiment |

`snd_autodetect_latency "1"` stays enabled in all three files so CS2 can continue tracking device/output latency while the experiment changes only the mixer buffer. This keeps the test focused: if behavior changes, the likely variable is `snd_mixahead`, not a disabled engine detection path.

Use the lower-buffer CFGs only as listen-and-benchmark experiments. Run a full map or deathmatch session and watch for crackle, dropouts, delayed cues, missing sounds, or audio/game desync. If any appear, revert immediately:

```text
exec audio_stable
```

The checked CVar reference exposes `snd_steamaudio_enable_reverb`. The suite
leaves it off as a listening preference. Reverb-level CVars such as
`snd_steamaudio_reverb_level_db` are not included in the optional low-latency
files because the suite does not enable reverb.

---

## Music Muting

Eight CVars set repository music-volume preferences:

| CVar | Value | What it mutes |
|------|-------|---------------|
| `snd_menumusic_volume` | `0` | Main menu music |
| `snd_roundstart_volume` | `0` | Round start sting |
| `snd_roundend_volume` | `0` | Round end music |
| `snd_roundaction_volume` | `0` | Action phase music |
| `snd_mvp_volume` | `0` | MVP music |
| `snd_mapobjective_volume` | `0` | Map objective music |
| `snd_tensecondwarning_volume` | `0.1` | 10-second bomb timer warning |
| `snd_deathcamera_volume` | `0` | Death camera music |

The 10-second bomb timer warning remains at `0.1` because it can convey round
timing without requiring the player to look at the HUD. Other music-volume
choices are preferences and can be changed.

---

## Windows Audio - Audio Ducking Disable

`UserDuckingPreference = 3` in `HKCU:\Software\Microsoft\Multimedia\Audio`

Windows Communications audio ducking can lower other audio streams when a
communications application is active.

Value `3` selects Do Nothing and disables this automatic ducking behavior.

This is a Windows system setting, not a CS2 CVar. It's applied in Step 33 rather than Step 34.

---

## Voice CVars

`voice_always_sample_mic "1"` requests continuous microphone sampling. This can
avoid capture initialization when push-to-talk begins, but the repository does
not contain a latency measurement for the setting.

`snd_voipvolume "0.5"` sets incoming voice chat volume to 50 percent. Adjust it
for the target output device and team-chat mix.

---

## `snd_mute_losefocus`

`snd_mute_losefocus "0"` keeps audio playing when CS2 loses focus. This is a user
preference and can expose game audio while another application is active.

---

## Exclusive mode

WASAPI exclusive mode can prevent other applications from sharing the output
device. The suite does not force it because CS2, voice chat, and system audio
commonly need to coexist, and the repository has no device-specific latency
measurements that justify changing the system-wide behavior.
