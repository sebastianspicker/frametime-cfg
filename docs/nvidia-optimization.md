# NVIDIA Optimization — Deep Dive

> Covers Phase 1 Steps 5, 19–21; Phase 2 (GPU driver removal); Phase 3 Steps 1–4.

The NVIDIA optimization path in this suite has two components that require separate explanation: the **driver installation methodology** (why clean install matters, how DDU replacement works) and the **per-game profile system** (why registry writes don't work on modern drivers, how DRS does).

---

## Driver Version and the R570 Question (Step 5)

### The R570 regression

NVIDIA's R570 driver branch (released late 2024) introduced changes to the frame pacing subsystem in the OpenGL and Vulkan paths. CS2 uses Vulkan on Windows. Several community reports (Blur Busters forum, ThourCS2) documented increased 1% low variance on RTX 30/40 series under R570+ compared to the last R560 release (566.36).

The regression is not universal — RTX 40 Super / 50 series shows neutral or positive results on R570. RTX 30 series shows the most consistent regression reports.

Step 5 checks your installed driver version and warns if you're on R570+. **It does not automatically roll back.** The decision is yours:

- If you're on RTX 40 Super/50: stay on the latest driver
- If you're on RTX 30 series and have observable stutter: benchmark both driver versions
- Use CapFrameX to compare — don't judge by feel alone

The stable pre-R570 reference driver is **566.36**.

---

## Clean Driver Install — Why It Matters

### What a "dirty" driver install leaves behind

When you install a new NVIDIA driver over an existing one, the installer:
1. Runs the new display driver in-place
2. Writes new registry entries
3. Copies new binaries to `System32\DriverStore`

It does NOT clean:
- Leftover configuration from the previous driver in `C:\ProgramData\NVIDIA`
- DRS profile entries from the old driver version (some settings are version-specific)
- Shader caches compiled by the old driver (stored in `AppData\Local\Temp\NVIDIA\GLCache` and `DXCache`)
- Old driver packages in `DriverStore\FileRepository`

Over multiple driver updates, this accumulation can cause:
- Shader recompilation stutters (new driver encounters old driver's incompatible cached shaders)
- DRS profile conflicts (old profile entries persist alongside new ones)
- Occasional driver initialization failures (rare but documented)

### Phase 2 — What the native removal does

The suite's GPU driver removal in Safe Mode is a five-phase process implemented in `helpers/gpu-driver-clean.ps1`:

1. **Stop GPU services** — `nvlddmkm`, `nvSCPAPISvr`, `nvvsvc`, `NVDisplay.ContainerLocalSystem`, and related services are stopped
2. **Remove driver package** — `pnputil /delete-driver {oemXX.inf} /uninstall /force` removes the INF-based driver registration
3. **Remove driver files** — NVIDIA-specific binaries in `System32`, `System32\DriverStore\FileRepository`, and driver-specific directories under `Windows\System32\drivers`
4. **Clean registry** — NVIDIA driver keys under `HKLM:\SYSTEM\CurrentControlSet\Services\nvlddmkm` and device-specific keys in `HKLM:\SYSTEM\CurrentControlSet\Enum\*\VideoController`
5. **Wipe shader caches** — `AppData\Local\Temp\NVIDIA\GLCache`, `DXCache`, `D3DSCache`, and the CS2-specific shader cache

### Why Safe Mode

GPU driver removal must happen in Safe Mode because Windows prevents deletion of the currently active display driver while the desktop session is running. DDU uses the same approach. The only difference is that DDU is a closed-source executable making the same kernel calls that our native PowerShell implementation makes — with the advantage that ours is fully auditable.

### Phase 3 Step 1 — NVCleanstall replacement

The NVIDIA driver `.exe` is a self-extracting 7-Zip archive. The suite:
1. Extracts the archive to a temp directory
2. Removes 15 bloat components/files:
   - GeForce Experience + telemetry
   - NVIDIA FrameView SDK
   - NVIDIA NVCAT container
   - NVIDIA SHIELD support
   - NVIDIA USB-C driver (not needed on most gaming rigs)
   - HDCP components (irrelevant for gaming)
   - 3D Vision (deprecated since 2019)
   - NVIDIA Installer components (the installer infrastructure itself)
3. Runs `setup.exe -s -noreboot` — silent install of display adapter + PhysX + audio driver only
4. Copies the NVIDIA DRS profile at the end (Step 4)

This is exactly what NVCleanstall does, implemented natively.

---

## The NVIDIA DRS Profile System (Phase 3 Step 4)

### Why registry writes mostly don't work

The common approach for "NVIDIA optimization" is to write values to:
```
HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d\
```

This works for approximately 4 settings. For everything else, it's ineffective on modern drivers (approximately driver version 460+).

**Here's why:** NVIDIA's per-application settings have two storage mechanisms:
1. **DRS binary database** (`nvdrs.dat`) — the authoritative source, read by `nvapi.dll` at profile lookup
2. **Registry `d3d\` path** — a legacy fallback, read only for a small set of global settings

When NVIDIA Profile Inspector imports a `.nip` file, it calls `nvapi64.dll` directly to write to the DRS binary database. When most optimization guides write registry values to `d3d\`, those writes hit the legacy path that the driver may not read at all for per-application settings.

**The only confirmed-effective registry write** is `PerfLevelSrc = 0x2222` in the GPU hardware class key:
```
HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968}\0000
```
This is a hardware-class-level key, not a per-application DRS setting. NVIDIA drivers read this unconditionally. Setting it to `0x2222` tells the driver that all performance levels should prefer maximum performance P-state from both the software and hardware sides.

### The DRS implementation

The suite implements DRS write via C# `Add-Type` in `helpers/nvidia-drs.ps1`:

```csharp
// Simplified — full implementation in nvidia-drs.ps1
[DllImport("nvapi64.dll", EntryPoint = "nvapi_QueryInterface")]
static extern IntPtr NvQueryInterface(uint id);
```

This calls `nvapi_QueryInterface(uint id)` to get function pointers for 12 DRS functions, then uses those pointers to:
1. Initialize the DRS session
2. Find or create the CS2 application profile
3. Write DWORD settings to the profile
4. Save the modified session to `nvdrs.dat`

This is the same API path that NVIDIA Profile Inspector uses. Our writes are indistinguishable from NPI writes.

### The 52 DRS Settings

The suite's NVIDIA profile was derived from public NVIDIA DRS documentation (`NvApiDriverSettings.h` canonical DRS enum IDs), `CustomSettingNames.xml`, community testing (djdallmann, valleyofdoom, Blur Busters), and the NVIDIA 2022 internal settings database.

**What matters most:**

| Setting | DRS ID | Value | Why |
|---------|--------|-------|-----|
| Power Management Mode | `PREFERRED_PSTATE_ID` | `PREFER_MAX (1)` | Single most impactful — locks GPU P-state to P0 (max performance clock) |
| Max Pre-Rendered Frames | `PRERENDERLIMIT_ID` | 1 | Minimizes render queue; reduces input lag and frametime variance |
| Threaded Optimization | `OGL_THREAD_CONTROL_ID` | Force On (1) | Default is Auto (0). The suite sets Force On — CS2 benefits from explicit multi-threading |
| VSync Force Off | `VSYNCMODE_ID` | `FORCEOFF` | Baseline — never add render queue latency |
| Frame Rate Limiter | `FRL_FPS_ID (NVCPL)` | 500 (or fpsCap) | If your FPS cap is calculated, it's written directly to the FRL setting |
| FXAA Disallow | `FXAA_ALLOW_ID` | `DISALLOWED` | Stronger than `FXAA_ENABLE=0` — master gate prevents injection |
| RT Disabled | DXR + Vulkan RT | 0 | CS2 doesn't use RT; prevents accidental activation |
| All G-SYNC/VRR | 6 settings | Off/Force Off | Suite default for fixed-refresh competitive play; benchmark-dependent rather than universal |
| Texture Filtering | `QUALITY_ENHANCEMENTS_ID` | `HIGHPERFORMANCE` | Maximum driver-side quality reduction for GPU headroom |

**Three excluded settings:**

1. **Smooth Motion APIs** (`0xB0CC0875`, value 1) — frame interpolation. Generates intermediate frames between real frames at the cost of 1–2 frames of input lag. In competitive CS2, you'd be reacting to interpolated frames that don't represent the server's current state. Strictly harmful.

2. **OpenGL GPU Affinity** (`OGL_IMPLICIT_GPU_AFFINITY_ID`) — a string-type setting that hardcodes a specific PCI device ID. Applying this on any other GPU confuses the driver's device routing.

3. **Depth Buffers** (string setting, value `"Buffers=(Depth)"`) — unclear semantics, possibly DLSS-related. No documented effect on CS2.

### CUDA P-State Lock — The Memory Clock Problem

`CUDA_STABLE_PERF_LIMIT` (DRS ID `0x50166C5E` / `1343646814`, value 0 = FORCE_OFF).

NVIDIA GPUs have CUDA compute power states (P2, P3, etc.) that can cause the GPU memory clock to downclock during CUDA workloads even when the GPU is in P0 for graphics. In CS2, this can manifest as a brief memory bandwidth reduction when certain shader dispatch patterns trigger CUDA path evaluation in the driver, causing a frametime spike while the memory clock recovers.

Setting this to FORCE_OFF prevents any CUDA-triggered memory clock reduction while CS2 is running.

> The original `CUDA_FORCE_P2_STATE` (`0x400E194F` / `1074665807`) was removed — undocumented duplicate. `CUDA_STABLE_PERF_LIMIT` handles the same P-state override at a different driver layer.

### `DisableDynamicPstate = 1` — The Second Lock

Added alongside `PerfLevelSrc` in the GPU class key:
```
HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968}\0000
    PerfLevelSrc       = 0x2222
    DisableDynamicPstate = 1
