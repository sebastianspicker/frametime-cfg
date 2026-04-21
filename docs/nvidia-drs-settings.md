# NVIDIA DRS Settings — Complete Table

> Covers Phase 3 Step 4, `helpers/nvidia-profile.ps1`, and `helpers/nvidia-drs.ps1`.
> For the DRS write mechanism itself, see [`docs/nvidia-optimization.md`](nvidia-optimization.md).

The suite writes 52 DWORD settings directly to the NVIDIA DRS binary database (`nvdrs.dat`) via `nvapi64.dll`. Two settings from the original profile were removed (VRR `279476686` — not in NPI, likely inert; CUDA Force P2 `1074665807` — duplicate of `1343646814`). Two rBAR settings were added (`983226`, `983227`). Three settings are intentionally excluded (see [Excluded Settings](#excluded-settings)). Two registry keys (`PerfLevelSrc`, `DisableDynamicPstate`) are always applied regardless of DRS availability.

---

## Full Settings Table

### Power & Performance

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Power management mode | `274197361` | `1` (Prefer Max Performance) | Locks GPU P-state to P0. Single most impactful setting — prevents the GPU from downclocking during the brief CPU-bound gaps between frames. |
| Maximum pre-rendered frames | `8102046` | `1` | Limits the driver render queue to 1 frame. Reduces input lag by preventing the CPU from running too far ahead of the GPU. |
| Threaded optimization | `549528094` | `1` (Force ON) | Default is Auto (0). CS2 uses Vulkan; Force ON explicitly enables multi-threaded OpenGL/Vulkan command submission. Auto can regress in some driver versions. |
| Triple buffering | `553505273` | `0` (OFF) | Irrelevant with VSync Force OFF (below), but explicitly disabled to prevent the driver from enabling it autonomously. |

### Texture Filtering

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Texture filtering quality | `13510289` | `20` (High Performance) | Disables driver-side texture quality enhancements. CS2 uses its own texture filtering; driver enhancement is overhead with no visual benefit in a competitive context. |
| Negative LOD bias | `1686376` | `1` (Clamp) | Prevents the driver from applying negative LOD bias (sharpening hack). Clamping avoids driver-injected aliasing. |
| Trilinear optimization | `3066610` | `0` (ON — shortcut enabled) | Enables the driver's trilinear performance shortcut (`0` = ON in NVIDIA DRS). Reduces trilinear filtering cost with negligible visual difference in CS2. |
| Anisotropic filter optimization | `8703344` | `0` (OFF) | Disables anisotropic filtering shortcuts. AF quality is controlled by CS2's own settings. |
| Anisotropic sample optimization | `15151633` | `0` (OFF) | Companion to above — ensures per-sample AF is not reduced by the driver. |
| Driver-controlled LOD bias | `6524559` | `0` (OFF) | Disables the driver's autonomous LOD bias adjustments. |

### Anti-Aliasing

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| AA gamma correction | `276652957` | `0` (OFF) | Disables driver gamma correction applied during AA resolve. CS2 manages its own gamma. |
| AA mode | `276757595` | `0` (Application Controlled) | Ensures no driver-injected AA. CS2 uses MSAA natively when enabled (4x is the competitive recommendation). |
| AA line gamma | `545898348` | `0` (OFF) | Disables AA line gamma processing. |
| Anisotropic filtering | `270426537` | `1` (Application Controlled) | Delegates AF mode to the application. CS2 sets its own AF level via video.txt. |
| Anisotropic mode | `282245910` | `0` (Application Controlled) | Companion mode setting — no driver override. |

### FXAA

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Enable FXAA (master gate) | `276089202` | `0` (Off) | Disables driver-level FXAA injection. In NVAPI DRS, `0` = Off, `1` = On. |
| Predefined FXAA usage | `271895433` | `0` | Secondary FXAA disable. Belt-and-suspenders with the master gate above. |

### VSync / Frame Rate

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| VSync | `11041231` | `138504007` (Force OFF) | Forces VSync off regardless of in-game setting. Prevents any accidental VSync that would cap FPS and add render queue latency. |
| Preferred refresh rate | `6600001` | `1` (Highest Available) | Ensures the driver uses the monitor's maximum reported refresh rate. |
| FRL Low Latency | `277041152` | `0` (OFF) | Disables the frame rate limiter's low latency mode, which can interfere with custom caps. |
| Frame Rate Limiter (legacy) | `277041154` | `0` (OFF) | Disables the legacy per-app FRL. |
| Frame Rate Limiter NVCPL | `277041162` | `500` | Sets the NVCP frame rate limiter to 500 FPS (effectively unlimited). If the FPS Cap Calculator computed a cap value, that value is used instead. |

### VRR / G-SYNC (Suite Default: Disabled)

The suite disables the remaining VRR / G-SYNC profile paths by default because many competitive CS2 users target framerate well above monitor refresh and prefer the lowest-latency fixed-refresh path. This should be treated as a suite default, not as a universal NVIDIA-documented truth for every monitor and workflow.

| Name | DRS ID | Value |
|------|--------|-------|
| VRR global feature | `278196567` | `0` (OFF) |
| VRR requested state | `278196727` | `0` (OFF) |
| G-SYNC | `279476652` | `1` (Force OFF) |
| G-SYNC (secondary) | `279476687` | `1` (Force OFF) |
| G-SYNC globally | `294973784` | `0` (OFF) |
| VSync tear control | `5912412` | `2525368439` (disabled) |

> `279476686` (Variable refresh rate) was removed — not present in NPI, likely inert. The remaining 6 G-SYNC/VRR settings cover all VRR paths.

### Ansel

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Ansel | `276158834` | `0` (OFF) | Disables NVIDIA Ansel (photo mode overlay). Unused in competitive; its background hook adds a small overhead. |
| Predefined Ansel usage | `271965065` | `0` | Secondary Ansel disable. |

### Optimus (Laptop dGPU Preference)

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Optimus rendering GPU | `284810369` | `17` | Forces discrete GPU rendering on Optimus laptops. On desktop systems with a single GPU this is a no-op. |
| Optimus shim mode | `284810372` | `16777216` | Companion Optimus shim setting — ensures CS2 render path goes through dGPU on hybrid systems. |

### Resizable BAR

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| rBAR Enable | `983226` | `0` (Disabled) | Sets per-application Resizable BAR to OFF for this suite's CS2 default. Community benchmarking has reported better 1% lows with rBAR OFF in CS2 on some systems, but this remains benchmark-dependent rather than settled vendor-backed guidance. System-wide BIOS rBAR stays enabled for other titles. NPI `CustomSettingNames.xml`: `0x000F00BA`. |
| rBAR Options | `983227` | `0` (Disabled) | Companion setting — mirrors rBAR Enable state. Both must be 0 to disable per-application rBAR in the DRS profile. |

### Shader Cache

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Shader disk cache max size | `11306135` | `10240` MB (10 GB) | Increases shader cache from the 1 GB driver default to 10 GB. CS2 compiles shaders aggressively on first launch and mid-game. A larger cache reduces re-compilation stutters on map load. 2026 community consensus recommends 10 GB+ (actual disk usage rarely exceeds 1 GB but prevents any eviction). |

### SLI / AFR

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Smooth AFR | `270198627` | `0` (OFF) | Disables SLI Alternate Frame Rendering. On single-GPU systems this is a no-op; included to prevent any SLI-related behavior if the profile is ever used on a multi-GPU system. |

### CUDA P-State Lock

> `1074665807` (CUDA Force P2 State) was removed — undocumented duplicate. The CUDA P-state override is handled by `1343646814` (CUDA_STABLE_PERF_LIMIT) in the decoded flags section below.

### Decoded NVIDIA-Internal Flags

These settings have no public DRS enum names. Decoded via reverse engineering against `Orbmu2k/nvidiaProfileInspector` (`CustomSettingNames.xml`) and the NVIDIA 2022 internal settings database (2,054 settings).

#### Confirmed Decoded

| Name | DRS ID | Value | Explanation |
|------|--------|-------|-------------|
| Ultra Low Latency - CPL State | `390467` | `1` (On) | **NPI-confirmed name and value.** Enables the CPU pipeline latency mode that NVIDIA Reflex requires. The companion setting "Ultra Low Latency - Enabled" is a separate DRS ID and is left Off — Reflex manages its own pre-render depth independently. |
| DXR_ENABLE | `14566042` | `0` (OFF) | DirectX Raytracing master enable. CS2 does not use DXR — disabling prevents the RT pipeline from being initialized for this app. |
| ANSEL_FREESTYLE_MODE | `274606621` | `4` (APPROVED_ONLY) | Ansel/Freestyle approval mode; 4 = approved-only app list. No active overhead — Ansel is also disabled above via its own settings. |
| VK_NV_RAYTRACING | `549198379` | `0` (DISABLE) | Disables the `VK_NV_ray_tracing` Vulkan extension for CS2. CS2 does not use Vulkan RT; this prevents the driver from advertising and initializing the RT extension for the CS2 Vulkan instance. |
| CUDA_STABLE_PERF_LIMIT | `1343646814` | `0` (FORCE_OFF) | Prevents a driver-internal CUDA performance cap that can cause GPU memory clock to downclock during CUDA workloads. Replaces the removed `1074665807` (CUDA Force P2 State). |
| GFE_MONITOR_USAGE | `2156231208` | `1` | GeForce Experience telemetry state flag. No impact when GFE is not installed. |

#### Partially Decoded

Identified by family name in the leak DB; precise semantics inferred from position and value patterns.

| DRS ID | Hex | Value | Inferred Meaning |
|--------|-----|-------|-----------------|
| `3224887` | `0x313537` | `4` | PS_ASYNC_SHADER_SCHEDULER variant — likely controls async shader thread count or scheduling mode. |
| `11313945` | `0xACA319` | `1` | PS_ pipeline / shader cache variant — `1` = enabled; exact sub-option unknown. |
| `12623113` | `0xC09D09` | `2` | FORCE_GPUKERNEL_COP_ARCH variant — GPU kernel cooperative architecture hint; value `2` selects a specific kernel execution path. |
| `270883746` | `0x10255BA2` | `0` | SHIM_RENDERING_OPTIONS companion flag — always `0` in known profiles; companion to the entry below. |
| `270883750` | `0x10255BA6` | `469762050` | SHIM_RENDERING_OPTIONS extended — `0x1C000002` = `EHSHELL_DETECT \| DISABLE_TURING_POWER_POLICY` per leak DB bitmask definitions. |
| `271076560` | `0x10284CD0` | `0` | MCSXX / SLI flag — disabled; no-op on single-GPU systems. |
| `539250342` | `0x20244EA6` | `1` | Vulkan workaround flag (VK_SLI_WAR family) — `1` = active. No-op on single-GPU. |
| `544173595` | `0x206F6E1B` | `60` | VK_LOW_LATENCY family — value `60` is likely a sleep/overlap target in microseconds for the low-latency Vulkan submission path. |

#### Unknown Post-2022 Flags

These two IDs are absent from `CustomSettingNames.xml`, the NVIDIA 2022 internal settings database, and all public NPI documentation. NPI verification confirmed they do **not** appear in the loaded profile — not in named sections, not in the Unknown section. The driver silently discards them, indicating they are version-specific IDs not supported by current driver releases.

| DRS ID | Hex | Value | Status |
|--------|-----|-------|--------|
| `276387096` | `0x10795518` | `60` | **Driver-ignored** on current drivers. Applied by suite via DRS write; driver discards silently if unsupported. |
| `276387097` | `0x10795519` | `0` | **Driver-ignored** on current drivers. Companion to the above. |

Writing these via `nvapi64.dll` is harmless — the DRS write succeeds at the API level but the driver does not store or act on the value if it doesn't recognize the ID.

---

## Registry Keys Applied Separately

These are not DRS settings — they go in the GPU hardware class key and are read by the driver unconditionally:

```
HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000
    PerfLevelSrc        = 0x2222
    DisableDynamicPstate = 1
```

**`PerfLevelSrc = 0x2222`** — The only registry key confirmed effective for P-state control on modern NVIDIA drivers. The `0x2222` value sets both the graphics and compute performance level source to "software controlled", telling the NVCP power management layer to maintain maximum P-state. This is distinct from the DRS "Power Management Mode" setting — both are needed.

**`DisableDynamicPstate = 1`** — Locks P-state at the driver level (the `nvlddmkm.sys` layer), independently from the NVCP layer targeted by `PerfLevelSrc`. Verified via `nvidia-smi --query-gpu=pstate --format=csv` showing `P0` when this key is set.

---

## Excluded Settings

Three settings are intentionally excluded:

| DRS ID | Hex | Reason |
|--------|-----|--------|
| `2966161525` | `0xB0CC0875` | **Smooth Motion APIs = 1** — frame interpolation. Generates intermediate frames between real frames. Adds 1–2 frames of input lag. In competitive CS2, you react to interpolated frames that don't represent the server's current state. Strictly harmful for competitive play. |
| `550564838` | `0x20D0F3E6` | **OpenGL GPU Affinity** — a string-type setting that hardcodes a specific GPU's PCI device ID. Applying this on any other GPU confuses the driver's device routing. |
| `269308407` | `0x100D51F7` | **String setting** `"Buffers=(Depth)"` — DRS string type (not DWORD). Unclear semantics, possibly DLSS-related. No documented effect on CS2. |

---

## Verifying the Profile

```powershell
# Check P-state after applying
& "C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe" --query-gpu=pstate --format=csv,noheader
# Should return: P0
```

In NVIDIA Profile Inspector (Orbmu2k): open → find "Counter-strike 2" profile → verify settings match the table above. The 8 partially-decoded and 2 unknown post-2022 flags will appear with their numeric IDs — this is expected.
