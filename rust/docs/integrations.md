# Native hardware and driver integration

The portable Rust application incorporates bounded capabilities derived from
the adjacent Northclock and Driver Foundry prototypes. Their standalone
executables, graphical shells, workspace locks, build outputs, and runtime
adapters are not part of the Frametime distribution.

## Hardware diagnostics

`frametime-hardware` defines versioned, platform-neutral diagnostic contracts.
`frametime-hardware-windows` implements the current read-only Windows boundary
with native APIs. The CLI exposes it through:

```text
frametime hardware doctor
frametime hardware cpu
frametime hardware gpu
frametime hardware system
frametime hardware whea --max-records 32
frametime hardware frames --duration-ms 10000
```

WHEA record counts and ETW capture durations are bounded before native calls.
The response schema is `frametime.hardware/v1`. Every capability reports
`hardware_verified: false` until a separate Windows hardware campaign records
evidence. Unsupported hosts return a typed unavailable result and perform no
fallback command execution.

The initial integration deliberately omits Northclock's eframe GUI, settings
store, tuning operations, experimental affinity controls, kernel-driver
research, and VRAM child process. Those surfaces do not authorize any workflow
step or mutation.

## Driver planning and transaction

`frametime-driver` is a platform-neutral evidence and planning domain. The CLI
accepts an explicit JSON input and prints a deterministic read-only plan:

```text
frametime driver plan --input driver-evidence.json
```

The input must bind one exact PCI GPU identity, canonical `oem<N>.inf` package
identities, a lower-case SHA-256 payload digest, and valid Authenticode evidence
to the same GPU. The generated plan describes P1:18, P1:19, P2:2, and P3:1
without granting mutation authority. It remains useful for offline review.

The authenticated Windows package also exposes a narrower live preparation
entry point:

```text
frametime driver prepare-nvidia --artifact-id <label> --artifact-file-name <leaf.exe> --server-path <fixed-host-relative-path>
```

The operator controls no URL, digest, signer, vendor, host, or install command.
Those identities come from the compiled NVIDIA policy. Preparation binds one
exact active NVIDIA GPU and its canonical installed OEM packages, downloads to
a create-new protected child, verifies the retained artifact with
WinVerifyTrust, and persists the read-back transaction consumed by P1:19.
P2:2 reobserves the exact package set in Safe Mode and records every fixed-vector
PnPUtil removal result. P3:1 accepts only the retained authorized artifact and
coherent removal evidence. Authorization lasts at most 24 hours and is checked
before both removal and installation. Immediately before launch, P3:1 repeats
whole-chain revocation-aware WinVerifyTrust over the retained handle, requires
the compiled NVIDIA signer policy and acquisition identity, persists that fresh
Authenticode observation, then records the post-install SetupAPI inventory.
None of these steps falls back to a shell, caller-supplied executable, fuzzy
package match, or generic driver-store deletion.

Driver Foundry's PowerShell and command-line mutation adapters, fuzzy package
matching, download catalog, DDU-derived data, language XML, archives, and
embedded executables are excluded. Their provenance and recovery behavior do
not satisfy the trusted-root, capture-before-mutation, exact-identity, and
readback requirements of the Rust workflow.

SetupAPI, WinHTTP, WinVerifyTrust, PnPUtil, installer behavior, Safe Mode, and
the exact target hardware remain Windows acceptance gates. Source-level
transaction tests do not claim those effects occurred on a live machine.
