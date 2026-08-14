# Native desktop interface

The GUI exposes seven keyboard-reachable task areas: Overview, Assess,
Setup / Verify, Benchmark, Network, Video, and Recovery. Phase work opens in a
visible terminal so reboot handoffs and partial failures remain observable.
The GUI must refuse to start in Safe Mode.

External launch and every mutation require an authenticated release package.
Authentication binds the current GUI, its sibling CLI, the fixed package
inventory, and the configured publisher while retaining the authenticated file
objects. An unsigned development package keeps those controls hidden. Setup
configuration is an explicit elevated action; changing a combo box or checkbox
does not write state. Start / resume Phase 1 invokes the authenticated package
CLI, which publishes the protected runtime and owns the reboot handoffs. Verify
is read only. The portable GUI never invokes protected-runtime-only Phase 2 or
Phase 3 commands directly.

The Windows build uses standard Win32 controls and native chrome. Release
validation must cover UI Automation names and events, tab order, keyboard
activation, focus restoration, cancellation, High Contrast, 100 to 200 percent
scaling, and a 960 by 540 effective viewport. Statuses always include text;
color is supplemental.

Assess runs the same typed, in-process hardware service as the CLI for doctor,
CPU, GPU, system, and bounded WHEA views. Benchmark can start a bounded
five-second ETW frame observation. These read-only workers render the
`frametime.hardware/v1` schema and explicit unavailable/failure states; they do
not advance workflow progress or claim hardware qualification.

Video provides a trusted-Steam-root preview of all 13 managed settings. Apply
requires confirmation, creates `video.txt.bak` once, performs an atomic
replacement, and refreshes the readback preview in a background worker.
Network applies the typed P1:16 transaction in process. If elevation is needed,
the authenticated GUI relaunches itself through UAC and requires the operator
to confirm the mutation again. Benchmark can also persist a validated VProf
capture through the authenticated CLI. None of these controls aliases an
unrelated command.
