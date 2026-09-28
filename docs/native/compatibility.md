# Compatibility and qualification

No source-only check qualifies a release. This table records what can be checked
on a host, and what still needs a Windows VM or hardware.

| Surface | Local evidence | Windows qualification still required |
| --- | --- | --- |
| Five-crate architecture and policy | workspace metadata, formatting, Clippy, and host builds | N/A |
| Strict preview | `cargo run -p frametime-cli --locked -- dry-run all` | preview is not live execution |
| Package authority | source inspection and host compilation of the parser and policy | signing, WinTrust, catalog, ACL, retained-handle, PE-role, package-root, and on-disk layout evidence |
| Protected work root | fixed-path and path-validation logic visible in source | ACL, race, interruption, and recovery evidence |
| Three reboot phases | typed transition and handoff logic visible in source | UAC, Safe Mode, Run or RunOnce, same-user, reboot interruption |
| Windows adapters | typed contract and target checks | registry, BCD, WMI, SetupAPI, network, NVAPI, filesystem, driver, and hardware behavior |
| CLI and GUI | host build and Windows-target compilation | x64 Windows execution, accessibility, scaling, elevation, and lifecycle |

Record the host, Windows version, architecture, privilege level, test
conditions, result, and recovery evidence for every live validation run.
