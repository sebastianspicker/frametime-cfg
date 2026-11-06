# Architecture

## Scope

The repository is one five-crate application plus two archived reference
workspaces. The application turns authenticated configuration and user commands
into typed plans, captures recoverable state, performs the supported Windows
operations, and verifies the results. Root hardware diagnostics are independent
read-only observations, not part of profile mutation or the archived tools.

Only the five-crate root workspace is actively supported. Northclock and Driver
Foundry are archived: their READMEs describe the retained source and its limits,
and they are excluded from CI, dependency automation, packaging, and the
contributor workflow. The root profile policy covers five settings. Production
signing stays with the vendor; lab signing is isolated, export-only, and
unqualified. The [NVIDIA driver lifecycle](native/driver-lifecycle.md) defines
the versioned transaction, component catalog, DRS merge policy, and
qualification boundary.

## System context

```mermaid
flowchart TB
    Operator[Windows operator] --> CLI[frametime CLI]
    Operator --> GUI[frametime GUI]
    CLI --> App[frametime-app]
    GUI --> App
    App --> Domain[frametime-domain]
    App --> Windows[frametime-windows]
    Windows --> Domain
    Windows --> OS[Windows, CS2, Steam, drivers and hardware]
    Windows --> State["Protected C:\\FRAMETIME_CFG state"]
    Package[Authenticated portable package] --> CLI
    Package --> GUI
    Package --> Windows
```

Where in the code:

- CLI entry: [`crates/frametime-cli/src/main.rs`](../crates/frametime-cli/src/main.rs)
  (DLL-search hardening) and [`app.rs`](../crates/frametime-cli/src/app.rs)
  (`clap` command → `frametime_app` call); commands are declared in
  [`cli.rs`](../crates/frametime-cli/src/cli.rs).
- GUI entry: [`crates/frametime-gui/src/main.rs`](../crates/frametime-gui/src/main.rs);
  button handlers run `frametime_app` calls on worker threads in
  [`app/actions.rs`](../crates/frametime-gui/src/app/actions.rs). The GUI does
  not run driver or Safe Mode work; that is terminal-only.
- Application API: [`crates/frametime-app/src/lib.rs`](../crates/frametime-app/src/lib.rs).
- Windows public surface: [`crates/frametime-windows/src/lib.rs`](../crates/frametime-windows/src/lib.rs).

The dependency direction is `domain <- windows <- app <- cli/gui`:

- `frametime-domain` owns the platform-neutral pieces: configuration
  validation, catalog entries, policy, state transitions, evidence, recovery
  contracts, and persistence formats.
- `frametime-windows` implements the Windows boundary the domain talks to:
  protected storage, package and runtime trust, discovery, planning, mutation,
  readback, reboot handoffs, and native integrations.
- `frametime-app` exposes typed commands, outcomes, and read models, and
  coordinates use cases without letting callers choose persistence paths.
- `frametime-cli` and `frametime-gui` translate input and output. They may use
  domain presentation values and narrowly scoped Windows startup or
  native-window plumbing, but workflow decisions stay in the app and domain
  layers.

Public surfaces are deliberately narrow:

- Domain items are reached only through their owning module
  (`frametime_domain::backup::BackupFile`); the crate root exports modules and
  `PRODUCT_VERSION`, nothing else.
- `frametime-windows` publishes one explicit list in `src/lib.rs`, identical on
  every target. Off Windows the native operations are fail-closed stubs, so
  the app, frontends, and source preview build and test on any host. Because
  of those stubs, host builds allow dead code in this crate, and the
  Windows-target Clippy run is the authority for it.

The unpublished [`repo-checks`](../repo-checks) workspace member holds source
helpers and the tests that `cargo test --workspace` runs: every `call :label`
in `scripts/*.cmd` resolves to a defined label, and the shipped CS2 cfg assets
and video-settings reference use only names from the pinned CVar and video-key
snapshots. The dependency direction, domain purity, frontend boundary, package
surface, and file-size rules in [CONTRIBUTING.md](../CONTRIBUTING.md) are
contributor rules reviewed by hand, not automated checks.

## What happens when you run a command

Every step-based command, preview or live, goes through one loop:
`frametime_domain::engine::Engine` walks steps from the compiled
`step_catalog()` and calls a `Backend` for each one. Preview and live work
differ only in which backend they get.

### `frametime dry-run all` (read-only preview)

