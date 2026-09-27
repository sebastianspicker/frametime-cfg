# Safety

> **Archived and unsupported.** This is a historical safety record for the
> preserved source. Northclock is excluded from active CI, Dependabot, issue
> routing, packaging, and contributor workflows. Report vulnerabilities through
> the repository [security policy](../../../.github/SECURITY.md).

Normal builds cannot authorize hardware writes. The optional
`experimental-hardware-writes` feature only compiles the shared authorization
path. A write still requires all of the following:

- an elevated process;
- `--experimental` and `--apply`;
- the exact acknowledgement `NORTHCLOCK-HARDWARE-WRITES-UNVERIFIED`;
- a non-mutating preview;
- a fresh preview confirming the backend and captured state did not change;
- fixed, named value bounds;
- captured before-state;
- backend readback and validation;
- rollback support.

The GUI uses a session-only experimental toggle and requires the acknowledgement
again for each previewed operation. It persists neither the toggle nor the
confirmation.

The driver protocol exposes no generic physical-memory, MSR, PCI, SMN, SMU, or
arbitrary-command request. It carries a protocol version, structure size,
sequence, AMD identity, protocol-table version, bounded core index, bounded Curve
Optimizer value, and watchdog timeout. A future driver must add elevation, rate
limiting, watchdog restoration, enforcement of the protocol's exact model
whitelist, and independent IOCTL review before it can be packaged.

ROM and firmware support is read-only. There is no firmware flashing command,
helper launcher, or firmware-write backend.

Windows system-status checks are read-only too. They do not register or delete
tasks, stop services, unload drivers, disable devices, or change VBS settings.
Potential-conflict matches are observations, not causal diagnoses.

The normal application never launches the VRAM worker, at any privilege level:
the repository has no authenticated worker-image capability or supported package
path. The direct worker contains the D3D12 implementation, but it is not an
operator-facing substitute for the missing parent boundary. Separately, an
elevated Northclock process does not persist beneath the interactive user's
`%LOCALAPPDATA%` tree because there is no protected privileged storage broker.
Other measurement commands can still return live results while elevated, but
they do not write history; persistent settings, profiles, and imports report
storage as unavailable.

Process affinity is read-only by default. Preview captures the process and system
masks. Apply and rollback require the experimental write feature and the same
runtime authorization as hardware tuning, followed by native readback.

Automated tests cannot establish that a setting is thermally safe, stable, or
recoverable on a particular machine. Save your work and keep an independent
recovery path before testing any future write build.