```

`PerfLevelSrc` targets the NVIDIA control panel's power management layer. `DisableDynamicPstate` locks P-state at the driver level (nvidia-smi confirms the lock when this is set). They operate at different abstraction layers in the driver stack — both set for redundancy.

### Profile Behavior

| Profile | DRS Writes | Registry Fallback |
|---------|-----------|------------------|
| SAFE | None | None |
| RECOMMENDED | None | None |
| COMPETITIVE | 52 DRS settings + GPU class key | 25 settings (22 d3d + 1 NVTweak + 2 GPU class key) |
| CUSTOM | Same as COMPETITIVE | Same as COMPETITIVE |

All settings are applied natively — no external tools or third-party files required.

---

## MSI Interrupts (Phase 3 Step 2)

### Line-based vs. message-signaled interrupts

Legacy PCI interrupt delivery (line-based, INTx) works by the device asserting a physical interrupt line. The interrupt controller must poll which device asserted, then route the interrupt. Multiple devices can share an IRQ line, creating queuing and priority conflicts.

**Message Signaled Interrupts (MSI)** replace this with a DMA write to a special memory address. The device writes a message that directly specifies which interrupt vector to invoke. No IRQ sharing, no interrupt controller polling, lower latency.

For GPU and NIC interrupts in CS2, MSI mode provides:
- Lower and more consistent interrupt latency
- No IRQ sharing conflicts (CS2's GPU interrupt never queues behind a shared IRQ device)
- More predictable DPC scheduling

### What the suite writes

```
HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\{NVIDIA device ID}\{instance}\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties
    MSISupported = 1  (DWORD)
