# NVIDIA Driver and Profile Workflow

> Covers Phase 1 Steps 5, 19–21; Phase 2 (GPU driver removal); Phase 3 Steps 1–4.

The NVIDIA path has two components: native driver removal and installation, and
per-application profile writes through NVIDIA DRS.

## Evidence boundary

The repository can verify package selection, signature checks, command
construction, DRS calls, requested values, and backup control flow. It does not
contain a committed driver-comparison, DPC, power-state, or frame-time dataset.
Driver behavior is time-sensitive and varies by GPU, Windows build, game build,
and profile. A successful installation or DRS write does not establish a
performance improvement.

---

## Driver version check

Step 5 reports the installed NVIDIA display-driver version. It does not classify
that version, encode a fixed rollback target, or select an older package.

Before manually selecting a package, confirm support for the installed GPU,
Windows build, security fixes, and current CS2 build. Compare driver versions
with the same workload and retain the version that is stable on the target
system.

---

## Driver cleanup and installation scope

### State considered by the cleanup

The exact state retained by an NVIDIA installer depends on package version and
selected options. The repository's separate cleanup considers:
- NVIDIA configuration under `C:\ProgramData\NVIDIA`
- existing NVIDIA DRS profile state
- Selected rebuildable cache paths under `%LOCALAPPDATA%\NVIDIA`,
  `%LOCALAPPDATA%\NVIDIA Corporation\NV_Cache`,
  `%PROGRAMDATA%\NVIDIA Corporation\NV_Cache`, and
  `%LOCALAPPDATA%\D3DSCache`
- NVIDIA display-driver packages reported by Windows package inventory

The cleanup removes or resets this state before installation. The repository
does not contain a trace that isolates a frame-time effect from each item.

### Phase 2 - What the native removal does

The suite's GPU driver removal in Safe Mode is implemented in `helpers/gpu-driver-clean.ps1`:

1. Locale-independent CIM data identifies NVIDIA display packages and supplies
   validated `oemNN.inf` names.
2. `pnputil /delete-driver {oemNN.inf} /uninstall /force` asks Windows to update
   package and Driver Store references. A discovery or removal failure stops the
   workflow before vendor cleanup.
3. Related services, applications, scheduled tasks, and vendor-specific
   registry state are removed only after every identified package is reported
   absent.
4. The suite does not directly delete `DriverStore\FileRepository` folders or
   broad display-class registry keys.
5. NVIDIA `GLCache`, `DXCache`, and `NV_Cache` targets, plus the shared
   `D3DSCache`, are cleared as rebuildable cache data.

### Why Safe Mode

GPU driver removal runs in Safe Mode so the active desktop session is not holding display-driver files and services open. The suite uses documented Windows inventory and package-removal interfaces and fails closed when package ownership or removal cannot be verified.

### Phase 3 Step 1 - NVIDIA package preparation

The suite validates the selected NVIDIA package, copies it into a restricted
temporary directory, and invokes the package's extraction mode. Before running
the extracted `setup.exe -s -noreboot`, it removes paths matching the checked-in
list:

- `GFExperience*`, `NvApp*`, and `NvBackend*`;
- root and container-plugin `NvTelemetry*` paths;
- `NvNodejs*`, `nodejs*`, `NvCamera*`, `ShadowPlay*`, and `NvVAD*`; and
- `EULA.txt`, `ListDevices.txt`, and `license.txt`.

Any failed targeted removal stops setup. The code does not remove FrameView,
NVCAT, SHIELD, USB-C, HDCP, 3D Vision, or installer infrastructure by name, and
it does not enforce an allowlist of installed driver components. The remaining
component selection is controlled by the NVIDIA package.

The workflow uses Windows tools and the vendor installer. It does not bundle or
invoke NVCleanstall. Phase 3 Step 4 applies the separate DRS profile.

---

## The NVIDIA DRS Profile System (Phase 3 Step 4)

### DRS and registry scope

The common approach for "NVIDIA optimization" is to write values to:
```
HKLM:\SOFTWARE\NVIDIA Corporation\Global\d3d\
```

This registry path is not the per-application DRS store used by the workflow.

NVIDIA exposes DRS APIs for per-application profile settings. The suite uses
those APIs rather than assuming that similarly named `d3d` registry values are
equivalent.

