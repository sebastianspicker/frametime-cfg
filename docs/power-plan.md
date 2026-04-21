# Power Plan — Deep Dive

> Covers Phase 1 Step 6 and `helpers/power-plan.ps1`.

The CS2 Optimized Power Plan is the single highest-confidence optimization in the suite. Most other tweaks have situational or hardware-dependent impact. Power plan settings affect every CPU clock decision the OS makes, and every CS2 frame involves hundreds of CPU clock decisions.

---

## Why Power Plans Matter More Than Expected

Windows power plans control over 100 kernel-level settings. The **Balanced** plan (Windows default on laptops; common on OEM desktop installs) is designed to minimize power consumption while maintaining adequate responsiveness. For a web browser or office application, "adequate" means responding within 15–50ms. For CS2, "adequate" means responding within 7.8ms (one tick).

The core problem: **core parking** and **C-state deep sleep**.

### Core parking

The Windows scheduler parks idle CPU cores to save power. "Parked" means the core is in a low-power state with its L1/L2 cache partially flushed. When the scheduler needs that core for a game thread, it must:

1. Signal the core to wake (10–50µs)
2. Wait for the core to exit its power state (5–50µs depending on depth)
3. Migrate the thread to the now-active core (5µs)

The total wake-up latency for a parked core is typically 20–100µs. CS2 uses 12–16 threads simultaneously across a full match. On a 6-core CPU with 4 cores parked (Balanced plan behavior), threads regularly need to wake parked cores. This shows up as irregular frametime spikes.

### C-state deep sleep

Between instruction bursts, the CPU itself enters idle states:
- **C0** — active
- **C1** — halt (1µs exit latency)
- **C1E** — enhanced halt (10µs exit latency)
- **C6** — deep package sleep (100µs+ exit latency)
- **C7/C8** — full package sleep (200µs+ exit latency)

The Balanced plan allows all C-states including C6 and deeper. A CPU core that enters C6 between game thread executions takes 100–300µs to return to C0. At 128 ticks/second, CS2 has 7.8ms per tick. A single 300µs wakeup is 4% of the entire tick budget for that frame — before any rendering has begun.

---

## The FPSHeaven Plan — What We Took and What We Fixed

The FPSHeaven 2026 power plan was reverse-engineered from its `.pow` binary hive format. Every setting was decoded against the Windows power plan GUID database. We identified **four bugs** and corrected them.

### Bug 1: System Cooling Policy (SAFE — T1)

FPSHeaven ships `SYSCOOLPOL = 0` (Passive cooling policy). Passive means Windows throttles CPU frequency *before* ramping the CPU fan. On a desktop tower cooler, this causes thermal throttling at ~80°C while the fan runs at 30%.

Active policy (1) runs fans at full speed first, throttles only as a last resort. On any system with an actively cooled CPU, Active is universally correct. We apply this in T1 (always active).

**FPSHeaven value:** 0 (passive) → **Our value:** 1 (active)

### Bug 2: CPU Min Performance State — AMD vs Intel (T2)

FPSHeaven sets `PROCTHROTTLEMIN = 100%` universally. This locks Intel CPUs at their base clock minimum — correct for Intel (prevents clock dips during brief lulls). But on AMD Ryzen:

AMD's **Precision Boost 2 (PB2)** is a firmware algorithm that decides clock speeds based on thermal headroom, load, and quality-of-service signals. The OS sends *hints* via CPPC2 (Collaborative Processor Performance Control); the CPU firmware decides the actual frequency.

Setting `PROCTHROTTLEMIN = 100%` tells the OS to lock the frequency minimum at 100% — but this bypasses PB2 entirely. The OS takes over frequency decisions from the CPU firmware, which knows less about thermal headroom and responds more slowly. The result: AMD Ryzen locked at 100% minimum burns more power and actually achieves *lower* boost clocks because PB2's thermal algorithm is no longer operating.

