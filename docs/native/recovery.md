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

The repository does not provide an unauthenticated break-glass status tool or a
WinRE helper. Outside the alpha escape procedure below, use an independently
prepared, environment-specific Windows recovery procedure and preserve
`C:\FRAMETIME_CFG` for diagnosis. Do not improvise other registry, BCD, driver,
or filesystem changes from these source docs.

## Alpha escape from a stuck reboot handoff

This procedure is for alpha testers only. It has not been qualified on Windows
hardware yet; qualify it on the VM before relying on it. It removes exactly the
three boot-time values the workflow sets and nothing else. Use it only when the
guarded phase command has failed and the machine keeps starting in Safe Mode, or
a handoff keeps failing at every sign-in.

1. Sign in (Safe Mode included) and open an elevated `cmd.exe`.
2. Copy the whole `C:\FRAMETIME_CFG` directory and the failing command's output
   somewhere outside the machine for the bug report. Do not edit its JSON. The
   copy holds account and hardware identifiers; review it before sharing.
3. Remove Safe Boot from the current boot entry, then confirm `safeboot` no
   longer appears:

   ```bat
   bcdedit /deletevalue {current} safeboot
   bcdedit /enum {current}
   ```

   System Configuration (`msconfig`), Boot tab, "Safe boot" unticked, changes
   the same setting.
4. Remove the handoff values if they are still present. "Unable to find the
   specified registry key or value" means it was already gone:

   ```bat
   reg delete "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce" /v "*!FRAMETIME_Phase2" /f
   reg delete "HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run" /v "FRAMETIME_CFG_FRAMETIME_Phase3" /f
   ```

   Run the `HKCU` line as the user who started `optimize`; an elevated prompt
   of that same user is fine. From another account it targets that account's
   own key, so "Unable to find" there proves nothing about the initiating user.
5. Restart normally. From the authenticated package, run `verify` and
   `backup-summary`, then `restore --yes` if they show captured entries. The
   active reboot transaction may make these commands refuse; that refusal is
   expected evidence, not something to work around.

After this procedure the workflow state still records the unfinished
transaction, so `optimize` refuses to start again. Starting over means moving
the live `C:\FRAMETIME_CFG` directory aside. That discards its captured backups
for `restore`, and a fresh `optimize` would record the already-changed settings
as the new originals, so a later `restore` could not return to the state before
the first run. Do it only after step 5, after the bug report has the copy, and
preferably after a successful `restore --yes`.

## Limits

Driver removal or installation, AppX changes, external application behavior,
files the workflow did not capture, a failed boot, and a Windows operation
interrupted inside a native mutation may all need manual recovery. After
recovery, retain sanitized package, state, stage, error, and restore evidence;
never publish machine identifiers, personal paths, credentials, private logs, or
signing material.
