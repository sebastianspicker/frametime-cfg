# Desktop interface guide

The supported desktop interface is a Windows Presentation Foundation (WPF)
application launched through `START-GUI.bat`. It complements the resumable
terminal phases; it does not replace Phase 1, Safe Mode Phase 2, or Phase 3.

## Launch and runtime

1. Extract the repository or release archive to a local Windows folder.
2. Right-click `START-GUI.bat` and choose **Run as administrator**.
3. Use the task navigation or Ctrl+1 through Ctrl+7.

Normal execution requires an x64 Windows desktop and administrator rights.
`CS2-Optimize-GUI.ps1 -SmokeTest` is the non-elevated entrypoint check used
by CI. WPF rendering, UI Automation, Narrator, and High Contrast behavior require
Windows and cannot be validated on macOS.

## Task areas

### 1. Overview

Overview presents hardware detection, recorded phase progress, the latest
benchmark comparison, startup drift status, and explicit next actions.

- Hardware detection runs in a background runspace.
- Progress comes from `C:\CS2_OPTIMIZE\progress.json`.
- Recorded phase completion and currently observed system state are different
  facts; use **Assess system** or **Verify supported settings** for fresh state.
- Phase launch buttons open the existing terminal workflows. Phase 2 and Phase
  3 buttons resolve `runtime-current.json` to the validated published payload
  in its immutable runtime generation; if it is missing or fails integrity validation, the
  GUI gives recovery guidance instead of starting an unvalidated handoff.

### 2. Assess

**Run full scan** performs non-destructive hardware, Windows, storage, network,
service, and CS2 configuration checks. Duplicate scans are blocked and the scan
can be cancelled.

Results include category, current value, recommended value, status, step
reference, and impact. Status always includes text or a symbol; color is
supplemental. **Export report** writes a user-selected CSV file.

Storage maintenance is shown in the same task area. **Enable TRIM** and
**ReTrim** are confirmed maintenance actions, not claimed FPS optimizations.

### 3. Setup and verify

This task combines the settings that affect execution with the step catalog:

- profile: Safe, Recommended, Competitive, Custom, or YOLO;
- preview mode, which preserves the terminal dry-run behavior;
- category and status filters;
- phase, tier, risk, recorded status, and reboot information;
- terminal launch actions for Phase 1, Safe Mode Phase 2, and Phase 3;
- non-mutating inline verification of supported settings.

Profile and preview state persists in `C:\CS2_OPTIMIZE\state.json`. Phase
internals still run in terminal processes so reboot handoffs and prompts remain
resumable. The Phase 2/3 payload is a fixed file set published as an immutable
runtime generation with exact-set SHA-256 manifest validation. The GUI
does not substitute a source-tree script when that payload is unavailable.

### 4. Benchmark

Benchmark history comes from `benchmark_history.json`. The table is the
accessible data alternative to the custom chart.

To record a result:

1. enter a result label;
2. paste a line such as `[VProf] FPS: Avg=387.2, P1=312.0`;
3. choose **Add result**.

Empty labels and invalid VProf output are not recorded. **Parse** calculates the
recommended cap and enables **Copy cap** only when the current text is valid.

### 5. Network

Network provides a Valve-region route-quality diagnostic and an explicit DNS
A/B workflow.

- **Run baseline test** records the current adapter and DNS state.
- **Run post-change retest** creates the comparison run.
- Cloudflare, Google, DHCP, and restore actions use the suite's backup boundary.
- Region block actions clearly state that they can affect matchmaking routes.

The measurements use configured candidate endpoints and are not guaranteed
in-match CS2 ping readings. ICMP-blocking endpoints appear as timeouts.

### 6. Video settings

Video settings compares the current trusted Steam `video.txt` with the
selected hardware-tier recommendations. Writing requires confirmation, preserves
unmanaged keys, and retains the first original as `video.txt.bak`. CS2
must be closed before writing.

### 7. Recovery

Recovery reads `C:\CS2_OPTIMIZE\backup.json` and disables restore actions
when data is empty, unreadable, or no row is selected.

- **Restore selected step** restores the selected step group.
- **Restore all** restores every recorded group.
- **Export JSON** saves a user-selected copy.
- **Clear all backups** is irreversible and requires confirmation.

Restore mutations run outside the UI thread. The application prevents window
closure while recovery is active so a user cannot accidentally interrupt the
operation through the normal close action.

## Accessibility and responsive behavior

The target is WCAG 2.2 AA-equivalent desktop behavior plus Windows UI
Automation conventions:

- keyboard access and visible focus for every action;
- native Windows window controls;
- connected labels and accessible names for inputs;
- polite announcements for changing status;
- status that does not rely on color alone;
- Windows High Contrast resource replacement;
- operation at 100–200% scaling and a 960-by-540 effective viewport;
- wrapping action regions and horizontal table scrolling instead of hidden
  critical columns.

A phone or touch-first interface is not a product goal.

## Maintainer references

- [Frontend architecture and conventions](frontend.md)
- [Product definition](../PRODUCT.md)
- [Visual and component rules](../DESIGN.md)
- [Application architecture](architecture.md)
- [Backup and restore behavior](backup-restore.md)

The public repository intentionally does not ship local agent ledgers, generated
analysis reports, personal paths, runtime state, or unvalidated legacy
screenshots.
