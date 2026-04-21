# Debloat & Telemetry — Deep Dive

> Covers Phase 1 Step 13 (debloat via `helpers/debloat.ps1`) and Step 14 (autostart cleanup in `Optimize-Hardware.ps1`).

"Debloat" is an imprecise term that covers a range of actions from aggressive (removing core Windows components) to conservative (removing third-party apps pre-installed by OEMs). The suite is conservative: it removes known Microsoft bloatware AppX packages, disables two telemetry services, disables telemetry scheduled tasks, and disables consumer features — nothing that is load-bearing for Windows functionality. Autostart cleanup is handled separately by Step 14.

---

## What Gets Removed

### AppX Packages

| Package | App | Why Removed |
|---------|-----|-------------|
| `Microsoft.BingNews` | Microsoft News | Background network activity; content update polling |
| `Microsoft.BingWeather` | Weather | Background location queries and content updates |
| `Microsoft.GetHelp` | Get Help / Virtual Agent | Background service; no competitive gaming use |
| `Microsoft.Getstarted` | Tips | Background tips service; no competitive gaming use |
| `Microsoft.MicrosoftOfficeHub` | Office Hub | Office upsell app; background telemetry |
| `Microsoft.MicrosoftSolitaireCollection` | Solitaire | Background game telemetry; not relevant to gaming rig |
| `Microsoft.People` | People | Contact sync background service |
| `Microsoft.Todos` | Microsoft To Do | Background sync |
| `Microsoft.WindowsFeedbackHub` | Feedback Hub | Telemetry upload; Windows diagnostic submission |
| `Microsoft.YourPhone` | Phone Link (legacy package name) | Background phone connection daemon |
| `Microsoft.Windows.PhoneLink` | Phone Link (Win11 23H2+ package name) | Same app, renamed package — both are removed to cover all Windows versions |
| `Microsoft.WindowsMaps` | Maps | Background map data updates |
| `Microsoft.ZuneMusic` | Groove Music / Media Player | Superseded by Windows 11's native Media Player |
| `Microsoft.ZuneVideo` | Movies & TV | Background store connectivity |
| `Clipchamp.Clipchamp` | Clipchamp | Video editor; background rendering service |
| `Microsoft.549981C3F5F10` | Cortana | Background voice assistant daemon; always-on audio monitoring |
| `Microsoft.MixedReality.Portal` | Mixed Reality Portal | VR setup app; irrelevant for gaming-only system |
| `Microsoft.SkypeApp` | Skype | Background communication daemon |
| `Microsoft.WindowsCommunicationsApps` | Mail & Calendar | Background email sync |

All packages are removed via `Remove-AppxPackage -AllUsers`, affecting all user accounts on the system, not just the current user.

**What does not get removed:** Core Windows components, Windows Store itself, Xbox app (addressed by Step 37 services separately), DirectX runtime packages, .NET packages, or any Microsoft package not on this explicit list.

### Telemetry Services

| Service | Name | What it does |
|---------|------|-------------|
| `DiagTrack` | Connected User Experiences and Telemetry | Collects Windows usage data and uploads to Microsoft |
| `dmwappushservice` | Device Management WAP Push | Handles WAP push messages for MDM/Intune |

Both are set to Disabled and stopped. `dmwappushservice` is irrelevant on non-enterprise home systems. `DiagTrack` runs as a continuous background daemon; stopping it eliminates its periodic CPU and network wakeups.

### Telemetry Scheduled Tasks

Tasks under these paths are disabled (not deleted — the task scheduler entries remain but won't execute):

- `\Microsoft\Windows\Application Experience\` — compatibility telemetry, program usage reports
- `\Microsoft\Windows\Customer Experience Improvement Program\` — CEIP data collection tasks

Disabling rather than deleting makes these easier to re-enable if needed.

### Consumer Features

```
HKLM:\SOFTWARE\Policies\Microsoft\Windows\CloudContent
    DisableWindowsConsumerFeatures = 1
    DisableSoftLanding             = 1
```

Prevents Windows from automatically installing "suggested apps" (third-party apps like Spotify, Disney+, Candy Crush) via the cloud content mechanism. These installs can happen silently after major Windows updates.

### Advertising ID

```
HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo
    Enabled = 0
```

Disables the Windows advertising identifier. Apps use this ID to correlate user behavior for targeted advertising. No impact on gaming performance; privacy benefit.

---

## What Debloat Does NOT Do

- Does not remove the Windows Store or any framework packages
- Does not modify Group Policy or Windows Update settings (that's Step 15)
- Does not remove drivers or hardware-related packages
- Does not touch any Microsoft package not explicitly in the list
- Does not remove applications installed by the user (Steam, browsers, etc.)
- Does not modify registry settings outside the specific keys listed above

---

## Impact on Gaming Performance

The direct FPS impact of these removals is small on a modern system with 16+ GB RAM — background processes on an idle system typically consume 1–3% CPU and a few hundred MB of RAM, neither of which meaningfully constrains CS2.

The real benefit is scheduling noise reduction: fewer background threads competing for CPU scheduler time and fewer network I/O events generating NIC interrupts and NDIS DPC activity during CS2 sessions.

djdallmann's classification (GamingPCSetup): "Debloat provides low-confidence improvements for systems with adequate RAM; the primary benefit is reducing scheduling noise floor rather than recovering meaningfully constrained resources."

---

## Rollback

AppX packages removed by the suite can be reinstalled from the Microsoft Store manually by searching for the app name. The Windows Store itself is not removed.

Telemetry services can be re-enabled:
```powershell
Set-Service DiagTrack -StartupType Automatic
Start-Service DiagTrack
```

Telemetry scheduled tasks can be re-enabled via Task Scheduler (taskschd.msc) → navigate to the task path → right-click → Enable.

Autostart entries (Step 14) are backed up by the suite's backup system and can be restored via START.bat → [7] Restore / Rollback → select Step 14.

Consumer features and Advertising ID can be reverted:
```powershell
Remove-ItemProperty "HKLM:\SOFTWARE\Policies\Microsoft\Windows\CloudContent" -Name DisableWindowsConsumerFeatures
Set-ItemProperty "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo" -Name Enabled -Value 1
```