```mermaid
flowchart LR
    cli["app.rs run()"] -->|"Branch"| dry["run_dry"]
    dry -->|"Engine::new"| engine["Engine"]
    engine -->|"inspect / plan"| planner["PlannerBackend"]
    planner -.->|"plan lines"| engine
    engine -->|"Event::Plan"| render["render::dry_run"]
    classDef focus fill:#fde68a,stroke:#b45309,color:#1f2937
    class planner focus
```

`run_dry` builds one `Engine` per GPU branch (1–4) and runs Phase 1, 2 and 3
steps against `PlannerBackend`. That backend's `apply` and `verify` are no-ops
and it never touches storage, so the preview needs no package, elevation or
Windows host. That is why CI can run it on Linux.

Where in the code: [`actions.rs` `run_dry`](../crates/frametime-app/src/actions.rs),
[`backend/planner.rs`](../crates/frametime-windows/src/backend/planner.rs),
[`engine.rs`](../crates/frametime-domain/src/engine.rs),
[`catalog.rs`](../crates/frametime-domain/src/catalog.rs).

### `frametime optimize` (Phase 1, live)

```mermaid
sequenceDiagram
    autonumber
    actor U as Operator
    participant cli as frametime.exe
    participant app as frametime-app
    participant dom as Engine (domain)
    participant win as LiveBackend
    participant st as C:\FRAMETIME_CFG
    U->>cli: frametime optimize
    cli->>app: run_live(Command::Optimize)
    app->>win: authenticate_current_package()
    win-->>app: AuthenticatedPackage
    app->>dom: authorize(PhaseRequest::Optimize)
    app->>win: LiveBackend::from_package (takes run lock)
    loop each Phase 1 step
        dom->>win: inspect, capture_backups
        win->>st: persist backup / audit
        dom->>win: apply, then verify
        dom->>win: persist_progress
        win->>st: progress.json
    end
    app->>win: publish_current_packaged_runtime (NVIDIA only)
    app->>win: launch_published_safe_mode_handoff
    app-->>cli: RunSummary
```

Steps 3–4 are the authority check: without an authenticated package the
command stops here. Step 5 asks the pure domain state machine whether the
saved state and progress allow Phase 1 right now. In the loop, the engine picks
a recovery route per step (lossless backup, rebuildable audit, or irreversible
audit). `LiveBackend::apply` refuses to run unless that step's capture
succeeded under the transaction lock. Steps 12–13 run only on NVIDIA and only
after consent: they copy the package into a protected runtime and arm the
Safe Mode handoff (P1:38) for Phase 2. Without a Windows host, step 2 stops
with "live commands require x64 Windows 10 or 11".

The other live commands (`configure`, `restore`, `phase2`, `phase3` and so on)
enter through the same `run_live` match. The GUI calls the same functions.

Where in the code:
[`workflow/live_commands.rs` `run_live`](../crates/frametime-app/src/workflow/live_commands.rs),
[`workflow.rs` `run_phase_one`, `run_live_steps`](../crates/frametime-app/src/workflow.rs),
[`orchestration.rs` `authorize`](../crates/frametime-domain/src/orchestration.rs),
[`package_trust/mod.rs`](../crates/frametime-windows/src/trust/package_trust/mod.rs),
[`backend/live.rs`](../crates/frametime-windows/src/backend/live.rs),
[`runtime_publish/mod.rs`](../crates/frametime-windows/src/trust/runtime_publish/mod.rs).

Cancellation is observed between workflow steps, never inside an in-progress
native mutation.

## Package and configuration authority

[`package-layout.txt`](../package-layout.txt) is the portable package inventory.
An authenticated release adds `package.manifest.json` and `package.cat` on top
of it and verifies the complete file set, hashes, catalog membership, executable
roles, publisher pin, package root, and retained identities. The configuration
is kept as the exact authenticated byte snapshot, so code never reopens a
mutable ambient configuration path mid-operation.

Fresh work uses the package snapshot; resumed reboot work uses the snapshot
inside the verified selected runtime. Runtime publication has its own compiled
24-file payload contract, so the release package and the reboot runtime are
related but separate inventories. See [configuration](CONFIGURATION.md),
[package security](package-security.md), and [deployment](DEPLOYMENT.md).

## State, evidence, and recovery

