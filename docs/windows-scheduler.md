# Windows Scheduling and Input Settings

This document covers the scheduling, presentation, timer, maintenance, and input
values applied in Phase 1.

## Evidence boundary

The repository can verify registry paths, requested values, dry-run behavior,
and backup capture. It does not contain a committed ETW, DPC, input-latency, or
frame-time dataset that isolates these values across supported systems.

Windows scheduling and presentation behavior changes between builds. A registry
value being present does not prove that a driver or current Windows component
acts on it. Treat performance rationales as hypotheses to test on the target
system. Review compatibility and restore the prior value if results regress.

## Implemented steps

| Phase 1 step | Area | Current behavior |
|---:|---|---|
| 4 | Fullscreen compatibility | Sets the per-executable compatibility value used by the suite. |
| 7 | Hardware-accelerated GPU scheduling | Presents a tiered NVIDIA path and writes `HwSchMode` when selected; AMD and Intel paths are informational. |
| 10 | Dynamic tick | Optionally writes the `disabledynamictick` BCD value. The repository does not set `useplatformtick`. |
| 11 | Multiplane Overlay | Writes DWM `OverlayTestMode=5` when the T3 step is selected. |
| 12 | Game Mode | Sets the two current-user Game Bar values used by the suite. |
| 23 | Fast Startup | Sets `HiberbootEnabled` to zero. |
| 26 | GameConfigStore | Writes four current-user fullscreen behavior values. |
| 27 | Scheduler and system policy | Applies MMCSS, foreground scheduling, memory-manager, maintenance, NTFS, co-installer, FTH, and selected Intel power-throttling values. |
| 28 | Timer requests | Enables `GlobalTimerResolutionRequests` on supported Windows builds. |
| 29 | Mouse input | Disables Windows pointer acceleration and sets `MouseDataQueueSize` to 50. |
| 31 | Game DVR | Disables selected capture values and policy. |

Risk and profile gating are defined by the runtime scripts. Several values
affect the whole system, not only CS2.

## Dynamic tick and Multiplane Overlay

Step 10 is an experimental comparison path for the BCD
`disabledynamictick=yes` value. Microsoft documents related platform timer
options for debugging, and this repository has no isolated CS2 result for the
change. It does not set `useplatformtick` or `useplatformclock`. The BCD value is
captured before mutation and requires a reboot to evaluate.

Step 11 writes `OverlayTestMode=5` under
`HKLM:\SOFTWARE\Microsoft\Windows\Dwm` to request an MPO-disabled state.
Desktop, video, and multi-monitor composition can change. Restore the recorded
value and reboot before comparing the default state.

## Hardware-accelerated GPU scheduling

HAGS moves part of GPU scheduling into a hardware-supported scheduling path.
The outcome depends on Windows, GPU architecture, driver, presentation mode,
and application behavior. The suite presents a profile-dependent choice for
NVIDIA systems and does not claim one state is generally faster.

The implementation writes `HwSchMode` under:

```text
HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers
```

A reboot and a supported driver are required before the active state can be
evaluated. Compare both states on the same driver and workload.

## Game Mode and Game DVR

Game Mode and Game DVR are separate controls. Step 12 enables the suite's Game
Mode preference:

```text
HKCU:\SOFTWARE\Microsoft\GameBar
    AllowAutoGameMode = 1
    AutoGameModeEnabled = 1
```

This is a Windows gaming-policy choice. The repository does not claim that it
suppresses Windows Update or guarantees foreground scheduling behavior.

Step 31 separately disables selected Game DVR and capture values. This removes
the corresponding Windows capture workflow and can affect users who rely on
Game Bar recording.

## Fast Startup

Step 23 writes:

```text
HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power
    HiberbootEnabled = 0
```

This disables hybrid shutdown. It is used so a shutdown performs a full device
initialization path after interrupt and driver changes. It increases startup
time on some systems. Full hibernation is a separate capability.

## Fullscreen compatibility values