**The correct value for AMD:** `PROCTHROTTLEMIN = 0%` — this gives the OS hint "minimum 0%" and lets CPPC2 pass EPP=0 hints to PB2, which then decides clocks using its own (superior) algorithm.

On Ryzen 7950X3D in testing, the difference between 0% (correct) and 100% (broken FPSHeaven value) was **~200MHz lower sustained all-core boost** at the same thermal conditions.

**FPSHeaven value:** 100% (both Intel and AMD) → **Our value:** 0% (AMD only), 100% (Intel only)

### Bug 3: Duty Cycling (T3)

Duty cycling inserts mandatory frequency reduction pauses when the CPU approaches thermal limits — typically 5ms every 100ms at threshold. FPSHeaven ships with duty cycling **enabled (1)**.

Duty cycling is a thermal safety mechanism, but it operates by creating forced frequency pauses — which appear in CS2 frametime traces as small but consistent 5ms spikes every 100ms exactly when the CPU is near throttle temperature. These spikes correlate exactly with round phases where GPU/CPU load is high (opening duels, active flashes).

Disabling duty cycling (0) allows thermal throttling to occur naturally via gradual frequency reduction rather than forced pause-and-resume cycles. The CPU handles high-temperature scenarios more gracefully, and the regular 5ms spike pattern disappears.

**FPSHeaven value:** 1 (duty cycling enabled) → **Our value:** 0 (disabled, T3 only)

### Bug 4: PERFAUTONOMOUS — Left Intentionally (Not Applied)

FPSHeaven sets `PERFAUTONOMOUS = 0` (disable autonomous CPPC). This is the most dangerous bug in the plan.

On AMD Ryzen, CPPC2 autonomous mode is how PB2 receives the continuous load/quality feedback it uses to boost clocks. Disabling autonomous CPPC removes this feedback channel. On Intel 12th gen+, Thread Director relies on autonomous scheduling to route threads between P-cores and E-cores. Disabling it forces the OS to make these decisions without hardware feedback.

**We do not apply this setting.** The power plan works without it, and applying it would negate the AMD bug fix (Bug 2) by breaking CPPC2 entirely.

---

## The Tier Structure — Why Not Just "Apply Everything"

The plan is divided into three tiers applied based on your chosen profile.

### T1 — Always Applied (9 settings)

These settings are either objectively better for any gaming system with no meaningful tradeoffs, or fixing a bug in the original:

| Setting | Value | Why Always Safe |
|---------|-------|----------------|
| CPU max perf state | 100% | Never let the governor voluntarily cap below max clock |
| Core parking max | 100% | All cores active at all times; eliminates park/unpark latency |
| USB selective suspend | Disabled | Eliminates USB host controller wakeup latency (~2ms per event) |
| Disk idle timeout | 0 (never) | No spindown latency; especially important on SATA SSD |
| AHCI HIPM off (partial) | Host-initiated power management off | First layer of SATA power management disabled |
| Standby timeout | 0 (never) | No accidental sleep during long warmup sessions |
| Hibernate timeout | 0 (never) | No accidental hibernation during long sessions |
| System cooling policy | 1 (active) | Bug fix — passive cooling is wrong for gaming desktops |
| PCIe ASPM | Off | Windows software ASPM can pull GPU/NIC/NVMe into lower link states between frames |

**PCIe ASPM GUID fix:** The previous implementation used incorrect GUIDs for the PCIe ASPM power setting, meaning the ASPM disable was never actually applied. Fixed to use the correct subgroup (`501a4d13-42af-4429-9fd1-a8218c268e20`) and setting GUID (`ee12f906-d277-404b-b6da-e5fa1a576df5`).

