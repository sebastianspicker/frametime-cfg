# Contributing

## Development

Use a current Rust stable toolchain on Windows 11 x64. Run the checks that
apply to your change before opening a pull request:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p xtask -- hygiene
```

Run the CLI contract tests when changing CLI behavior. Run the isolated driver
workspace checks when changing its protocol or driver-facing source.

## Scope and safety

- Keep `northclock-core` independent of physical hardware so user-mode tests
  can use mocks.
- Keep Windows integration in `northclock-platform-windows`.
- Keep CLI and GUI behavior aligned through the shared application layer.
- Preserve the read-only default. Do not represent an untested request as a
  physical write.
- Do not commit local configuration, logs, device dumps, keys, certificates,
  proprietary SDKs, or vendor binaries.

## Pull requests

Keep changes focused and include tests or an explanation of why tests are not
applicable. Update public documentation when capability states, local
configuration, the CLI contract, or driver protocol change. State any
hardware-dependent validation separately from mock or CI results.

By contributing, you agree that your contribution is licensed under the [MIT License](LICENSE).
