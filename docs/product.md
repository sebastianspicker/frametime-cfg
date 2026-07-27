# Product constraints

## Purpose

frametime.cfg provides a Windows PowerShell workflow for inspecting, applying,
verifying, and restoring selected Counter-Strike 2 and Windows configuration.
The terminal workflow owns the three reboot phases. The WPF interface provides
assessment, launch, verification, diagnostics, history, and recovery controls.

The application records average FPS, 1-percent low FPS, and a ratio derived
from those values. It does not capture per-frame telemetry and cannot determine
that a setting will improve a particular machine.

## Users

The primary user is an experienced Windows and Counter-Strike 2 user who can
review privileged system changes and recover a Windows installation if a
driver or boot transition fails.

Maintainers and support contributors need exact paths, state provenance,
timestamps, logs, backup entries, and partial-failure details. The interface
must not replace those details with an unqualified success state.

## Operational model

- Live operations run on x64 Windows desktop with administrator rights.
- Normal-mode, Safe Mode, and post-reboot work remains in visible terminal
  processes.
- Recorded execution and current observed state are separate facts.
- A step is complete only after its required result is applied and recorded.
- Recovery coverage must be stated per operation. It is not a system snapshot.
- Full DRY-RUN is a no-persistence preview, not proof that live Windows APIs
  will succeed.

## Interface constraints

- Show the next applicable action without hiding prerequisite or recovery
  information.
- Use tables for repeated state and comparison data.
- Use cards only for distinct functional units.
- Preserve native Windows window, focus, keyboard, and dialog behavior.
- Pair every status color with text or another non-color cue.
- Expose loading, empty, stale, error, cancellation, and partial-failure states.
- Keep consequential operations out of success-only callbacks.
- Do not use slogans, decorative metrics, RGB styling, gradients, glow, or
  marketing claims.

## Accessibility requirements

The interface must support keyboard operation, visible focus, Windows UI
Automation labels, Narrator-compatible status announcements, High Contrast,
200-percent display scaling, and operation at 960 by 540 effective pixels.
These requirements still need manual validation on Windows before compatibility
can be claimed.

## Non-goals

- Browser and mobile interfaces
- A separate light theme
- Automatic proof of performance improvement
- Complete Windows backup or disaster recovery
- Hidden or unattended execution of reboot-sensitive phases
