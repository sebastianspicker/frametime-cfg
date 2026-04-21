# Network Condition CFGs — Deep Dive

> Covers the `cfgs/` directory deployed by Phase 1 Step 34.

CS2's network CVars do not fix a bad connection — they trade latency for resilience. The four CFGs map to a 2×2 matrix of the two independent failure modes:

| | **Stable route** | **Unstable (jitter / loss)** |
|---|---|---|
| **Low ping** | `net_stable` — +0ms | `net_unstable` — +31ms |
| **High ping (60ms+)** | `net_highping` — +16ms | `net_bad` — +23ms |

Use from the CS2 console: `exec net_stable`, `exec net_bad`, etc. Reset with `exec net_stable`.

---

## The Two Failure Modes

CS2 uses UDP at 128 ticks/second — one packet every 7.8ms. Two distinct problems can affect packet delivery, and each requires a different CVar:

### Jitter — `cl_net_buffer_ticks`

Jitter is variance in packet delivery timing. The server sends packets on a perfectly regular 7.8ms interval. If your connection introduces ±20ms of delivery variance, the client receives packets in bursts and gaps rather than at steady intervals.

`cl_net_buffer_ticks` holds received ticks in a ring buffer and drains them at a fixed rate. This converts irregular delivery into steady rendering. A buffer of 4 ticks can absorb up to ~31ms of delivery variance before it reaches the renderer.

**Trade-off:** every tick you buffer adds one tick of latency. At 128-tick, `cl_net_buffer_ticks 4` adds exactly 31.25ms above your base ping.

### Packet Loss — `cl_interp_ratio`

Packet loss means a tick never arrives at all. Unlike jitter, a lost packet cannot be recovered — CS2 uses UDP with no retransmit. The client must bridge the gap using the positions it did receive.

`cl_interp_ratio` controls the rendering interpolation window. With ratio `1`, the client interpolates between the last two received positions (~7.8ms window). With ratio `2`, it uses the last two tick intervals (~15.6ms window) — wide enough to bridge a single fully dropped packet without a visible position pop.

**Trade-off:** slightly increased positional lag, because entity positions are rendered slightly behind real-time to maintain a smooth window.

### Why both are needed simultaneously

Jitter and loss frequently co-occur. A congested Wi-Fi channel, a 4G cell with marginal signal, or a bad ISP peering point typically produces both erratic delivery timing *and* occasional outright dropped packets. Setting only `cl_net_buffer_ticks` without `cl_interp_ratio 2` means that the ticks which do arrive are buffered smoothly, but the gaps from dropped packets are still visible. The `net_unstable` and `net_bad` configs set both.

---

## The High Ping Tension

The difficult case is `net_bad` — high base ping combined with an unstable connection.

An unstable connection wants a deep buffer (`cl_net_buffer_ticks 4`, +31ms). But if your base ping is already 80ms, adding 31ms on top produces a 111ms round-trip — at which point the jitter absorption is visibly making the problem worse rather than better.

`net_bad` uses `cl_net_buffer_ticks 3` (+23ms) as the compromise. This absorbs most jitter bursts (jitter up to ~23ms is fully hidden) while keeping the added latency cost smaller than the full 4-tick buffer. On a 80ms base ping:

- 4 ticks: 80 + 31 = 111ms effective latency
- 3 ticks: 80 + 23 = 103ms effective latency
- 2 ticks: 80 + 16 = 96ms effective latency ← `net_highping` value (stable route only)

The 3-tick choice acknowledges that at 80ms+ base ping you are already in a degraded gameplay state. The buffer protects against the worst jitter spikes without making an already-bad experience worse.

---

## Per-Config Reference

### `net_stable` — low ping, stable connection

```
cl_interp_ratio "1"               // 7.8ms interpolation window
cl_net_buffer_ticks "0"           // no buffer — immediate rendering
cl_tickpacket_desired_queuelength "0"
cl_timeout "30"
```

