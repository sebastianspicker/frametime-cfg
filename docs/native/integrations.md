# Windows integrations

Windows functionality resides in `frametime-windows` and is invoked through `frametime-app`. The adapter uses typed Windows interfaces and protected persistence rather than accepting arbitrary shell text or executable paths.

## Boundaries

- Registry, BCD, services, scheduled tasks, WMI, SetupAPI, IP Helper, DXGI, and filesystem actions validate target identity before apply, verify, and restore.
- Reboot work uses exact Run or RunOnce values, a selected authenticated runtime, and user-binding evidence.
- NVIDIA acquisition accepts only compiled policy and server-relative input, retains the acquired artifact under the protected root, validates hash and Windows trust, and uses a fixed argument vector.
- NVAPI, driver, network, and hardware observations reject ambiguous or unsupported identity rather than guessing.

## Qualification

The code can be built and contract-tested on a host, but live integration needs Windows qualification. In particular, UAC, Safe Mode, WinTrust, SetupAPI, registry permissions, WMI, NVAPI, network-driver behavior, filesystem race resistance, driver installation, and device effectiveness require disposable Windows VMs and representative hardware.

The adapter should report unsupported or unavailable evidence, never promote it to success.
