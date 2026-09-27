# Package security

Native mutation requires an authenticated release package. A source build can
run the strict preview and the test suite, but it cannot become package
authority just by being compiled locally.

The exact unsigned and authenticated assembly commands are in
[release packaging](DEPLOYMENT.md).

## Authentication

The source contract requires the current PE to be one of the two package roles.
Its host-tested parser accepts only the exact compiled payload layout, one or two
distinct SHA-256 publisher pins, normalized relative paths, typed sizes, and
SHA-256 hashes. Unknown, missing, duplicate, case-colliding, malformed, and
oversized input fails closed. The Windows adapter then defines package-root
eligibility, retained-identity and hash validation, catalog membership through
Windows trust, and publisher-pin enforcement. The executable and payload
handles stay retained for the lifetime of the capability.

Authentication fails closed for an unconfigured pin, an unsigned or invalid
catalog, a duplicate or missing file identity, a changed payload, an incorrect
executable role, or a package rooted outside the allowed local boundary. The
signer, catalog, root, ACL, identity, and retained-handle portions require live
Windows qualification.

## Runtime and state

The fixed `C:\FRAMETIME_CFG` root has its own trusted-directory boundary.
Runtime publication copies a verified subset of the authenticated package into a
new protected generation, hashes source and destination through retained
handles, and selects it atomically. Reboot stages use that selected runtime,
never an arbitrary caller path.

State-changing commands authenticate before elevation or mutation. The runtime
and handoff records bind phase actions to verified package evidence, the
selected generation, and the required same-user checks.

## Maintainer checks

- Keep [`package-layout.txt`](../package-layout.txt), package generation,
  manifest, and catalog inputs aligned.
- Treat publisher-pin changes as security-sensitive release changes.
- Run the host parser tests for malformed, oversized, missing, extra, duplicate,
  case-colliding, path-escape, invalid-size, and invalid-hash manifests, plus
  publisher-pin parsing.
- Qualify missing files, identity change, catalog failure, wrong entrypoint
  role, and all signer or retained-handle behavior on Windows.
- Qualify package signing, WinTrust, protected ACLs, retained handles, UAC, and
  reboot handoffs on Windows before release.

No source-only or non-Windows check proves the Windows package boundary.

The NVIDIA production builder adds a separate authenticated-installer boundary:
bounded in-process SFX extraction, a byte manifest for the prepared package, and
an exact signer and retained-identity check immediately before fixed-argument
launch. The test-signed lab builder is export-only and cannot mint production
package or transaction authority. See the
[driver lifecycle](native/driver-lifecycle.md).
