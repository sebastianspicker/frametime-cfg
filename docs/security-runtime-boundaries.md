# Elevated process boundaries

Elevated phases never shell-execute `steam://` or `https://` links. Steam actions
and vendor links are displayed and copied for the operator to open from an
unelevated desktop session.

The retained WPF implementation restricts `Launch-Terminal` to the reviewed
Phase 1 and Safe Mode entry scripts and resolves each to a regular, non-reparse
descendant of the GUI root. The portable WPF entrypoint currently fails closed,
so this is a defense-in-depth contract for a future authenticated release.

The Phase 2 Safe Mode RunOnce value executes its manifest-verified payload using a
fixed PowerShell `-File` command. Normal boot uses the fixed
`PhaseRuntime-ElevationBootstrap.ps1` member of that same exact-file manifest. It
accepts only `PostReboot-Setup.ps1`, verifies the complete generation, then requests
UAC using parameterized, validate-set target and execution-policy arguments rather
than evaluated command text.

Each protected entrypoint also requires the normalized runtime root to match
`C:\FRAMETIME_CFG\runtime-generations\<32-hex>` and rejects reparse points at the
work-root, generations-root, and generation-directory levels before reading its
manifest.

The work root, generation tree, manifest, and payload files must be owned by
Administrators or SYSTEM with protected ACLs. The account that published the
generation is recorded by SID and receives read/execute access only, including
non-inheriting traversal on `C:\FRAMETIME_CFG`; that SID is rechecked before UAC
and again in the elevated phase. Existing state, progress, and backup files are
never repaired and then trusted: their regular-file, reparse, owner, and ACL
properties must already pass validation before any JSON bytes are read.
