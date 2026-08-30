# Pull request

## Summary

<!-- What changed and why? -->

## Evidence

<!-- Commands run, Windows VM or hardware evidence, and relevant documents. -->

## Checklist

- [ ] `cargo fmt --all -- --check` passes.
- [ ] Root Clippy and test gates pass.
- [ ] `cargo run -p frametime-cli -- dry-run all` was run when workflow behavior changed.
- [ ] Windows target check was run for root workspace changes.
- [ ] Independent `tools/` workspace gates were run when applicable.
- [ ] Package, protected-root, retained-handle, and publisher-pin invariants remain intact.
- [ ] State-changing changes include focused Windows evidence or explicitly retain the qualification gap.
- [ ] Recovery coverage and documentation were updated when behavior changed.
- [ ] No secrets, local runtime state, personal paths, or private diagnostics are included.
- [ ] The product surface remains native Rust and the five application crates remain in the root workspace.
