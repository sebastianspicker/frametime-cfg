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
- `cleanup quick` and `cleanup full` both delete Windows and user Temp contents
  irreversibly and require `--yes --acknowledge-irreversible`. In this alpha
  `full` runs the same actions as `quick`; the repair actions (shader caches,
  Prefetch, Winsock reset, event logs) are not offered.

State-changing construction takes the fixed protected root `C:\FRAMETIME_CFG`,
requires elevation, and uses the immutable configuration snapshot attached to
the authenticated package or selected runtime. See
[configuration](../CONFIGURATION.md).

## Before you run optimize

Two Phase 1 steps depend on earlier commands. Run these in order:

1. `frametime baseline-benchmark --vprof-file <capture>` persists the P1:17
   baseline. Without it, `optimize` stops at P1:17.
2. On NVIDIA, `frametime driver prepare-nvidia` (with `--official-url` or
   `--local-installer`) persists the P1:18/P1:19 driver package evidence.
   Without it, `optimize` stops at P1:19.
3. `frametime optimize`. Without `--yes`, it first asks once whether to continue
   from the existing Phase 1 state (left by the earlier commands), then asks per
   step. `--yes` skips both.

## Three phases

In this alpha only the NVIDIA branches continue past Phase 1. On AMD and Intel
Arc, `optimize` completes Phase 1 and stops without arming Safe Mode, because
P2:2 and P2:3 require a prepared NVIDIA driver transaction.

1. **Phase 1 — normal boot.** Validates requested work, captures and persists
   prerequisite evidence, and may publish a verified runtime before arming the
   exact Safe Mode handoff.
2. **Phase 2 — Safe Mode.** Requires the selected runtime, transaction stage,
   and exact handoff; performs guarded work, clears Safe Boot, and records the
   same-user Phase 3 handoff. All of it runs in the one Safe Mode launch,
   because Windows removes the RunOnce handoff when that process exits. If it
   fails after Safe Boot is cleared, follow the stuck-handoff procedure in
   [recovery](recovery.md).
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
attempts only supported identity-bound restores. `abort-handoff --yes` removes
exactly the Safe Boot and Run/RunOnce values an active reboot transaction armed
and clears that transaction; see [recovery](recovery.md).

Every state-changing live command (`optimize`, `configure`, `cs2-cfg`,
`boot-safe-mode`, `phase2`, `phase3`, `phase3-handoff`, `cleanup`, `restore`,
`reset-progress`, `abort-handoff`) appends to `C:\FRAMETIME_CFG\Logs\frametime_current.log`. The
`Logs` directory and file are created on first use with the protected work-directory
security, inside an existing work directory: the tool never creates `C:\FRAMETIME_CFG`
just to log, and a command run without elevation writes no log. Each run writes a start record, a command record, one record per Engine
step (applied, skipped, satisfied, inspect-only, advisory, or failed with the error
text), and an `ERROR` record if the command fails. Computer and user names and
`C:\Users\<name>` paths are redacted. When the log passes 8 MiB it is renamed to
`frametime_<timestamp>.log` in the same directory and a new one is started. A log
write failure prints `warning: could not write log: ...` on stderr and does not
change the command's outcome. Read-only commands (`dry-run`, `verify`, `hardware`,
`fps-cap --no-persist`) do not write the log.

`show-log` reads the bounded `Logs\frametime_current.log`; before any live command
has run it prints `No log has been written yet (...)` and exits 0. Attach this file
(and the archives, if the problem spans a rotation) to bug reports.

Read [recovery](recovery.md) before attempting a live operation or intervening
in an incomplete phase.

The consolidated NVIDIA package, cleanup, DRS backup, reconciliation, and lab
export commands are documented in the [driver lifecycle](driver-lifecycle.md).
