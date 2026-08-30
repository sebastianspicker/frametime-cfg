# Native architecture

The root workspace has five crates:

```text
frametime-cli ─┐
               ├─> frametime-app ─┬─> frametime-domain
frametime-gui ─┘                  └─> frametime-windows ─> frametime-domain
```

`frametime-domain` owns platform-independent configuration validation, models, catalog entries, policy, workflow transitions, and persistence formats. `frametime-windows` owns Windows APIs and trust boundaries: package authentication, protected storage, runtime publication, reboot handoffs, and hardware integrations. `frametime-app` composes domain rules with the adapter. The CLI and GUI are separate presentation entrypoints that call the application layer.

The entrypoints may use domain value types for parsing and rendering. Their only direct Windows dependency is startup or presentation plumbing such as process hardening, package authentication, and native window control; workflow decisions belong in `frametime-app`.

## Placement rules

- Add a value type, policy, state transition, or pure validation to `frametime-domain`.
- Add a Windows handle, registry, filesystem, process, SetupAPI, WinTrust, NVAPI, or reboot operation to `frametime-windows`.
- Add command orchestration to `frametime-app`.
- Keep argument parsing and console output in `frametime-cli`; keep window construction and UI interaction in `frametime-gui`.
- Do not create reverse dependencies or let a frontend reproduce domain workflow rules.

The Windows adapter mirrors those responsibilities physically under `backend/`, `diagnostics/`, `driver/`, `operations/`, `reboot/`, `storage/`, `system/`, and `trust/`. `lib.rs` is a narrow façade; new implementation files belong under one of those owners.

The domain boundary is enforced by the `architecture_boundary` integration test and [`scripts/check-domain-boundary.sh`](../scripts/check-domain-boundary.sh). Both reject host filesystem, environment, process, clock, and Windows dependencies in production domain code. [`scripts/check-architecture-boundaries.sh`](../scripts/check-architecture-boundaries.sh) separately enforces workspace dependency direction, presentation ownership, and catalog-derived GUI phase totals.

## Package inputs and persistence

`frametime.toml` is the validated configuration source. `assets/` supplies packaged CFG and video-data assets embedded by the domain crate. A release package also carries the two PE entrypoints, package manifest, catalog, documentation, and licenses as listed in [`package-layout.txt`](../package-layout.txt).

The work root is always `C:\FRAMETIME_CFG`. The Windows adapter validates and protects that root before reading or writing state, backups, logs, artifacts, or runtime generations. Package input and work-root state are separate authorities.

`tools/northclock` and `tools/driver-foundry` are independent workspaces. They do not sit in the main dependency graph.
