# Desktop interface guide

The experimental Windows Presentation Foundation (WPF) source remains a
development and smoke-test surface. Its portable launcher is fail-closed until
an installed or independently signed payload authenticates source before UAC.

## Launch and runtime

Use [`demo/index.html`](../demo/index.html) for non-privileged interface review.
`START-GUI.bat` exits nonzero and does not request elevation.

Future live execution requires an x64 Windows desktop, administrator rights,
and a trusted installation/signing boundary that is not yet implemented.
`frametime-gui.ps1 -SmokeTest` is the non-elevated entrypoint check used
by CI. The smoke check does not validate WPF rendering or interaction.

## Task areas

### 1. Overview

Overview presents hardware detection, recorded phase progress, the latest
benchmark comparison, startup drift status, and explicit next actions.

- Hardware detection runs in a background runspace.
- Progress comes from `C:\FRAMETIME_CFG\progress.json`.
- Recorded phase completion and currently observed system state are different
  facts; use `Assess system` or `Verify supported settings` for fresh state.
- Phase launch buttons open the existing terminal workflows. Phase 2 and Phase
  3 buttons resolve `runtime-current.json` to the validated published payload
  in its immutable runtime generation. If the payload fails integrity validation
  or is missing, the GUI provides recovery guidance instead of starting
  the handoff.

### 2. Assess

`Run full scan` performs non-destructive hardware, Windows, storage, network,
service, and CS2 configuration checks. Duplicate scans are blocked and the scan
can be cancelled.

Results include category, current value, recommended value, status, step
reference, and impact. Status includes text or a symbol; color is supplemental.
The theme uses amber for its principal action and active state. `Export report`
writes a user-selected CSV file.

Storage maintenance is shown in the same task area. `Enable TRIM` and `ReTrim`
are confirmed maintenance actions, not claimed FPS optimizations.

### 3. Setup and verify

This task combines the settings that affect execution with the step catalog:

- profile: Safe, Recommended, Competitive, Custom, or YOLO;
- a persisted preview preference for terminal setup;
- category and status filters;
- phase, tier, risk, recorded status, and reboot information;
- terminal launch actions for Phase 1, Safe Mode Phase 2, and Phase 3;
- non-mutating inline verification of supported settings.

This describes the retained UI design, not an enabled portable runtime. Use
`START.bat dry-run` for the strict zero-persistence product preview. Phase 2/3
recovery remains bound to a protected immutable runtime generation with
exact-set SHA-256 manifest validation.

### 4. Benchmark

Benchmark history comes from `benchmark_history.json`. The table is the
accessible data alternative to the custom chart.

To record a result:

1. enter a result label;
2. paste a line such as `[VProf] FPS: Avg=387.2, P1=312.0`;
3. choose `Add result`.

Empty labels and invalid VProf output are not recorded. `Parse` calculates the
recommended cap and enables `Copy cap` only when the current text is valid.

### 5. Network

Network provides a Valve-region route-quality diagnostic and an explicit DNS
A/B workflow.

- `Run baseline test` records the current adapter and DNS state.
- `Run post-change retest` creates the comparison run.
- Cloudflare, Google, DHCP, and restore actions use the suite's backup boundary.
- Region block actions clearly state that they can affect matchmaking routes.

The measurements use configured candidate endpoints and do not represent
in-match CS2 ping readings. Live SDR targets use ICMP. If the live target fetch
fails, checked-in Steam connection-manager candidates use TCP port 27017.
Blocked or unreachable endpoints appear as timeouts.

### 6. Video settings

Video settings compares the current trusted Steam `video.txt` with the
selected repository preset. `Auto` is a vendor heuristic: it selects `HIGH`
when an NVIDIA driver is detected and `MID` otherwise. It does not measure GPU
performance, VRAM, resolution, or frame rate. Writing requires confirmation, preserves
unmanaged keys, and retains the first original as `video.txt.bak`. CS2
must be closed before writing.

### 7. Recovery

Recovery reads `C:\FRAMETIME_CFG\backup.json` and disables restore actions
when data is empty, unreadable, or no row is selected.

- `Restore selected step` restores the selected step group.
- `Restore all` restores every recorded group.
- `Export JSON` saves a user-selected copy.
- `Clear all backups` deletes the recorded backup history and requires
  confirmation.

Restore mutations run outside the UI thread. The application prevents window
closure while recovery is active so a user cannot accidentally interrupt the
operation through the normal close action.

## Accessibility and scaling targets

The current release targets WCAG 2.2 AA-equivalent desktop behavior and Windows
UI Automation conventions. These targets are not validated guarantees for the
current alpha. Windows release validation still needs to cover:

- keyboard access and visible focus for every action;
- native Windows window controls;
- connected labels and accessible names for inputs;
- polite announcements for changing status;
- status that does not rely on color alone;
- Windows High Contrast resource replacement;
- operation at 100 to 200 percent scaling and a 960-by-540 effective viewport;
- wrapping action regions and horizontal table scrolling instead of hidden
  critical columns.

A phone or touch-first interface is not a product goal.

## Screenshots

No current screenshots are published pending validated capture on Windows.
Replacement images should be captured only after keyboard, High Contrast,
200 percent scaling, and 960-by-540 viewport checks pass, and after the image is
reviewed for personal data and machine-specific paths.

The required replacement set is:

1. Initial Overview state, showing the alpha notice, phase status, and next
   available action.
2. Assess after a completed scan, showing the populated status table and its
   non-color status labels.
3. Setup before live execution, showing the three phase boundaries and reboot
   or handoff guidance.
4. Recovery after a disposable-machine validation run, showing sanitized
   recorded entries and the restore controls.

Capture each image from the same validated candidate. Crop to the application
window and remove usernames, machine names, paths, adapter addresses, and other
host-specific data before publication.

## Maintainer references

- [Frontend architecture and conventions](frontend.md)
- [Product constraints](product.md)
- [Visual and component rules](ui-design.md)
- [Application architecture](architecture.md)
- [Backup and restore behavior](backup-restore.md)

The public repository does not include private work notes, local analysis
reports, personal paths, runtime state, or the removed legacy screenshots.
