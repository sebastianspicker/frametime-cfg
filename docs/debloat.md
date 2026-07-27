# Debloat and Telemetry Deep Dive

> Covers Phase 1 Step 13 (debloat via `helpers/debloat.ps1`) and Step 14 (autostart cleanup in `Optimize-Hardware.ps1`).

"Debloat" is an imprecise term. In this repository it means removing an explicit
AppX package allowlist, disabling two services and selected scheduled tasks, and
setting two Windows CloudContent policy values. These actions can remove apps a
user relies on or conflict with managed-device policy. Review the inventory
before applying them. Autostart cleanup is handled separately by Step 14.

For a fresh machine, start from an official Microsoft Windows image supported by
the repository and apply the suite only after Windows Update, Microsoft Store
updates, and chipset, GPU, and NIC drivers are installed. Do not use a modified
image unless you have reviewed its component and policy changes.
See [Fresh Windows Baseline](fresh-windows-baseline.md).

---

## What Gets Removed

Step 13 prints a preflight inventory before it changes anything. The inventory
lists matched installed AppX packages, provisioned AppX packages, telemetry
services that need disabling, and telemetry scheduled tasks that need disabling.
Strict Full DRY-RUN does not enumerate installed AppX state. It renders the
configured package, service, task, and registry operation plan without removing
or changing anything. Live execution performs the preflight inventory before
asking or applying changes.

### AppX Packages

| Package | App | Functional scope removed |
|---------|-----|--------------------------|
| `Microsoft.BingNews` | Microsoft News | News application |
| `Microsoft.BingWeather` | Weather | Weather application and its location-based features |
| `Microsoft.GetHelp` | Get Help / Virtual Agent | Windows support application |
| `Microsoft.Getstarted` | Tips | Windows tips application |
| `Microsoft.MicrosoftOfficeHub` | Office Hub | Microsoft 365 hub application |
| `Microsoft.MicrosoftSolitaireCollection` | Solitaire | Microsoft Solitaire Collection |
| `Microsoft.People` | People | People and contact application |
| `Microsoft.Todos` | Microsoft To Do | To Do application and account-backed task access |
| `Microsoft.WindowsFeedbackHub` | Feedback Hub | Feedback Hub and its submission interface |
| `Microsoft.YourPhone` | Phone Link (legacy package name) | Phone Link features represented by this package identity |
| `Microsoft.Windows.PhoneLink` | Phone Link package name | Phone Link features represented by this package identity |
| `MicrosoftCorporationII.PhoneLink` | Phone Link package name | Phone Link features represented by this package identity |
| `Microsoft.WindowsMaps` | Maps | Windows Maps application and offline map UI |
| `Microsoft.ZuneMusic` | Groove Music / Media Player | Media application represented by this package identity |
| `Microsoft.ZuneVideo` | Movies & TV | Movies & TV application |
| `Clipchamp.Clipchamp` | Clipchamp | Clipchamp video editor |
| `Microsoft.549981C3F5F10` | Cortana | Cortana application package |
| `Microsoft.MixedReality.Portal` | Mixed Reality Portal | Mixed Reality Portal setup application |
| `Microsoft.SkypeApp` | Skype | Skype application |
| `Microsoft.WindowsCommunicationsApps` | Mail & Calendar | Mail and Calendar applications |
| `Microsoft.OutlookForWindows` | New Outlook for Windows | New Outlook application |
| `Microsoft.Windows.DevHome` | Dev Home | Dev Home application |
| `MSTeams` | Microsoft Teams (new) | Microsoft Teams application |
| `Microsoft.BingSearch` | Bing Search integration | Bing-backed Windows search integration package |
| `Microsoft.PowerAutomateDesktop` | Power Automate Desktop | Power Automate Desktop application |

Installed packages are removed via `Remove-AppxPackage -AllUsers`, affecting all
user accounts on the system, not just the current user. Matching provisioned
packages are also removed via `Remove-AppxProvisionedPackage -Online`, removing
them from the current image's provisioning set for new user profiles. Later
Windows servicing or Store activity can add packages again.

The step does not remove core Windows components, the Microsoft Store, the Xbox
app, DirectX runtime packages, .NET packages, or any Microsoft package outside
the explicit list. Step 37 handles selected Xbox services separately.

