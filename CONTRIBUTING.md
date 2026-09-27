# Contributing

Thanks for helping. Contributions must preserve the native trust, recovery, and
Windows-evidence boundaries.

## Development

Use the root workspace and the pinned toolchain. Before opening a pull request,
run the host gate from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -W clippy::too_many_lines -W clippy::cognitive_complexity
cargo test --workspace --all-targets --all-features --locked
cargo run -p frametime-cli --locked -- dry-run all
cargo check --workspace --all-targets --all-features --target x86_64-pc-windows-msvc --locked
```

The workspace lint policy denies Clippy's full `all` group plus the additional
`too_many_lines`, `cognitive_complexity`, `suspicious_operation_groupings`,
`cast_possible_truncation`, `cast_sign_loss`, and `struct_excessive_bools`
lints. Fix violations with checked conversions, named predicates, smaller
functions, or cohesive value types. An item-level exception is acceptable only
when a serialized or FFI representation cannot be changed, and the exception must
explain that compatibility constraint.

Tests also enforce a 600-physical-line limit for every Rust source file beneath
`crates/` and `repo-checks/`, with no allowlist, enforced by `repo-checks`.
Split files along existing ownership and responsibility boundaries instead of
hiding source from the check.

On Windows, `scripts\verify.cmd` is the authoritative root source gate and also
requires `cargo-audit`. Northclock and Driver Foundry are archived and
unsupported; they are not part of the primary contributor workflow.

## Design rules

- Keep the dependency direction explicit: Windows depends on domain; app depends
  on domain and Windows; CLI/GUI depend on app. Entrypoints may reach domain only
  for presentation value types, and Windows only for startup or native UI
  plumbing.
- Put policy, values, and platform-independent state in `frametime-domain`; keep
  Windows APIs, handles, registry, process, and filesystem boundaries in
  `frametime-windows`.
- Route entrypoint behavior through `frametime-app`. The CLI and GUI translate
  presentation input and output; they do not duplicate workflow rules.
- Treat `assets/` and `frametime.toml` as canonical package inputs. Do not create
  a second configuration or asset authority.
- Preserve the fixed `C:\FRAMETIME_CFG` protected root and retained-handle
  checks. State-changing code must require authenticated package authority.
- Capture supported state before mutation, bind recovery to exact identities,
  reobserve mutable targets, and fail closed on missing evidence.
- Keep source preview non-persistent, and never present it as Windows
  qualification.

## Windows evidence

Host checks cannot prove Windows integration. Changes to privilege, package
trust, registry, BCD, Safe Mode, services, drivers, NVAPI, networking, filesystem
protection, or GUI accessibility need focused Windows VM or hardware evidence.
Document what was exercised, the host and Windows version, privileges, input
conditions, observed result, and recovery result. Never replace missing live
evidence with a claim based on source tests.

## Documentation and package changes

Update the README and the relevant focused document when behavior changes.
Package changes must update [`package-layout.txt`](package-layout.txt) and
preserve manifest, catalog, hash, signature, publisher-pin, and retained-identity
verification. See [package security](docs/package-security.md) and
[deployment](docs/DEPLOYMENT.md).

Refresh [`licenses/THIRD_PARTY_NOTICES.md`](licenses/THIRD_PARTY_NOTICES.md) when
the distributed dependency graph, copied code, assets, SDK-derived material, or
binary inputs change. Its dependency inventory is based on:

```sh
cargo tree --workspace --target x86_64-pc-windows-msvc --edges normal,no-proc-macro
```

Keep the product surface native Rust, and keep the five application crates in the
root workspace.
