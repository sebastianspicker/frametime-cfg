# Process Priority and CCD Affinity

This document covers Phase 3 Step 10 and
`helpers/process-priority.ps1`.

The current workflow applies a persistent IFEO priority value where selected.
It does not automatically pin a dual-CCD Ryzen X3D processor because the Windows
data used by the suite does not authoritatively map logical processors to CCDs.

## Evidence boundary

The repository can verify the registry write, backup handling, and absence of an
automatic affinity task. It does not contain a committed scheduler trace or CS2
benchmark dataset that isolates High priority from Normal priority. Higher
priority can reduce scheduling opportunities for other applications and system
work. Validate game behavior together with audio, input, network, capture, and
background applications.

## IFEO PerfOptions

The suite writes:

```text
HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\
  Image File Execution Options\cs2.exe\PerfOptions

CpuPriorityClass = 3
```

Windows reads IFEO `PerfOptions` when it creates a matching process. The value
maps to the High priority class.

| Value | Priority class |
|---:|---|
| 1 | Idle |
| 2 | Normal |
| 3 | High |
| 4 | Realtime |
| 5 | Below Normal |
| 6 | Above Normal |

The suite does not use Realtime. Realtime priority can interfere with time-critical
system, input, audio, and device work.

## Difference from `-high`

The launch option depends on application or launcher handling. IFEO is a
persistent Windows process-creation policy and applies regardless of the launch
shortcut. This difference explains why the repository uses IFEO; it does not
prove a performance improvement.

No resident helper process is used. Windows reads the configured value during
process creation.

## X3D topology

Single-CCD and dual-CCD Ryzen X3D products have different cache topology. Model
identity and aggregate `Win32_Processor` core counts are not a logical-processor
to CCD map. Processor numbering can also change with firmware, SMT, processor
groups, and Windows configuration.

For this reason, the current suite does not derive and apply a CCD affinity mask.
On a dual-CCD system, use an authoritative machine-specific topology source and
verify the logical-processor mapping before setting affinity manually. A copied
mask can assign the process to the wrong CCD or processor group.

## Recovery

Use the Recovery workflow to restore the prior IFEO state. For manual removal,
delete the `PerfOptions` values recorded by the suite and remove the `cs2.exe`
subkey only if it is empty and repository-owned.

The current suite does not create an affinity task. If an older run created
`CS2_Optimize_CCD_Affinity`, inspect it before removal, then use the recorded
recovery entry or:

```powershell
Unregister-ScheduledTask -TaskName "CS2_Optimize_CCD_Affinity" -Confirm:$false
```

Delete an older affinity script only after confirming that no task or other
workflow references it.
