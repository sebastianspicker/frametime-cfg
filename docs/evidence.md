# Evidence and Risk Boundaries

This document describes what the repository can substantiate about its
optimization actions. It does not assign expected FPS or latency improvements.
The repository does not contain a controlled benchmark dataset that supports
general performance estimates across hardware, Windows builds, drivers, and CS2
versions.

For the implemented phase order and current profile behavior, inspect the phase
scripts and `helpers/tier-system.ps1`. For non-mutating validation behavior, see
[Full dry-run](dry-run.md). For recovery coverage, see
[Backup and restore](backup-restore.md).

## Evidence categories

The documentation uses the following evidence categories:

| Category | Meaning |
|---|---|
| Implemented | The behavior is present in the checked-in PowerShell or configuration files and is covered by repository tests where noted. |
| Platform-documented | The mechanism is described by Microsoft, Valve, NVIDIA, AMD, or another responsible platform vendor. A vendor description does not establish a CS2 performance benefit. |
| Community-derived | The setting or rationale comes from community testing or reverse engineering. Results may not reproduce on other systems. |
| Experimental | The mechanism is implemented, but compatibility or outcome evidence is incomplete. It requires before-and-after validation on the target system. |
| Informational | The step reports state or presents guidance and does not itself apply the described system change. |

Repository tests primarily establish parsing, control flow, dry-run safety,
backup handling, and command construction. They do not establish an FPS,
frametime, input-latency, network-latency, audio, or image-quality improvement.

## Risk labels

Risk labels describe mutation scope and recovery concerns. They are not a
performance ranking.

| Label | Meaning | Typical review requirement |
|---|---|---|
| SAFE | Read-only behavior or a limited change with a straightforward recovery path. Device-specific and software-specific exceptions can still exist. | Confirm the target and review the recorded result. |
| MODERATE | Changes shared Windows, device, boot, driver, service, or network behavior. | Review applicability, capture current state, and test the affected device or workflow after application. |
| AGGRESSIVE | Performs a broad or compatibility-sensitive change, such as driver or service work. | Use only with a tested recovery path and current backups. |
| CRITICAL | Reduces a security control or servicing capability. | Avoid on general-purpose systems unless the user accepts the security and maintenance consequences. |

The runtime scripts, not this document, determine whether a profile applies,
prompts for, skips, or only describes a step. Preparation, transition, and
informational blocks are not necessarily profile-gated even when the related
mutation is tiered elsewhere.

## Implementation and evidence map

| Area | Current implementation | Evidence boundary | Recovery boundary |
|---|---|---|---|
| System assessment | Reads hardware, Windows, driver, service, registry, and configuration state. | A detected condition does not by itself predict a performance problem. | Read-only checks require no restore. |
| Shader and cache cleanup | Removes selected rebuildable cache contents. | Repository tests can verify path selection and dry-run behavior, not the benefit of clearing a specific cache. | Deleted cache data is rebuilt by the application or driver and is not restored from `backup.json`. |
| Power plan | Creates and activates a repository-defined plan through `powercfg`, with profile-dependent settings. | The applied GUID values are inspectable. Performance and power effects remain hardware-dependent. | Prior active-plan state is recorded; review the power-plan document for setting scope. |
| HAGS, MPO, Game Mode, scheduler, and timer settings | Applies documented Windows registry or boot configuration changes where selected. | Windows exposes the mechanisms, but the repository has no cross-system CS2 benchmark dataset for them. | Supported registry and boot changes use backup helpers; boot changes require additional care. |
| Pagefile | Can replace automatic management with a fixed configuration on eligible systems. | A fixed pagefile is not universally beneficial. Workload and memory pressure determine the result. | Pagefile state is recorded, but restoration can require a reboot and manual completion if Windows rejects a live update. |
| AppX and telemetry cleanup | Removes an explicit AppX allowlist and changes selected services, tasks, and policy values. | Reduced background activity does not imply a measurable CS2 improvement. | AppX packages require manual reinstallation. Service, task, and registry recovery coverage is documented separately. |
| NIC settings and DNS | Applies selected adapter, RSS, URO, QoS, interrupt, and DNS changes when supported. | Adapter property names, driver support, routes, and results vary by device and network. | Supported adapter and DNS state is recorded. Firewall and device behavior must be checked after restoration. |
| Driver cleanup and installation | Removes validated display-driver packages, clears selected rebuildable caches, and can install a validated NVIDIA package. AMD-wide application and registry roots are excluded to preserve non-display AMD software. | Successful installation does not establish that one driver version performs better than another. | Removed driver packages are not copied into `backup.json`; a replacement or manual reinstall is required. |
| NVIDIA DRS profile | Writes the checked-in 42-setting alpha set through NVIDIA DRS APIs. | The alpha set is limited to identifiers supported by public NVAPI or NPI references. Ten undocumented development entries were removed. A public name still does not establish a performance benefit. | Prior DRS values are recorded where supported. Driver changes can alter setting availability. |
| CS2 configuration | Generates repository-defined video, launch-option, audio, input, HUD, and network defaults. | Many values are preferences or community-derived choices rather than demonstrated universal improvements. | Existing files are backed up where the workflow supports it; user-edited files require review before overwrite or restore. |
| Benchmark workflow | Stores user-supplied or parsed before-and-after benchmark results and calculates an FPS-cap suggestion. | The workflow can compare supplied measurements. The repository does not ship representative benchmark results for public performance claims. | Benchmark history is local state and is not a system mutation. |

## Technical documentation rules

When a deep-dive document describes a mechanism, distinguish these questions:

1. Does the operating system, driver, game, or repository expose the setting?
2. Does the checked-in implementation apply or inspect it as described?
3. Is the change supported on the target device and software version?
4. Does a repeatable before-and-after measurement show a useful result on that
   system?

Evidence for one question does not answer the others. In particular, a registry
value being accepted does not prove that a driver acts on it, and a community
benchmark does not establish a result for other hardware.

## Validation expectations

Before accepting a performance-sensitive change:

1. Record the Windows build, firmware, hardware, driver, CS2 build, graphics
   settings, map or workload, and background process state.
2. Capture a repeatable baseline with the same tool and run conditions used for
   the comparison.
3. Change one independent variable at a time where practical.
4. Repeat enough runs to distinguish the change from normal run-to-run variance.
5. Check system logs, device state, connectivity, audio, and application
   stability in addition to frame statistics.
6. Restore the prior state when the result is neutral, negative, or unstable.

The included benchmark workflow assists with local comparison but does not
replace a controlled test design.

## Known evidence gaps

- No committed raw benchmark corpus covers the supported hardware and Windows
  combinations.
- No committed latency trace establishes an input, DPC, network, or audio result
  for every applied setting.
- NVIDIA DRS identifiers can change availability or behavior between driver
  branches.
- CS2 console variables and accepted launch options can change without a stable
  compatibility guarantee.
- Network buffer behavior depends on current engine implementation and cannot be
  converted to a fixed millisecond cost from an assumed server tick rate.
- Accessibility, High Contrast, scaling, and native WPF behavior require manual
  validation on Windows.
- AppX removal, removed driver packages, and some file changes do not have a
  complete automated rollback path.

These gaps are acceptable only when they are stated as limitations and the user
can inspect, decline, measure, and restore the relevant change.
