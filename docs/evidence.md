# Optimization Evidence & Risk Analysis

> Per-optimization impact estimates, cumulative improvement ranges, and risk trade-off analysis.
> See the [README](../README.md) for the phase step tables.

---

## Optimization Evidence Table

Every optimization tracked by this suite, with estimated impact ranges from isolated benchmarks and community testing.

| Optimization | Tier | Risk | 1% Low (%) | Avg FPS (%) | Confidence | Source | Note |
|---|---|---|---|---|---|---|---|
| Clear Shader Cache | T1 | SAFE | 0–5 | 0–1 | HIGH | Industry standard; NVIDIA/AMD docs | Only if stale shaders present |
| Fullscreen Optimizations | T1 | SAFE | 1–5 | 0–2 | HIGH | m0NESY (G2); ThourCS2 | FSE prevents DWM compositing |
| CS2 Optimized Power Plan | T1 | MODERATE | 2–8 | 1–5 | HIGH | fREQUENCYcs/FPSHeaven; valleyofdoom | Tiered: T1 parking/USB, T2 CPU freq (vendor), T3 C-states off |
| FPS Cap (NVCP) | T1 | SAFE | 5–20 | −9 (intentional) | HIGH | Blur Busters; FPSHeaven methodology | Stabilizes frametimes |
| Clean Driver Install | T1 | SAFE | 2–10 | 0–5 | HIGH | NVIDIA/AMD official recommendation | Bloat-free driver |
| Mouse Acceleration Off | T2 | SAFE | 0 | 0 | HIGH | Universal competitive standard | Input consistency, not FPS |
| Game DVR / Game Bar Off | T2 | SAFE | 0–3 | 0–2 | MEDIUM | Microsoft docs; community testing | Background recording overhead |
| Disable Overlays | T2 | SAFE | 0–3 | 0–2 | MEDIUM | Community consensus | Overlay rendering overhead |
| Autoexec CVars | T2 | SAFE | 0–2 | 0 | MEDIUM | Valve console documentation | Network interpolation tuning |
| MSI Interrupts | T2 | MODERATE | 0–5 | 0–1 | MEDIUM | valleyofdoom/PC-Tuning | Reduces DPC latency |
| HAGS Toggle | T2 | MODERATE | −3–5 | −2–3 | MEDIUM | ThourCS2; Blur Busters 2026; community benchmarking | Suite default leans ON on newer GPUs, but benchmark both on your own system |
| NIC Tweaks | T2 | MODERATE | 0–3 | 0 | LOW | LatencyMon community guides | Only if NIC DPC spikes detected |
| Debloat | T2 | MODERATE | 0–2 | 0–1 | LOW | Background process reduction | Fewer background tasks |
| Timer Resolution | T2 | SAFE | 0–2 | 0 | MEDIUM | valleyofdoom/PC-Tuning | More precise system timer |
| SysMain Disable | T3 | MODERATE | 0–3 | 0–1 | LOW | Community consensus | Only on HDD or low RAM |
| Visual Effects | T3 | SAFE | 0–1 | 0–1 | LOW | DWM overhead reduction | Minimal impact |
| VBS/Core Isolation Off | T2 | MODERATE | 2–8 | 1–5 | MEDIUM | Microsoft VBS docs; Phoronix benchmarks | Removes hypervisor overhead on OEM Win11. Skip if FACEIT/Vanguard. |
| Windows Update Blocker | T3 | CRITICAL | 0 | 0 | N/A | Security trade-off | Disables security updates — not recommended |

> **Reading this table:** "1% Low" is frametime consistency (higher = fewer stutters). "Avg FPS" is average framerate. Negative values mean intentional reduction (FPS Cap) or possible regression (HAGS on older GPUs). Confidence reflects the quality and reproducibility of the evidence.

---

## Cumulative Theoretical Improvement

These ranges assume all applicable improvements stack independently. Real-world gains are typically 30–60% of these values.

