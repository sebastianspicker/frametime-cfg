# Third-party notices

The repository does not bundle third-party executables, PowerShell modules, or
NuGet packages. Some implementation comments, identifiers, and configuration
metadata cite external projects. Maintainers must verify the origin of copied
or adapted material and retain the applicable notices and license terms.

## NVIDIA Profile Inspector

Repository: <https://github.com/Orbmu2k/nvidiaProfileInspector>

The NVIDIA DRS implementation and setting metadata refer to NVIDIA Profile
Inspector and `CustomSettingNames.xml`. The upstream project uses the MIT
License. Confirm the derivation of `helpers/nvidia-drs.ps1`,
`helpers/nvidia-profile.ps1`, and the NVIDIA settings documentation before a
release. Preserve the upstream copyright and license notice when required.

## NvAPIWrapper

Repository: <https://github.com/falahati/NvAPIWrapper>

`helpers/nvidia-drs.ps1` cites NvAPIWrapper as a reference implementation. The
upstream project uses LGPL-3.0. This repository does not bundle or load an
NvAPIWrapper binary. Copied or adapted source must comply with the upstream
license.

## osu!

Repository: <https://github.com/ppy/osu>

`helpers/nvidia-drs.ps1` cites the project's NVAPI query-interface pattern. The
upstream project uses the MIT License. Copied or adapted source must retain the
required upstream notice and license terms.

## NVIDIA interfaces and downloaded software

The toolkit calls `nvapi64.dll` installed with the user's NVIDIA driver and
uses NVAPI function identifiers and data layouts. It does not redistribute the
NVAPI DLL, NVIDIA SDK headers, or an NVIDIA driver installer. A live workflow
can download a driver package from NVIDIA after user confirmation and validates
the package before execution.

Ten partially decoded or unidentified settings are excluded from the active
42-setting DRS table. See `helpers/nvidia-profile.ps1` and
`docs/nvidia-drs-settings.md` for the maintained table.

## FPSHeaven references

`helpers/power-plan.ps1`, `helpers/benchmark-history.ps1`, and
`docs/power-plan.md` cite FPSHeaven material. No `.pow` file or benchmark-map
asset is included. Maintainers must confirm that implemented values, names, and
descriptions are independently documented or used with permission.

## Other source references

Source comments also cite `jNizM`, `djdallmann/GamingPCSetup`,
`valleyofdoom/PC-Tuning`, Blur Busters, and `NvApiDriverSettings.h`. Those
comments do not establish whether referenced material is independently
derived, copied, or adapted. Replace informal citations with durable source
links where possible and comply with the applicable license for copied or
adapted material.

This notice records source provenance that still requires maintainer review. It
is not a legal conclusion.
