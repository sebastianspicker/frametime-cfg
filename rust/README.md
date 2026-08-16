# frametime.cfg Rust rewrite

This directory is a self-contained Rust implementation of the frametime.cfg
workflow. It does not import or execute the repository's PowerShell sources.
The browser demo remains a separate documentation surface.

## Requirements

- Rust 1.96.0 for source builds
- x64 Windows 10 or 11 for live operations and the GUI
- MSVC target `x86_64-pc-windows-msvc`

Build the portable executables:

```text
cargo build --release --target x86_64-pc-windows-msvc
scripts\package.cmd /unsigned
```

## Portable package lanes

The package layout has exactly 27 payload files listed in
[`package-layout.txt`](package-layout.txt). Every package also carries an
in-package `package.manifest.json` containing hashes for exactly those 27
payloads. That manifest is authentication metadata; it is not the external ZIP
checksum or transport manifest.

Development and pull-request CI must use the explicit unsigned lane:

```text
scripts\package.cmd /unsigned
scripts\package.cmd /verify /unsigned
```

This lane checks the fixed payload inventory, the in-package manifest, ZIP
structure, and the external ZIP checksum/transport manifest. It intentionally
does not contain `package.cat`, does not sign either executable, and does not
claim an authenticated or releasable package. Package builds use the default
feature set; `--all-features` is reserved for source validation because it
includes qualification-only mutation code. Package authentication itself also
rejects a binary compiled with that qualification feature, so a mistakenly
signed all-feature build cannot mint live package authority.

An authenticated release uses a separately provisioned Windows signing host.
Before the `cargo build` that produces the package executables, set these
explicit inputs without adding their values to this repository:

```text
FRAMETIME_PUBLISHER_SPKI_SHA256=<one or two semicolon-separated SHA-256 SPKI pins>
FRAMETIME_SIGNTOOL_PATH=<full path to signtool.exe>
FRAMETIME_MAKECAT_PATH=<full path to makecat.exe>
FRAMETIME_SIGNING_CERT_SHA1=<40-hex code-signing certificate thumbprint>
FRAMETIME_SIGNING_TIMESTAMP_URL=<RFC 3161 timestamp URL>
```

Use those inputs for the build and release assembly:

```text
cargo build --release --target x86_64-pc-windows-msvc
scripts\package.cmd /release
```

The release lane fails closed when any input or tool path is absent. It signs
the CLI and GUI before generating `package.manifest.json`, creates a MakeCat
catalog over that manifest and all 27 payloads, signs `package.cat`, then
verifies both direct executable/catalog signatures and catalog membership for
the manifest and every payload. It performs no PATH lookup for signing tools.
The external `*.zip.sha256` and `*.transport.json` describe ZIP transport only;
the runtime trusts only the in-package manifest and signed catalog plus the
publisher SPKI pin compiled into the executables.

An independent verification host uses the repository verifier, the existing
`dist` package/ZIP metadata, and an explicit `FRAMETIME_SIGNTOOL_PATH`; it does
not need MakeCat, the signing certificate selector, or the timestamp service.
The executable carries the publisher pin used by the authentication smoke:

```text
scripts\package.cmd /verify /release
```

Run the strict preview directly without elevation:

```text
frametime.exe dry-run all
```

The preview uses the same 54-step catalog as live execution, performs no
backup, state, progress, log, runtime, registry, service, task, driver, network,
or file writes, and prints the legacy completion markers. It exits nonzero
when any native action is still unsupported, so a complete-looking preview
cannot mask a missing live adapter. `frametime.exe` without arguments opens
the terminal menu. Launch `frametime-gui.exe` directly for the desktop surface.
The release contains no executable script shim: only the two directly signed
PE entry points run before package authentication.

Configuration is data-only [`frametime.toml`](frametime.toml). Invalid bounds
are rejected; configuration is never executed as code.

## Distribution license material

The portable package contains its third-party Rust dependency notice and the
Apache-2.0 and MIT license texts under
[`licenses/`](licenses/). When `Cargo.lock` changes, regenerate and verify the
component list from the target-selected normal graph before packaging:

```text
cargo tree --workspace --target x86_64-pc-windows-msvc --edges normal,no-proc-macro
```

## Current implementation boundary

The native source now contains the complete catalog, configuration and asset
contracts, typed inspection and mutation adapters, capture-before-apply backup
and audit schemas, recovery dispatch, protected runtime publication, reboot
coordinator, cleanup modes, benchmark receipts, and CLI/Win32 GUI surfaces.
Irreversible AppX and driver operations use explicit audit records rather than
claiming lossless recovery. P1:2 and P1:9 remain truthful advisories because the
repository has no authoritative API for the requested firmware facts. P1:3 is
implemented but its destructive path is build-gated until the Windows
reparse/sharing/disposition matrix is qualified.

An authenticated release package binds the exact inventory, manifest, catalog,
publisher pin, current process identity, and retained payload handles. Only
that capability enables portable CLI mutations, GUI CLI launch, GUI elevation,
and in-process GUI mutation. Phase 1 publishes an independently protected
18-file runtime; Phase 2, Phase 3, final benchmark, and reboot recovery accept
only that selected runtime identity. Unsigned development packages remain
useful for structural and read-only checks but cannot mint live authority.

Source completion is not release equivalence. MakeCat/WinTrust, UAC, ACL and
handle semantics, WMI/CIM providers, SetupAPI, NVAPI, AppX, driver operations,
network changes, reboot interruption, recovery retries, and GUI accessibility
still require the documented Windows VM and hardware acceptance campaign.

## Workspace

- `frametime-core`: catalog, policy, state, backup, engine, runtime manifests,
  migration, and atomic persistence.
- `frametime-driver`: exact device/package/signature evidence and deterministic
  read-only driver plans adapted from the safe Driver Foundry domain boundary.
- `frametime-hardware`: versioned, platform-neutral Northclock-derived
  diagnostic contracts.
- `frametime-hardware-windows`: native CPU, DXGI, WHEA, system, and ETW
  diagnostic adapters with bounded requests and no command subprocesses.
- `frametime-windows`: Windows inspection and mutation adapters plus strict
  planner backend.
- `frametime-cli`: terminal menu and command interface.
- `frametime-gui`: native Win32 shell for the seven task areas.

Operational, integration, and recovery boundaries are documented in
[`docs/operations.md`](docs/operations.md),
[`docs/integrations.md`](docs/integrations.md), and
[`docs/recovery.md`](docs/recovery.md). Current parity evidence is tracked in
[`docs/compatibility-ledger.md`](docs/compatibility-ledger.md).