**PCIe ASPM** deserves explanation: even with BIOS ASPM disabled, Windows maintains an independent software ASPM layer. Between frames (when the GPU briefly idles while waiting for the CPU to submit the next frame's draw calls), Windows can transition the PCIe link to L1 or L1.1. The link has a fixed re-entry latency (50–200µs). Setting PCIe ASPM to Off in the power plan prevents these software-initiated link state transitions, ensuring the PCIe bus is always at full speed when needed.

### T2 — RECOMMENDED+ (15–16 settings)

Settings with hardware dependencies or mild thermal tradeoffs:

- **EPP = 0** — tells the firmware to prioritize maximum performance. Correct for gaming desktops; on laptops may increase heat under sustained load.
- **CPU min perf state (AMD/Intel split)** — as described in Bug 2 above
- **Max idle state C1E** — allows C1 and C1E but blocks C6/C8 deep sleep. C1/C1E exit in <10µs; C6 exits in >100µs.
- **AHCI adaptive link-power timeout tuning** — the hidden storage setting discussed here is an AHCI adaptive link-power control, not NVMe/APST. The repo no longer presents that GUID as NVMe-specific control.
- **Wi-Fi power saving off** — relevant for wireless players; reduces ping variance during transitions

### T3 — COMPETITIVE+ (5 settings)

Settings with real, measurable thermal costs that require the user to consciously accept:

- **All C-states off** — eliminates all idle states. CPU never leaves C0. Eliminates >100µs wake latency entirely. Cost: +5–15°C idle temperature, louder fans, higher power draw.
- **Duty cycling off** — as described in Bug 3 above
- **Performance history count = 1** — no averaging; governor responds to instantaneous load
- **Performance increase time = 0** — minimum documented interval setting for faster ramp checks
- **Performance decrease time = 100** — maximum documented interval setting so the CPU does not drop quickly during brief load dips

---

## Per-Setting Technical Details

### Why USB Selective Suspend Causes Mouse Input Stalls

Windows powers down "idle" USB devices to save energy. The USB host controller determines idleness. A standard optical gaming mouse generates no traffic between movements — the host controller may classify it as idle within 125ms (one USB poll cycle of inactivity).

When the next mouse movement arrives, the host controller must:
1. Resume the USB device (0.5–2ms depending on suspend depth)
2. Process the buffered events
3. Deliver to HID driver

This 0.5–2ms latency is intermittent and irregular — it appears as mouse input stutters that disappear immediately after the mouse starts moving (because the device is no longer idle). Disabling USB selective suspend keeps the host controller always active.

### The Storage Power-Management Caveat

Older revisions of this repo described the hidden storage setting as NVMe/APST control. That was wrong. The corrected code and this document now treat the relevant hidden setting as the **AHCI adaptive link-power timeout**. That makes it relevant to SATA/AHCI storage power management, not a general NVMe/APST master switch.

The practical conclusion is narrower than older versions of this guide suggested: the suite is still opinionated about reducing storage-side latency transitions where Windows exposes them, but it no longer claims to disable NVMe APST through that particular GUID.

---

## Verifying the Power Plan

### Check that the plan is active

```powershell
powercfg /getactivescheme
```

Should show "CS2 Optimized" with the plan's GUID.

### Check specific settings

```powershell
$guid = (powercfg /getactivescheme | Select-String -Pattern '(\{[0-9a-f-]+\})' | ForEach-Object { $_.Matches[0].Value })
powercfg /query $guid SUB_PROCESSOR PROCTHROTTLEMAX   # Should be 100
powercfg /query $guid SUB_PROCESSOR PROCTHROTTLEMIN   # Should be 0 (AMD) or 100 (Intel)
powercfg /query $guid SUB_PROCESSOR IDLEDISABLE        # Should be 1 (T3 only — COMPETITIVE+)
```

### Verify with HWiNFO64

Run CS2 benchmark map while HWiNFO64 monitors CPU frequencies. Look for:
- No clock drops below base frequency during active gameplay (T1 effect)
- Smooth frequency curves without sawtooth patterns (T3 duty cycle effect)
- Consistent maximum boost across cores (EPP=0 T2 effect)

If you see regular clock drops to base frequency during active gameplay, check that the correct plan is active and that no other power management software (Ryzen Master, Intel XTU, laptop OEM utility) is overriding it.
