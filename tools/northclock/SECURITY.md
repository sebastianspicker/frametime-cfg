# Security policy

> **Archived and unsupported.** This policy remains the security-reporting route
> for the preserved Northclock source, manifests, and lockfiles. The workspace is
> excluded from active CI, Dependabot, issue routing, packaging, and contributor
> workflows.

## Reporting a vulnerability

Report suspected vulnerabilities privately to the maintainers through the
repository's security-reporting channel. Include the affected version or commit,
the Windows version, reproduction steps, the expected and actual behavior, and
whether a driver or vendor component was involved.

Do not include credentials, device serial numbers, private logs, or proprietary
vendor binaries in a public issue.

## Scope

Reports may cover unsafe privilege boundaries, protocol-validation flaws,
arbitrary file or command execution, information disclosure, or supply-chain
concerns.

Tuning and stress activity can itself make a system unstable. That operational
risk is documented behavior, not automatically a security vulnerability.
Hardware-specific physical writes stay unverified unless explicitly qualified on
supported equipment.

## Supported development path

The maintained target is Windows 11 x64. CI covers the user-mode path with mocks
and does not certify a driver package, installation, signing, or hardware
behavior.