| Scenario | Steps Applied | 1% Low Range | Avg FPS Range | Risk Level |
|---|---|---|---|---|
| **SAFE Profile** (T1 only) | Shader, FSO, Power Plan, FPS Cap, Driver | +10–48% | +1–14% (excl. cap) | Minimal (1 MODERATE step) |
| **RECOMMENDED** (T1 + safe T2) | + Game DVR, Overlays, Autoexec | +12–56% | +1–18% | Low |
| **COMPETITIVE** (T1 + T2 + T3) | + MSI, HAGS, NIC, Timer, SysMain, Debloat, Visual | +12–65% | +1–20% | Moderate |
| **CUSTOM** (everything) | + Windows Update Blocker | +12–65% | +1–20% | High |

> **Why the wide ranges?** A bloated system with stale drivers and no power plan will see large gains. A clean Windows install with recent drivers might see almost nothing. The 1% low improvement is where most users see the biggest difference — that's frametime consistency, not raw FPS.

---

## Risk Trade-off Analysis

### Risk Categories Explained

Every step in the suite is tagged with a risk level that determines whether it runs automatically, requires confirmation, or is skipped entirely — depending on your chosen profile. This isn't just a label: each risk category reflects a real engineering trade-off.

| Risk Level | What It Means | Reversible? | Example | What Could Go Wrong |
|---|---|---|---|---|
| **SAFE** | Read-only check, or universally beneficial change with no side effects | Yes, trivially | Shader cache wipe, fullscreen optimizations, IFEO priority | Nothing — these are inherently safe operations |
| **MODERATE** | Changes Windows behavior in a way that's beneficial for gaming but affects the whole system | Yes, with backup/restore | MSI interrupts, HAGS, NIC tweaks, registry power settings | Other apps may behave slightly differently; rare device compatibility issues |
| **AGGRESSIVE** | Disables Windows services or modifies boot config; edge cases possible on unusual hardware | Yes, but requires knowledge | SysMain disable, aggressive debloat, driver rollback | Service-dependent apps may fail; boot config changes survive reset |
| **CRITICAL** | Security implications; modifies system integrity | Yes, but risky to leave on | Deep driver removal in Safe Mode | Windows Update issues; driver installation complications |

### Why Some "Obvious" Settings Aren't SAFE

**MSI Interrupts (MODERATE):** Writing `MSISupported=1` to a device's registry key switches it from legacy line-based interrupts to Message Signaled Interrupts. This is objectively better technology — lower latency, no IRQ sharing. So why MODERATE and not SAFE? Because not all devices properly support MSI. A network adapter that claims MSI support but has a buggy firmware implementation can cause intermittent packet loss. The suite enables MSI for GPU, NIC, and Audio — all well-tested classes — but the possibility of device-specific issues makes this MODERATE.

**HAGS (MODERATE):** Hardware-Accelerated GPU Scheduling hands VRAM page management from the Windows kernel (`dxgkrnl.sys`) to the GPU's own scheduler. On paper, this reduces CPU overhead. In practice, results vary wildly: +5% on some systems, −3% on others, depending on GPU generation, driver version, and game engine. You must benchmark both states on your specific hardware.

**Process Priority IFEO (SAFE):** Setting `CpuPriorityClass=3` via IFEO is SAFE because High priority is the standard recommended level for games, the change is trivially reversible (delete the registry key), and it cannot cause system instability — the Windows scheduler handles High priority processes correctly by design.

**Game Mode — Why We ENABLE It (Step 12):** Many optimization guides from 2020–2022 recommended *disabling* Game Mode, citing a valleyofdoom/PC-Tuning finding about "thread priority interference." This recommendation has not been reproduced in CS2-specific benchmarks, but the repo no longer presents Game Mode as a proved Windows Update suppression contract either. The narrower claim is that Game Mode remains the Windows gaming-default scheduling choice, while Step 27 separately tunes MMCSS and scheduler behavior. Critically, Game Mode and Game DVR/Bar are *separate systems* despite living in the same Windows Settings panel: Step 31 correctly disables DVR (recording overhead), while Step 12 keeps the game-priority path enabled.

