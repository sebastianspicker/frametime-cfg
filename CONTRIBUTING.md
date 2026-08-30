# Contributing

Contributions must preserve the native trust, recovery, and Windows-evidence boundaries.

## Development

Use the root workspace and pinned toolchain. Before opening a pull request, run:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -W clippy::too_many_lines -W clippy::cognitive_complexity
cargo test --workspace --all-targets --all-features --locked
cargo run -p frametime-cli -- dry-run all
cargo check --workspace --all-targets --all-features --target x86_64-pc-windows-msvc --locked
```

Run the matching workspace commands inside `tools/northclock` or `tools/driver-foundry` when changing those independent tools.

## Design rules

- Keep dependency direction explicit: Windows depends on domain; app depends on domain and Windows; CLI/GUI depend on app. Direct entrypoint access to domain is limited to presentation value types, and direct Windows access is limited to startup or native UI plumbing.
- Put policies, values, and platform-independent state in `frametime-domain`; keep Windows APIs, handles, registry, process, and filesystem boundaries in `frametime-windows`.
- Route entrypoint behavior through `frametime-app`. CLI and GUI crates should translate presentation input and output, not duplicate workflow rules.
- Treat `assets/` and `frametime.toml` as canonical package inputs. Do not create a second configuration or asset authority.
- Preserve the fixed `C:\FRAMETIME_CFG` protected root and retained-handle checks. State-changing code must require authenticated package authority.
- Capture supported state before mutation, bind recovery to exact identities, reobserve mutable targets, and fail closed on missing evidence.
- Keep source preview non-persistent and do not represent it as Windows qualification.

## Windows evidence

Host checks cannot prove Windows integration. Changes to privilege, package trust, registry, BCD, Safe Mode, services, drivers, NVAPI, networking, filesystem protection, or GUI accessibility need focused Windows VM or hardware evidence. Document what was exercised, the host and Windows version, privileges, input conditions, observed result, and recovery result. Never replace missing live evidence with a claim based on source tests.

## Documentation and package changes

Update the README and relevant `docs/native/` page with behavior changes. Package changes must update [`package-layout.txt`](package-layout.txt) and preserve manifest, catalog, hash, signature, publisher-pin, and retained-identity verification. See [package security](docs/package-security.md).

Keep the product surface native Rust and keep the five application crates in the root workspace.
