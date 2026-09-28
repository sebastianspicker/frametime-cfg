# Driver lifecycle qualification

Run qualification only on a disposable Windows x64 VM or a representative test
machine with independent recovery media. Host builds and cross-compilation do
not substitute for this evidence.

Use
`scripts\qualify-driver-lifecycle.cmd <vendor-preserved|lab-export>
<packaged-frametime.exe> <unused-output-directory>`. Pass the exact executable
from the package under qualification; the harness never resolves `frametime.exe`
through `PATH`. It records the executable leaf name, byte length, SHA-256,
Windows version, architecture, whether the shell is elevated, the selected
lane, timestamps, and the structured `driver inspect` result. It deliberately
omits the executable's local path, hostname, user name, serial numbers, device
instance paths, and raw logs.

For `vendor-preserved`, attach separately reviewed evidence for the installer
input, GPU model family, old and new driver/package versions, every observed
transaction stage, verification, and the recovery result. For `lab-export`,
attach the export manifest and WDK tool versions. Never run a lab export through
the production prepare, cleanup, install, or recovery commands.

The harness does not modify Secure Boot, BCD, or Windows test-signing policy.
Those settings are outside Frametime's qualification authority.
