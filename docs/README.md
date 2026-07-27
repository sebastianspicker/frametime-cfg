# Documentation

Use the README at the repository root for installation, configuration, common
commands, development setup, testing, deployment, and troubleshooting. The
documents below cover individual subsystems and operational decisions.

## Operation and recovery

- [Architecture](architecture.md): entry points, phase handoffs, runtime
  payloads, state, and helper ownership.
- [Dry-run behavior](dry-run.md): supported preview commands and the
  no-persistence contract.
- [Backup and restore](backup-restore.md): captured state, restore coverage,
  and recovery limits.
- [Desktop interface](gui.md): WPF capabilities, launch behavior, and manual
  validation.
- [Frontend implementation](frontend.md): controller boundaries, XAML
  conventions, and interface tests.
- [Product constraints](product.md): users, operational limits, accessibility
  targets, and non-goals.
- [UI design](ui-design.md): visual tokens and component rules used by the WPF
  interface.

## System configuration

- [Fresh Windows baseline](fresh-windows-baseline.md)
- [Debloat behavior](debloat.md)
- [Windows services](services.md)
- [Windows scheduler](windows-scheduler.md)
- [Power plan](power-plan.md)
- [Process priority](process-priority.md)
- [Storage health](storage-health.md)
- [MSI interrupts and NIC affinity](msi-interrupts.md)
- [NIC latency stack](nic-latency-stack.md)

## Driver, game, and network configuration

- [NVIDIA optimization](nvidia-optimization.md)
- [NVIDIA DRS settings](nvidia-drs-settings.md)
- [Video settings](video-settings.md)
- [CS2 video.txt example](video.txt)
- [Audio settings](audio.md)
- [Network CFG files](network-cfgs.md)
- [Network diagnostics](network-diagnostics.md)

## Evidence and limits

- [Evidence policy](evidence.md): evidence categories and documentation rules.
- [Excluded and disputed settings](debunked.md): settings not applied by the
  toolkit and the reason for each exclusion.
