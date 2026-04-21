# Storage Health

This repo now exposes SSD TRIM state as a **maintenance/correctness** surface, not as a claimed competitive-performance meta tweak.

## What the Feature Does

The Analyze footer can:

- read current `DisableDeleteNotify` state
- show whether TRIM is enabled for NTFS/ReFS where Windows reports it
- show whether eligible fixed volumes are available for `ReTrim`
- enable TRIM if it has been disabled
- run an optional `ReTrim` maintenance pass on eligible fixed volumes

## What the Feature Does Not Claim

This repo does **not** claim:

- a guaranteed FPS increase from enabling TRIM
- that ReTrim is a 2026 CS2 meta optimization
- that storage-maintenance actions should be part of the normal Phase 1/2/3 tuning path

The right framing is:

- storage maintenance
- storage correctness
- optional remediation when Windows configuration has drifted

## Why It Lives in GUI / Verify Instead of the Core Flow

TRIM is usually already enabled on a healthy modern Windows install. When it is not, the correct action is remediation, not "optimization." That makes it a better fit for:

- `Verify-Settings.ps1`
- the GUI Analyze footer
- manual maintenance sessions

instead of the competitive tuning sequence.

## Verification Surface

`Verify-Settings.ps1` now reports a dedicated `Storage maintenance: TRIM` line:

- `OK` when reported filesystems show TRIM enabled
- `CHANGED` when one or more reported filesystems show TRIM disabled
- `MISSING` when the state is not readable in the current runtime

## ReTrim

`ReTrim` is intentionally presented as optional and manual. It is a maintenance action for eligible fixed volumes, not a benchmark-driven tuning recommendation.
