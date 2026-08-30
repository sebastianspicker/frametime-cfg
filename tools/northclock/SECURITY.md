# Security policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately to the maintainers through
the repository's security-reporting channel. Include affected version or
commit, Windows version, reproduction steps, expected and actual behavior, and
whether a driver or vendor component was involved.

Do not include credentials, device serial numbers, private logs, or proprietary
vendor binaries in a public issue.

## Scope

Security reports may include unsafe privilege boundaries, protocol validation
flaws, arbitrary file or command execution, information disclosure, or
supply-chain concerns.

Tuning and stress activity can itself make a system unstable. That operational
risk is documented behavior, not automatically a security vulnerability.
Hardware-specific physical writes remain unverified unless explicitly qualified
on supported equipment.

## Supported development path

The maintained target is Windows 11 x64. CI covers the user-mode path with
mocks and does not certify a driver package, installation, signing, or hardware
behavior.
