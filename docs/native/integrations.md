# Windows integrations

Windows functionality lives in `frametime-windows` and is invoked through
`frametime-app`. The adapter uses typed Windows interfaces and protected
persistence rather than accepting arbitrary shell text or executable paths.

## Boundaries

- Registry, BCD, services, scheduled tasks, WMI, SetupAPI, IP Helper, DXGI, and
  filesystem actions validate target identity before apply, verify, and restore.
- Reboot work uses exact Run or RunOnce values, a selected authenticated
  runtime, and user-binding evidence.
- NVIDIA acquisition accepts only the compiled download host and validated
  server-relative input, uses the default WinHTTP proxy, refuses redirects,
  requires HTTP 200, bounds the response size, retains the acquired artifact
  under the protected root, validates hash and Windows trust, and uses a fixed
  argument vector. The current implementation does not configure explicit
  connect/send/receive timeouts or in-download cancellation.
- NVAPI, driver, network, and hardware observations reject ambiguous or
  unsupported identity rather than guessing.

## Qualification

The code builds and passes contract tests on a host, but live integration needs
Windows qualification. UAC, Safe Mode, WinTrust, SetupAPI, registry permissions,
WMI, NVAPI, proxy and offline behavior, stalled or interrupted downloads,
network-driver behavior, filesystem race resistance, driver installation, and
device effectiveness all need disposable Windows VMs and representative
hardware.

The adapter reports unsupported or unavailable evidence, and never promotes it
to success.
