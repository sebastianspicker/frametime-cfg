# Reserved embedded-helper directory

> **Archived and unsupported.** This directory is preserved with the Driver
> Foundry source, history, Cargo manifests, lockfiles, and
> [security policy](../../SECURITY.md). It is not an active helper or packaging
> surface.

Only this README is tracked. Other files below `data/embedded/` are ignored and
must not be committed.

The current application has no supported embedded-helper workflow. CLI
materialization, external archive helpers, 7z/SFX creation, signing helpers, and
helper execution all fail closed because no authenticated release manifest or
signer policy exists. ZIP extraction is implemented in Rust and needs no helper.

Future helper support must define a fixed inventory, a provenance and license
record, cryptographic identity, signer policy, an immutable materialization
path, and focused Windows verification before any executable is accepted. Do not
use PATH-resolved tools, or copy proprietary closed-product dumps, Windows SDK
binaries, certificates, or private keys into this directory.
