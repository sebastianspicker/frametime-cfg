# Contributing

> **Archived and unsupported.** Northclock has no active contributor workflow.
> The workspace remains in place for source, history, Cargo manifests and
> lockfiles, and the repository
> [security-reporting route](../../.github/SECURITY.md).

## Historical maintenance notes

The commands in [development](docs/development.md) are historical reference, not
requested pull-request checks. Do not treat the preserved workspace as an active
product or packaging surface.

## Scope and safety

- Keep `northclock-core` independent of physical hardware so user-mode tests can
  use mocks.
- Keep Windows integration in `northclock-platform-windows`.
- Keep CLI and GUI behavior aligned through the shared application layer.
- Preserve the read-only default. Never present an untested request as a
  physical write.
- Do not commit local configuration, logs, device dumps, keys, certificates,
  proprietary SDKs, or vendor binaries.

## Security reports

Report vulnerabilities through the repository
[security policy](../../.github/SECURITY.md). Do not disclose secrets, device
dumps, proprietary SDKs, vendor binaries, keys, or certificates.
