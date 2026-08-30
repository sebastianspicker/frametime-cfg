# Native operations

`frametime.exe` is the terminal entrypoint. `frametime-gui.exe` exposes the same application workflow through a Windows desktop interface. Both require authenticated package authority before state-changing operations.

## Source preview

`frametime dry-run [1|2|3|4|all]` renders the guarded workflow without persistence or elevation. It is intended for source control-flow review. It cannot prove a Windows API call, hardware operation, package authentication, or reboot sequence.

## Three phases

1. Phase 1 runs in normal boot. It validates requested work, records prerequisite evidence, and can publish a verified runtime before arming a Safe Mode handoff.
2. Phase 2 runs in Safe Mode. It requires the selected runtime and exact handoff, performs guarded work, clears Safe Boot, and records the Phase 3 handoff.
3. Phase 3 runs in normal boot under the bound user. It verifies its runtime and handoff, performs guarded work, and clears the handoff only after coherent completion.

Commands reject missing or inconsistent state, unknown runtime selection, and unmet phase prerequisites. Do not run internal handoff commands manually or substitute a path to a different executable.

## Configuration and assets

`frametime.toml` is parsed as a validated immutable configuration snapshot. Its `work_dir` must remain `C:\FRAMETIME_CFG`. Optional CS2 CFG deployment uses embedded assets from `assets/cfgs/`, validates the Steam and CS2 target binding, captures supported prior bytes, and verifies managed bytes after deployment.

See [recovery](recovery.md) before attempting changes that need manual intervention.
