# Process Priority & CCD Affinity — Deep Dive

> Covers Phase 3 Step 10 and `helpers/process-priority.ps1`.

Two mechanisms work together here. IFEO PerfOptions handles CPU priority for every system. The CCD affinity scheduled task only applies to dual-CCD Ryzen X3D processors.

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

## X3D CCD Affinity — Dual-CCD Only

### The V-Cache topology problem

AMD Ryzen X3D processors use 3D-stacked cache (V-Cache) — a second SRAM die stacked on top of the standard L3 cache, tripling its capacity. This dramatically reduces cache miss latency for game workloads.

The problem: not all X3D chips have V-Cache on all cores.

| Processor | CCDs | V-Cache |
|-----------|------|---------|
| 5700X3D, 5800X3D, 7800X3D, 9800X3D | 1 | All cores — no pinning needed |
| 7900X3D, 7950X3D, 9900X3D, 9950X3D | 2 | **CCD0 only** — CCD1 is plain cache |

**9900X3D asymmetric CCD fix:** The 9900X3D has an 8+4 core layout (8 cores on CCD0 with V-Cache, 4 cores on CCD1). Earlier code assumed all dual-CCD X3D chips had symmetric CCDs (e.g., 8+8 or 6+6). The suite now detects the 9900X3D specifically and uses `ccd0Cores = 8` for the correct affinity mask, instead of `Floor(totalCores / 2) = 6`.

On a 7950X3D, cores 0–7 (CCD0) have 96 MB of L3 cache per CCD. Cores 8–15 (CCD1) have 32 MB of standard L3. If CS2's game thread runs on CCD1, it has one-third the cache capacity, leading to significantly more cache misses during game state lookups.

AMD's own game detection heuristic in AGESA firmware is supposed to route CS2 to CCD0 automatically. In practice, as of early 2026, this heuristic is unreliable — it can route CS2 to CCD1 during the initial seconds of a match, and Windows thread migration can move threads between CCDs during gameplay.

### The affinity mask

For a 16-core/32-thread 7950X3D with SMT enabled:

- Physical cores 0–7: CCD0 (V-Cache)
- Physical cores 8–15: CCD1 (plain)
- Logical processors 0–15: first SMT thread of each core
- Logical processors 16–31: second SMT thread (SMT partners)

CCD0 affinity mask covers logical processors 0–7 (first threads) and 16–23 (SMT partners):

```
LP 0  = CCD0 Core 0, Thread 0  → bit 0
LP 1  = CCD0 Core 1, Thread 0  → bit 1
...
LP 7  = CCD0 Core 7, Thread 0  → bit 7
LP 16 = CCD0 Core 0, Thread 1  → bit 16
...
LP 23 = CCD0 Core 7, Thread 1  → bit 23
```

Mask = `0x00FF00FF` for 7950X3D (hex), computed by the helper:

```powershell
$ccd0Cores = 8  # half of 16 total cores
[long]$mask = 0
for ($i = 0; $i -lt 8; $i++) {
    $mask = $mask -bor (1L -shl $i)          # First thread
    $mask = $mask -bor (1L -shl (16 + $i))   # SMT partner
}
# $mask = 0x00FF00FF
```

### Why a scheduled task, not a registry key

Process affinity cannot be set persistently via registry — Windows does not have an IFEO equivalent for affinity. The only way to enforce it is to set `Process.ProcessorAffinity` on the running process.

The scheduled task runs `cs2_affinity.ps1` every 2 minutes after logon:

```powershell
$procs = Get-Process cs2 -ErrorAction SilentlyContinue
if ($procs) {
    foreach ($p in $procs) {
        if ($p.ProcessorAffinity -ne [IntPtr]$mask) {
            $p.ProcessorAffinity = [IntPtr]$mask
        }
    }
}
```

Each execution takes ~50ms and only modifies affinity if it has drifted. The task is hidden and runs at `HighestAvailable` privilege under the logged-on user.

### Verifying affinity

With CS2 running, open Task Manager → Details tab → right-click `cs2.exe` → Set affinity. The checked cores should match CCD0 (cores 0–7 on a 16-core X3D). If all cores are checked, the task has not run yet or failed.

Manual check in PowerShell:

```powershell
(Get-Process cs2).ProcessorAffinity.ToInt64().ToString("X")
# Should output: FF00FF (7950X3D example)
```

---

## Rollback

IFEO key: delete `HKLM:\...\Image File Execution Options\cs2.exe\PerfOptions` (or the entire `cs2.exe` subkey if empty).

Affinity task: `Unregister-ScheduledTask -TaskName "CS2_Optimize_CCD_Affinity" -Confirm:$false`. The affinity script at `C:\CS2_OPTIMIZE\cs2_affinity.ps1` can also be deleted.

Both are handled automatically by `Restore-Interactive` (START.bat option 7).