NVIDIA Profile Inspector uses NVIDIA DRS interfaces for application profiles.
The fallback `d3d` registry values written by this repository are not an
equivalent per-application profile, and current drivers can ignore them.

The suite separately writes `PerfLevelSrc = 0x2222` in the selected GPU hardware
class key:
```
HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968}\0000
```
This is a hardware-class value, not a per-application DRS setting. Its effective
behavior must be observed on the target driver.

### The DRS implementation

The suite implements DRS write via C# `Add-Type` in `helpers/nvidia-drs.ps1`:

```csharp
// Simplified - full implementation in nvidia-drs.ps1
[DllImport("nvapi64.dll", EntryPoint = "nvapi_QueryInterface")]
static extern IntPtr NvQueryInterface(uint id);
```

This calls `nvapi_QueryInterface(uint id)` to get function pointers for 12 DRS functions, then uses those pointers to:
1. Initialize the DRS session
2. Find or create the CS2 application profile
3. Write DWORD settings to the profile
4. Save the modified session to `nvdrs.dat`

NVIDIA Profile Inspector also uses public NVAPI DRS interfaces. The repository
uses it as a public reference for setting names and inspection.

### The 42 DRS settings

The alpha profile is limited to 42 DWORD identifiers supported by public NVIDIA
NVAPI headers or the public NVIDIA Profile Inspector setting reference. Eight
partially decoded and two unknown development entries were removed because
their semantics could not be supported from public references.

Selected profile values include:

| Setting | DRS ID | Value | Why |
|---------|--------|-------|-----|
| Power Management Mode | `PREFERRED_PSTATE_ID` | `PREFER_MAX (1)` | Requests the driver's Prefer Maximum Performance policy. |
| Max Pre-Rendered Frames | `PRERENDERLIMIT_ID` | 1 | Requests a one-frame driver queue limit where applicable. |
| Threaded Optimization | `OGL_THREAD_CONTROL_ID` | Force On (1) | Records the suite's driver profile choice; effect is API- and driver-dependent. |
| VSync Force Off | `VSYNCMODE_ID` | `FORCEOFF` | Requests VSync Off for the application profile. |
| Frame Rate Limiter | `FRL_FPS_ID (NVCPL)` | 500 (or fpsCap) | If your FPS cap is calculated, it's written directly to the FRL setting |
| FXAA Disallow | `FXAA_ALLOW_ID` | `DISALLOWED` | Requests the public NPI-named FXAA allow state. |
| RT Disabled | DXR + Vulkan RT | 0 | Requests the named ray-tracing profile states as Off. |
| All G-SYNC/VRR | 6 settings | Off/Force Off | Suite default for fixed-refresh competitive play; benchmark-dependent rather than universal |
| Texture Filtering | `QUALITY_ENHANCEMENTS_ID` | `HIGHPERFORMANCE` | Requests the named High Performance quality policy. |

Three settings are excluded from the profile:

1. Smooth Motion APIs (`0xB0CC0875`, value 1) is excluded because frame
   interpolation is outside the suite's default rendering policy.

2. OpenGL GPU Affinity (`OGL_IMPLICIT_GPU_AFFINITY_ID`) is device-specific and
   cannot be copied safely across GPUs.

3. Depth Buffers (`Buffers=(Depth)`) is a string setting with no documented CS2
   role in the public references used by the project.

### CUDA performance-limit setting

`CUDA_STABLE_PERF_LIMIT` (DRS ID `0x50166C5E` / `1343646814`, value 0 = FORCE_OFF).

The suite requests the public NPI-named `CUDA_STABLE_PERF_LIMIT` state. It does
not claim that CS2 invokes a CUDA workload or that this value prevents a measured
frame-time event.

### `DisableDynamicPstate = 1`

Added alongside `PerfLevelSrc` in the GPU class key:
```
HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968}\0000
    PerfLevelSrc       = 0x2222
    DisableDynamicPstate = 1
```

These values are written separately from DRS. Use `nvidia-smi` to observe the
effective driver state. A P-state sample does not prove that either registry
value alone caused the state.

### Profile Behavior

