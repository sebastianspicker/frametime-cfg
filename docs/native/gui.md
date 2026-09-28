# Native GUI

`frametime-gui.exe` is a native Windows desktop entrypoint. It uses the same
application layer as the CLI and shows package-authentication state before it
exposes protected operations.

The GUI does not make an unauthenticated package trusted and does not bypass
command, phase, recovery, or elevation checks. When authority is unavailable, it
stays a read-only or unavailable surface as appropriate.

Host builds do not establish keyboard navigation, screen-reader output, High
Contrast, scaling, window behavior, elevation, or live Windows integration.
Record UI Automation and manual evidence on Windows for changes to those areas.

## Read model and refresh

Overview, benchmark history, recovery, video discovery, and driver inspection
refresh through a dedicated read worker. Navigation, F5, the visible refresh
buttons, video input changes, and completed operations all request fresh
snapshots. The category/status filter uses cached rows and performs no filesystem
reads.

The snapshot row reports loading, ready, empty, stale, or failed independently of
operation status. A failed refresh keeps the last successful rows and labels them
stale. Obsolete reads are discarded, including reads from before a completed
operation. Cached rows carry no mutation authority: execution still validates
current targets and retained package capabilities.

## VProf input

VProf input is limited to 8 MiB of UTF-8 and 1,000 valid observations. A nonzero
cap requires at least five complete runs, and every unrounded P1 result must
support it. Parsing displays individual results and their P1 range. Persistence
reuses the validated capture through the application service.

The VProf edit control raises its native text limit above the application byte
limit and rejects any input reported incomplete by `EN_MAXTEXT`. Clear the field
before replacing a rejected paste; shortening a truncated prefix cannot turn it
into valid evidence. This accounts for the native control's
[default text limit](https://learn.microsoft.com/en-us/windows/win32/controls/em-limittext).

## Classic desktop presentation

The native GUI uses a Windows 98-inspired client surface: gray group panels, a
navy task title strip, raised native buttons, inset fields, and DPI-scaled Tahoma
text. The outer window frame stays managed by the current Windows version. High
Contrast uses system colors. This styling does not add support for Windows 98;
the application still requires supported x64 Windows 10/11.

The nine existing destinations remain available in the top task toolbar and
through Ctrl+1 through Ctrl+9. Their operations, integrations, and authority
checks remain in the shared application layer. F5 refreshes the current cached
read model, F6 restores focus, and Escape keeps the existing cancellation
semantics. The minimum client size is 960 by 640 logical pixels; controls,
columns, and spacing resize with the window and scale with per-monitor DPI.
Read-only evidence and permission fields can be focused, copied, and scrolled.

## Guided FPS strategy

Benchmark contains a four-stage FPS workflow:

1. **Import.** Choose a VProf text file through the native file dialog, or paste
   complete results into the labeled source field. File reading and parsing run
   on a worker through the shared bounded application reader. No benchmark is
   launched and no measurement is fabricated.
2. **Evaluate.** Evaluate the accepted run evidence using raw mode (the existing
   default, zero for uncapped) or a VRR refresh rate and ceiling margin. The GUI
   does not detect or enable VRR, and the refresh rate starts empty rather than
   assuming a monitor value. Individual parsed Avg/P1 numbers stay visible,
   including weak runs when the selected cap fails. Editing source or strategy
   invalidates the prior evaluation. The shared service authorizes the cap.
3. **Review.** Review the selected cap and capture label. Saving requires
   authenticated package authority and administrator access. The administrator
   button opens a separate elevated GUI through the retained-capability
   watchdog; it does not transfer unsaved evidence, and the review explains that
   the source must be imported again there. The unelevated calculation remains
   retained.
4. **Save.** Save cap and evidence in-process, with controls disabled while the
   operation runs. Closing is blocked during persistence. Success is displayed
   only when the service reports persisted state and history after readback.
   Saving a cap does not deploy CS2 configuration, complete a final benchmark
   receipt, or establish improved game performance.

Cap state and history are independently atomic, so errors and disconnected save
workers are reported as possible partial saves, never as automatic rollback.
Inspect history and verified state before starting another save. A completed
final-benchmark receipt keeps blocking advisory replacement. The saved screen's
Done action starts a new calculation and retains prior persisted evidence.

History / diagnostics opens the existing history snapshot, filter, and bounded
five-second ETW capture. Returning preserves the in-memory FPS session. Other
configuration, diagnostics, driver inspection, optional CFG deployment, and
recovery capabilities are preserved outside the guided calculation.

## Design reference and validation boundary

The approved visual reference is
[`guided-session-flow.png`](../assets/concepts/guided-session-flow.png). The
implementation uses actual Win32 controls and text, not that bitmap as a screen.
The source defines evidence validation, stale-result invalidation, review/save
transitions, partial-save errors, and source byte limits. Windows
cross-compilation checks the native code but does not execute it.

Native rendering and screenshots remain unverified from the macOS implementation
host, which has no registered Windows VM or Wine runtime. Before accepting visual
parity, run the built application on Windows at 1180 by 760 and 960 by 640 client
pixels, then at 150% and 200% scaling on a suitably sized display. Capture Import,
Evaluate, Review, and Saved, and compare typography, wrapping, bevels, spacing,
and focus with the reference. Also verify High Contrast, keyboard-only operation,
and screen-reader names, including the run table and scrollable readouts.

Use disposable, non-personal VProf input. Check an empty source, four runs, a
weak fifth run, invalid UTF-8, oversized input, an invalid refresh/margin, and an
unauthenticated package. A source build can demonstrate analysis and blocked
saving; it cannot demonstrate an authenticated save. Qualify actual persistence
and its success screenshot separately with a signed package, appropriate
privilege, and a recoverable Windows test environment. Record the Windows
version, architecture, privilege, inputs, result, and recovery evidence without
personal identifiers.
