# NIC Latency Stack — Deep Dive

> Covers Phase 1 Step 16 and Phase 3 Step 3.

Most "network optimization" guides for CS2 are either written for TCP applications (CS2 uses UDP), apply settings that only matter on Wi-Fi, or recommend disabling things that empirical testing shows make performance worse. This document explains what the suite actually does to the NIC receive path, why each layer matters, and how to verify it's working.

---

## The CS2 Network Profile

Before optimizing anything, it helps to know what CS2's traffic actually looks like:

- **Protocol:** UDP only, for all game state
- **Direction:** Bidirectional, but receive (server → client) is latency-critical
- **Packet rate:** ~128 packets/second (one per server tick)
- **Packet size:** ~80–120 bytes per datagram
- **Ports:** UDP 27015–27036 (game), UDP 27020 (GOTV)

This profile is radically different from what most NIC drivers are tuned for out of the box. NICs ship optimized for TCP bulk transfers (large packets, sustained throughput) and VoIP (small packets but few per second). CS2 is small packets at high regularity — the worst case for the default interrupt coalescing settings.

---

## Layer 1: PHY Power-State Wake Latency

### What happens by default

**Energy Efficient Ethernet (EEE, IEEE 802.3az)** puts the PHY chip into Low Power Idle (LPI) during traffic gaps. At CS2's 128 pkt/sec rate — one packet every 7.8 milliseconds — the PHY enters LPI between every single packet burst. LPI exit adds **10–100µs of wake latency** to the first packet after each idle window. This appears in CapFrameX frametime traces as irregular jitter that's not correlated with GPU or CPU load.

Realtek cards have two additional proprietary variants:
- **`*GreenEthernet`** — Realtek's own power-save extension (different from IEEE EEE)
- **`*PowerSavingMode`** — dynamic downclocking of the PHY when traffic is low

**Flow Control (`*FlowControl`)** is different: it's IEEE 802.3x PAUSE frames, which allow a switch to halt your NIC's transmit path for up to ~50ms. This doesn't apply to receive latency, but unexpected transmit stalls are still unwanted.

### What the suite does

Disables all four in the NIC's driver registry key:

```
HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e972-...}\000X
    *EEE                = 0  (if present — Intel/Broadcom)
    *GreenEthernet      = 0  (if present — Realtek)
    *PowerSavingMode    = 0  (if present — Realtek)
    *FlowControl        = 0
```

These are set via `RegistryKeyword`, meaning the NIC driver reads them at device initialization. Changes require a device restart (handled by `Disable-NetAdapter` + `Enable-NetAdapter` or reboot).

### DisplayName Fallback for Realtek NICs

Intel and Realtek use different `DisplayName` strings for the same settings (e.g., Intel: `"EEE"`, Realtek: `"Energy Efficient Ethernet"`). The suite tries the Intel-style name first; if the property is not found, it falls back to the Realtek-style `DisplayName` via `$CFG_NIC_Tweaks_AltNames`. This covers the Realtek RTL8125/RTL8126 family (2.5 GbE / 5 GbE) which ships on many 2024–2026 gaming boards.

### 5 GbE Buffer Sizing

NICs at 5+ Gbps link speed (e.g., Realtek RTL8126) use larger receive/transmit buffers (`ReceiveBuffers = 2048`) to handle the higher packet rate without overflow. The suite detects link speed via `$nic.Speed` and scales automatically.

### How to verify

LatencyMon → Drivers tab. If NIC DPC latency shows spikes correlated with traffic bursts (not constant spikes), PHY wake latency is likely the cause. After this step, those burst-correlated spikes should flatten.

---

## Layer 2: Interrupt Coalescing — Why "Disabled" Is Worse

This is the most counter-intuitive optimization in the suite, and the one most commonly set wrong by other guides.

### The intuitive argument (wrong)

*"Every CS2 packet should trigger an immediate interrupt. Disabling coalescing means the CPU processes each packet as soon as it arrives. Therefore: less coalescing → lower latency."*

