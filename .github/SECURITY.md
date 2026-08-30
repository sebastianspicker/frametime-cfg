# Security policy

## Scope

Reports are welcome for package authentication, publisher-pin validation, catalog and manifest verification, retained-handle and protected-root checks, privilege transitions, reboot handoffs, untrusted input, Windows API boundaries, recovery, and data exposure.

State-changing operations require an authenticated package. The implementation verifies the package manifest and catalog, file identities and hashes, configured publisher pin, current PE role, and retained file handles. It persists runtime state only below `C:\FRAMETIME_CFG` through a protected-directory boundary.

## Reporting

Use GitHub private vulnerability reporting when available. Otherwise open a public issue asking for a private contact channel without technical details. Do not publish exploit code, sensitive logs, machine identifiers, state files, package signatures, or reproduction details before agreement.

Include the affected commit or package version, entrypoint, Windows version and architecture, required privileges, minimized reproduction, observed and expected behavior, and sanitized evidence.

## Boundaries

Source builds and strict previews are not authenticated releases and do not prove live Windows behavior. Windows package-signing, UAC, Safe Mode, driver, registry, network, NVAPI, filesystem, and recovery behavior require dedicated VM or hardware validation.

The project does not upload local state, logs, or diagnostics by default. Do not place secrets in `frametime.toml`, package inputs, logs, or bug reports.
