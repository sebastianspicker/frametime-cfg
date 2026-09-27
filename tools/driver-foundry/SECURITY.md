# Security policy

> **Archived and unsupported.** This policy remains the security-reporting route
> for the preserved Driver Foundry source, history, Cargo manifests, and
> lockfiles. The workspace is excluded from active CI, Dependabot, issue routing,
> packaging, and contributor workflows.

## Current mutation boundary

The archived source can parse and plan sensitive driver, registry, service,
filesystem, AppX, and Safe Mode operations. Current public execution paths do
not authorize those plans:

- `clean --execute` fails until packaged cleanup catalogs can be independently
  authenticated.
- `install --force-install` and live registry application fail until a platform
  signer verifier and authenticated vendor policy exist.
- BCD mutation, restart, and shutdown are unavailable. No
  `DFOUNDRY_ALLOW_BCDEDIT` bypass is supported.
- Embedded-helper materialization/execution, 7z/SFX creation, and signing-helper
  execution are unavailable until authenticated helper manifests exist.
- `DFOUNDRY_UNINSTALL_DELETE=1` gates an inner OEM-delete stage, but the live
  install authorization blocks the pipeline before that stage. Do not treat it
  as a usable public mutation switch.

The supported environment controls are:

- `DFOUNDRY_DATA_DIR` selects the data root used for settings and catalogs; its
  contents remain input that must be validated.
- `DFOUNDRY_NO_UAC_RELAUNCH=1` suppresses a UAC relaunch attempt; it does not
  grant elevation or mutation authority.
- `DFOUNDRY_FORCE_CLI_HELP` and `DFOUNDRY_AUTO_GUI` select startup presentation
  behavior.

Treat every alternate data root, local archive, downloaded file, driver catalog,
and future helper binary as untrusted until the relevant parser and
authentication boundary validates it. HTTPS and an optional caller-supplied
digest do not establish installer launch authority.

## Reporting

Use the repository host's private vulnerability-reporting channel when it is
available. Otherwise open a public issue requesting a private contact channel,
without technical details or exploit material.

Include the affected commit, Windows build, exact `dfoundry` command, input
source, elevation state, requested live flag, observed result, and a minimized
sanitized reproduction. Do not publish credentials, device identifiers, private
logs, proprietary vendor binaries, driver packages, certificates, or keys.

## Non-affiliation

Driver Foundry is not affiliated with NVIDIA, AMD, Intel, Wagnardsoft, or
TechPowerUp. Vendor catalog attribution does not imply endorsement.
