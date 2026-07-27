# Windows Services

This document covers the seven services in Phase 1 Step 37, the two telemetry
services in Step 13, and the optional Windows Update service block in Step 15.

## Evidence boundary

The repository can verify service discovery, requested startup state, stop
operations, and backup handling. It does not contain committed process, disk,
network, DPC, or CS2 benchmark traces that isolate the effect of disabling each
service. A service being idle or periodically active does not establish a
measurable game impact.

Disabling a service removes its functionality system-wide. Review the
compatibility consequences and retain the Windows default when the machine is
not dedicated to the documented use case.

## Phase 1 Step 37

Step 37 is a higher-tier action that disables these services when present:

| Service | Windows function | Main consequence when disabled |
|---|---|---|
| `SysMain` | Application prefetch and memory-use optimization. | Application cold launches can become slower. |
| `WSearch` | Windows file, content, and application indexing. | Start, Explorer, Outlook, and content search can become slower or incomplete. |
| `QWAVE` | Quality Windows Audio Video Experience APIs and QoS support. | Applications using qWave APIs can lose related QoS behavior. |
| `XblAuthManager` | Xbox Live authentication. | Xbox Live and Game Pass sign-in can fail. |
| `XblGameSave` | Xbox cloud-save synchronization. | Cloud saves stop synchronizing. |
| `XboxNetApiSvc` | Xbox Live multiplayer networking services. | Xbox Live multiplayer and related networking can fail. |
| `XboxGipSvc` | Xbox accessory management. | Xbox Wireless controllers, headsets, or accessories can stop working. |

### SysMain

SysMain can perform background prefetch work and can also improve application
launch behavior. Storage type alone does not determine whether disabling it is
beneficial. Compare application launch time, memory pressure, storage activity,
and game traces before retaining the change.

### Windows Search

The Windows Search indexer performs background indexing according to Windows
policy and idle detection. Disabling it prevents future index maintenance and
can materially reduce search functionality. Prefer configuring indexed
locations or maintenance timing when search is required.

### qWave

Step 16 creates separate Windows QoS policies for CS2. Those policies are not a
proof that qWave is redundant for every other application. Re-enable `QWAVE` if
multimedia, streaming, or other QoS-aware software regresses.

### Xbox services

CS2 does not require Xbox Live services, but other installed games and devices
can. Do not disable the group on a PC used for Game Pass, Microsoft Store games,
Xbox cloud saves, Xbox multiplayer, or Xbox Wireless accessories unless the
loss of those features is accepted.

## Phase 1 Step 13

Step 13 disables these services when present:

| Service | Windows function | Main consequence when disabled |
|---|---|---|
| `DiagTrack` | Connected User Experiences and Telemetry. | Windows diagnostic and usage-data workflows can change. |
| `dmwappushservice` | Device Management WAP Push support. | Mobile-device management and Intune workflows can fail. |

`dmwappushservice` must not be disabled on an organization-managed or MDM-enrolled
PC without administrator approval. `DiagTrack` can also be controlled by
organization policy. A local write can be reverted or overridden by management
policy.

## Phase 1 Step 15

Step 15 is a `CRITICAL` T3 operation that targets `wuauserv`, `UsoSvc`, and
`WaaSMedicSvc` when present. If the profile risk gate permits the step and the
operator accepts it, the workflow captures all present service states, persists
those records, then requests `Disabled` and stops each service. Any capture,
persistence, mutation, or postcondition failure prevents the step from being
recorded as complete.

This is not a performance optimization. Disabling these services interferes
with normal Windows security and quality updates. Skip the step on systems that
depend on normal Windows servicing. Recovery restores recorded startup and
running state, but organization policy or a later Windows change can override
that state.

## Current requested state

The suite requests `Disabled` and stops each selected service. Windows defaults
vary by edition, build, installation history, and policy, so the document does
not assign one universal default startup type.

## Recovery

The backup system records the original startup type and running state before a
supported service mutation. Restore the relevant step through the Recovery
workflow rather than assuming `Manual` or `Automatic` for every machine.

For manual recovery, inspect the recorded entry first, then restore that startup
type and running state:

```powershell
Set-Service <ServiceName> -StartupType <RecordedStartType>
Start-Service <ServiceName>  # only if it was previously running
```

After recovery, test search, application launch, media, Game Pass, Xbox devices,
cloud saves, multiplayer, Windows diagnostics, and device-management behavior as
applicable.