| Profile | DRS Writes | Registry Fallback |
|---------|-----------|------------------|
| SAFE | None | None |
| RECOMMENDED | None | None |
| COMPETITIVE | 42 DRS settings + GPU class key | 25 settings (22 d3d + 1 NVTweak + 2 GPU class key) |
| CUSTOM | Same as COMPETITIVE | Same as COMPETITIVE |
| YOLO | Same as COMPETITIVE | Same as COMPETITIVE |

Settings are applied through Windows and NVIDIA interfaces without bundling a
third-party profile tool.

---

## Optional post-install changes

After a verified NVIDIA driver installation, the current workflow attempts
four telemetry registry values, `RMHdcpKeyglobZero` on detected NVIDIA display
class entries, `EnableWriteCombining`, the DWM `OverlayTestMode` value, and two
NVIDIA telemetry services when present. These changes are separate from the DRS
profile and are not evidence that the driver installation itself performs
better.

Registry and service state is captured before mutation. The driver installation
can succeed while one or more optional post-install changes fail or are absent.
The runtime reports that outcome as partial rather than claiming that every
post-install value was applied.

---

## MSI Interrupts (Phase 3 Step 2)

### Line-based vs. message-signaled interrupts

Legacy PCI interrupt delivery (line-based, INTx) works by the device asserting a physical interrupt line. The interrupt controller must poll which device asserted, then route the interrupt. Multiple devices can share an IRQ line, creating queuing and priority conflicts.

Message-signaled interrupts replace a shared interrupt line with an in-band
memory write that identifies the interrupt vector. Whether this changes
observable latency depends on the device, driver, and existing interrupt mode.

The implemented registry value requests MSI support for selected GPU, NIC, and
audio devices. The repository does not contain an interrupt trace or CS2
benchmark that establishes a general performance result from this change.

### What the suite writes

```
HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\<device-instance>\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties
    MSISupported = 1  (DWORD)
```

Phase 3 Step 2 is one T2 operation. If the selected profile permits it and the
operator accepts the prompt, the helper processes eligible display, network,
and media-class devices together. There is no separate profile gate for the
audio class.

A cold boot is required for validation because interrupt mode is negotiated
during device initialization. Fast Startup can preserve kernel and device state,
so the workflow disables it before requesting the mode change.

### MSI vs. MSI-X

Some guides distinguish MSI from MSI-X. The `MSISupported=1` registry value
requests message-signaled operation; the device and driver determine the mode
and vector count. Verify the effective state on the target driver rather than
inferring MSI-X from the registry value alone.

### Native NIC affinity policy (Phase 3 Step 3)

The suite writes the Windows NIC affinity policy directly and does not invoke
GoInterruptPolicy. Its scope is not claimed to match that third-party tool:

```
HKLM:\...\Device Parameters\Interrupt Management\Affinity Policy
    DevicePolicy = 4           (Specified Processors)
    AssignmentSetOverride = X  (bitmask of target core)
```

Phase 3 Step 3 is a T3 operation available to the COMPETITIVE, CUSTOM, and YOLO
profiles. The `AssignmentSetOverride` value depends on logical-processor
topology. An unsuitable mask can concentrate interrupt work on a busy processor
or cross a topology boundary, so treat the helper's selected mask as
experimental and verify it on the target machine.

---

## Verifying the NVIDIA Profile

### Checking DRS settings

```powershell
# After Phase 3 Step 4, verify the profile was created
& "C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe" --query-gpu=power.management,pstate --format=csv
```

Record the result and compare it with the expected workload state. The command
does not attribute the result to an individual registry value.

### Checking MSI mode

List display devices and copy the `InstanceId` for the device being checked:

```powershell
Get-PnpDevice -Class Display | Select-Object FriendlyName, InstanceId
$instanceId = '<copy the PCI\VEN_... instance ID>'
$devPath = "HKLM:\SYSTEM\CurrentControlSet\Enum\$instanceId"
Get-ItemProperty "$devPath\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties"
```

`MSISupported = 1` confirms only that the requested registry value is stored.
The device and driver negotiate the effective mode during initialization. Use
device and driver diagnostics after reboot, and do not infer MSI-X support or
vector count from this value alone. See [MSI interrupts and NIC affinity](msi-interrupts.md).

### Using LatencyMon before/after

Use the same trace duration and workload before and after a single change. Check
device errors and raw DPC or ISR traces in addition to summary values. Do not
assume that a neutral or worse result requires a driver rollback; restore the
changed setting first and isolate variables.
