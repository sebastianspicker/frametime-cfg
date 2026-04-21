# MSI Interrupts & NIC Affinity — Deep Dive

> Covers Phase 3 Steps 2 and 3, and `helpers/msi-interrupts.ps1`.

---

## What Interrupts Are

When a hardware device has data ready — the GPU finishes a frame, the NIC receives a packet, the audio controller needs a buffer — it signals the CPU using an interrupt. The CPU stops its current work, runs the device's Interrupt Service Routine (ISR), and then resumes.

The interrupt delivery mechanism has changed significantly since legacy PCI. Two mechanisms exist on modern systems:

### Line-Based Interrupts (INTx — legacy)

Each PCI device is wired to one of a fixed set of IRQ lines (typically IRQ 16–23 on modern systems). The interrupt controller detects which line was asserted, identifies the device, and routes the interrupt to the CPU.

Problems:
- **IRQ sharing**: multiple devices can share a line. If the GPU and audio controller share IRQ 16, the kernel must query each device to determine which one triggered the interrupt — even if only the GPU did.
- **Polling overhead**: every shared-IRQ interrupt causes unnecessary work for non-triggering devices on that line.
- **Serialized delivery**: only one interrupt per line can be in flight at a time.

### Message Signaled Interrupts (MSI)

MSI replaces line assertion with a DMA write to a special memory address. The device writes a message directly specifying which interrupt vector to invoke. The interrupt controller reads the message and routes it without polling.

Benefits:
- No IRQ sharing — each MSI-capable device gets its own vector.
- No polling overhead — the interrupt message identifies the source exactly.
- Lower and more consistent latency — the DMA write is immediate, no interrupt controller arbitration.
- **MSI-X** (the modern variant) allows a single device to have *multiple* independent interrupt vectors, one per queue. A GPU can have separate vectors for display, compute, and copy engines.

---

## Why This Matters for CS2

CS2 involves three high-frequency interrupt sources:

**GPU** — frame completion notifications, VRAM transfer completions, command buffer processing. At 300+ FPS, these fire hundreds of times per second. A GPU on a shared IRQ line causes spurious interrupt queries for every other device on that line, hundreds of times per second.

**NIC** — packet receive events at 128 packets/second for CS2 game traffic, plus background traffic (Discord, Steam, browser). Without MSI, the NIC ISR competes for the shared IRQ line with the GPU — a scenario where a frametime spike and a packet receive event can interfere with each other at the interrupt level.

**Audio** — buffer refill interrupts. Less frequent, but audio DSP on a shared IRQ line can add jitter to audio buffer delivery timing.

MSI gives each device an exclusive interrupt vector. GPU interrupts never interfere with NIC interrupts at the hardware level.

---

## What the Suite Applies

```
HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\{device-id}\{instance}\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties
    MSISupported = 1  (DWORD)
```

Applied to:
- **GPU** (Display class) — T2, RECOMMENDED+, with `MessageNumberLimit = 16` (requests MSI-X with up to 16 vectors)
- **NIC** (Net class) — T2, RECOMMENDED+
- **Audio** (Media class) — T3, COMPETITIVE+ (more variance in results across hardware)

The driver negotiates MSI vs. MSI-X based on device capability and OS support. Setting `MSISupported = 1` enables the mechanism; the driver and OS negotiate the details. On Windows 10/11 with modern NVIDIA drivers, MSI-X is typically negotiated automatically.

---

## Why a Cold Boot Is Required

Line-based interrupt routing is established by the BIOS/UEFI during hardware initialization at POST. The OS uses the ACPI tables written during POST to configure interrupt routing for the session.

MSI mode is negotiated between the OS PnP manager and the device driver during device initialization — which happens during a full cold boot, not a resume from hibernation.

**Windows Fast Startup** (the default "Shut Down" behavior on Windows 11) saves the kernel session state to `hiberfil.sys` and resumes it on next boot. This skips hardware reinitialization — the interrupt routing from the previous session persists, including line-based mode for any device that was in line-based mode when the snapshot was taken.

This is why **Step 23 (Fast Startup disable) must run before Step 2 (MSI)**. Without disabling Fast Startup, MSI settings survive only until the next Shut Down, reverting on resume. Restart (not Shut Down) always does a cold boot — but players shut down between sessions.

After applying MSI settings, do a full cold boot via Restart (not Shut Down), then shut down normally going forward.

