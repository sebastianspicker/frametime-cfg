# Security

## What Driver Foundry does

Driver Foundry can remove display and audio drivers, edit the registry, stop services, and optionally launch vendor installers. Those capabilities are intentional and can damage a system if used carelessly.

## Safe defaults and opt-in controls

- Cleanup is plan/journal only until `--execute` (or its `--live` alias) is supplied.
- Installation filters and dry-runs by default; it does not launch `setup.exe` until force-install is explicitly requested.
- Safe Mode BCD edits stay disabled unless `DFOUNDRY_ALLOW_BCDEDIT=1` is set.
- Live OEM-driver deletion remains disabled unless `DFOUNDRY_UNINSTALL_DELETE=1` is set.
- Non-interactive environments can suppress a UAC relaunch with `DFOUNDRY_NO_UAC_RELAUNCH=1`; this does not grant elevation.
- `DFOUNDRY_DATA_DIR` selects an alternate data directory. Treat its catalogs and embedded helpers as trusted input.

Power restart and shutdown flags are journal-only.

## Reporting issues

If you find a vulnerability in the open-source Driver Foundry code, use a private report if the host supports it. Otherwise, open an issue without publishing a working exploit.

Include the Windows build, Driver Foundry version or commit, exact `dfoundry` command, whether the process was elevated, and whether a live-mutation flag was used.

## Scope and non-affiliations

`archive/legacy/` is preserved historical material, excluded from runtime and CI. It is not a supported execution surface.

Driver Foundry is not affiliated with NVIDIA, AMD, Intel, Wagnardsoft, or TechPowerUp. Vendor catalog text may originate from Display Driver Uninstaller community settings; this attribution does not imply endorsement or affiliation.
