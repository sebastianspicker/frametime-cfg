# Backup & Restore System — Deep Dive

> Covers the backup-restore module (`helpers/backup-restore.ps1`) and `C:\CS2_OPTIMIZE\backup.json`.

Every modification the suite makes is recorded before it happens. If something goes wrong — a setting causes an unexpected problem, a driver install fails, or you simply want to undo a step — the backup system lets you restore to pre-optimization state at any granularity: a single step, a group, or everything.

---

## How Automatic Backup Works

The suite's two primary write primitives — `Set-RegistryValue` and `Set-BootConfig` — automatically call the backup functions before writing. You don't need to do anything to enable backups. The current value of every registry key and boot config entry is recorded before it is overwritten.

For service state, power plans, DRS settings, and scheduled tasks, the helpers call their respective backup functions explicitly at the beginning of each action block.

All backup data is stored in a single JSON file: `C:\CS2_OPTIMIZE\backup.json`.

---

## Backup Types

### `registry`

Recorded by `Backup-RegistryValue` before every `Set-RegistryValue` call.

```json
{
  "type": "registry",
  "path": "HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Power",
  "name": "HiberbootEnabled",
  "originalValue": 1,
  "originalType": "DWord",
  "existed": true,
  "step": "Disable Fast Startup",
  "timestamp": "2026-03-13 14:22:01"
}
```

If `existed: false`, the key did not exist before the suite created it. Restoring deletes the key rather than writing back a value.

### `service`

Recorded by `Backup-ServiceState` before any service start type or status change.

```json
{
  "type": "service",
  "name": "SysMain",
  "originalStartType": "Automatic",
  "originalStatus": "Running",
  "step": "Disable SysMain + Search + QWAVE + Xbox",
  "timestamp": "2026-03-13 14:35:00"
}
```

Restore sets the start type back and starts the service if it was Running.

### `bootconfig`

Recorded by `Backup-BootConfig` before any `bcdedit` modification.

```json
{
  "type": "bootconfig",
  "key": "disabledynamictick",
  "originalValue": "No",
  "existed": true,
  "step": "Timer Optimization",
  "timestamp": "2026-03-13 14:22:45"
}
```

If `existed: false`, restore runs `bcdedit /deletevalue <key>` rather than setting the previous value.

### `powerplan`

Recorded, flushed, and verified by `Backup-PowerPlan` before the suite creates or activates a replacement power plan.

```json
{
  "type": "powerplan",
  "originalGuid": "381b4222-f694-41f0-9685-ff5bb260df2e",
  "originalName": "Balanced",
  "step": "CS2 Power Plan",
  "timestamp": "2026-03-13 14:20:10"
}
```

Restore activates the original plan by GUID and deletes the suite's custom power plan.

### `drs`

Recorded by `Backup-DrsSettings` before writing NVIDIA DRS settings.

```json
{
  "type": "drs",
  "step": "NVIDIA CS2 Profile",
  "profile": "Counter-strike 2",
  "profileCreated": false,
  "settings": [
    { "id": 274197361, "previousValue": 0, "existed": true },
    { "id": 8102046, "previousValue": 4, "existed": true },
    ...
  ],
  "timestamp": "2026-03-13 15:01:22"
}
```

If `profileCreated: true`, restore deletes the entire profile. If the profile existed before, restore writes back each setting's previous value individually via `nvapi64.dll`.

### `scheduledtask`

Legacy backups may contain this entry from older releases that created an X3D CCD affinity task. Current releases do not infer CCD affinity from aggregate WMI counts and do not create this task automatically.

```json
{
  "type": "scheduledtask",
  "taskName": "CS2_Optimize_CCD_Affinity",
  "taskPath": "\\",
  "existed": false,
  "wasEnabled": false,
  "scriptPath": "C:\\CS2_OPTIMIZE\\cs2_affinity.ps1",
  "step": "Process Priority + CCD Affinity",
  "timestamp": "2026-03-13 15:05:00"
}
```

If `existed: false` (the task was created by the suite), restore unregisters it and deletes the affinity script. If `existed: true`, restore uses `taskPath`, `taskName`, and `wasEnabled` to restore the exact task identity and enabled/disabled state rather than blindly re-enabling by name.

---

## Accessing Restore

### From START.bat

```
[7] Restore / Rollback
```

Shows the backup summary grouped by step, then presents:

```
[1]  Disable Fast Startup  (1 change)
[2]  Timer Optimization    (3 changes)
[3]  CS2 Power Plan        (1 change)
...
[A]  Restore ALL
[0]  Cancel
```

Select a number to restore that step, or `A` to restore everything.

### From the GUI

Recovery → shows the same grouped list and enables per-step restore after a row
is selected.

---

## Backup File Location

`C:\CS2_OPTIMIZE\backup.json`

The file is plain JSON — readable in any text editor. Each `entries` array element is one backed-up setting. The `step` field groups changes made by the same optimization step.

---

## What Is NOT Backed Up

- **Removed AppX packages** (Step 13 debloat) — Windows AppX removal is not trivially reversible. Removed packages can be reinstalled from the Microsoft Store manually, but the suite does not back up their state.
- **Driver packages removed during GPU driver clean** (Phase 2) — Windows removes validated INF packages through `pnputil`; the suite does not copy those packages into `backup.json`. Phase 3 installs the selected replacement driver. The suite does not directly delete DriverStore folders or display-class registry keys.
- **Autoexec.cfg edits** — the suite only appends one line (`exec optimization.cfg`) to autoexec.cfg. The original file is preserved; removing that line reverts the change. `optimization.cfg` itself can simply be deleted.
- **Video.txt writes** (GUI) — the original video.txt is renamed to `video.txt.bak` before any write. The `.bak` file serves as the backup.

---

## DRY-RUN and Backups

In DRY-RUN mode (`$SCRIPT:DryRun = $true`), `Set-RegistryValue` and `Set-BootConfig` print what they *would* write but do not write. Backup functions are also skipped in DRY-RUN — no backup entries are recorded for non-executed changes.

DRY-RUN is activated by selecting any profile and answering "yes" to the dry run prompt in Phase 1. It is particularly useful for inspecting what a step would do before committing.

---

## Manual Restore Without the Tool

If the backup system itself is unavailable (e.g., `backup.json` was deleted), common settings can be restored manually:

**Registry — Power plan (revert to Windows default):**
```powershell
powercfg /setactive 381b4222-f694-41f0-9685-ff5bb260df2e  # Balanced
```

**Boot config — revert timer settings:**
```powershell
bcdedit /deletevalue disabledynamictick
bcdedit /deletevalue useplatformtick
```

**Fast Startup — re-enable:**
```powershell
Set-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power" -Name HiberbootEnabled -Value 1
```

**IFEO process priority — remove:**
```powershell
Remove-Item "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\cs2.exe\PerfOptions" -Force
```

**Services — re-enable:**
```powershell
Set-Service SysMain -StartupType Automatic
Set-Service WSearch -StartupType Automatic
Set-Service qWave   -StartupType Manual
```

**NVIDIA DRS — remove CS2 profile:** Open NVIDIA Profile Inspector → find "Counter-strike 2" → right-click → Delete profile → Save.
