# Recovery

Recovery is scoped to state the workflow captured and can bind to the same target identity. It is not a system image or a promise that every external action is reversible.

## Stored evidence

The protected root contains state, progress, backups, audit records, benchmark history, driver transaction evidence, and runtime selection. Native persistence validates the fixed root, uses locks and protected writes, and reads back critical state after mutation.

Supported restore paths revalidate identity before restoring captured registry, configuration, network, or other owned entries. The recovery command refuses incoherent state rather than applying a best-effort reconstruction.

## Manual limits

Driver removal or installation, AppX changes, external application behavior, files not captured by the workflow, a failed boot, and interrupted Windows operations can require manual recovery. Before any reboot, driver, or irreversible cleanup action, ensure independent backups and recovery media are available.

If a phase handoff is pending, use the selected authenticated runtime and its guarded phase path. Do not replace files beneath the work root or replay a handoff from a source checkout.
