# Native operations

`frametime.exe` is the terminal entrypoint. `frametime-gui.exe` exposes the same
application workflow through a Windows desktop interface. A source build can
preview behavior; both entrypoints require authenticated package authority
before any state-changing operation.

## Source preview

From the repository root:

```sh
cargo run -p frametime-cli --locked -- dry-run all
```

The optional argument selects a GPU branch, not a workflow phase: `1` is NVIDIA
RTX 5000, `2` is NVIDIA, `3` is AMD, `4` is Intel Arc, and `all` evaluates every
branch. Preview performs no persistence, elevation, package authentication, or
Windows mutation. It cannot prove a Windows API call, a hardware operation, or a
reboot sequence.

## Live preconditions

- Use an authenticated release package whose complete inventory, catalog,
  signatures, executable role, retained identities, and compiled publisher pin
  all validate.
- Run on supported x64 Windows with administrator access for mutation.
- Review the planned operation and keep independent backups and recovery media
  before any driver, Safe Mode, reboot, or irreversible cleanup step.
- Do not replace package, selected-runtime, handoff, or protected-root files to
  work around a failed check.

State-changing construction takes the fixed protected root `C:\FRAMETIME_CFG`,
requires elevation, and uses the immutable configuration snapshot attached to
the authenticated package or selected runtime. See
[configuration](../CONFIGURATION.md).

## Three phases

1. **Phase 1 — normal boot.** Validates requested work, captures and persists
   prerequisite evidence, and may publish a verified runtime before arming the
   exact Safe Mode handoff.
2. **Phase 2 — Safe Mode.** Requires the selected runtime, transaction stage,
   and exact handoff; performs guarded work, clears Safe Boot, and records the
   same-user Phase 3 handoff.
3. **Phase 3 — normal boot, bound user.** Verifies the runtime, user binding,
   and handoff; performs guarded work; and clears the handoff only after
   coherent completion.

Commands reject missing or inconsistent state, unknown runtime selection, and
unmet phase prerequisites. Do not invoke internal handoff commands manually or
substitute a path to another executable. Cancellation is honored between
workflow steps; it does not interrupt a native mutation already in progress.

## Evidence surfaces

### Benchmark commands

`fps-cap` accepts exactly one source: the legacy positional average,
`--vprof-text`, `--vprof-file`, or `--clipboard`. The positional aggregate is
accepted only for the raw uncapped choice. A nonzero `--measured-cap` or VRR
ceiling requires VProf run evidence. Inputs are limited to 8 MiB of complete
UTF-8 and 1,000 valid observations; file and clipboard reads are bounded before
parsing. Invalid result lines retain the legacy behavior of being ignored.

The terminal output prints every accepted Avg/P1 run in source order, the P1
minimum and maximum, and the supporting and failing counts for the selected cap.
A parsed run with P1 above Avg is retained for compatibility but counts as
invalid support evidence. Rejection reports both failing and invalid counts.
`--no-persist` keeps the calculation advisory; persistence and native clipboard
writes require authenticated Windows package authority.

`baseline-benchmark` persists the P1:17 observation without changing FPS-cap
state. `final-benchmark` persists transaction-bound P3:13 evidence after the
earlier Phase 3 steps resolve. Both require exactly one complete VProf source.
New history records and final receipts retain versioned ordered run evidence on
top of the compatible rounded aggregate fields.

From an authenticated package, `verify` prints a read-only structured state
snapshot, `backup-summary` reports captured backup counts, and `restore --yes`
attempts only supported identity-bound restores. `show-log` reads a bounded
existing `Logs\frametime_current.log`; the current implementation does not create
or append that file, so a fresh installation may have no log to show.

Read [recovery](recovery.md) before attempting a live operation or intervening
in an incomplete phase.

The consolidated NVIDIA package, cleanup, DRS backup, reconciliation, and lab
export commands are documented in the [driver lifecycle](driver-lifecycle.md).
