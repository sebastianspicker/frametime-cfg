# Contributing to Driver Foundry

> **Archived and unsupported.** Driver Foundry has no active contributor
> workflow. The workspace remains in place for source, history, Cargo manifests
> and lockfiles, and its [security policy](SECURITY.md).

The validation and boundary notes below are historical reference. Do not treat
the preserved workspace as an active product or packaging surface.

## Historical validation commands

Run from `tools/driver-foundry`:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo build --release --workspace --all-features --locked
cargo audit --deny warnings
```

`cargo-audit` is an external CI prerequisite. The release build compiles the
workspace; it does not qualify live Windows mutation. Use `DFOUNDRY_DATA_DIR`
only when a repeatable local check needs an explicit versioned data root.

## Historical safety boundaries

- Keep dry-run planning as the supported cleanup path. Live cleanup must stay
  blocked until packaged cleanup catalogs have independent authentication.
- Keep force-install, live registry application, embedded-helper execution, and
  non-ZIP helper paths blocked until a platform signer verifier and
  authenticated signer/helper policy exist.
- BCD mutation and host restart/shutdown are unavailable. Do not add an
  environment-variable bypass or describe planned actions as executed ones.
- Keep HTTPS acquisition bounded, redirect-free, credential-free, and separate
  from launch authorization. ZIP extraction must reject traversal, duplicate or
  case-colliding entries, and resource-limit violations.
- Do not commit proprietary helper binaries, driver packages, private keys,
  certificates, or closed-product dumps. Only the README is tracked below
  `data/embedded/`.
- Keep cohesive Rust source files at or below 600 physical lines when splitting
  would not weaken a domain boundary.

Report vulnerabilities through [SECURITY.md](SECURITY.md). Do not disclose
private keys, certificates, proprietary binaries, or device dumps.
