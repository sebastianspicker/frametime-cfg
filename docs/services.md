# Windows Services — Deep Dive

> Covers Phase 1 Step 37 and partial Step 13 (telemetry services).

The suite disables seven Windows services across two steps. Each service is disabled for a specific, measured reason — not because "less services = faster". This document explains what each service does, why it's disabled, what breaks if you disable it, and when to re-enable it.

---

## Step 37 Services (T3 — COMPETITIVE+)

These require user confirmation and are applied only at COMPETITIVE profile or above.

### SysMain (Superfetch)

**What it does:** Prefetches frequently-used application data into RAM during idle periods, so apps launch faster from cold.

**Why disabled:** SysMain's prefetch engine monitors disk access patterns continuously and moves data between RAM and disk in the background. On spinning HDDs, this is a net win — cold launches go from 10+ seconds to 2–3 seconds. On NVMe drives, where cold read speeds exceed 3 GB/s, the prefetch benefit is negligible (a 100 MB load takes 33ms from NVMe vs. 32ms from RAM prefetch). The cost remains: SysMain's background I/O competes with CS2's shader cache reads and config file accesses.

**djdallmann measurement:** SysMain can consume 5–12% CPU and significant disk I/O during active prefetch passes, which can overlap with CS2 play sessions if the system was recently booted or a new application was run.

**What breaks:** Application cold launch times increase. First-launch of Steam, Chrome, or Office is noticeably slower. This is the main reason this is T3 — the trade-off is real and user-visible outside CS2.

**When to re-enable:** If you notice significantly slower app startup times and your CS2 sessions are not showing measurable improvement, re-enable SysMain (`Set-Service SysMain -StartupType Automatic`).

**Impact on CS2:** Low-to-moderate. Most measurable on systems with SATA SSDs or when SysMain prefetch passes happen to overlap with play sessions.

---

### WSearch (Windows Search)

**What it does:** Indexes file system content (file names, document text, email) to enable fast Windows Search results.

**Why disabled:** The indexer runs continuous background I/O passes, prioritized below active foreground I/O but still consuming disk throughput. The indexer's maintenance window is supposed to run at 3 AM during idle, but it reschedules to "next idle moment" if the system was off at 3 AM — which includes CS2 sessions for many players.

**What breaks:** Windows Search (Start menu search, File Explorer search) becomes dramatically slower for content-based queries. Searching for a file by name still works via filesystem scan; searching by content or metadata requires the index.

**When to re-enable:** If you rely on Windows Search for work (searching Outlook, searching documents), re-enable WSearch. The CS2 impact is mild on NVMe systems; the Windows usability cost is high for productivity users.

---

### qWave (Quality Windows Audio/Video Experience)

**What it does:** A QoS probe service that periodically sends UDP probes to network endpoints to estimate available bandwidth and latency. Applications can use the qWave API to request priority for their network traffic.

**Why disabled:** Two reasons:

1. **Redundant with Step 16.** The suite already applies DSCP EF=46 marking to CS2's network traffic via NDIS policies (Step 16). DSCP marking is applied at the packet level by the network stack, independently of qWave. qWave's probe packets and CS2's DSCP marking coexist, but qWave adds no additional benefit once DSCP is configured correctly.

2. **Periodic DPC noise.** qWave's probe mechanism generates UDP packets on a timer, causing periodic NDIS DPC events. These are small (microsecond range) but measurable in LatencyMon traces as regular DPC spikes from `ndis.sys` that have no relation to CS2's own traffic.

