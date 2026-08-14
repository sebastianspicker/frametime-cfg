# Pull request

## Summary

Describe the user-visible change and its capability state.

## Verification

- [ ] Formatting, linting, and applicable tests pass
- [ ] CLI contract tests pass when CLI behavior changed
- [ ] Isolated driver protocol checks pass when driver-facing code changed
- [ ] Hardware-dependent behavior is described separately from CI or mock
      evidence

## Safety and documentation

- [ ] Read-only default and explicit-write boundaries are preserved
- [ ] No machine configuration, secrets, certificates, proprietary SDKs, or
      vendor binaries are included
- [ ] Relevant documentation is updated
