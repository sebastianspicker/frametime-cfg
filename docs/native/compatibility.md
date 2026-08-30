# Compatibility and qualification

| Surface | Local evidence | Windows qualification still required |
| --- | --- | --- |
| Five-crate architecture and policy | formatting, Clippy, unit and integration tests | N/A |
| Strict preview | CLI tests and `dry-run` | preview is not live execution |
| Package authority | host parser tests for exact manifest layout, hash and size shape, publisher pins, paths, duplicate keys, and fail-closed rejection | signing, WinTrust, catalog, ACL, retained-handle, PE-role, package-root, and on-disk layout evidence |
| Protected work root | fixed-path, persistence, lock, and path-validation tests | ACL, race, interruption, and recovery evidence |
| Three reboot phases | transition and handoff contract tests | UAC, Safe Mode, Run or RunOnce, same-user, reboot interruption |
| Windows adapters | typed contract and target checks | registry, BCD, WMI, SetupAPI, network, NVAPI, filesystem, driver, and hardware behavior |
| CLI and GUI | host build and focused tests | x64 Windows execution, accessibility, scaling, elevation, and lifecycle |
| Independent tools | each workspace's own checks | their Windows or hardware features |

Source tests alone do not qualify a release. Record the host, Windows version, architecture, privilege level, test conditions, result, and recovery evidence for each live validation run.
