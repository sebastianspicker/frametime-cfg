# Backup and restore

The backup module is implemented in `helpers/backup-restore.ps1`. It records
supported rollback data in `C:\FRAMETIME_CFG\backup.json`. This file does not
cover every operation in the three-phase workflow.

## Storage and persistence

Registry and BCD write wrappers capture their inputs automatically. Services,
power plans, scheduled tasks, NIC properties, QoS and URO, pagefile settings,
DNS, and NVIDIA DRS use explicit `Backup-*` calls. The restore dispatcher also
retains compatibility with Defender exclusion entries written by older state.

The shared registry and boot-configuration wrappers write and verify their
restore records before mutation. Selected service groups, power-plan
activation, scheduled-task changes, and NVIDIA DRS changes use the same
pre-mutation persistence rule. Other entries remain buffered until the step
boundary. `Complete-Step` flushes the buffer before saving progress, and a flush
failure prevents the progress record from being saved.

`backup.lock` prevents concurrent optimization or restore processes from
writing the file. JSON writes are atomic and the file ACL is hardened. If
`backup.json` cannot be parsed, the module first copies and hash-verifies it as
`backup.corrupt.<timestamp>.json`; it resets the active file only after that
preservation succeeds.

Within a step, registry, service, scheduled-task, BCD, power-plan, NIC, and DNS
identities are deduplicated so a re-run retains the first captured value. Other
entry types may have more than one record.

## Implemented entry types

The module writes and restores eleven entry types:

| Type | Captured state | Restore behavior and boundary |
|---|---|---|
| `registry` | Path, value name, original value and type, and whether the value existed | Restores the typed value or removes the value if it was absent. It does not remove a now-empty key. Paths and names must pass the restore allowlist. |
| `service` | Service name, start mode, delayed-start flag, and running status | Restores the supported start mode and starts a service that was previously running. It does not recreate a missing service or explicitly stop one that was previously stopped. |
| `bootconfig` | Managed BCD element, original value, and whether it existed | Restores or deletes the element through `bcdedit`. Only the managed key and value combinations accepted by the restore allowlist are used. |
| `powerplan` | Original active plan GUID and name, plus recorded suite-owned plan GUIDs | Activates the original plan and deletes only validated suite-owned plans. Restore fails if the original plan no longer exists. |
| `scheduledtask` | Task path and name, prior existence, enabled state, and an optional suite script path | Restores enabled state for an existing task, or removes a suite-created task and trusted suite script. It does not serialize or recreate a task definition. |
| `nic_adapter` | Adapter name and description, property name and kind, and original value | Restores the advanced property only when the current adapter identity matches. Properties that were not exposed and captured cannot be restored. |
| `qos_uro` | Supplied QoS policy names and the observed URO state | Removes the recorded policies and restores URO when a usable state was captured. Policy definitions are not serialized. The current caller records only suite-named policies that existed before replacement, so this entry is not a complete QoS rollback. |
| `defender` | Legacy exclusion paths and process names recorded by an older workflow | Removes those exclusions. Current alpha steps do not add Defender exclusions. The old entry does not record whether an exclusion already existed, so it cannot preserve that distinction during restore. |
| `pagefile` | Automatic-management flag, pagefile path, and original initial and maximum sizes | Uses CIM to restore the captured mode and size. A reboot is required. If automation fails, the entry is retained and manual instructions are shown. Other-drive pagefiles are not changed or captured by this step. |
| `dns` | Adapter name, interface index, and original IPv4 DNS server list | Resolves the current adapter by name, uses its current index, and restores the saved servers or DHCP. Restore stops if the adapter name no longer resolves. |
| `drs` | NVIDIA profile identity, whether the suite created it, and prior DWORD values for managed settings | Deletes a suite-created profile or restores prior values in an existing profile. A setting that did not previously exist is left in place because the implementation has no delete-setting operation. NVIDIA DRS must be available when restore runs. |

Restore treats `backup.json` as untrusted input. Registry, service, BCD,
scheduled-task, script-path, and power-plan identities are validated before
commands run. Registry recovery accepts exact path and value-name pairs, with
narrow patterns only for validated CS2 paths, adapter instances, display-class
instances, and device interrupt subkeys. Unknown entry types are rejected.

Successful entries are removed from `backup.json`. Failed entries and partial
pagefile restores remain available for another attempt.

Restore All processes individual entries in reverse capture order. When two
steps changed the same target, the later mutation is undone before the earlier
mutation. Step-specific restore also processes that step's entries in reverse
capture order.

## Operations outside `backup.json`

The following operations use separate recovery or have no automatic rollback:

- Installed and provisioned AppX package removals are not recorded. Recovery
  depends on Windows package availability and manual reinstallation.
- GPU driver packages removed in Phase 2 are not copied. Phase 3 installs the
  selected replacement driver; this is not restoration of the previous package.
- An existing `autoexec.cfg` receives `exec optimization.cfg`; if the file did
  not exist, the suite creates a small stub. Neither state is recorded in
  `backup.json`. Remove the line, or remove the created stub after confirming it
  contains no user changes.
- An existing `optimization.cfg` is copied once to `optimization.cfg.bak` before
  overwrite. That copy is not restored by the Recovery workflow. A newly created
  file has no prior copy.
- The GUI copies the first existing `video.txt` to `video.txt.bak` before a
  write. Later writes preserve that first backup. Recovery does not restore it.
- Firmware, BIOS, vendor control-panel guidance, and other manual actions are
  outside the backup system.

The partial cases documented in the entry table also apply. In particular,
QoS policy definitions, pre-existing Defender exclusion identity, newly added
DRS settings, service stopped state, and scheduled-task definitions are not
fully reconstructed.

If `backup.json` and any separate `.bak` files are unavailable, the suite cannot
infer the machine's original settings. Use the recorded values or the relevant
Windows configuration interface instead of assuming a generic default.

## Starting a restore

`START.bat` option `[7] Restore / Rollback` groups records by step and offers a
single-step or all-recorded restore. The GUI Recovery task presents the same
step groups and enables restore after a row is selected.

Both paths operate on `C:\FRAMETIME_CFG\backup.json`. Restore commands can fail
when hardware, drivers, services, plans, or adapter identities have changed
since capture; failed records remain in the file for review or retry.

## Dry-run behavior

Full DRY-RUN renders planned operations but skips backup initialization,
capture, locking, and flushes. Existing backup, state, progress, and log files
remain unchanged, and no rollback artifact is created. See
[Full DRY-RUN](dry-run.md) for the process contract and supported launch forms.

## Maintainer verification

Run focused backup tests through the repository wrapper:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-LocalTests.ps1 `
    -Path .\tests\helpers\backup-restore.Tests.ps1 `
          .\tests\helpers\backup-dryrun.Tests.ps1 `
          .\tests\helpers\backup-restore-safety.Tests.ps1 `
          .\tests\Optimize-GameConfig-rollback-safety.Tests.ps1 `
          .\tests\integration\backup-restore-roundtrip.Tests.ps1 `
          .\tests\integration\backup-restore-entrypoints.Tests.ps1
```

Run the full wrapper after a change that affects shared write wrappers, progress
persistence, or an optimization step:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-LocalTests.ps1
```