The Windows adapter exclusively owns live state under `C:\FRAMETIME_CFG`. It
validates the fixed root and its ACL/handle boundary, uses locks and atomic
writes where the storage contract requires them, and reads critical data back
after writes. State covers workflow progress, backups, audit and irreversible
operation evidence, driver transactions, benchmark history, and selected
runtime generations.

| File under `C:\FRAMETIME_CFG` | Holds |
| --- | --- |
| `state.json` | Selected profile, GPU branch, FPS-cap decision, reboot transaction |
| `progress.json` | Current phase, last completed/skipped step, completed step ids |
| `backup.json` (+ `backup.lock`) | Captured pre-mutation values used by `restore` |
| `audit.json`, `evidence.json` | Rebuildable/irreversible audit records and observation receipts |
| `benchmark_history.json` | Imported VProf benchmark records |
| `driver-profile-backup.json` | NVIDIA DRS profile backup |
| `Logs\frametime_current.log` | The log `show-log` prints |

File names are defined in
[`frametime-windows/src/lib.rs`](../crates/frametime-windows/src/lib.rs),
[`storage/persistence.rs`](../crates/frametime-windows/src/storage/persistence.rs)
and [`driver/transaction.rs`](../crates/frametime-windows/src/driver/transaction.rs);
their typed shapes are in
[`frametime-domain/src/state`](../crates/frametime-domain/src/state) and
[`backup.rs`](../crates/frametime-domain/src/backup.rs).

The domain engine captures and persists applicable state before mutation.
Restore paths act only on captured, identity-bound targets and fail closed when
state is missing or incoherent. This is transaction evidence, not a system
image or a universal rollback facility. See
[operations](native/operations.md) and [recovery](native/recovery.md).

## Windows and external boundaries

`frametime-windows` owns the registry, services, Task Scheduler, WMI, SetupAPI,
networking, filesystem, process, graphics, WinTrust, and driver-facing calls.
External system tools come from a compiled allowlist, resolve below System32,
and receive typed arguments rather than caller-provided command text. NVIDIA
acquisition is restricted to a compiled host and a server-relative path, then
checked through package and driver trust policy.

Package authentication and elevation are the authorization boundaries; there is
no application account or network authentication system. Publisher pins are
compile-time release inputs. Signing private keys and certificates are external
release infrastructure and must never enter the repository.

## Reboot topology

The driver workflow may publish a verified runtime and span three stages:

```mermaid
stateDiagram-v2
    [*] --> Phase1: frametime optimize
    Phase1 --> Done: AMD or Intel Arc, or handoff declined
    Phase1 --> Phase2: Safe Boot + RunOnce armed, reboot
    Phase2 --> Phase3: Safe Boot cleared, Run armed, reboot
    Phase3 --> Done: handoff cleared after success
    Phase2 --> Aborted: abort-handoff
    Phase3 --> Aborted: abort-handoff
    Done --> [*]
    Aborted --> [*]
```

1. **Phase 1 — normal boot.** Captures state and may arm an exact Safe Mode
   handoff.
2. **Phase 2 — Safe Mode.** Verifies the selected runtime and handoff, performs
   its guarded work, clears Safe Boot, and records the same-user Phase 3
   handoff.
3. **Phase 3 — normal boot.** Verifies the retained runtime and user binding,
   completes guarded work, and clears the handoff only after coherent success.

Where in the code: `run_phase_two`, `run_phase_three` and
`run_phase_three_handoff` in
[`workflow.rs`](../crates/frametime-app/src/workflow.rs); arming, clearing and
abort in [`frametime-windows/src/reboot`](../crates/frametime-windows/src/reboot).

The selected runtime and handoff records are capabilities, not resumable paths
that a caller may substitute. A missing or inconsistent stage fails instead of
guessing the next action.

## Build and deployment boundaries

```mermaid
flowchart LR
    pr["Dependabot PR or<br/>manual dispatch"] --> host["frametime-host<br/>fmt, clippy, test, dry-run"]
    pr --> wingate["frametime-windows<br/>verify.cmd"]
    wingate -->|"cargo build --release"| unsigned["package.cmd /unsigned"]
    unsigned --> zip[("dist ZIP + .sha256")]
    rel["Release machine"] -->|"signing inputs"| release["package.cmd /release"]
    release --> signed[("signed package + package.cat")]
    classDef focus fill:#fde68a,stroke:#b45309,color:#1f2937
    class release focus
```