**Intel Power Throttling (SAFE, auto-detected):** Intel 12th gen+ CPUs (Alder Lake and newer) introduced a hybrid architecture with Performance cores and Efficiency cores. Windows' "Power Throttling" feature can migrate threads from P-cores to E-cores during brief load troughs — a problem when CS2's render thread gets briefly deprioritized and shifted to an E-core with ~40% lower IPC. Setting `PowerThrottlingOff=1` in the `Control\Power\PowerThrottling` key disables this behavior system-wide. The suite detects Intel hybrid CPUs automatically and only applies this on affected hardware.

### Are Higher-Risk Categories Worth It?

| Category | Typical Gain | Typical Risk | Verdict |
|---|---|---|---|
| **SAFE** | +10–48% 1% lows | None | **Always worth it** |
| **MODERATE** | +2–8% 1% lows | Rarely causes issues; easily reversed | **Generally worth it** for gaming PCs |
| **AGGRESSIVE** | +0–5% 1% lows | May cause issues on some configurations | **Only if you benchmark before/after** |
| **CRITICAL** | 0% FPS gain | Disables security updates | **Not recommended** unless tournament PC |

> **Bottom line:** SAFE and MODERATE optimizations cover ~90% of achievable gains with minimal risk. AGGRESSIVE adds marginal improvement. CRITICAL adds zero FPS benefit and significant security risk. Every change is automatically backed up and can be rolled back individually — see [Undo / Rollback](../README.md#undo--rollback).

---

## Step Decision Matrix

Shows exactly how each step behaves under every profile. **DRY-RUN** is a modifier on top of any profile — it preserves the profile's skip/prompt/auto logic but replaces all registry, boot config, and power plan writes with preview messages. No system state changes under DRY-RUN.

| Symbol | Meaning |
|--------|---------|
| `auto` | Applied automatically — no prompt shown |
| `prompted` | Full info card shown (risk / improvement / side effects / undo); user confirms yes or no |
| `skip` | Not applied at this profile level |
| `info` | Informational display only — no system changes made |
| `—` | Auto-completes silently (prep, reserved, or transition step) |

### Phase 1 — 38 Steps

| # | Step | Tier | Risk | SAFE | RECOMMENDED | COMPETITIVE | CUSTOM | Notes |
|---|------|------|------|------|-------------|-------------|--------|-------|
| 1 | Config + profile selection | — | — | `auto` | `auto` | `auto` | `auto` | Required setup; always runs |
| 2 | XMP/EXPO check | T1 | SAFE | `auto` | `auto` | `auto` | `prompted` | Read-only BIOS guide if inactive |
| 3 | Shader cache clear | T1 | SAFE | `auto` | `auto` | `auto` | `prompted` | First CS2 launch +30–60s recompile |
| 4 | Fullscreen Optimizations off | T1 | SAFE | `auto` | `auto` | `auto` | `prompted` | Needs cs2.exe path |
| 5 | NVIDIA driver rollback check | T2 | AGGRESSIVE | `skip` | `skip` | `prompted` | `prompted` | Only shown if NVIDIA + R570+ installed |
| 6 | CS2 Optimized Power Plan | T1 | MODERATE | `auto`† | `auto`† | `auto`† | `prompted` | †T1 settings always; T2 added in RECOMMENDED+; T3 in COMPETITIVE+ |
| 7 | HAGS | T2 | MODERATE | `skip` | `prompted` | `prompted` | `prompted` | AMD/Intel GPU: informational only |
| 8 | Pagefile fixed size | T2 | MODERATE | `skip` | `prompted` | `prompted` | `prompted` | Auto-skipped if RAM ≥ 32 GB regardless of profile |
| 9 | Resizable BAR / Smart Access Memory | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | BIOS guide only — no PowerShell changes |
| 10 | Dynamic Tick | T3 | MODERATE | `skip` | `skip` | `prompted` | `prompted` | Reversible: `bcdedit /set disabledynamictick no` |
| 11 | Disable MPO | T3 | SAFE | `skip` | `skip` | `prompted` | `prompted` | Eliminates DWM multiplane microstutter |
| 12 | Enable Windows Game Mode | T3 | SAFE | `skip` | `skip` | `prompted` | `prompted` | Windows gaming-default scheduling path; DVR remains separately disabled |
| 13 | Gaming Debloat | T2 | MODERATE | `skip` | `prompted` | `prompted` | `prompted` | AppX removal is permanent |
| 14 | Autostart cleanup | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | Apps stay installed; only autostart removed |
| 15 | Windows Update Blocker | T3 | CRITICAL | `skip` | `skip` | `skip` | `prompted` | Security risk — skipped in COMPETITIVE; CUSTOM only |
| 16 | NIC latency stack | T2 | MODERATE | `skip` | `prompted` | `prompted` | `prompted` | EEE/PHY power-save off, RSS, URO (Win11 24H2+), DSCP EF=46, IPv6 left enabled |
| 17 | Baseline benchmark (CapFrameX) | T1 | SAFE | `auto` | `auto` | `auto` | `prompted` | Required for before/after comparison |
| 18 | GPU driver clean (prep) | T1 | SAFE | `—` | `—` | `—` | `—` | Informational; removal happens in Phase 2 |
| 19 | NVIDIA driver download | T1 | SAFE | `auto` | `auto` | `auto` | `prompted` | AMD/Intel: manual download link shown |
| 20 | NVIDIA profile (prep) | — | — | `—` | `—` | `—` | `—` | Passthrough; actual gating in Phase 3 Step 4 |
| 21 | MSI interrupts (prep) | — | — | `—` | `—` | `—` | `—` | Passthrough; actual gating in Phase 3 Step 2 |
| 22 | NIC interrupt affinity (prep) | — | — | `—` | `—` | `—` | `—` | Passthrough; actual gating in Phase 3 Step 3 |
| 23 | Disable Fast Startup | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | MSI interrupt settings require cold boot — Fast Startup bypasses this |
| 24 | Dual-channel RAM detection | T1 | SAFE | `auto` | `auto` | `auto` | `prompted` | Warns + guides if single-channel detected |
| 25 | Nagle's Algorithm disable | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | Affects TCP only; no in-game latency change |
| 26 | GameConfigStore FSE | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | Supplements Step 4 fullscreen tweak |
| 27 | System scheduling + gaming priority + latency tweaks | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | Scheduler quantum, FTH disable, Maintenance disable, NTFS metadata, DisableCoInstallers, Intel PowerThrottlingOff |
| 28 | Timer resolution | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | Requires Windows 10 build 19041+ |
| 29 | Mouse acceleration disable + mouclass queue | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | mouclass queue default→50; requires reboot |
| 30 | CS2 GPU preference | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | Critical for iGPU+dGPU laptops; no-op on desktop |
| 31 | Xbox Game Bar / Game DVR off | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | No more Win+G recording |
| 32 | Overlay disable | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | Discord/AMD/GFE require manual steps |
| 33 | Audio optimization | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | Guide only — manual Sound settings |
| 34 | Autoexec.cfg generator + Launch Options | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | 73 CVars; m_rawinput stub; Intel thread_pool_option=2 auto-detected |
| 35 | Chipset driver check | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | Download link only; no auto-install |
| 36 | Visual effects + Defender exclusions + Auto HDR off | T3 | SAFE | `skip` | `skip` | `prompted` | `prompted` | Defender: cs2.exe + shader cache exclusions; Win11 Auto HDR disabled |
| 37 | SysMain + WSearch + qWave + Xbox services off | T3 | MODERATE | `skip` | `skip` | `prompted` | `prompted` | Measurable on HDD. qWave: UDP DPC noise redundant with Step 16. Xbox: skip if using Game Pass/wireless |
| 38 | Enable Safe Mode + register P2 | T1 | SAFE | `auto` | `auto` | `auto` | `prompted` | Mandatory final step — reboots to Safe Mode |

**Profile totals (Phase 1):**

| Profile | Auto | Prompted | Skipped |
|---------|------|----------|---------|
| SAFE | 23 | 0 | 11 |
| RECOMMENDED | 9 | 18 | 7 |
| COMPETITIVE | 9 | 24 | 1 |
| CUSTOM | 1 | 33 | 0 |

### Phase 2 — Safe Mode (3 steps)

Phase 2 runs automatically from `RunOnce`. **No profile interaction — all steps execute unconditionally.**

| Step | Action | Notes |
|------|--------|-------|
| 2.1 | Remove Safe Mode boot flag | `bcdedit /deletevalue safeboot` |
| 2.2 | Native GPU driver removal | PowerShell-based removal (stops services, removes drivers, cleans registry) |
| 2.3 | Register Phase 3 via RunOnce | Registers `PostReboot-Setup.ps1` for next normal boot |

### Phase 3 — 13 Steps

| # | Step | Tier | Risk | SAFE | RECOMMENDED | COMPETITIVE | CUSTOM | Notes |
|---|------|------|------|------|-------------|-------------|--------|-------|
| 1 | Install NVIDIA driver (clean) | T1 | SAFE | `auto` | `auto` | `auto` | `prompted` | AMD/Intel: manual install guide shown |
| 2 | MSI Interrupts | T2 | MODERATE | `skip` | `prompted` | `prompted` | `prompted` | Run LatencyMon beforehand recommended |
| 3 | NIC interrupt affinity | T3 | MODERATE | `skip` | `skip` | `prompted` | `prompted` | Only useful with LatencyMon-confirmed NIC DPC |
| 4 | NVIDIA CS2 profile (native) | T3 | SAFE | `skip` | `skip` | `prompted` | `prompted` | 52 DWORD settings applied natively via DRS |
| 5 | FPS cap info | — | — | `info` | `info` | `info` | `info` | — |
| 6 | Launch options + video settings | — | — | `info` | `info` | `info` | `info` | — |
| 7 | VBS / Core Isolation disable | T2 | MODERATE | `skip` | `prompted` | `prompted` | `prompted` | Disables HVCI; skip if FACEIT/Vanguard |
| 8 | AMD GPU settings guide | T2 | SAFE | `auto` | `prompted` | `prompted` | `prompted` | AMD GPU only |
| 9 | DNS server configuration | T3 | SAFE | `skip` | `skip` | `prompted` | `prompted` | Not for corporate/managed networks |
| 10 | Process priority / CCD affinity | T3 | SAFE | `skip` | `skip` | `prompted` | `prompted` | IFEO PerfOptions + scheduled task for X3D CCD |
| 11 | VRAM leak awareness | — | — | `info` | `info` | `info` | `info` | CS2-specific VRAM leak warning |
| 12 | Final checklist + summary | — | — | `info` | `info` | `info` | `info` | — |
| 13 | Final benchmark + FPS cap calc | T1 | SAFE | `auto` | `auto` | `auto` | `prompted` | Compares against Phase 1 Step 17 baseline |

### Power Plan Step 6 — Internal Decision Matrix

Step 6 is T1 (always runs), but `Apply-PowerPlan` applies three separate tiers of settings internally based on profile:

| Setting Group | SAFE | RECOMMENDED | COMPETITIVE | CUSTOM |
|---------------|------|-------------|-------------|--------|
| **T1** — CPU max, no parking, USB/disk off, sleep off, active cooling, PCIe ASPM off (9 settings) | Applied | Applied | Applied | Applied |
| **T2** — EPP=0, boost 254/255, max idle C1, NVMe/USB-C off, GPU pref=4, vendor CPU min (15–16 settings) | Skipped | Applied | Applied | Applied |
| **T3** — C-states off, duty cycling off, perf history=1, fast ramp (5 settings) | Skipped | Skipped | Applied | Applied |
