# Package security

Native mutation requires an authenticated release package. A source build can run strict preview and tests, but cannot become package authority merely by being compiled locally.

## Authentication

The source contract requires the current PE to be one of the two package roles. Its host-tested parser accepts only the exact compiled payload layout, one or two distinct SHA-256 publisher pins, normalized relative paths, typed sizes, and SHA-256 hashes; unknown, missing, duplicate, case-colliding, malformed, and oversized input fails closed. The Windows adapter source then defines package-root eligibility, retained-identity and hash validation, catalog membership through Windows trust, and publisher-pin enforcement. The executable and payload handles are designed to remain retained for the capability lifetime.

Authentication is specified to fail closed for an unconfigured pin, unsigned or invalid catalog, duplicate or missing file identity, changed payload, incorrect executable role, or a package rooted outside the allowed local boundary. The signer, catalog, root, ACL, identity, and retained-handle portions require live Windows qualification.

## Runtime and state

The fixed `C:\FRAMETIME_CFG` root has a separate trusted-directory boundary. Runtime publication copies a verified subset of the authenticated package into a new protected generation, hashes source and destination through retained handles, and selects it atomically. Reboot stages use that selected runtime, not an arbitrary caller path.

State-changing commands authenticate before elevation or mutation. The runtime and handoff records bind phase actions to verified package evidence, selected generation, and required same-user checks.

## Maintainer checks

- Keep [`package-layout.txt`](../package-layout.txt), package generation, manifest, and catalog inputs aligned.
- Treat publisher pin changes as security-sensitive release changes.
- Run host parser tests for malformed, oversized, missing, extra, duplicate, case-colliding, path-escape, invalid-size, and invalid-hash manifests, plus publisher-pin parsing.
- Qualify missing files, identity change, catalog failure, wrong entrypoint role, and all signer or retained-handle behavior on Windows.
- Qualify package signing, WinTrust, protected ACLs, retained handles, UAC, and reboot handoffs on Windows before release.

No source-only or non-Windows check proves the Windows package boundary.