### Telemetry Services

| Service | Name | What it does |
|---------|------|-------------|
| `DiagTrack` | Connected User Experiences and Telemetry | Collects Windows usage data and uploads to Microsoft |
| `dmwappushservice` | Device Management WAP Push | Handles WAP push messages for MDM/Intune |

Both are set to Disabled and stopped when present. `dmwappushservice` can be
required for mobile-device management, so this action is not suitable for an
enrolled or organization-managed PC. The repository has no committed trace that
quantifies the performance effect of disabling either service.

### Telemetry Scheduled Tasks

Tasks under these paths are disabled (not deleted - the task scheduler entries remain but won't execute):

- `\Microsoft\Windows\Application Experience\` - compatibility telemetry, program usage reports
- `\Microsoft\Windows\Customer Experience Improvement Program\` - CEIP data collection tasks

Disabling rather than deleting makes these easier to re-enable if needed.

### Consumer Features

```
HKLM:\SOFTWARE\Policies\Microsoft\Windows\CloudContent
    DisableWindowsConsumerFeatures = 1
    DisableSoftLanding             = 1
```

These values request the documented CloudContent policies. Effective behavior
depends on the Windows edition, version, and any organization-managed policy.

### Advertising ID

```
HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo
    Enabled = 0
```

Requests that the Windows advertising identifier be disabled for the current
user. This can affect applications that rely on that identifier. The repository
does not establish a gaming-performance effect.

---

## What Debloat Does NOT Do

- Does not remove the Windows Store or any framework packages
- Does not invoke the Local Group Policy editor or change Windows Update policy;
  it does write the two CloudContent policy registry values listed above
- Does not remove drivers or hardware-related packages
- Does not remove Windows Defender, Windows Update, WinSxS, Edge, WebView2, App Installer, DirectX, or .NET runtimes
- Does not touch any Microsoft package not explicitly in the list
- Does not remove applications installed by the user (Steam, browsers, etc.)
- Does not modify registry settings outside the specific keys listed above

---

## External debloat tools

The suite does not require a third-party debloat tool. Combining tools makes the
origin and recovery path of a change difficult to determine. Review overlapping
AppX, service, task, policy, and image-component changes before using another
tool on the same installation.

Upstream references for comparison:

- Microsoft Windows 11 download: https://www.microsoft.com/en-us/software-download/windows11
- Microsoft Dev Home support note: https://learn.microsoft.com/en-us/previous-versions/windows/dev-home/
- Raphire Win11Debloat app removal: https://github.com/Raphire/Win11Debloat/wiki/App-Removal
- AtlasOS Windows version support: https://docs.atlasos.net/faq/install-faq/windows-version-support/
- tiny11builder README: https://github.com/ntdevlabs/tiny11builder
- WinUtil Win11 Creator: https://winutil.christitus.com/userguide/win11creator/
- MicroWin .NET README: https://github.com/CodingWonders/MicroWin

---

## Evidence boundary

The repository does not include before-and-after process traces or CS2 benchmark
artifacts that isolate the effect of this step. The implemented behavior reduces
the number of selected installed apps, services, and scheduled tasks. Whether
that changes frame performance depends on whether those components were active
and contending for resources on the target system. Treat privacy and system
scope as the primary reasons to accept or reject the step.

---

## Rollback

AppX packages removed by the suite can be reinstalled from the Microsoft Store manually by searching for the app name. The Windows Store itself is not removed.

Service and scheduled-task state should be restored through the suite's Recovery
flow when corresponding backup entries exist. A manual restore must use each
service's recorded original start type and running state. Do not assume the same
start type for `DiagTrack` and `dmwappushservice`.

Telemetry scheduled tasks can be re-enabled via Task Scheduler (taskschd.msc) → navigate to the task path → right-click → Enable.

Autostart entries (Step 14) are backed up by the suite's backup system and can be restored via START.bat → [7] Restore / Rollback → select Step 14.

The CloudContent and AdvertisingInfo values are captured before mutation and
restored through the Recovery flow. Use the values recorded in `backup.json`
when manual recovery is necessary. Do not assume that deleting a value or
setting Advertising ID to `1` reproduces the machine's prior state.