CI ([`rust.yml`](../.github/workflows/rust.yml)) runs only for Dependabot pull
requests or manual dispatch. The Windows job
runs the same gate a contributor runs locally
([`verify.cmd`](../scripts/verify.cmd): fmt, Clippy, tests with and without `--all-features`,
`dry-run all`, `cargo audit`) and then builds and structurally verifies the **unsigned**
package. The signed `/release` lane runs only on a controlled release machine
([`package.cmd`](../scripts/package.cmd), [deployment](DEPLOYMENT.md)).
[`security.yml`](../.github/workflows/security.yml) (secret scan, workflow
integrity) also runs on push. [`pages.yml`](../.github/workflows/pages.yml)
publishes [`site/`](../site) on manual dispatch.

The root workspace has its own lockfile and validation gates. The archived
workspaces keep their separate Cargo manifests and lockfiles as historical
records, not active validation lanes. Root release packaging consumes prebuilt
Windows CLI/GUI binaries and produces a portable directory and ZIP; it does not
package `repo-checks` or either archived workspace. CI assembles only the structurally checked
unsigned lane and never exercises authenticated signing.

## Code map

In the order a newcomer would read it:

| Path | Responsibility |
| --- | --- |
| [`crates/frametime-cli/src/cli.rs`](../crates/frametime-cli/src/cli.rs) | Every terminal command and flag |
| [`crates/frametime-cli/src/app.rs`](../crates/frametime-cli/src/app.rs) | Maps a parsed command to one `frametime_app` call |
| [`crates/frametime-app/src/lib.rs`](../crates/frametime-app/src/lib.rs) | Public use-case API shared by CLI and GUI |
| [`crates/frametime-app/src/workflow.rs`](../crates/frametime-app/src/workflow.rs) | Phase 1/2/3 orchestration and live step runs |
| [`crates/frametime-app/src/benchmark.rs`](../crates/frametime-app/src/benchmark.rs) | VProf import and FPS-cap evaluation |
| [`crates/frametime-domain/src/catalog.rs`](../crates/frametime-domain/src/catalog.rs) | The compiled 54-step catalog |
| [`crates/frametime-domain/src/engine.rs`](../crates/frametime-domain/src/engine.rs) | `Engine` loop and the `Backend` trait |
| [`crates/frametime-domain/src/orchestration.rs`](../crates/frametime-domain/src/orchestration.rs) | Which phase transition is allowed |
| [`crates/frametime-domain/src/policy.rs`](../crates/frametime-domain/src/policy.rs) | Profiles and the per-step auto/prompt/skip decision |
| [`crates/frametime-windows/src/backend`](../crates/frametime-windows/src/backend) | `PlannerBackend` (preview) and `LiveBackend` (real) |
| [`crates/frametime-windows/src/operations`](../crates/frametime-windows/src/operations) | Step → native action descriptors |
| [`crates/frametime-windows/src/system`](../crates/frametime-windows/src/system) | Registry, services, network, pagefile, CS2/Steam |
| [`crates/frametime-windows/src/driver`](../crates/frametime-windows/src/driver) | NVIDIA package, DRS and driver transaction |
| [`crates/frametime-windows/src/storage`](../crates/frametime-windows/src/storage) | Protected state files, backup, audit, recovery |
| [`crates/frametime-windows/src/trust`](../crates/frametime-windows/src/trust) | Package auth, runtime publication, trusted I/O |
| [`crates/frametime-windows/src/reboot`](../crates/frametime-windows/src/reboot) | Safe Mode / Run handoffs and abort |
| [`crates/frametime-gui/src`](../crates/frametime-gui/src) | Win32 window (`app/`) and testable view model (`model/`) |
| [`repo-checks/src`](../repo-checks/src) | Batch-label and CS2 cfg consistency tests |
| [`scripts`](../scripts) | `verify.cmd` gate, `package.cmd` packaging |

## Invariants and non-goals

- No reverse dependencies, and no duplicated workflow policy in a frontend.
- The trusted work root, runtime payload, and package inventory are never
  caller-selectable.
- Hashes supplied by the same untrusted caller are not treated as independent
  authorization.
- Unsupported, unavailable, or unverified hardware behavior is reported as
  such, never as success.
- Source tests, mocks, cross-compilation, and protocol validation are not
  described as live Windows or hardware qualification.
- Universal performance gains and a general-purpose system rollback facility
  are not current capabilities. Archived Northclock and Driver Foundry content
  is not an active product claim.
