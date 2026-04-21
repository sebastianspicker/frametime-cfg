# Windows Scheduler Optimizations — Deep Dive

> Covers Phase 1 Steps 7, 10, 11, 12, 23, 27, 28, 31, 36.

CS2's 1% low problems are almost always a scheduling problem, not a raw performance problem. When your 1% low is 40% of your average FPS, it means that 1% of your frames are taking 2.5× longer than average to render. That extra time is almost never spent on actual rendering — it's spent waiting for a CPU core to become available, waiting for a DPC to finish, or waiting for the OS to reschedule a thread that got preempted by something else.

The optimizations in this category reduce the frequency and severity of these scheduling interruptions.

---

## HAGS — Hardware-Accelerated GPU Scheduling (Step 7)

### What it is

Normally, the Windows kernel (`dxgkrnl.sys`) manages GPU memory page tables and decides when to submit work to the GPU. HAGS transfers a significant portion of this management to a small firmware scheduler that runs **on the GPU itself**. The GPU's own memory controller directly manages VRAM paging, bypassing the CPU-based kernel scheduler for memory management decisions.

The theoretical benefit: fewer CPU-to-GPU context switches, lower DPC overhead from the display kernel, faster VRAM allocation.

### Why the results were inconsistent (pre-2026)

HAGS requires three things working correctly in concert: the GPU hardware (adequate hardware queue support), the driver (WDDM 2.7+ with proper HAGS implementation), and the game engine (using the GPU memory APIs correctly).

One commonly reported variable in HAGS-related stutter reports was **Multi-Plane Overlay (MPO)** interaction with DWM compositing. The repo no longer treats "24H2 removed MPO by default" as an established Microsoft-documented fact; treat HAGS results as build-, driver-, and hardware-dependent.

### 2026 evidence

**RTX 40/50 + AMD RX 9000:** the suite currently leans HAGS ON on the basis of recent community benchmarking. Treat this as a benchmark-driven default, not as a vendor-backed universal rule.

**RTX 30 / RDNA2:** Neutral to slightly positive. Test both on your specific system.

**RTX 20 and older:** HAGS can still reduce 1% lows by 3–8% because the hardware queue implementation is less complete. Benchmark before committing.

### The suite's recommendation

The suite defaults HAGS ON for RTX 40/50 and OFF for X3D CPUs (per the X3D tuning guide). Other configurations are prompted with evidence. If you enable HAGS and your 1% lows get worse in the Phase 3 benchmark compared to baseline, disable it.

**Important:** HAGS must also be enabled if you want to use DLSS Frame Generation (DLSS 3+). If you use a GPU that supports it and want frame generation in other games, enable HAGS regardless of CS2 results.

---

## Game Mode — Why We Enable It, Not Disable It (Step 12)

This step often surprises users familiar with older optimization guides (2020–2022), which uniformly recommended *disabling* Game Mode.

### The original argument for disabling

valleyofdoom/PC-Tuning documented that Windows Game Mode could interfere with "process and thread priority boosts" for foreground applications. The mechanism: Game Mode has its own scheduling layer that occasionally conflicts with MMCSS and IFEO priority assignments, causing unexpected thread priority inversions.

### Why this recommendation was outdated

**First,** the priority interference concern is resolved by Phase 3 Step 10's IFEO PerfOptions (`CpuPriorityClass=3`). IFEO is a kernel mechanism applied at process creation — it assigns the process priority before any MMCSS or Game Mode scheduling decisions run. IFEO supersedes Game Mode scheduling. There is no interference in practice when both are active.

**Second,** the suite still prefers enabling Game Mode, but the reasoning is now narrower. Official Microsoft material supports the general idea that Game Mode prioritizes gaming activity; it does not give a strong enough contract to present "suppresses Windows Update installations during gameplay" here as a proved guarantee.

**Third,** Game Mode activates the MMCSS `Games` scheduling path for threads that register under that category — the same CPU priority boost configured in Step 27.

### Game Mode vs Game DVR — different systems

Game Mode (`AutoGameModeEnabled=1`) and Game DVR (`AppCaptureEnabled=0`) are separate Windows systems that happen to live in the same Settings panel. Step 12 enables Game Mode; Step 31 disables Game DVR (background recording overhead). These are independent choices, not contradictory.

