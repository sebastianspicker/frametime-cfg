# Valve Region Latency Diagnostic

The GUI Network panel adds a before/after latency workflow without claiming to measure a real CS2 matchmaking ping.

## Scope

This feature is a diagnostic proxy:

- When Valve's live SDR relay config is available
  (`ISteamApps/GetSDRConfig`, appid 730), it measures ICMP round-trip time to
  relay IP addresses.
- When the live fetch fails, it loads the checked-in Steam connection-manager
  candidates and measures TCP connection time to port 27017. Those fallback
  endpoints are not asserted to be CS2 SDR relays.
- It helps compare route quality before and after DNS changes.
- It does not claim to be a Valve-official server ping or a guaranteed in-match latency predictor.

That wording matters. Valve's matchmaking and SDR routing decisions are more complex than a simple client-side ping to one public endpoint.

## Data Model

Runs are stored in `C:\FRAMETIME_CFG\latency_history.json`.

Each run records:

- run kind: `baseline` or `post`
- timestamp
- active adapter name and adapter type
- active DNS provider and IPv4 servers
- per-region results

Each region result records:

- `RegionCode`
- `TargetLabel`
- `ResolvedEndpoint`
- `ProtocolUsed`
- `SampleCount`
- `SuccessfulSamples`
- `MinRttMs`
- `MedianRttMs`
- `AvgRttMs`
- `TimeoutCount`
- `FallbackUsed`
- `Notes`
- `Provenance`

`FallbackUsed` means the first candidate for that region did not respond and a
later candidate from the same region definition was used instead. There is no
shared global fallback that would make multiple regions silently collapse to
the same number. It does not indicate whether the live SDR fetch fell back to
the checked-in target file; provenance and protocol identify that distinction.

## Target Definitions

Targets live in `cfgs/valve-latency-targets.json`.

That file is:

- repo-owned
- versioned
- explicitly heuristic

Live target loading uses Valve's `ISteamApps/GetSDRConfig/v1/?appid=730`
response. The GUI parses `pops`, skips China entries, and sends ICMP probes to
each region's relay IPv4 candidates until one responds. If the API cannot be
reached, the checked-in JSON supplies Steam connection-manager candidates that
are probed with TCP connection attempts on port 27017.

The checked target data associates `fsn` with Falkenstein but does not treat the
following known-host addresses as a current CS2 SDR relay set. The GUI labels
them separately as `Falkenstein (Germany) - Hetzner hosted`:

- `138.199.142.208`
- `138.199.142.209`
- `138.199.142.210`
- `138.199.142.211`
- `138.199.142.212`
- `138.199.142.213`
- `138.199.142.214`

The address list is time-sensitive and can become stale. Live Valve data and
the checked fallback are route-quality inputs only. They do not establish which
endpoint matchmaking will select.

## DNS Workflow

The panel exposes:

- Cloudflare
- Google
- DHCP reset
- restore latest GUI DNS backup

DNS writes use documented Windows DNS cmdlets and reuse the suite's existing backup/restore path. GUI-created DNS changes are backed up before modification so they can be restored via the GUI or the normal rollback surface.

## Evidence Posture

This panel is intentionally evidence-constrained:

- It can be described as a route-quality comparison tool. ICMP and TCP
  connection timing are different measurements and should not be compared as
  if they were the same protocol.
- It must not be described as "real CS2 ping" or "Valve-official ping."
- DNS A/B testing may reveal route changes, but it does not prove one provider is universally better for all users or all times of day.
