# Safety

Normal builds cannot authorize hardware writes. The optional
`experimental-hardware-writes` feature only compiles the shared authorization
path. A write still requires all of the following:

- an elevated process;
- `--experimental` and `--apply`;
- the exact acknowledgement
  `NORTHCLOCK-HARDWARE-WRITES-UNVERIFIED`;
- a non-mutating preview;
- a fresh preview that confirms the backend and captured state did not change;
- fixed, named value bounds;
- captured before-state;
- backend readback and validation;
- rollback support.

The GUI uses a session-only experimental toggle and requires the acknowledgement
again for each previewed operation. It does not persist the toggle or
confirmation.

The driver protocol exposes no generic physical-memory, MSR, PCI, SMN, SMU, or
arbitrary-command request. It carries a protocol version, structure size,
sequence, AMD identity, protocol-table version, bounded core index, bounded
Curve Optimizer value, and watchdog timeout. A future driver must add elevation,
rate limiting, watchdog restoration, enforcement of the protocol's exact model
whitelist, and independent IOCTL review before it can be packaged.

ROM and firmware support is read-only. There is no firmware flashing command,
helper launcher, or firmware-write backend.

Windows system-status checks are also read-only. They do not register or delete
tasks, stop services, unload drivers, disable devices, or change VBS settings.
Potential-conflict matches are observations rather than causal diagnoses.

An elevated Northclock process does not launch the separately packaged VRAM
worker and does not enable persistence beneath the interactive user's
`%LOCALAPPDATA%` tree. Those operations fail closed because the current
development package has no authenticated worker-image capability or protected
privileged storage broker. Run the VRAM test and persistent measurement history
from an unelevated session; elevation remains limited to native observations
that explicitly require it. Measurement commands can still return live results
while elevated, but they do not write history; persistent settings, profiles,
and imports report storage as unavailable.

Process affinity is also read-only by default. Preview captures the process and
system masks. Apply and rollback require the experimental write feature and the
same runtime authorization as hardware tuning, followed by native readback.

Automated tests cannot establish that a setting is thermally safe, stable, or
recoverable on a particular machine. Save work and maintain an independent
recovery path before testing any future write build.