Step 4 writes the per-application compatibility setting. Step 26 supplements it
with these current-user GameConfigStore values:

```text
HKCU:\System\GameConfigStore
    GameDVR_DXGIHonorFSEWindowsCompatible = 1
    GameDVR_FSEBehavior = 2
    GameDVR_FSEBehaviorMode = 2
    GameDVR_HonorUserFSEBehaviorMode = 1
```

Presentation behavior still depends on the current Windows compositor, game,
driver, overlays, and display configuration. The repository does not assign a
fixed latency improvement to these values.

## Step 27 system policy

Step 27 groups several independent system-wide mutations behind one tiered
confirmation. Review each one before applying the group.

### MMCSS and foreground scheduling

The suite writes `SystemResponsiveness = 10` and deliberately does not set
`NetworkThrottlingIndex`. It also writes `Win32PrioritySeparation = 0x2A` and
the repository's Games task values.

These values change Windows multimedia and foreground scheduling policy.
Background media, capture, streaming, or communication workloads can behave
differently. The bit-level policy exists, but the repository has no committed
trace proving a CS2 improvement from `0x2A` or `SystemResponsiveness = 10`.

### Memory manager

`DisablePagingExecutive = 1` requests that pageable kernel and driver code stay
resident. This can increase physical memory use and is not a substitute for
adequate RAM or a correctly configured pagefile.

### Intel power throttling

On the detected Intel hybrid-CPU branch, the suite writes
`PowerThrottlingOff = 1`. This changes system-wide Windows power-throttling
policy. It can increase power use and heat. No value is written by this branch
for other detected CPU vendors.

### Fault Tolerant Heap

The suite writes `HKLM:\SOFTWARE\Microsoft\FTH\Enabled = 0`. FTH is a Windows
application-compatibility mechanism. Disabling it globally can expose crashes
that Windows would otherwise attempt to mitigate. The repository does not
contain a heap trace demonstrating that FTH is active for a given CS2 install.

### Automatic Maintenance

The suite sets `MaintenanceDisabled = 1`. This prevents automatic maintenance
from running through that policy and can delay maintenance work. Users must
decide when to run required maintenance manually.

### NTFS and device co-installers

The suite writes:

```text
NtfsDisableLastAccessUpdate = 0x80000001
NtfsDisable8dot3NameCreation = 1
DisableCoInstallers = 1
```

Disabling 8.3 name creation can affect legacy software. Disabling device
co-installers can affect vendor device setup or optional software. These are
compatibility-sensitive system policies, not CS2-only values.

## Timer-resolution requests

Step 28 writes `GlobalTimerResolutionRequests = 1` on supported Windows builds.
This permits applications to make the relevant timer-resolution request. It
does not itself force a permanent one-millisecond global timer.

Timer behavior is observable with platform tracing tools. The repository does
not claim that the value changes frame rate or latency when the game, driver, or
another application already requests the same effective resolution.

## Mouse input

Step 29 writes these current-user pointer values:

```text
MouseSpeed = 0
MouseThreshold1 = 0
MouseThreshold2 = 0
```

This disables Windows pointer acceleration for affected input paths. It changes
desktop pointer behavior as well as game behavior where Windows pointer
processing is used.

The step also writes `MouseDataQueueSize = 50` under the mouse class-driver
parameters. The queue value is a repository policy choice. The repository has
no committed input trace establishing that 50 is optimal or a fixed latency
bound. A value that is too small for a device and system load can drop input.

## Verification

After applying these settings:

1. Reboot when requested.
2. Confirm the expected registry values and active HAGS state in Windows.
3. Test fullscreen and borderless presentation, overlays, Alt+Tab, capture, and
   multi-monitor behavior.
4. Test audio, streaming, communication tools, legacy applications, device
   installation, maintenance, sleep, shutdown, and hibernation as applicable.
5. Compare repeatable frame-time and input measurements with the same workload.
6. Restore the prior values if compatibility, power, temperature, or measured
   performance regresses.