**Added latency:** 0ms. All CVars at their minimum. Use on wired ethernet or fiber with consistent sub-5ms jitter and <1% loss.

---

### `net_highping` — high ping, stable route

```
cl_interp_ratio "2"               // 15.6ms window — high-ping routes have mild loss
cl_net_buffer_ticks "2"           // +16ms buffer — absorbs minor variance
cl_tickpacket_desired_queuelength "1"
cl_timeout "60"                   // extended — high-ping routes spike briefly
```

**Added latency:** +16ms. Intended for consistently high but *stable* ping: connecting to a distant region server, Starlink with clear sky, or an ISP with inefficient routing that nonetheless delivers packets reliably.

`cl_interp_ratio 2` is included because high-ping routes carry a higher baseline probability of mild loss even when they appear stable. The 2-tick buffer is kept small deliberately — the connection is already expensive; don't add unnecessary depth.

---

### `net_unstable` — low ping, jitter + loss

```
cl_interp_ratio "2"               // 15.6ms window — covers dropped packets
cl_net_buffer_ticks "4"           // +31ms buffer — absorbs up to 31ms jitter
cl_tickpacket_desired_queuelength "2"
cl_timeout "60"
```

**Added latency:** +31ms. Use when your base ping is acceptable but the delivery is erratic: Wi-Fi with interference, 4G hotspot, congested home network (streaming/downloads), or ISP peering instability.

This is the worst-case config for connection quality on a low-latency network — maximum buffer depth combined with loss coverage. If you are seeing rubberbanding or position pops with `net_stable` but your ping reads low in the scoreboard, this config addresses that.

---

### `net_bad` — high ping + jitter/loss

```
cl_interp_ratio "2"               // 15.6ms window — loss coverage
cl_net_buffer_ticks "3"           // +23ms buffer — compromise vs. highping
cl_tickpacket_desired_queuelength "2"
cl_timeout "60"
```

**Added latency:** +23ms above baseline (on top of your existing high base ping). Use when you have both problems at once: satellite internet in poor conditions, mobile roaming, hotel or hostel Wi-Fi, tethering with weak signal.

At this connection quality, no CVar configuration produces competitive gameplay. These settings make the experience as consistent as possible within the constraint. The 3-tick buffer is a deliberate compromise — see the tension section above.

---

## Diagnosing Your Condition

### Check jitter

Enable the HUD telemetry overlay set up by Step 34:

```
cl_hud_telemetry_net_quality_graph 1
cl_hud_telemetry_serverrecvmargin_graph 1
```

Watch the graphs during a full map. Irregular spikes in the quality graph indicate jitter. Gaps (missing bars) indicate packet loss.

### Check base ping

Scoreboard ping is your base round-trip time. Consistently above 60ms → `net_highping` or `net_bad`. Under 40ms with instability → `net_unstable`.

### Check routing

```
tracert <server-ip>
ping <server-ip> -n 100
```

Look for:
- Hops with >10% loss in `tracert` (loss mid-route, not just last hop)
- High standard deviation in `ping -n 100` output (jitter)
- Unexpected geographic hops (routing inefficiency)

If `tracert` shows consistent loss at an intermediate hop, `net_client_steamdatagram_enable_override 1` (already set in all configs) routes through Valve's SDR network instead of direct IP, which often bypasses the problematic hop.

---

## Settings That Cannot Help

These are common suggestions that do not apply to CS2 or do not address connection quality:

- **`cl_cmdrate`** — removed in CS2. The Source 2 netcode sends inputs every frame. Silently ignored.
- **`cl_updaterate`** — server-side setting. The client cannot increase the server's send rate by setting this higher.
- **`rate`** — already at maximum (`1000000`) in `optimization.cfg`. Higher values are clamped by the server.
- **TCP-layer settings** (RSC, LSO, TCP autotuninglevel) — CS2 uses UDP for game traffic. TCP optimizations do not affect it.
- **DNS changes** — DNS resolves the matchmaking server address once at connection time. DNS latency does not affect in-game packet delivery.