```

For three device classes: GPU, NIC, and Audio controller. The device classes are T2 (RECOMMENDED+ prompted for GPU and NIC; COMPETITIVE+ for audio since audio MSI has more variance in results).

**Why a cold boot is required:** Line-based interrupt routing is established by the BIOS/UEFI during hardware initialization. MSI mode is negotiated between the OS and the device during PnP initialization. Fast Startup (Step 23) preserves the previous session's interrupt state — which is why Step 23 must run before Step 2, and why a proper cold boot (not Fast Startup) is required after Step 2.

### MSI vs. MSI-X

Some guides distinguish MSI (single interrupt vector) from MSI-X (multiple interrupt vectors per device). Modern NVIDIA GPUs support MSI-X. The `MSISupported=1` registry key enables MSI mode; the driver negotiates MSI vs. MSI-X based on device capability and OS support. On Windows 10/11 with modern NVIDIA drivers, MSI-X is typically negotiated automatically.

### GoInterruptPolicy replacement (Phase 3 Step 3)

GoInterruptPolicy pinned NIC interrupts to a specific CPU core via the affinity policy registry key. The suite replaces this with native registry writes:

```
HKLM:\...\Device Parameters\Interrupt Management\Affinity Policy
    DevicePolicy = 4           (ClosestProcessor — or 1 for specific core)
    AssignmentSetOverride = X  (bitmask of target core)
```

This is T3 / COMPETITIVE+ only, because the correct `AssignmentSetOverride` value depends on your core layout. Setting it incorrectly (e.g., pinning to Core 0 which has high OS interrupt load) can worsen performance.

---

## Verifying the NVIDIA Profile

### Checking DRS settings

```powershell
# After Phase 3 Step 4, verify the profile was created
& "C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe" --query-gpu=power.management,pstate --format=csv
```

Should show `Enabled, P0` if both `PerfLevelSrc` and `DisableDynamicPstate` are active.

### Checking MSI mode

Open Device Manager → View → Show hidden devices → expand your GPU → Properties → Details → Property: "Hardware Ids". Then check:

```powershell
$devPath = "HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\DEV_XXXX..."  # your GPU's path
Get-ItemProperty "$devPath\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties"
```

`MSISupported = 1` confirms MSI mode is registered. Actual MSI negotiation state is visible in MSINFO32 (Start → Run → `msinfo32`) → Components → Sound, video and game controllers → your GPU → Check "IRQ assignment" — if it shows a very large number (like 0xFFFFFFF0+), that's MSI mode. A small number (like 16, 17) is line-based mode.

### Using LatencyMon before/after

Run LatencyMon for 5 minutes before Phase 3 and 5 minutes after (at desktop, not in-game). Check:

- `nvlddmkm.sys` DPC execution times should decrease after clean driver install + MSI
- `dxgkrnl.sys` ISR latency should decrease after clean driver + HAGS change
- Overall "highest execution" should improve

If LatencyMon shows `nvlddmkm.sys` as consistently problematic after optimization, the driver version rollback option (Step 5) should be considered.
