# Process Priority & CCD Affinity — Deep Dive

> Covers Phase 3 Step 10 and `helpers/process-priority.ps1`.

IFEO PerfOptions handles CPU priority for every system. The suite does not automatically pin a dual-CCD Ryzen X3D processor because the Windows data it reads does not authoritatively map logical processors to CCDs.

---

## IFEO PerfOptions — Persistent High Priority

### What IFEO is

Image File Execution Options (IFEO) is a Windows kernel mechanism, primarily known for attaching debuggers to processes. Its `PerfOptions` subkey is less-known: the kernel reads it at process creation time and applies the specified process priority before the process entry point runs.

```
HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\cs2.exe\PerfOptions
    CpuPriorityClass = 3  (DWORD)
```

`CpuPriorityClass` maps to the kernel `PROCESS_PRIORITY_CLASS` enum:

| Value | Priority Class |
|-------|---------------|
| 1 | Idle |
| 2 | Normal |
| 3 | High |
| 4 | Realtime |
| 5 | Below Normal |
| 6 | Above Normal |

Value `3` (High) is the correct choice for CS2. Realtime (`4`) is never appropriate — it starves system interrupt handlers and causes audio dropouts and input processing failures.

### Why IFEO beats the `-high` launch flag

The `-high` Steam launch flag sets process priority via the Win32 `SetPriorityClass` API after the process is already running. It works, but:

- It takes effect after the process has already started (initial thread scheduling at Normal)
- Steam applies it, then the process can re-set itself to Normal — some games do this
- It's a per-launch-flag, not persistent — stripped if you edit launch options

IFEO `PerfOptions` is applied by the kernel at `NtCreateProcess` — before the process entry point. It cannot be bypassed by the process itself and is persistent across any launch method (Steam, desktop shortcut, command line).

### Zero overhead

IFEO is a registry read that happens once at process creation. There is no background service, no polling, no daemon watching for CS2 to launch. The kernel reads the key, applies the priority, and that is the end of it.

---

## Why High Priority Helps

CS2's game thread and render thread compete with background processes for CPU time. On Windows, the scheduler gives threads at equal priority equal quantum time — if Steam update threads, browser GPU processes, or antivirus workers are at Normal priority and CS2 is also at Normal, they all compete.

High priority gives CS2 threads scheduler preference over Normal-priority processes. The quantum is the same, but preemption rules favor the higher-priority process when both want the CPU simultaneously.

**The effect is most visible in 1% lows**, not average FPS. Average FPS is CPU-bound work. 1% lows include scheduling interruptions — frames where CS2 had to wait for a CPU that was briefly occupied by a Normal-priority task.

---

## X3D CCD Topology — Manual Verification Required

### The V-Cache topology problem

AMD Ryzen X3D processors use 3D-stacked cache (V-Cache) — a second SRAM die stacked on top of the standard L3 cache, tripling its capacity. This dramatically reduces cache miss latency for game workloads.

The problem: not all X3D chips have V-Cache on all cores.

| Processor | CCDs | V-Cache |
|-----------|------|---------|
| 5700X3D, 5800X3D, 7800X3D, 9800X3D | 1 | All cores — no pinning needed |
| 7900X3D, 7950X3D, 9900X3D, 9950X3D | 2 | V-Cache CCD only, but only after machine-specific topology verification |

Model identity and aggregate `Win32_Processor` core/logical-processor counts are not an LP-to-CCD map. Windows processor numbering, processor groups, firmware, and scheduler behavior mean that deriving a mask such as `0x00FF00FF` from those counts can pin CS2 to the wrong CPUs.

The suite therefore keeps only the persistent IFEO priority change. For dual-CCD X3D systems it reports that topology must be verified manually and creates no affinity task or live-process affinity assignment. Do not apply a mask unless it comes from a trustworthy, machine-specific LP-to-CCD source and has been verified for the current firmware and Windows configuration.

---

## Rollback

IFEO key: delete `HKLM:\...\Image File Execution Options\cs2.exe\PerfOptions` (or the entire `cs2.exe` subkey if empty).

The suite no longer creates an affinity task. If an older suite run created `CS2_Optimize_CCD_Affinity`, remove it with `Unregister-ScheduledTask -TaskName "CS2_Optimize_CCD_Affinity" -Confirm:$false` and delete its affinity script after confirming that it is no longer needed. `Restore-Interactive` also handles recorded older tasks.