This argument is correct in a vacuum. It fails on a real gaming PC.

### The empirical reality

**djdallmann (GamingPCSetup)** ran controlled xperf/ETL measurements on an Intel Gigabit CT (one of the most common enterprise-grade NICs used in gaming motherboards) and measured actual DPC latency variance — not theoretical latency, but measured kernel interrupt scheduling jitter — under three conditions:

1. Interrupt Moderation **Disabled**
2. Interrupt Moderation **Medium** (~50–200µs coalescing window)
3. Interrupt Moderation **High** (~200–500µs)

**Result:** Medium produced the lowest DPC latency *variance*. Disabled was *worse* than Medium.

### Why disabled causes interrupt storms

A gaming PC always has background network traffic. Discord audio: 50 pkt/sec. Steam presence: ~10 pkt/sec. Windows telemetry: burst every few minutes. Browser tabs with websocket connections: variable. None of these are large individually. But with Interrupt Moderation disabled, each of these packets fires its own CPU interrupt, interleaved with CS2's 128 pkt/sec. The interrupt controller has to schedule and service each one individually.

Under this load, the CPU interrupt scheduling path becomes irregular. CS2 packets that arrive simultaneously with Discord VoIP packets compete for DPC processing time. The result is DPC latency *variance* — individual processing delays that vary from 5µs to 200µs depending on what else arrived in the same microsecond. Variance is what causes frametime spikes.

Medium coalescing collects packets within a ~50–200µs window and delivers one DPC for the batch. CS2 receives its packet no more than 200µs late — well within the 7.8ms tick window. But the DPC scheduling is deterministic: no surprise competition from background packet interrupts.

### The suite's choice

**Medium for all profiles** — including COMPETITIVE. This is the suite's current default based on community measurement, not an Intel or Microsoft universal-best recommendation. If you use a headless server with zero background traffic, Disabled might be correct. On a gaming PC with Discord, Steam, and a browser open, the suite currently prefers Medium.

---

## Layer 3: RSS (Receive Side Scaling)

### The Core 0 problem

Intel I225-V, I226-V, and I219-V NICs (standard on most Z490/Z590/Z690/Z790/B650 motherboards) ship without RSS indirection table entries in their driver registry key. Without explicit RSS configuration, Windows NDIS assigns all receive-side DPC processing to **CPU Core 0** by default.

Core 0 is also the default handler for most OS bookkeeping: APIC timer interrupts, PCI-e MSI routing on some configurations, DWM scheduling on older drivers. When CS2's game thread and NIC DPCs both contend on Core 0, DPC scheduling becomes irregular.

### What the suite adds

Five entries in the NIC's driver registry key (created if absent):

| Entry | Value | Meaning |
|-------|-------|---------|
| `*RSS` | 1 | Master switch — some Realtek drivers ship with `*RSS=0`, silently ignoring all sub-parameters |
| `*RSSProfile` | 1 (ClosestProcessor) | RSS queues processed on the CPU core whose L3 cache is nearest to where the DMA'd packet data landed |
| `*RssBaseProcNumber` | 2 | Start RSS queues from Core 2, keeping Core 0/1 free for OS tasks |
| `*MaxRssProcessors` | 4 (or 8 for 5+ GbE) | Spread processing across 4 cores maximum — adequate for 128 pkt/sec at 1–2.5 GbE; 8 for 5+ GbE |
| `*NumRssQueues` | 4 (or 8 for 5+ GbE) | Explicit queue count, scaled to link speed |

**Speed-aware scaling:** NICs at 5+ Gbps (e.g., Realtek RTL8126 5 GbE) use 8 queues and 8 max processors to handle higher packet rates. Queue count is capped at the actual processor count.

**Existing values are never overwritten.** If your NIC driver or motherboard vendor already configured RSS, the suite leaves it alone.

### Phase 3 Step 3 — NIC Interrupt Affinity