---

## Fast Startup Disable (Step 23)

### Why this matters for MSI interrupt persistence

Windows Fast Startup (hybrid boot) is the default shutdown behavior in Windows 11. When you click Shut Down, it:
1. Logs out user sessions
2. **Saves the kernel session state to a hibernation file** (`hiberfil.sys`)
3. Powers off

On next boot, Windows resumes the saved kernel state rather than fully reinitializing the hardware. This is why Windows 11 boots in ~10 seconds vs. the 30–60 seconds of a true cold boot.

The problem: MSI (Message Signaled Interrupt) mode, configured in Phase 3 Step 2, requires a cold boot to activate. The NIC and GPU drivers read their MSI configuration during device initialization. Fast Startup skips this initialization — it resumes the saved driver state from before Step 2 was applied.

**Without disabling Fast Startup, MSI interrupt settings do not persist across shutdowns.** Phase 3 Step 2 would appear to work but silently revert every time you shut down instead of restarting. Restart (vs. Shut Down) always does a cold boot — but players shut down between sessions, not restart.

### What it does not affect

Hibernate and Sleep remain fully functional. Fast Startup only affects "Shut Down." The change is:
- **Before:** Shut Down → hybrid hibernation snapshot → fast resume
- **After:** Shut Down → full hardware teardown → full cold boot

Boot time increases by ~5–15 seconds. MSI interrupts persist correctly.

---

## MPO Disable (Step 11)

### What MPO is

Multi-Plane Overlay (MPO) is a Windows display subsystem feature that allows the Desktop Window Manager (DWM) to send multiple independent layers to the GPU for compositing, rather than pre-compositing them in software. When MPO is active, the GPU overlay hardware assembles the final frame from 2–4 independent planes.

### Why it causes microstutter in CS2

MPO was designed for media applications that display video overlays (video player behind a browser, PiP modes). In a gaming context, MPO can cause **DWM flickering** and **frame discontinuities** when:

1. The game runs in fullscreen exclusive mode
2. Windows Ink or tablet mode features request additional overlay planes
3. HDR mode transitions occur

