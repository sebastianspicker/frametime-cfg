# Network Condition CFGs

This document covers the network configuration files deployed by Phase 1 Step
34. These files select different client buffering and timeout values. They
cannot repair packet loss, congestion, Wi-Fi interference, or poor ISP routing.

| Connection condition | Configuration | Buffer depth | Timeout |
|---|---|---:|---:|
| Low ping and stable delivery | `net_stable` | 0 | 30 seconds |
| High ping and stable delivery | `net_highping` | 2 | 60 seconds |
| Low ping with jitter or loss | `net_unstable` | 4 | 60 seconds |
| High ping with jitter or loss | `net_bad` | 3 | 60 seconds |

Use a file from the CS2 console, for example `exec net_unstable`. Return to the
least-buffered repository baseline with `exec net_stable`.

## Evidence boundary

`cl_net_buffer_ticks` is the primary buffering control used by these files.
`cl_net_buffer_ticks_use_interp` and
`cl_tickpacket_desired_queuelength` are accepted current-console settings in the
repository's reference material, but their public semantics and effect are less
well documented. Treat them as experimental configuration choices.

CS2 uses Source 2 subtick input processing. A buffer value expressed in ticks
must not be converted to a fixed millisecond cost using an assumed 64 Hz or 128
Hz server tick rate. The repository contains no packet capture or engine trace
that establishes an exact latency cost for these values. Compare configurations
on the same route and game build before retaining a deeper buffer.

## Connection conditions

### Jitter

Jitter is variation in packet arrival time. Bursts and gaps can occur even when
the average round-trip time looks acceptable. A deeper client buffer can make
irregular delivery less visible, but it can also delay the state presented by
the client. The exact tradeoff depends on current engine behavior and the live
connection.

### Packet loss

Packet loss means a datagram does not arrive. Client interpolation may conceal
some visual discontinuity, but it cannot recover the missing packet.
`cl_interp_ratio 2` is retained in the non-stable profiles as a
community-derived compatibility choice. Current Source 2 builds may manage this
internally or ignore legacy interpolation controls, so the document does not
assign a fixed interpolation window to the value.

### High base latency

A connection with high base latency has less room for additional buffering. The
`net_bad` profile therefore uses a depth of 3 instead of the depth of 4 used by
`net_unstable`. This is a repository policy choice, not a measured universal
optimum. Try the shallower profile first and keep the deeper profile only if it
improves repeatable delivery symptoms without unacceptable responsiveness.

## Configuration reference

### `net_stable`

```text
cl_interp_ratio "1"
cl_net_buffer_ticks "0"
cl_tickpacket_desired_queuelength "0"
cl_timeout "30"
```

Use this as the baseline on a connection with consistent delivery. It requests
no additional repository-configured network buffer.

### `net_highping`

```text
cl_interp_ratio "2"
cl_net_buffer_ticks "2"
cl_tickpacket_desired_queuelength "1"
cl_timeout "60"
```

Use this only when round-trip time is consistently high but delivery remains
stable. The longer timeout tolerates transient route delay. The buffer remains
shallower than the unstable profiles.

### `net_unstable`

```text
cl_interp_ratio "2"
cl_net_buffer_ticks "4"
cl_tickpacket_desired_queuelength "2"
cl_timeout "60"
```

Use this for repeatable jitter or packet-loss symptoms on a connection whose
base round-trip time is otherwise acceptable. It is the deepest buffer provided
by the repository.

### `net_bad`

```text
cl_interp_ratio "2"
cl_net_buffer_ticks "3"
cl_tickpacket_desired_queuelength "2"
cl_timeout "60"
```

Use this for the combination of high base latency and unstable delivery. No
client configuration can make a severely degraded route suitable for
latency-sensitive play.

## Diagnostics

Step 34 deploys two optional diagnostic files:

```text
exec debug_hud
exec debug_hud_off
```

`debug_hud` shows CS2 telemetry and requests these console reports:

```text
net_print_sdr_ping_times
net_status
cl_ticktiming print detail
```

The diagnostic file uses the current `_show` telemetry names and sets detailed
telemetry to display continuously during diagnosis:

| CVar | Diagnostic value | Repository use |
|---|---:|---|
| `cl_hud_telemetry_frametime_poor` | `8.0` | Highlights frame-time samples above the configured threshold. |
| `cl_hud_telemetry_ping_poor` | `60.0` | Highlights round-trip time above the configured threshold. |
| `cl_hud_telemetry_net_misdelivery_poor` | `1.0` | Highlights command or snapshot anomaly rates above the configured threshold. |
| `cl_hud_telemetry_net_detailed` | `2` | Continuously displays detailed network telemetry. |

`debug_hud_off` returns visibility and threshold controls to the repository's
quiet defaults. The diagnostic files contain no key bindings,
`host_writeconfig`, `developer 1`, or personal HUD and input preferences.

### Compare profiles

1. Use the same network, matchmaking region, and background traffic conditions.
2. Observe a full match or repeatable local test with `net_stable` first.
3. Record base round-trip time, jitter or late-delivery indicators, and loss.
4. Apply one alternative profile and repeat the observation.
5. Return to `net_stable` if the alternative does not produce a repeatable
   improvement.

Scoreboard ping is a round-trip indicator, not a complete route-quality
measurement. Use the detailed telemetry for late delivery and loss.

### Route checks

```text
tracert <server-ip>
ping <server-ip> -n 100
```

These commands can reveal route changes and variation, but intermediate routers
may rate-limit ICMP responses. Apparent loss at an intermediate hop is not proof
of forwarded game-traffic loss when later hops respond normally.

`net_client_steamdatagram_enable_override 1` requests Valve's Steam Datagram
Relay path. That path can differ from direct routing, but it is not guaranteed to
reduce latency or loss. Compare the observed result on the target connection.

## Settings outside this problem

- `cl_cmdrate` is a legacy control and does not configure Source 2 input
  processing as it did in CS:GO.
- `cl_updaterate` does not let a client increase a server's update behavior.
- `rate` is already set to `1000000` by the repository; servers can apply their
  own limits.
- TCP RSC, LSO, and TCP auto-tuning do not control CS2's UDP game datagrams.
- DNS selection can change name-resolution behavior but does not change packet
  delivery after a game endpoint has been resolved.