This goes further: pinning the NIC's interrupt handling to a specific non-game core via `DevicePolicy=4` + `AssignmentSetOverride` in the interrupt affinity policy registry key. T3 / COMPETITIVE+ only, because it requires LatencyMon-confirmed NIC DPC spikes to be worth the risk of misaligning the affinity with your specific core layout.

**When to use it:** Run LatencyMon for 10 minutes while gaming. If `ndis.sys` or your NIC driver (`e1g6032e.sys`, `rtx*.sys`, etc.) shows DPC execution times above 100µs consistently, interrupt affinity pinning is worth trying.

---

## Layer 4: URO (UDP Receive Offload)

### What URO is

Per current Microsoft documentation, URO is a Windows 11 **24H2+** feature where the NIC coalesces multiple UDP datagrams from the same flow into a single large kernel-mode delivery. The goal is to reduce CPU interrupt overhead for high-throughput UDP applications — video streaming servers, DNS resolvers, etc.

For those workloads, URO is appropriate. For CS2, it is not.

### Why URO hurts CS2

CS2 sends exactly one complete game state snapshot per packet. There is no meaningful aggregation possible — each datagram is its own atomic unit. When URO holds two or three of these datagrams waiting for the batch to fill before delivering the DPC, those packets are **artificially delayed**. The game state they carry is stale by the time it arrives in the application layer.

At 128 pkt/sec, URO can delay a packet by up to ~15ms if it's waiting for a batch partner that never arrives (URO has a timeout). This is 2× the tick interval, turning a perfectly delivered packet into what the game engine perceives as a late or duplicate update.

### The suite's fix

```
netsh int udp set global uro=disabled
```

On Windows versions without URO support, this command is a no-op or unsupported. On Windows 11 24H2+ it disables the feature system-wide. This is the only `netsh` command the suite uses because it's the only one in this area that directly targets UDP receive behavior.

---

## Layer 5: QoS DSCP — The NLA Prerequisite Trap

### What DSCP does

DSCP (Differentiated Services Code Point) is a 6-bit field in the IP header that signals forwarding priority to network equipment. EF=46 (Expedited Forwarding) is the highest priority class — it tells routers and switches "process this before everything else."

The Windows QoS Packet Scheduler can mark outgoing packets with DSCP values based on policies you define. The suite creates two policies to catch CS2 traffic:

1. **Port-based:** UDP 27015–27036 → DSCP EF=46
2. **App-path:** `cs2.exe` → DSCP EF=46 (belt-and-suspenders if ports change)

### The silent failure problem

Here is the issue that renders most guides' DSCP instructions useless:

`New-NetQosPolicy` will report success even when DSCP marking is completely disabled.

Windows refuses to apply DSCP marks on "unidentified" or "Public" network profiles. If your network adapter is categorized as "Public" (common for newly connected networks, VPN adapters, or any adapter without a recognized gateway), every QoS policy silently does nothing. No error, no warning — `Get-NetQosPolicy` shows your policy, but packets leave without the DSCP mark.

The fix is a registry key that most guides never mention:

```
HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\QoS
    Do not use NLA = "1"
```

This tells the QoS subsystem to ignore the network profile and always apply DSCP marking. Without this key, your policies are inert on Public networks. The suite always sets this key before creating the policies.

### Real-world impact

DSCP only benefits the LAN segment between your PC and your router. Consumer ISP routers (most home setups) strip DSCP markings at the first hop before packets enter the internet. Valve's servers receive packets without any DSCP marking regardless of what you set.

On a managed network (university, work, esports venue with managed switches), DSCP EF marking can noticeably reduce queueing delay within that network. At home, it costs nothing to enable and may help with router QoS prioritization if your router supports DSCP-based queuing (some gaming routers do). The suite enables it correctly and leaves the user to assess their specific network.

---

## Layer 6: IPv6 — Left Enabled (2026 Reversal)

### Why the recommendation changed

**Previous guidance (2023–2024)** recommended `DisabledComponents = 0xFF` to eliminate NDP/RA/DHCPv6 background traffic. That advice was based on the premise that CS2 game servers are IPv4-only, so IPv6 stack activity was pure overhead.

