# Valve Region Latency Diagnostic

The GUI Network panel adds a before/after latency workflow without claiming to measure a real CS2 matchmaking ping.

## Scope

This feature is intentionally framed as a **diagnostic proxy**:

- It measures ICMP reachability and round-trip time to a versioned set of heuristic Valve/Steam region candidates.
- It helps compare route quality before and after DNS changes.
- It does **not** claim to be a Valve-official server ping or a guaranteed in-match latency predictor.

That wording matters. Valve's matchmaking and SDR routing decisions are more complex than a simple client-side ping to one public endpoint.

## Data Model

Runs are stored in `C:\CS2_OPTIMIZE\latency_history.json`.

Each run records:

- run kind: `baseline` or `post`
- timestamp
- active adapter name and adapter type
- active DNS provider and IPv4 servers
- per-region results

Each region result records:

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

`FallbackUsed` means the first candidate for that region did not respond and a later candidate from the **same region definition** was used instead. There is no shared global fallback that would make multiple regions silently collapse to the same number.

## Target Definitions

Targets live in `cfgs/valve-latency-targets.json`.

That file is:

- repo-owned
- versioned
- explicitly heuristic

The current candidate sets were adapted from the public `cs2-omz` region list, but this repo uses PowerShell-native ICMP probing rather than TCP handshake timing. Some candidates will therefore show timeouts when ICMP is filtered. That is expected.

## DNS Workflow

The panel exposes:

- Cloudflare
- Google
- DHCP reset
- restore latest GUI DNS backup

DNS writes use documented Windows DNS cmdlets and reuse the suite's existing backup/restore path. GUI-created DNS changes are backed up before modification so they can be restored via the GUI or the normal rollback surface.

## Evidence Posture

This panel is intentionally evidence-constrained:

- It is safe to describe it as a route-quality comparison tool.
- It is **not** safe to describe it as "real CS2 ping" or "Valve-official ping."
- DNS A/B testing may reveal route changes, but it does not prove one provider is universally better for all users or all times of day.
