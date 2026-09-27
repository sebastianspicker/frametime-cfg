# NVIDIA driver lifecycle

The root five-crate application owns the supported NVIDIA lifecycle. Archived
Northclock and Driver Foundry code is not loaded, invoked, or used as a mutable
catalog at runtime.

Frametime uses NVIDIA's public NVAPI ABI. It does not import a hidden-setting
database, decryption material, or an arbitrary setting editor.

## Commands

`driver plan --input` remains the path-free evidence planner. The supported
driver commands are:

```text
driver inspect
driver prepare-nvidia --official-url <url> [--preset <preset>] [--select <id>] [--deselect <id>]
driver prepare-nvidia --local-installer <path> [--preset <preset>] [--select <id>] [--deselect <id>]
driver status
driver reconcile-profiles --accept-driver-incompatible-profile-items --yes
driver build-nvidia-lab --output <path> --deep-inf --test-certificate-sha256 <sha256> --acknowledge-unqualified-driver
```

`--official-url` accepts only HTTPS URLs below
`international.download.nvidia.com`. A local installer is copied to the fixed
trusted root and then opened through the same retained-file, hash, and exact
NVIDIA signer checks. Hidden artifact ID, artifact filename, and server-path
options remain only for compatibility; new integrations must use `--official-url`
or `--local-installer`.

The presets are `minimal`, `clean`, `recommended`, `notebook`, `gaming`, and
`full`, with `recommended` as the default. The canonical 35-component catalog is
compiled from [`assets/nvidia-components.v1.json`](../../assets/nvidia-components.v1.json).
Dependency closure is automatic. `Display.Driver` and required dependencies
cannot be deselected. Unknown source directories are retained and reported as
required-unclassified.

## Production package

The NVIDIA SFX payload is extracted in process. Before any output is created,
the reader rejects traversal, absolute paths, reparse attributes, case
collisions, duplicates, excessive paths, entry counts, file sizes, total size,
and decompression ratios. Extraction uses create-new files and removes only its
new destination after failure.

Selected vendor files are copied byte for byte. Frametime generates only
`setup.cfg` and `frametime-package.json`. The manifest records stable relative
paths, byte lengths, and SHA-256 hashes. The complete manifest is verified again
before the transaction becomes durable, and its canonical SHA-256 digest and
component selection are bound into that transaction. The same binding is
checked before removal and again immediately before `setup.exe` is opened
through a retained handle. The installer must carry the exact NVIDIA signer
recorded during acquisition. The launched `setup.exe` digest and fresh
Authenticode result are recorded as the identity actually launched, and it runs
only as `-s -noreboot` from the fixed prepared-package directory.

## Durable transaction

`driver-transaction.json` schema 2 records these stages:

1. `installerAuthenticated`
2. `profileBackupPersisted`
3. `stateCapturePersisted`
4. `safeModeHandoffArmed`
5. `cleanupComplete`
6. `installationComplete`
7. `profilesRestored` or `needsProfileReconciliation`
8. `baselineApplied`
9. `verified`

`recoveryRequired` is reachable from every stage. Each transition is written
atomically and read back before the next operation. A schema-1 preparation with
no capture remains readable; any schema-1 resume boundary is ambiguous because it
lacks the mandatory profile backup and is refused.

Re-running `prepare-nvidia` never replaces files owned by an existing
transaction. At `installerAuthenticated` it verifies the same artifact locator,
effective component selection, package bytes, and manifest digest before completing the
profile backup; at `profileBackupPersisted` it returns the same verified
transaction. A legacy pre-mutation schema-2 record without the new package
digest is upgraded only after its package and recorded selection verify. Once
mutation begins, a replacement preparation is refused. A `verified` transaction
may start a later driver update only after its complete transaction and exact
profile snapshot are atomically copied to the fixed, readback-verified
`driver-previous-transaction.json` archive; the active record is not replaced
until the next package is ready. Profile restoration,
reconciliation, baseline apply, and baseline verification resume from their
last durable stage. The exact CS2 DRS recovery record is persisted and read back
before the baseline is changed.

The separately bounded `driver-profile-backup.json` is referenced by its SHA-256
digest. The public NVAPI DRS implementation enumerates customized profiles,
application bindings, and DWORD, QWORD, Unicode-string, and binary values. Domain
validation enforces 4,096 profiles, 4,096 applications or settings per profile,
65,536 total records, 2,048 UTF-16 units per string, 4 KiB per binary value, and
64 MiB serialized.

Restoration merges captured records without deleting new driver defaults. A
rejected value or conflicting binding persists `needsProfileReconciliation`.
Accepting omissions requires both explicit flags; the omitted keys remain in
transaction evidence. Frametime's existing five-setting CS2 baseline is applied
only after restoration or reconciliation.

## Cleanup and recovery boundary

The mutation boundary requires one exact active NVIDIA GPU, its exact published
packages, an authenticated installable replacement, confirmed Safe Mode, durable
recovery evidence, and an available Windows basic-display fallback. The default
package path removes only captured NVIDIA display packages. The fixed System32
`pnputil.exe` is Microsoft-authenticated and held open while its fixed argument
vector runs, with file identity, length, and digest checked again after exit.
Each captured package includes a bounded SHA-256 identity over the actual INF
and catalog files resolved for its `oemN.inf` by SetupAPI. That identity, the
target GPU binding, provider, version, and INF name are re-observed immediately
before each delete; a legacy capture without the Store digest cannot authorize
removal. A package already removed by an interrupted prior attempt is recorded
as absent, and that outcome is accepted only when the final complete inventory
independently confirms the captured package remains absent. Before any retry
mutation, the complete current target-package records must be an exact subset
of the immutable capture; a reused name, changed Store digest, provider or
version, or new target package fails closed. If every captured package is still
present, removal has not begun and the original live
authorization and 15-minute capture window still apply. Only a proven partial
cleanup can resume later, and it requires and records a new confirming Safe
Mode observation while retaining the original package set. The subsequent
recovery install may outlive the original 24-hour acquisition authorization:
the immutable package digest, component selection, signer, and recovery records
remain mandatory so expiry cannot strand a machine after cleanup has begun.
This makes multi-package cleanup resumable without broadening the original
captured package set.
Application suites, Broadcast, Control Panel, PhysX, HD Audio, monitors, and
driver-search policy are separate typed optional scopes and are preserved when
not selected. There is no raw execute bypass.

Driver-search policy, if selected in a qualified future cleanup catalog, must be
captured and restored to its exact prior state after installation and on
recovery. System Restore is supplementary evidence only; Frametime's
identity-bound transaction remains the rollback authority.

## Lab export

The lab command is a separate export-only call graph. It cannot construct or
advance a driver transaction. It requires a new output directory, all three
explicit acknowledgement inputs, authenticated Microsoft WDK `Inf2Cat` and
`SignTool`, and an explicitly selected certificate already in the current-user
certificate store. Private keys stay external. The result is marked unqualified
and cannot be installed by Frametime. The command never changes BCD,
test-signing, or Secure Boot.

Source tests and cross-compilation are not live driver qualification. Use the
[qualification harness](driver-qualification.md) on a disposable Windows system
and retain sanitized evidence before making release claims.