**2025–2026 evidence reverses this:**

- **Steam (late 2023+)** updated its networking stack to prefer IPv6 when round-trip time is demonstrably lower
- **Riot Games 2025 infrastructure paper:** 68% of EU-West connections use IPv6, with 4.2ms median latency improvement vs IPv4-only paths
- **Valve SDR relay network** supports IPv6 — disabling IPv6 removes a potentially faster routing path for the SDR relay hops between your client and Valve's game servers
- **Disabling IPv6 on modern ISPs** often forces traffic through IPv4 CGNAT (Carrier-Grade NAT) gateways, which **adds** 5–15ms of latency — the exact opposite of what the optimization intended

### Background traffic is negligible

The original concern — NDP, Router Advertisements, DHCPv6 — generates less than 1 packet per second of background traffic. On modern NICs with interrupt moderation (Layer 2), this adds zero measurable DPC overhead. The routing benefit of leaving IPv6 enabled now far outweighs the sub-1-packet/sec background traffic cost.

### Current suite behavior

The suite **leaves IPv6 enabled** and displays manual disable instructions:

```
To disable manually if needed:
  Set DisabledComponents = 0xFF in HKLM:\...\Tcpip6\Parameters
```

This is a troubleshooting step for users who specifically diagnose IPv6-related latency issues, not a default recommendation.

---

## What This Suite Does NOT Do to the NIC

Many common "network optimization" steps that appear in guides are omitted because they are TCP-only features with zero effect on CS2's UDP traffic:

| Setting | Why skipped |
|---------|-------------|
| `netsh int tcp set global autotuninglevel=disabled` | TCP receive window scaling — CS2 uses UDP |
| Disable RSC (Receive Segment Coalescing) | Coalesces TCP *segments* — not UDP datagrams |
| Disable LSO (Large Send Offload) | NIC-based TCP segmentation — irrelevant for 80-byte UDP |
| Disable TCP/UDP Checksum Offload | UDP checksum for 80-byte datagrams takes nanoseconds in hardware; software is slower |
| `*TransmitBuffers = 256` | Suite sets 512 (safe default; 2048 for 5+ GbE NICs); reducing below 512 risks overflows during background traffic bursts |
| Disable ARPOffload / NSOffload | Active only during system sleep — zero effect during gameplay |
| Disable WakeOnMagicPacket | Active only when system is powered off |

The guiding principle: if a setting only affects TCP, it cannot affect CS2 game packet delivery. CS2's UDP path bypasses the TCP stack entirely.

---

## Verifying NIC Optimization

### Tools needed

- **LatencyMon** (Resplendence) — measures DPC and ISR latency; shows which drivers are responsible
- **WireShark** (optional) — can verify DSCP markings on outbound packets

### What to look for in LatencyMon

Run for 10 minutes while playing a practice server match. Check:

1. **Maximum DPC execution time** — should be below 150µs on a well-configured system. Values above 500µs consistently indicate a driver problem.
2. **Problematic drivers** — `ndis.sys` above 100µs average indicates NIC interrupt issues. Your NIC driver (`e1i65x64.sys` for Intel, `rt640x64.sys` for Realtek) should be below 30µs average.
3. **Interrupt frequency** — after RSS is configured, interrupt counts should be spread across cores 2–5, not concentrated on Core 0.

### Checking RSS assignment

```powershell
Get-NetAdapterRss | Format-List Name, Enabled, *Processor*
```

The `BaseProcessorNumber` should reflect your configured `*RssBaseProcNumber`. `NumberOfReceiveQueues` should match `*NumRssQueues`.

### Checking DSCP policies

```powershell
Get-NetQosPolicy | Where-Object { $_.Name -match "CS2" }
```

Verify both the port-based and app-path policies appear. To confirm marking is active (not silently disabled), check:

```powershell
(Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\QoS" -ErrorAction SilentlyContinue)."Do not use NLA"
```

Should return `"1"`.
