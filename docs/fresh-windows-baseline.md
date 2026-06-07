# Fresh Windows Baseline

Use this when building or reinstalling a dedicated CS2 Windows machine before
running the optimization suite.

## Recommended Baseline

Use an official Microsoft Windows 11 image. As of 2026-05-26, Microsoft's
download page lists Windows 11 2025 Update / version 25H2 as the current
release and offers Installation Assistant, Media Creation Tool, and x64 ISO
options:

https://www.microsoft.com/en-us/software-download/windows11

Do not use prebuilt debloated ISOs as the default baseline. They make it harder
to reason about servicing, Windows Update behavior, anti-cheat compatibility,
Store/AppX dependencies, driver state, and rollback.

## Install Order

1. Install Windows 11 25H2 from official Microsoft media.
2. During first-run setup, decline optional privacy, advertising, diagnostics,
   suggested-content, and cloud-backup prompts that are not needed.
3. Run Windows Update until no more quality, platform, Defender, or driver
   updates are offered.
4. Open Microsoft Store, update inbox apps, and reboot if Store/App Installer
   or framework packages were updated.
5. Install current chipset, GPU, audio, and NIC drivers from the OEM or hardware
   vendor. Prefer official vendor packages over driver-bundle sites.
6. Confirm Device Manager has no unknown devices and Windows Security is healthy.
7. Install Steam, CS2, benchmark tools, and normal peripherals.
8. Create a restore point or full image backup.
9. Run this suite with the SAFE or RECOMMENDED profile first; use COMPETITIVE
   only after measuring baseline behavior.

## Tool Guidance

External Windows debloat tools are not prerequisites for this suite. Do not stack
WinUtil, AtlasOS, tiny11builder, MicroWin, or similar tools with this suite unless
you have reviewed exactly which AppX packages, services, tasks, policies, and
image components overlap.

For a lean install, prefer the repo-owned Step 13 debloat and Step 14 autostart
cleanup because their behavior is explicit, documented, tested, and scoped to
the suite's gaming-PC assumptions.

## Keep Installed

Do not remove Windows Store, App Installer, framework packages, DirectX/.NET
runtimes, Defender, Windows Update, WinSxS, Edge/WebView runtime, driver store
components, or hardware/OEM control panels unless you have a separate, tested
recovery plan.