The most common symptom is a brief frame drop or stutter visible only in fulltime measurements (CapFrameX frametime graph shows isolated spikes of 2–5ms that appear nowhere in CPU/GPU profiling — they're in the DWM layer).

### The fix

```
HKLM:\SOFTWARE\Microsoft\Windows\Dwm
    OverlayTestMode = 5  (DWORD)
```

Value 5 disables MPO globally. This is a T3 / COMPETITIVE+ setting because on some monitors (particularly ultrawide multi-plane displays) and some GPU/driver combinations, this can itself cause rendering artifacts. The fix is straightforward: delete the key and reboot.

---

## System Scheduling Tweaks (Step 27)

### MMCSS SystemResponsiveness

MMCSS reserves a percentage of CPU time for registered multimedia threads. Default is 20%.

Setting 10% gives CS2 more CPU headroom during high-load scenes — the engine's render thread and game thread compete with fewer reserved MMCSS slots. Setting 0% is documented by Microsoft as unsupported and can cause audio dropouts under MMCSS.

The suite sets 10% — half the default, audio-safe.

**NetworkThrottlingIndex is NOT set.** This value appears in almost every gaming optimization guide (typically as `0xFFFFFFFF`). djdallmann's controlled xperf measurement found that `0xFFFFFFFF` increases NDIS DPC latency compared to the default value of 10. The "10 Mbps cap" story that motivated this recommendation is Windows Vista era and doesn't apply to modern NIC drivers. The default value of 10 is correct.

### Win32PrioritySeparation

Controls how the scheduler allocates CPU time slices to foreground vs. background threads.

```
HKLM:\SYSTEM\CurrentControlSet\Control\PriorityControl
    Win32PrioritySeparation = 0x2A
```

The value `0x2A` decodes as: short quantum (bits 4-5 = 10), fixed length (bits 2-3 = 10), maximum foreground priority separation (bits 0-1 = 10). "Fixed" means every thread gets the same quantum length regardless of foreground/background state — the foreground advantage comes entirely from the scheduler's priority-based preemption (PsPrioritySeparation = 2), not from dynamic quantum adjustment. The previous value `0x26` used variable quantum, where the foreground thread received a 3× longer time slice than background threads (`PspForegroundQuantum = {6,12,18}`). 2025 Blur Busters testing showed that fixed quantum (`0x2A`) produces lower 1% low variance than variable (`0x26`) — the elimination of dynamic quantum resizing makes thread scheduling more predictable.

### MMCSS Tasks\Games

These entries set the scheduling parameters when a thread calls `AvSetMmThreadCharacteristics("Games")`:

```
Priority = 6, Scheduling Category = "High", GPU Priority = 8
```

GPU Priority 8 (max) is a DXGI hint to the GPU command queue scheduler — when CS2's render thread submits work, the GPU scheduler processes it ahead of lower-priority submissions from background processes (Chrome GPU process, video decode, etc.).

### DisablePagingExecutive

```
HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management
    DisablePagingExecutive = 1
```

Locks kernel executable code, driver code, and system DLLs in physical RAM. On systems with 32 GB+ RAM and NVMe storage, you'll never notice this — the kernel rarely pages on such systems anyway. On systems with 8–12 GB RAM or SATA SSD, it eliminates rare but catastrophic latency spikes when the kernel must fault in its own code from disk during high I/O moments.

Windows Server has this enabled by default. Client OS defaults to 0 as a power-saving measure. Safety: zero risk. It's a hint to the memory manager.

### Intel PowerThrottlingOff (auto-detected)

Intel 12th gen+ (Alder Lake, Raptor Lake, Meteor Lake, Arrow Lake) introduced P-cores and E-cores. Windows' Power Throttling mechanism can migrate threads from P-cores to E-cores when a thread appears briefly idle. The heuristic is based on recent CPU usage history.

The problem in CS2: the render thread must wait for a GPU vsync signal or a frame limiter sync point before submitting the next frame's work. During this wait, the thread is idle — brief, but measurably idle. Power Throttling can classify this as "background behavior" and migrate the thread to an E-core. The next frame's render work then runs on an E-core with ~40% lower IPC and no large L3 cache, producing a frametime spike.

`PowerThrottlingOff = 1` in `HKLM:\SYSTEM\CurrentControlSet\Control\Power\PowerThrottling` disables this migration system-wide.

The suite detects Intel hybrid CPUs via `Win32_Processor.Name` pattern matching and applies this setting **only on affected hardware**. AMD CPUs and pre-12th gen Intel do not have E-cores and are unaffected by this key.

---

## FTH Disable — The Silent Regression Trap (Step 27)

The Fault Tolerant Heap is a Windows compatibility shim that activates silently after CS2 crashes:

1. CS2 crashes (happens — driver updates, shader compilation edge cases, anti-cheat updates)
2. Windows marks CS2 as a "problematic" process
3. On the next CS2 launch, FTH activates its patched heap allocator
4. Every heap allocation in CS2 is now 10–15% slower — permanently, with no indication to the player

`HKLM:\SOFTWARE\Microsoft\FTH\Enabled = 0` disables FTH globally. The only theoretical downside: heap allocation errors that FTH would have silently papered over could surface as visible crashes. On a well-maintained system that isn't hitting heap corruption bugs, this path is never reached.

**How to check if FTH is active on your system:**

```powershell
Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\FTH\State" -ErrorAction SilentlyContinue |
    Select-Object -Property *
```

If you see `cs2.exe` or `steamwebhelper.exe` listed in the state, FTH has been activated for those processes. Step 27 clears this.

---

## Automatic Maintenance Disable (Step 27)

Windows Task Scheduler's Automatic Maintenance umbrella includes:
- Disk defragmentation (TRIM on SSDs)
- Full Defender scan
- `RunFullMemoryDiagnostic` — scans RAM for bit errors
- Diagnostic data collection and upload

By design these run at 3 AM. In practice, if the system was off at 3 AM, Windows reschedules them to "the next idle moment." A CS2 session qualifies as an idle moment for some of these tasks.

djdallmann measured `RunFullMemoryDiagnostic` consuming 12–14% CPU during active gameplay. This appears in frametime traces as a sustained ~5ms frametime increase for 30–60 seconds.

`MaintenanceDisabled = 1` prevents these from starting automatically. You can still trigger maintenance manually from Task Scheduler. This is standard on any system where timing predictability matters.

---

## Timer Resolution (Step 28)

The Windows multimedia timer (`timeBeginPeriod`) defaults to a 15.6ms resolution — the system clock "ticks" 64 times per second. Sleep calls, thread scheduling quanta, and timeout calculations are all rounded to this resolution.

CS2 internally calls `timeBeginPeriod(1)` to request 1ms timer resolution when it launches. However, this request is per-process and affects only the requesting process's timer behavior.

Step 28 enables the registry path that allows applications to request the lowest supported timer resolution (works on Windows 10 build 19041+ via `HKLM:\...\Control\Session Manager\kernel → GlobalTimerResolutionRequests = 1`). It does not itself force a permanent global 1ms timer.

**What timer resolution actually affects in CS2:** Thread sleep accuracy in the frame limiter, audio buffer timing, and the granularity of scheduler quantum transitions. The improvement is measured in frametime variance reduction, not raw FPS. On systems already at 1ms due to other applications (audio apps often request this), the setting has no additional effect.

---

## Mouse Acceleration and mouclass Queue (Step 29)

### Mouse acceleration

`MouseSpeed=0` + `MouseThreshold1=0` + `MouseThreshold2=0` in `HKCU:\Control Panel\Mouse` disables Windows pointer acceleration globally. This is the most universally applied competitive gaming setting — it ensures that moving the mouse 10cm always translates to the same cursor movement regardless of speed.

Current CS2 builds already force raw input on, so the generated `m_rawinput 1` line in Step 34 is mainly a documentation/forward-compatibility stub. Step 29 still matters because it keeps Windows pointer acceleration disabled for the desktop, menus, and any non-CS2 input path, while the CS2-side `m_mouseaccel*` guards keep the in-game config explicit.

### mouclass kernel queue

More interesting: the mouclass driver (the Windows kernel mouse class driver) maintains an internal FIFO buffer of unprocessed mouse events. The default size is **100 events**.

At 1000 Hz polling (standard for modern competitive mice), 100 events = 100ms of buffering capacity. If the kernel is slow processing events (DPC congestion, high interrupt load), it can build up a 100ms backlog before dropping events.

The suite sets this to 50 events:

```
HKLM:\SYSTEM\CurrentControlSet\Services\mouclass\Parameters
    MouseDataQueueSize = 50  (DWORD)
```

At 1000 Hz polling, 50 events = 50ms of buffering capacity. The value was previously set to 16, but 2025 testing (Overclock.net, Blur Busters) found zero measurable latency benefit at that level and occasional input skipping on systems with brief DPC congestion. Values below 30 cause event drops; 50 is the lowest safe value. This bounds kernel-side buffering without risking missed inputs.

**Source:** djdallmann's mouclass.sys kernel analysis (GamingPCSetup); 2025 community testing.

---

## Summary: The Scheduling Stack

After all steps are applied, CS2 benefits from a layered scheduling advantage:

| Layer | Mechanism | Applied By |
|-------|-----------|------------|
| Process creation | IFEO PerfOptions `CpuPriorityClass=3` | Step 10 (Phase 3) |
| Thread quantum | `Win32PrioritySeparation=0x2A` | Step 27 |
| MMCSS registration | `SystemResponsiveness=10`, `Games` category | Step 27 |
| GPU queue | MMCSS `GPU Priority=8` | Step 27 |
| Maintenance deferral | Game Mode + Automatic Maintenance disabled | Steps 12, 27 |
| P-core retention | `PowerThrottlingOff=1` (Intel hybrid) | Step 27 |
| Input latency bound | mouclass queue 100→50 | Step 29 |
| Timer granularity | Global 1ms timer resolution | Step 28 |

These layers are additive. Each one reduces the probability of a scheduling interruption during CS2's render or game loop. The cumulative effect is measurable frametime consistency improvement rather than raw FPS — which is exactly what 1% low optimization looks like in practice.