**What breaks:** Applications using the qWave API for QoS requests lose their priority (Windows Media Player's multimedia mode, for example). WMV hardware acceleration may degrade on some configurations. Most games do not use qWave.

**Important:** The DSCP policies written by Step 16 survive qWave being disabled. DSCP is enforced by NDIS, not by the qWave service. Disabling qWave does not remove CS2's packet prioritization.

**When to re-enable:** If Windows Media Player or streaming applications behave unexpectedly, or if you want qWave's QoS for other applications.

---

### Xbox Services

Four Xbox Live services are disabled as a group. Each has a specific function and a specific re-enable condition.

#### XblAuthManager (Xbox Live Auth Manager)

**What it does:** Handles Xbox Live authentication tokens for Windows Store games and applications. Periodically refreshes auth tokens in the background, making network requests to Xbox Live servers.

**Why disabled:** Background periodic network requests from `XblAuthManager` appear in packet captures as intermittent HTTPS traffic to Xbox Live endpoints. These happen even when no Xbox or Windows Store content is running. The requests are small but generate occasional NIC receive interrupts and brief NDIS DPC activity.

**What breaks:** Windows Store games requiring Xbox Live authentication will fail to sign in. Xbox Game Pass requires this service.

**When to re-enable:** If you use Xbox Game Pass or any Windows Store game requiring Xbox Live signin.

---

#### XblGameSave (Xbox Live Game Save)

**What it does:** Syncs game save data to Xbox Live cloud storage for Xbox/Windows Store games.

**Why disabled:** Like XblAuthManager, this service has periodic background I/O — reading local save files and syncing to Xbox Live. The sync events are disk and network I/O with unpredictable timing.

**What breaks:** Xbox Live cloud saves stop syncing. Local saves are preserved — data is not lost, only cloud sync stops.

**When to re-enable:** If you use any game with Xbox cloud saves that you want synchronized.

---

#### XboxNetApiSvc (Xbox Live Networking Service)

**What it does:** Provides network connectivity abstractions for Xbox Live — handling NAT traversal, relay servers, and peer-to-peer networking for Xbox Live multiplayer.

**Why disabled:** Background periodic network activity for Xbox Live networking maintenance, even when no game is running.

**What breaks:** Xbox Live multiplayer in Windows Store games. CS2 does not use Xbox Live networking.

**When to re-enable:** If you use any Windows Store game with Xbox Live multiplayer.

---

#### XboxGipSvc (Xbox Accessory Management Service)

**What it does:** Manages Xbox wireless accessories — controllers, headsets, and other peripherals using the Xbox Wireless protocol.

**Why disabled:** Background service overhead. No periodic network activity, but it runs a driver monitoring loop.

**What breaks:** Xbox wireless controllers and headsets stop working. This is the most likely to affect users. USB-connected Xbox controllers use a different driver path and are unaffected. Only Xbox Wireless protocol devices need this service.

**When to re-enable:** If you use an Xbox wireless controller or Xbox headset. Run: `Set-Service XboxGipSvc -StartupType Manual`.

---

## Step 13 Services (T2 — RECOMMENDED+)

### DiagTrack (Connected User Experiences and Telemetry)

**What it does:** Collects Windows diagnostic and usage data and uploads it to Microsoft. Runs background data collection, compresses it, and transmits it periodically.

**Why disabled:** Background CPU and network activity with no benefit to the user. The service runs as a long-lived daemon with periodic wakeups.

**What breaks:** Microsoft telemetry upload stops. Windows Update recommendations and Windows Insider feedback are affected. No user-facing functionality is lost.

---

### dmwappushservice (Device Management WAP Push)

**What it does:** Handles WAP push messages for mobile device management (MDM) — primarily relevant in enterprise environments where devices are managed via Microsoft Intune or similar MDM solutions.

**Why disabled:** Irrelevant on non-managed home gaming systems. On a standalone gaming PC not enrolled in enterprise MDM, this service does nothing. Disabling it removes its daemon overhead.

**What breaks:** MDM/Intune provisioning stops working. On home gaming systems: nothing.

---

## Service State Summary

| Service | Default State | Suite Sets | What Breaks If Disabled | Re-enable If |
|---------|--------------|------------|------------------------|--------------|
| SysMain | Automatic | Disabled | Slower app cold launches | Noticeable launch time regression |
| WSearch | Automatic | Disabled | Slow File Explorer search | You rely on content search |
| qWave | Manual | Disabled | WMP QoS, some multimedia apps | Streaming apps behave poorly |
| XblAuthManager | Manual | Disabled | Xbox Game Pass signin | You use Game Pass |
| XblGameSave | Manual | Disabled | Xbox cloud saves | You use Xbox cloud saves |
| XboxNetApiSvc | Manual | Disabled | Xbox Live multiplayer | You play Xbox Live multiplayer |
| XboxGipSvc | Manual | Disabled | Xbox wireless controller/headset | You use Xbox wireless peripherals |
| DiagTrack | Automatic | Disabled | Microsoft telemetry | Never (for home users) |
| dmwappushservice | Manual | Disabled | MDM/Intune provisioning | Enterprise MDM enrolled |

---

## Rollback

All service states are backed up before modification. Restore any step via START.bat → [7] Restore / Rollback → select "Disable SysMain + Search + QWAVE + Xbox".

Manual re-enable for any service:
```powershell
Set-Service <ServiceName> -StartupType Manual   # or Automatic
Start-Service <ServiceName>
```
