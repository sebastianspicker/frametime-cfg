# Debunked & Contested Settings

> Settings that appear in popular CS2 optimization guides but are either ineffective, harmful, or context-dependent.
> Based on evidence from valleyofdoom/PC-Tuning, djdallmann/GamingPCSetup, and controlled community benchmarks.

---

## Settings We Skip (and Why)

These settings are actively present in popular guides. We have not included them — or have explicitly reversed course — based on evidence.

| Setting | Common Claim | Why We Don't Use It | Source |
|---------|-------------|---------------------|--------|
| `-tickrate 128` launch option | Forces 128-tick servers | Complete no-op in CS2. Sub-tick architecture means the client and server no longer operate on a fixed tick rate for input processing. The flag is parsed and silently ignored. | Valve CS2 devblog |
| `-threads N` launch option | Uses N CPU threads | Valve's own support documentation explicitly warns against this. Source 2 manages its own thread pool. Manually setting `-threads` can cause instability, crashes, or reduced performance. | totalcsgo.com (removed from their guide) |
| `tscsyncpolicy enhanced` (bcdedit) | Better TSC synchronization | valleyofdoom proved via WinDbg kernel analysis that this flag is equivalent to the default (value 0). Both resolve to the same code path. No-op with extra complexity. | valleyofdoom/PC-Tuning §11.50 |
| `NetworkThrottlingIndex = 0xFFFFFFFF` | Removes MMCSS 10 Mbps cap | Increases NDIS.sys DPC latency per djdallmann xperf testing. The default value (10) is correct for gaming. | djdallmann/GamingPCSetup |
| Disable Spectre/Meltdown mitigations | +3–10% FPS | Results vary from −2% to +8%. Causes regressions on Zen4 in some workloads. Not worth the security risk without per-system benchmarking. Valve's servers run with mitigations enabled. | Phoronix, valleyofdoom |
| `useplatformclock true` (bcdedit) | Better performance counter accuracy | Switches from TSC (fast, CPU-internal) to HPET (slow, external chip). TSC is faster and more accurate on all CPUs released after 2008. Do NOT use. | djdallmann HAL research |
| Disable Hyper-Threading / SMT | More single-thread IPC | With HT/SMT enabled, the second logical core handles OS tasks, leaving the first free for game threads. Disabling forces ALL tasks onto fewer physical cores — strictly worse on 8+ core CPUs. | valleyofdoom, Hardware Unboxed |
| `LargeSystemCache = 1` | More RAM for file caching | Server default. Allocates most free RAM to file cache, reducing RAM available to CS2. | Windows internals documentation |
| `mat_queue_mode 2` in autoexec | Async rendering boost | Source 1 CVar. No-op or undefined behavior in CS2 Source 2. | CS2 community testing |
| `r_dynamic 0` in autoexec | Reduces dynamic lighting load | Disables muzzle flash and grenade lighting. Removes gameplay-relevant visual information for minimal FPS gain. | Community testing |
| QoS DSCP tagging *(without NLA prerequisite)* | Network priority at router | Requires `Do not use NLA` registry key to actually take effect. Without it, `New-NetQosPolicy` silently succeeds but DSCP marks are never applied. The suite implements this correctly in Step 16. | Windows QoS documentation |
| `mm_dedicated_search_maxping 40` | Forces low-ping servers | Value 40 is too aggressive for low-server-density regions. Suite uses 80ms with a note to tune by region. | CS2 community feedback |
| `cl_cmdrate` in autoexec | Forces command send rate | Removed in CS2. Source 2 sends inputs every frame. The CVar is parsed and silently ignored. | CS2 Source 2 architecture |
| `net_graph 1` in autoexec | Shows network stats | Removed in CS2. Replaced by `cl_hud_telemetry_*` CVars. | CS2 changelog |
| `-nojoy` launch option | Removes controller support | Single-digit MB freed. No measurable FPS or latency impact. | ArminC-AutoExec notes |
| `-softparticlesdefaultoff` | Disables soft particle blending | Source 1 launch option. Not parsed by CS2's Source 2 engine. | CS2 Source 2 engine |
| `+cl_forcepreload 1` | Pre-loads models/textures | Source 1 behavior. Causes VRAM spike at map load without mid-game benefit. | CS2 engine testing |
| `-vulkan` (Windows) | Vulkan backend for performance | Not officially supported on Windows as a stable path. May cause crashes. | Valve CS2 platform support |
| `developer 1` | Verbose debugging output | Significant console spam and logging overhead. No competitive benefit. | CS2 developer documentation |
| `-dxlevel N` | Force DirectX version | Source 1 only. CS2 uses DirectX 11 natively. Not parsed by CS2. | CS2 Source 2 engine |
| `-high` launch option | Set High priority | Resets to Normal on exit. Not persistent. IFEO PerfOptions is superior (kernel-level, persistent). | djdallmann; Windows IFEO docs |
| AMD Radeon Boost | Higher FPS during movement | Lowers rendering resolution during camera movement — resolution drops precisely when aiming. | AMD documentation |
| AMD Radeon Chill | Power-saving FPS limiter | Variable FPS limiter. Adds latency when throttling. | AMD documentation |
| AMD Fluid Motion Frames (AFMF) | Higher perceived FPS | Frame interpolation. ~1 frame of input lag. Harmful for competitive. | AMD documentation |
| `DisablePagingFile` | Prevents page faults | Causes stuttering despite adequate RAM. Windows occasionally pages out cold-start memory for non-game processes. | valleyofdoom PC-Tuning |
| RTSS as frame limiter | Precise frame cap | Busy-wait adds ~1ms latency vs NVCP cap. Use RTSS for OSD only. | valleyofdoom PC-Tuning |
| ISLC (Standby List Cleaner) | Reduces standby RAM stutters | Only relevant on ≤12–16 GB systems under memory pressure. No-op on 32 GB+. | Wagnardsoft; valleyofdoom |
| Win11 Auto HDR | Better visuals | Tone-mapping overhead + overbright windows/sun. Disabled by Step 36. | Windows 11 feature docs |
| PS/2 keyboard lower DPC | Legacy PS/2 bypasses USB | djdallmann: USB 4–8µs vs PS/2 8µs. USB is equal or faster on modern systems. | djdallmann WINKERNEL research |
| AMD Anti-Lag VAC ban warning | Anti-Lag is banned | Resolved. Current drivers (Nov 2023+) are safe. See [AMD Anti-Lag History](#the-amd-anti-lag-incident-september-2023) below. | AMD driver release notes |
| `*InterruptModeration = Disabled` | Per-packet = lowest latency | Empirically wrong. djdallmann: Medium outperformed Disabled under real-world conditions. Background traffic causes interrupt storms. Suite uses Medium. | djdallmann Intel NIC test |
| Disable RSC | Reduces NIC latency | TCP-only feature. Zero effect on CS2's UDP traffic. | Windows NDIS architecture |
| Disable LSO (`*LsoV2*`) | Reduces NIC offload complexity | TCP-only. CS2 uses ~80-byte UDP datagrams — LSO never invoked. | Windows NDIS LSO docs |
| Disable `*UDPChecksumOffload*` | Removes checksum variable | Hardware checksum on ~80-byte packets takes nanoseconds. Disabling forces slower software computation. | NIC offload architecture |
| Disable `*TCPChecksumOffload*` | Removes TCP checksum | TCP-only. CS2 is UDP. | NIC offload architecture |
| Disable `*ARPOffload`/`*NSOffload` | Cleaner NIC state | Sleep-state-only features. Inactive during gameplay when CPU is awake. | NIC offload architecture |
| Disable `*WakeOnMagicPacket` etc. | Cleaner NIC state | Only activate when system is off/sleeping. Zero runtime effect during CS2. | NIC power management docs |
| Disable Jumbo Frames | Prevents fragmentation | CS2 UDP datagrams are ~80 bytes. Never Jumbo Frames regardless. | RFC 2923 |
| `SystemResponsiveness = 0` | 100% CPU for multimedia | Can starve NDIS interrupt scheduling. Microsoft documents 0 as unsupported. Suite uses 10. | djdallmann; Microsoft MMCSS docs |
| `netsh int tcp` settings (all) | Network optimization | TCP-only. autotuninglevel, ICW, minrto, delayedack, RACK, congestion provider, ECN — all apply only to TCP, not CS2's UDP. | Windows TCP/IP stack |
| `*ReceiveBuffers = 256` | Reduces DMA descriptor overhead | At 256, ring buffer can overflow under background traffic, causing packet drops. Suite uses 512 for safety. | NIC ring buffer design |

---

## Context-Dependent Settings

These aren't wrong — but they're not universally applicable, which is why they require user judgment:

| Setting | When Useful | When to Skip |
|---------|------------|-------------|
| `net_client_steamdatagram_enable_override 1` | Poor routing to Valve GC servers. SDR routes through Valve's backbone. | Already on a direct low-latency connection. SDR can add latency in some regions. |
| CPU C-states off (BIOS) | Tournament PCs, always-plugged desktops. | Laptops, inadequate cooling, systems at thermal limits. +5–15°C idle. |
| Disable Virtualization (BIOS) | Minor reduction in VM overhead; disables VBS/HVCI. | Systems running WSL, Docker, Android emulators. Near-zero overhead when VMs idle. |
| ReBAR force-enable | May help in games without native ReBAR support. | Suite disables per-app rBAR for CS2 (ThourCS2 2026: ~6% better 1% lows with OFF). System-wide BIOS rBAR stays enabled for other titles. |
| `DisablePagingExecutive = 1` | Systems with ≤12 GB RAM or slower storage. | NVMe + 32 GB: near-zero effect. |
| VBS/HVCI disable | Older hardware (pre-Zen2, pre-Kabylake) — up to 10% FPS impact. | Modern hardware has near-zero overhead. FACEIT/Vanguard require HVCI. |
| C-states off (BIOS) | Tournament PCs where every µs matters. | Increases idle temp 10–20°C. Interferes with Turbo/PBO frequency algorithms. |
| `SystemResponsiveness=0` vs `10` | May marginally help in CPU-bound scenarios. | 0 vs 10 delta unmeasured in CS2. Suite uses 10. |
| `Win32PrioritySeparation=0x28` vs `0x2A` | Both are "foreground boost" variants. `0x28` = long fixed quantum. | `0x2A` = short fixed quantum (2025 Blur Busters: better 1% lows). Suite uses 0x2A. |
| HAGS | Required for DLSS 3+ Frame Generation. May reduce scheduling latency. | Suite default leans ON on newer GPUs, but benchmark both. Keep claims benchmark-dependent rather than universal. |
| fTPM → Discrete TPM (AMD) | Random 1–2s stutters from SMI interrupts on some Ryzen platforms. | Systems already using hardware TPM or not experiencing fTPM spikes. |

---

## The AMD Anti-Lag Incident (September 2023)

AMD Anti-Lag reduces input lag by coordinating CPU frame submission timing with GPU readiness. The mechanism is sound. The delivery was not.

### What Happened

AMD's September 2023 implementation used `AppInit_DLLs` — a legacy Windows mechanism that injects a DLL into every GUI process. Valve's VAC flagged the foreign DLL in `cs2.exe`'s address space as a cheat vector. CS2 players using AMD Anti-Lag received VAC bans.

### The Fix

AMD redesigned the injection for the November 2023 driver release, using a method Valve whitelisted. **Anti-Lag 2** (RDNA3+, 2024) uses a game-engine-integrated SDK — no external injection.

### Current Status

| Version | Hardware | VAC Status | Mechanism |
|---------|----------|------------|-----------|
| Anti-Lag 1 (drivers 23.11+) | All RDNA | **Safe** | Driver-level injection (patched) |
| Anti-Lag 2 | RDNA3+ only | **Safe** | Game SDK integration |
| Anti-Lag 1 (drivers 23.9.x) | All RDNA | **Banned** — do not use | AppInit_DLLs injection |

If you are on a current AMD driver (late 2023+), Anti-Lag is safe. The community warnings from 2023–2024 are outdated.

---

## Why Our Approach Differs From YouTube Guides

Most CS2 YouTube optimization guides cite each other in a circular chain. A 2018 Windows 10 guide gets copy-pasted into a 2025 video. The original source (often one person's anecdotal experience) gets laundered through enough iterations that it appears to be consensus.

This suite applies a different standard: a setting must have a mechanistic explanation for why it works AND some form of measurement confirming the direction of effect. The result is fewer settings than most guides — but a higher probability of actually helping.

---

## Known Limitations & Evidence Gaps

Things the suite currently does or recommends without strong CS2-specific proof. Transparency matters.

| Gap | Current Behavior | What's Missing |
|-----|-----------------|----------------|
| **XMP/EXPO → 1% lows** | Checks and warns; caveats explained | Isolated CS2 benchmark comparing JEDEC vs. XMP on same hardware |
| **Native driver removal vs. DDU** | Uses native PowerShell removal (T1) | Side-by-side comparison: native removal vs. DDU |
| **Fullscreen Optimizations** | T1 based on m0NESY documentation | Reproduced controlled test |
| **CS2 Optimized Power Plan vs. High Performance** | Native tiered plan (T1–T3) | Isolated CS2 benchmark: each tier vs. baseline |
| **`-noreflex` 1% low improvement** | Presented as contested; user chooses | Valid benchmark methodology (non-PresentMon-2.2-affected tool) |
| **NVIDIA Profile (native)** | T3 (skipped in SAFE/RECOMMENDED) | Isolated benchmark for individual profile flags |
| **Resizable BAR in CS2** | T2 with BIOS instructions | CS2-specific 1% low test |

### Out-of-Scope by Design

| Not Addressed | Reason |
|---------------|--------|
| **AMD GPU per-game profile settings** | No public API equivalent to `nvapi64.dll` DRS. AMD stores profiles in a private SQLite database with no stable documented write interface. |
| **Steam `localconfig.vdf` (launch options)** | Writing directly risks corrupting Steam's cloud-synced config. Manual paste is safer and trivial. |

If you run controlled before/after tests on any evidence gaps and have reproducible data, contributions are welcome.
