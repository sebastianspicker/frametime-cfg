# NVIDIA DRS Settings

This document covers the native P1:20 preparation receipt and P3:4 DRS
transaction. The Windows adapter loads the pinned public NVAPI interface from
the absolute System32 driver DLL, binds one exact NVIDIA display identity,
captures the original profile settings and application owners before save, and
requires a reload with exact readback. Restore rejects identity drift and
removes only suite-created bindings or profile state.

The alpha default writes 42 DWORD settings through NVIDIA DRS via
`nvapi64.dll`. The table is limited to identifiers named by public NVAPI or
NVIDIA Profile Inspector references used by the project. Eight partially
decoded entries and two unknown entries from the earlier development profile
were removed from the alpha default because their semantics could not be
supported from public references.

Three additional settings remain deliberately excluded; see
[Excluded settings](#excluded-settings). The native DRS transaction performs no
GPU class-key registry writes.

## Evidence boundary

This table records requested setting IDs and values. A successful DRS write does
not prove that a driver branch stores, honors, or benefits from the value for
CS2. NVIDIA can change setting availability and interpretation between drivers.
The repository contains no committed per-setting performance dataset. Verify
the resulting profile with a public inspection tool and compare behavior on the
target driver.

---

## Full Settings Table

### Power & Performance

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Power management mode | `274197361` | `1` (Prefer Max Performance) | Requests the named driver power-management policy. Effective clocks remain workload, power, temperature, and driver controlled. |
| Maximum pre-rendered frames | `8102046` | `1` | Requests a one-frame pre-render limit where the driver and API use this setting. |
| Threaded optimization | `549528094` | `1` (Force ON) | Requests the named threaded-optimization policy. Effect depends on graphics API and driver. |
| Triple buffering | `553505273` | `0` (OFF) | Requests triple buffering Off for the profile. |

### Texture Filtering

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Texture filtering quality | `13510289` | `20` (High Performance) | Requests the named High Performance texture-filtering policy. |
| Negative LOD bias | `1686376` | `1` (Clamp) | Prevents the driver from applying negative LOD bias (sharpening hack). Clamping avoids driver-injected aliasing. |
| Trilinear optimization | `3066610` | `0` (ON) | Requests the NPI-defined On value. Visual and performance effects are driver-dependent. |
| Anisotropic filter optimization | `8703344` | `0` (OFF) | Disables anisotropic filtering shortcuts. AF quality is controlled by CS2's own settings. |
| Anisotropic sample optimization | `15151633` | `0` (OFF) | Companion to above - ensures per-sample AF is not reduced by the driver. |
| Driver-controlled LOD bias | `6524559` | `0` (OFF) | Disables the driver's autonomous LOD bias adjustments. |

### Anti-Aliasing

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| AA gamma correction | `276652957` | `0` (OFF) | Disables driver gamma correction applied during AA resolve. CS2 manages its own gamma. |
| AA mode | `276757595` | `0` (Application Controlled) | Requests application-controlled anti-aliasing. |
| AA line gamma | `545898348` | `0` (OFF) | Disables AA line gamma processing. |
| Anisotropic filtering | `270426537` | `1` (Application Controlled) | Delegates AF mode to the application. CS2 sets its own AF level via video.txt. |
| Anisotropic mode | `282245910` | `0` (Application Controlled) | Companion mode setting - no driver override. |

### FXAA

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Enable FXAA (master gate) | `276089202` | `0` (Off) | Disables driver-level FXAA injection. In NVAPI DRS, `0` = Off, `1` = On. |
| Predefined FXAA usage | `271895433` | `0` | Secondary FXAA disable. Belt-and-suspenders with the master gate above. |

### VSync / Frame Rate

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| VSync | `11041231` | `138504007` (Force OFF) | Requests VSync Off for the profile. |
| Preferred refresh rate | `6600001` | `1` (Highest Available) | Requests the highest refresh rate reported to the driver. |
| FRL Low Latency | `277041152` | `0` (OFF) | Requests the named frame-limiter low-latency mode Off. |
| Frame Rate Limiter (legacy) | `277041154` | `0` (OFF) | Disables the legacy per-app FRL. |
| Frame Rate Limiter NVCPL | `277041162` | `500` | Requests 500 FPS unless the FPS Cap Calculator supplies another value. |

### VRR / G-SYNC (Suite Default: Disabled)

The suite requests disabled VRR and G-SYNC profile states as its fixed-refresh
default. This may be unsuitable for a display workflow that relies on VRR.
Compare the available modes on the target system.

| Name | DRS ID | Value |
|------|--------|-------|
| VRR global feature | `278196567` | `0` (OFF) |
| VRR requested state | `278196727` | `0` (OFF) |
| G-SYNC | `279476652` | `1` (Force OFF) |
| G-SYNC (secondary) | `279476687` | `1` (Force OFF) |
| G-SYNC globally | `294973784` | `0` (OFF) |
| VSync tear control | `5912412` | `2525368439` (disabled) |

`279476686` is not part of the alpha default because it is absent from the
public NPI reference used by the repository.

### Ansel

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Ansel | `276158834` | `0` (OFF) | Requests NVIDIA Ansel Off for the profile. |
| Predefined Ansel usage | `271965065` | `0` | Secondary Ansel disable. |

### Optimus (Laptop dGPU Preference)

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Optimus rendering GPU | `284810369` | `17` | Requests the public NPI rendering-GPU value for hybrid systems. |
| Optimus shim mode | `284810372` | `16777216` | Companion public NPI shim value for hybrid systems. |

### Resizable BAR

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| rBAR Enable | `983226` | `0` (Disabled) | Sets the suite's per-application Resizable BAR profile choice to Off. This does not change the system-wide BIOS setting. NPI `CustomSettingNames.xml`: `0x000F00BA`. |
| rBAR Options | `983227` | `0` (Disabled) | Companion setting - mirrors rBAR Enable state. Both must be 0 to disable per-application rBAR in the DRS profile. |

### Shader Cache

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Shader disk cache max size | `11306135` | `10240` MB (10 GB) | Requests a 10 GB maximum for the application profile. Actual allocation and eviction remain driver-controlled. |

### SLI / AFR

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Smooth AFR | `270198627` | `0` (OFF) | Disables SLI Alternate Frame Rendering. On single-GPU systems this is a no-op; included to prevent any SLI-related behavior if the profile is ever used on a multi-GPU system. |

### Settings named by NVIDIA Profile Inspector

These identifiers are not part of the public NVAPI enum header used by the
repository, but their names are present in the public
`Orbmu2k/nvidiaProfileInspector` setting reference.

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Ultra Low Latency - CPL State | `390467` | `1` (On) | Requests the named NPI profile state. Reflex and driver behavior remain application- and driver-dependent. |
| DXR_ENABLE | `14566042` | `0` (Off) | Requests the named DXR state for the profile. |
| ANSEL_FREESTYLE_MODE | `274606621` | `4` (Approved only) | Requests the named Ansel or Freestyle approval mode. |
| VK_NV_RAYTRACING | `549198379` | `0` (Disable) | Requests the named Vulkan ray-tracing state. |
| CUDA_STABLE_PERF_LIMIT | `1343646814` | `0` (Force off) | Requests the named CUDA performance-limit state. |
| GFE_MONITOR_USAGE | `2156231208` | `1` | Requests the named GeForce Experience monitor-usage state. |

---

## Excluded legacy registry writes

The earlier PowerShell workflow experimented with `PerfLevelSrc` and
`DisableDynamicPstate` under a display class key. They are not DRS settings and
are not part of the native P1:20/P3:4 contract. The Rust workflow does not infer
a mutable class-key instance or write either value.

---

## Excluded Settings

Three settings are intentionally excluded:

| DRS ID | Hex | Reason |
|--------|-----|--------|
| `2966161525` | `0xB0CC0875` | Smooth Motion API state is excluded because frame interpolation is outside the suite's default rendering policy. |
| `550564838` | `0x20D0F3E6` | OpenGL GPU Affinity is a device-specific string setting and cannot be copied safely across GPUs. |
| `269308407` | `0x100D51F7` | `Buffers=(Depth)` is a string setting with no documented CS2 role in the public references used by the project. |

---

## Verifying the Profile

```powershell
# Observe the P-state under a defined workload after applying
& "C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe" --query-gpu=pstate --format=csv,noheader
```

The observed P-state depends on workload, power, temperature, and driver
behavior. A P0 sample does not prove that an individual profile or registry
value caused that state.

In NVIDIA Profile Inspector (Orbmu2k), open the Counter-strike 2 profile and
compare the stored values with the 42-setting alpha table above. The removed
partially decoded and unknown entries are not expected in the alpha profile.
