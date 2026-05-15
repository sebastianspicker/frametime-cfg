# Valve Region Latency Diagnostic

The GUI Network panel adds a before/after latency workflow without claiming to measure a real CS2 matchmaking ping.

## Scope

This feature is intentionally framed as a **diagnostic proxy**:

- It fetches Valve's live SDR relay config for CS2 (`ISteamApps/GetSDRConfig`, appid 730) and measures ICMP round-trip time to those relays.
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

`FallbackUsed` means the first candidate for that region did not respond and a later candidate from the **same region definition** was used instead. There is no shared global fallback that would make multiple regions silently collapse to the same number.

## Target Definitions

Targets live in `cfgs/valve-latency-targets.json`.

That file is:

- repo-owned
- versioned
- explicitly heuristic

The live target loading follows the same broad data source used by Server Picker X: Valve's `ISteamApps/GetSDRConfig/v1/?appid=730` response. The GUI parses `pops`, skips China/Perfect World entries, and probes each region's relay IPv4 list until one responds. If the API cannot be reached, the checked-in JSON remains an offline fallback.

Valve's current CS2 SDR config lists `fsn` as Falkenstein, but does not expose CS2 relay IPs for it. The GUI therefore adds a separately labeled known-host target, `Falkenstein (Germany) - Hetzner hosted`, from public Valve Source server listings for `srcds1001-1007-fsn-hetz`:

- `138.199.142.208`
- `138.199.142.209`
- `138.199.142.210`
- `138.199.142.211`
- `138.199.142.212`
- `138.199.142.213`
- `138.199.142.214`

This target is deliberately not described as a current CS2 SDR relay set. It exists to separate Frankfurt (`fra`) from the known Falkenstein Hetzner-hosted Valve addresses when comparing and blocking regions.

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