---

## Verifying MSI Mode

### Method 1 — msinfo32

Start → Run → `msinfo32` → Components → Display → your GPU → check **IRQ**:
- Large number (e.g. `4294967294`, `0xFFFFFFF0+`): **MSI mode active**
- Small number (e.g. `16`, `17`): line-based mode

### Method 2 — Registry check

```powershell
# Find your GPU's instance path
$gpu = Get-PnpDevice -Class Display | Where-Object { $_.InstanceId -match "^PCI\\" } | Select-Object -First 1
$msiPath = "HKLM:\SYSTEM\CurrentControlSet\Enum\$($gpu.InstanceId)\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties"
(Get-ItemProperty $msiPath -ErrorAction SilentlyContinue).MSISupported
# Should return: 1
```

### Method 3 — LatencyMon

Run LatencyMon for 5 minutes (desktop, not in-game) before and after. After MSI:
- `nvlddmkm.sys` ISR execution times should decrease
- `ndis.sys` DPC times should be more consistent
- Overall highest execution should improve

If `nvlddmkm.sys` remains problematic after MSI mode, check driver version (Step 5 R570 regression) and HAGS compatibility (Step 7).

---

## NIC Interrupt Affinity (Phase 3 Step 3)

### The Core 0 Problem

Windows assigns interrupts to CPU cores using the ACPI MADT table from BIOS. Without explicit affinity configuration, the interrupt controller defaults to Core 0 for most devices — including the NIC.

Core 0 is the busiest core on a gaming system. It handles:
- OS scheduler interrupt bookkeeping
- Most Windows kernel-mode driver ISRs (by default affinity)
- Hyper-V / virtualization overhead if enabled
- Hardware timer interrupts

When the NIC fires a receive interrupt and Core 0 is already processing a game thread instruction burst, the NIC interrupt must wait for Core 0 to be interruptible. This delay appears as packet receive latency jitter — variable timing between when the NIC DMA-lands the packet and when the kernel TCP/UDP stack processes it.

### The Fix

```
HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\{NIC instance}\Device Parameters\Interrupt Management\Affinity Policy
    DevicePolicy = 4              (SpecifiedProcessors)
    AssignmentSetOverride = 0x08  (Core 3, for example — the last physical core)
```

`DevicePolicy = 4` means "use the explicit core mask". `AssignmentSetOverride` is a bitmask where bit N corresponds to logical processor N.

The suite targets the **last physical core** (not Core 0 or Core 1). Core 0 has OS overhead. Core 1 is its SMT sibling. The last core typically has the least contention.

This is T3 / COMPETITIVE+ because the correct target core depends on your CPU layout. On an Intel 12th gen+ system with E-cores, pinning to the wrong core can reduce performance.

---

## RSS — Receive Side Scaling

RSS is a related but distinct mechanism. MSI sets the interrupt delivery mode; RSS controls how receive-side interrupt processing is distributed across cores once an interrupt fires.

Many NIC drivers (notably Intel I219-V, I225-V, I226-V — the most common gaming board NICs as of 2025) omit RSS registry entries. Without them, all receive processing runs on Core 0, regardless of MSI mode.

The suite adds missing RSS entries to the NIC driver key:

```
*RSS               = 1  (master switch — created/enabled if absent or 0)
*RSSProfile        = 1  (ClosestProcessor — use cache-local core)
*RssBaseProcNumber = 2  (start from Core 2, skip Core 0/1)
*MaxRssProcessors  = 4  (spread across up to 4 cores; 8 for 5+ GbE)
*NumRssQueues      = 4  (explicit queue count; 8 for 5+ GbE)
```

**RSS master switch (`*RSS`):** Some Realtek drivers ship with `*RSS=0` (or the key missing entirely), which silently ignores all RSS sub-parameters. The suite checks and enables `*RSS` before writing the queue/processor settings.

**Speed-aware queue count:** NICs at 5+ Gbps link speed (e.g., Realtek RTL8126 5 GbE) use 8 RSS queues and 8 max processors instead of 4, to handle higher packet rates without saturating individual cores. The suite detects link speed via `$nic.Speed` and scales automatically. Queue count is also capped at the actual processor count to avoid driver errors.

These are added only if absent — existing values are never overwritten. The Intel I225-V/I226-V are specifically flagged because they are known to ship without these entries on most OEM board drivers.
