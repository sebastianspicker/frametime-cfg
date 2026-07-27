# Excluded and Conditional Settings

This document records settings that the repository deliberately omits or treats
as conditional. It does not claim that every omitted value is ineffective on
every system.

## Evidence boundary

The repository has no committed benchmark corpus for the settings below.
Mechanistic explanations, vendor documentation, and community measurements can
justify exclusion from the default workflow, but they do not establish a
universal performance result. CS2, Windows, firmware, anti-cheat, and driver
behavior can also change after this document is written.

## Legacy or unrelated controls

| Setting | Repository position | Reason |
|---|---|---|
| `-tickrate 128` | Omitted | Source 2 subtick input processing is not configured through this legacy launch option. |
| `-threads N` | Omitted | The suite does not override Source 2 thread-pool selection. |
| `mat_queue_mode 2` | Omitted | Legacy Source 1 guidance with no documented current role in the suite. |
| `-softparticlesdefaultoff` | Omitted | Legacy Source 1 launch guidance. |
| `+cl_forcepreload 1` | Omitted | Legacy preload guidance without a demonstrated current benefit. |
| `-dxlevel N` | Omitted | Legacy engine-selection guidance that does not configure the suite's CS2 path. |
| `-novid` | Omitted | The suite does not rely on an intro-video flag. |
| `-nojoy` | Omitted | It removes controller behavior and has no demonstrated repository benefit. |
| `cl_cmdrate` | Omitted | It does not configure Source 2 input delivery as it did in CS:GO. |
| `net_graph 1` | Replaced | The suite uses `cl_hud_telemetry_*` controls. |
| `netsh int tcp` tuning | Omitted | TCP controls do not tune CS2 UDP game datagrams. |
| TCP RSC and LSO changes | Omitted | These offloads concern TCP traffic, not CS2 UDP game datagrams. |
| Wake-on-LAN offloads | Omitted | Wake settings apply to sleep or powered-down states rather than an active match. |

An accepted console variable is not necessarily active. Check current console
output and restore a setting if the client rejects it.

## System controls omitted from defaults

| Setting | Reason for omission |
|---|---|
| Disable Spectre, Meltdown, or related mitigations | Reduces operating-system security. The repository does not trade these controls for an unverified game result. |
| `useplatformclock true` | Forces a timer-source policy without target-system evidence. |
| `tscsyncpolicy enhanced` | Adds a boot policy without a documented repository need. |
| Disable Hyper-Threading or SMT | Changes CPU topology globally and requires workload-specific testing. |
| `LargeSystemCache = 1` | Changes memory-manager policy globally and can reduce memory available to applications. |
| `DisablePagingFile` | Removes a Windows memory-management fallback and can destabilize memory-constrained workloads. |
| `SystemResponsiveness = 0` | Microsoft documents zero as unsupported for MMCSS. The suite uses a nonzero value. |
| `NetworkThrottlingIndex = 0xFFFFFFFF` | The suite retains the Windows value rather than applying an unverified global multimedia-network change. |
| `-high` | The suite uses persistent IFEO process-priority configuration where selected. |
| Third-party standby-list cleaners | Adds a resident tool and is not required by the current workflow. |

## Device and network controls that require local evidence

| Setting | When it may be worth testing | Reason it is not universal |
|---|---|---|
| Valve SDR override | Direct routing is unstable or inefficient. | The relay route can be better or worse for a given location and time. |
| Deeper network buffers | Telemetry shows repeatable jitter or late delivery. | Buffering can trade responsiveness for resilience, and its exact time cost is not fixed from an assumed tick rate. |
| DNS provider change | Name resolution or the resulting route differs during a controlled comparison. | DNS does not improve packet delivery after an endpoint is resolved, and routing can change later. |
| NIC interrupt moderation | DPC measurements justify comparing driver modes. | Driver names and behavior vary by adapter and version. |
| NIC interrupt affinity | NIC DPC work interferes with a measured workload and CPU topology is known. | An incorrect mask can move work onto a busier or inappropriate processor. |
| CPU C-state restrictions | A repeatable power-state transition issue is demonstrated. | They increase idle power and temperature and can affect boost behavior. |
| VBS or HVCI disable | The user accepts the security and compatibility consequences after measuring overhead. | Anti-cheat, organization policy, and security requirements can require these controls. |
| ReBAR profile override | The target driver and game build are tested in both states. | Results vary by GPU, driver, and application profile. |
| HAGS | Both states can be compared on the target Windows, GPU, and driver combination. | Results are hardware and driver dependent. |
| Alternate frame limiter | Cap accuracy and latency are measured with an appropriate tool. | Limiter behavior depends on presentation mode, driver, and workload. |

## Vendor features

AMD Radeon Boost, Radeon Chill, Fluid Motion Frames, NVIDIA Reflex, frame
generation, and similar controls change rendering, pacing, or input behavior.
The repository does not assign a permanent safe or harmful status to a feature
whose implementation can change with a driver or game update. Use current
vendor documentation, confirm anti-cheat compatibility, and compare the target
system in both states.

### AMD Anti-Lag history

An AMD Anti-Lag implementation distributed in 2023 triggered Valve Anti-Cheat
action because of how it interacted with the game process. AMD and Valve later
changed the relevant driver and game support. This history is a reason to check
current AMD and Valve compatibility information before enabling a driver-level
latency feature, not a basis for a permanent compatibility guarantee in this
repository.

Do not use old driver builds associated with the incident. The repository does
not determine whether an arbitrary current or future driver is anti-cheat safe.

## Known evidence gaps

The current workflow still exposes or applies choices that need stronger
CS2-specific evidence:

| Area | Current behavior | Missing evidence |
|---|---|---|
| XMP or EXPO | Detects state and presents guidance. | Controlled comparison that isolates memory configuration. |
| Native driver removal | Uses Windows and vendor-native removal paths. | Reproducible comparison with other cleanup methods. |
| Fullscreen compatibility settings | Applies per-application Windows values. | Controlled comparison across supported Windows presentation modes. |
| Power plan | Applies tiered settings by profile. | Isolated comparison of each tier against Windows plans on representative hardware. |
| Reflex comparison | Presents an optional choice. | Repeatable measurement with a validated capture tool. |
| NVIDIA DRS profile | Applies selected settings in higher profiles. | Isolated testing for individual settings and driver branches. |
| Resizable BAR | Presents guidance and applies profile choices where implemented. | Current CS2 comparison by GPU and driver. |
| Audio defaults | Writes a headphone-focused baseline and optional buffer experiments. | Device-specific latency and listening tests. |
| Network defaults | Writes buffer, timeout, and relay preferences. | Packet captures and current-engine traces that isolate each value. |

## Out of scope

| Area | Reason |
|---|---|
| AMD per-game profile database writes | There is no stable public write interface used by this repository. |
| Direct edits to Steam `localconfig.vdf` | The suite avoids modifying Steam's cloud-synchronized configuration and presents launch options for manual entry. |

Contributed measurements should include the hardware, firmware, Windows build,
driver, CS2 build, settings, workload, capture tool, raw results, and repeat-run
variance.
