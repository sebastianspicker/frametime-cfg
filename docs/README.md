# Documentation

The root [README](../README.md) is the entry point for newcomers. Each document
below is the focused source of truth for one subject.

## For maintainers and contributors

- [Architecture](architecture.md) — component boundaries, dependency direction,
  runtime flow, and who owns state.
- [Configuration](CONFIGURATION.md) — the exact TOML schema, validation rules,
  and authority precedence.
- [Release packaging](DEPLOYMENT.md) — the unsigned and authenticated Windows
  package lanes.
- [Package security](package-security.md) — the trust and retained-identity
  model.
- [Contributing](../CONTRIBUTING.md) — development gates and change rules.

## For users and operators

- [Operations](native/operations.md) — commands, live preconditions, and the
  three reboot phases.
- [Recovery](native/recovery.md) — what can be restored, and how to handle an
  incomplete phase.
- [Windows integrations](native/integrations.md) — the native boundaries and
  what still needs Windows qualification.
- [Native GUI](native/gui.md) — the desktop entrypoint and its four-stage FPS
  workflow.
- [NVIDIA DRS settings](native/nvidia-drs-settings.md)
- [NVIDIA driver lifecycle](native/driver-lifecycle.md)
- [Driver qualification](native/driver-qualification.md)
- [Compatibility and qualification](native/compatibility.md) — the evidence
  table.
- [Evidence policy](research/README.md)

## Archived reference workspaces

[Northclock](../tools/northclock/README.md) and
[Driver Foundry](../tools/driver-foundry/README.md) are archived and
unsupported. Their READMEs describe the retained source, the security-reporting
route, and the limits of the preserved work.
