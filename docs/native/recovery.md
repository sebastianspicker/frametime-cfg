# Recovery

Recovery is limited to state the workflow captured and can bind to the same
target identity. It is not a system image, a general Windows rollback tool, or a
promise that every external action is reversible.

## Before live work

- Use a disposable Windows VM for first qualification of package, privilege,
  Safe Mode, reboot, registry, driver, network, or filesystem changes.
- Keep independent system backups and bootable recovery media.
- Record the Windows version, architecture, privilege level, package identity,
  selected operation, and pre-operation state outside the target machine.
- Confirm that the authenticated package and the protected `C:\FRAMETIME_CFG`
  root are available, and do not modify either one.

## Workflow-owned evidence

The protected root can contain workflow state and progress, captured backups,
audit and pending irreversible-operation records, driver transaction evidence,
benchmark history, and selected runtime generations. Native persistence
validates the fixed root, uses locks and protected writes, and reads critical
state back after mutation.

Benchmark history writes precede state and progress; progress stays the final
completion marker. An exact retry can repair a history-only or state-only crash
prefix and reuses its timestamp, receipt identity, and versioned run evidence. A
conflicting capture or a changed run order fails closed. Legacy aggregate-only
prefixes keep the same exact-retry behavior, but their missing per-run evidence
cannot authorize a new nonzero cap.

Supported restore paths revalidate target identity before restoring captured
registry, configuration, network, or other owned entries. The recovery command
refuses incoherent state instead of applying a best-effort reconstruction. From
an authenticated package, inspect `verify` and `backup-summary` before deciding
whether `restore --yes` applies.

## Incomplete reboot phases

A pending handoff must be executed only by the selected authenticated runtime
through its guarded phase path. Do not copy in a new executable, edit selector
or transaction JSON, replay a Run/RunOnce command from a source checkout, or
delete evidence to force progress. A failed check is evidence that normal
automatic continuation is unsafe.

Phase 1 can arm Safe Boot and a Phase 2 handoff before final phase completion.
Phase 2 clears Safe Boot only after the runtime, transaction stage, and handoff
verify. If the selected runtime cannot start, or the evidence is damaged, the
machine can require recovery outside this application.

The repository does not provide an unauthenticated break-glass status tool, a
WinRE helper, or a tested generic command sequence for clearing an incomplete
boot transaction by hand. Use an independently prepared, environment-specific
Windows recovery procedure and preserve `C:\FRAMETIME_CFG` for diagnosis. Do not
improvise registry, BCD, driver, or filesystem changes from these source docs.

## Limits

Driver removal or installation, AppX changes, external application behavior,
files the workflow did not capture, a failed boot, and a Windows operation
interrupted inside a native mutation may all need manual recovery. After
recovery, retain sanitized package, state, stage, error, and restore evidence;
never publish machine identifiers, personal paths, credentials, private logs, or
signing material.
