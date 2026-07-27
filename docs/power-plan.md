# Power Plan

This document covers Phase 1 Step 6 and `helpers/power-plan.ps1`.

The step creates a repository-owned Windows power plan, applies AC settings by
profile, activates the plan, and records the previously active scheme for
recovery. It does not modify DC or battery values.

## Evidence boundary

The checked-in implementation and dry-run tests can establish which
`powercfg` values the suite requests. They do not establish a general CS2
performance benefit. CPU firmware, Windows processor drivers, laptop firmware,
cooling, and hardware support determine whether a value is accepted and what it
does.

The repository contains no committed power, temperature, frequency, or
frame-time dataset for these tiers. Review temperature, clock behavior, fan
noise, idle power, sleep behavior, storage, USB devices, and network behavior
after applying the plan.

## Plan creation and activation

The helper creates a named plan from a Windows base plan, applies the selected
tier values with `powercfg /setacvalueindex`, and activates it. Unsupported
values can be rejected by the platform. The helper records and reports those
results rather than treating every GUID as universally supported.

The plan incorporates settings previously compared with an FPSHeaven `.pow`
plan, but the repository applies its own decoded GUID values. It does not bundle
or import the external binary plan.

## Tier selection

| Profile | T1 | T2 | T3 |
|---|---|---|---|
| SAFE | Applied | Skipped | Skipped |
| RECOMMENDED | Applied | Applied | Skipped |
| COMPETITIVE | Applied | Applied | Applied |
| CUSTOM | Applied | Applied | Applied |
| YOLO | Applied | Applied | Applied |

CUSTOM still presents the surrounding step confirmation. Internal power-plan
tiers are selected from the active profile once the step runs.

## T1 settings

T1 is applied whenever the power-plan step runs.

| Setting | Requested AC value | Scope or tradeoff |
|---|---:|---|
| Processor maximum performance state | 100 | Removes a plan-level maximum below 100 percent. Firmware and thermal controls still apply. |
| Core parking maximum cores | 100 | Requests that all cores be available. |
| USB selective suspend | Disabled | Increases the chance that USB devices remain powered while the plan is active. |
| Disk idle timeout | 0 | Prevents plan-driven disk idle timeout. |
| AHCI link power management | HIPM-only when T2 is skipped | Applies only where the platform exposes the setting. |
| Standby timeout | 0 | Prevents automatic standby from this AC plan. |
| Hibernate timeout | 0 | Prevents automatic hibernation from this AC plan. |
| System cooling policy | Active | Requests active cooling before passive processor reduction where supported. |
| PCIe Link State Power Management | Off | Disables plan-driven PCIe link-state power saving on AC power. |

The PCIe subgroup is
`501a4d13-42af-4429-9fd1-a8218c268e20`; the Link State Power Management setting
is `ee12f906-d277-404b-b6da-e5fa1a576df5`.

## T2 settings

T2 adds vendor-aware processor values and additional storage, USB, Wi-Fi, and
GPU power preferences.

| Setting | Requested AC value | Scope or tradeoff |
|---|---:|---|
| Processor minimum performance state | AMD: 0; Intel: 100 | Vendor branch selected by detected CPU vendor. A high minimum increases idle power and heat. |
| Processor energy performance preference | 0 | Requests a performance preference where firmware supports CPPC or the exposed Windows control. |
| Secondary energy performance preference | 0 | Can be unsupported on some platforms. |
| Processor boost policy | 254 | Platform interpretation can differ. |
| Processor boost mode | 255 | Can be clamped or rejected by platform-supported ranges. |
| Maximum processor idle state | 2 | Restricts deeper idle states where supported. |
| Core parking minimum cores | 100 | Requests no plan-driven core parking. |
| Intel ring minimum cores | 100 on Intel | Applied only on the Intel branch where available. |
| AHCI link power management | Off | T2 replaces T1 HIPM-only behavior. |
| AHCI adaptive link-power timeout | 0 | Applies to the AHCI setting, not a general NVMe APST control. |
| NVMe idle timeout | 0 | Applies only when Windows and the device expose the setting. |
| Disk adaptive power | Off | Reduces plan-driven storage power saving. |
| USB-C connector power management | Disabled | Can increase idle power. |
| USB hub suspend | Disabled | Can increase idle power and affect laptop battery life. |
| Wi-Fi power saving | Off | Can increase adapter power use. |
| GPU preference | 4 | Requests the Windows high-performance preference where supported. |

`PERFAUTONOMOUS` is deliberately not changed. The repository avoids disabling
hardware-autonomous processor performance control because AMD CPPC and Intel
hybrid scheduling can depend on firmware feedback.

## T3 settings

T3 changes processor idle and governor behavior and has the largest power and
thermal tradeoffs.

| Setting | Requested AC value | Scope or tradeoff |
|---|---:|---|
| Processor idle disable | 1 | Requests that processor idle states be disabled, except where the implementation deliberately skips it for detected topology. |
| Processor duty cycling | Off | Changes processor thermal and power behavior. Hardware thermal protection still applies. |
| Performance history count | 1 | Reduces the history window used by the exposed policy. |
| Performance increase time | 0 | Requests the minimum exposed interval. |
| Performance decrease time | 100 | Requests a slower decrease policy. |

Do not use T3 on a laptop, thermally constrained system, or machine whose idle
power and fan behavior matter without first measuring those effects. A platform
can ignore, clamp, or reject a requested value.

## Source-plan differences

The repository records the following intentional differences from the external
plan it analyzed:

- system cooling policy is requested as Active;
- processor minimum state is selected by CPU vendor;
- duty cycling is disabled only in T3;
- `PERFAUTONOMOUS` is not changed;
- only AC values are changed;
- display timeout and broad device-idle policies outside the documented list are
  not imported.

These are implementation differences, not proof that either complete plan
performs better on a particular system.

## Storage setting terminology

An older repository description called the AHCI adaptive link-power GUID an
NVMe APST control. The implementation and documentation now distinguish them.
The AHCI value does not act as a general NVMe APST master switch. T2 separately
requests an NVMe idle-timeout value where Windows exposes it.

## Verification

Check the active scheme:

```powershell
powercfg /getactivescheme
```

Query selected processor values:

```powershell
$guid = (powercfg /getactivescheme |
    Select-String -Pattern '(\{[0-9a-f-]+\})' |
    ForEach-Object { $_.Matches[0].Value })

powercfg /query $guid SUB_PROCESSOR PROCTHROTTLEMAX
powercfg /query $guid SUB_PROCESSOR PROCTHROTTLEMIN
powercfg /query $guid SUB_PROCESSOR IDLEDISABLE
```

Compare the output with the selected profile and detected CPU vendor. Also check
for power-management utilities that can override Windows plan values, including
OEM laptop software, Ryzen Master, and Intel XTU.

If temperatures, fan noise, idle power, sleep, storage, USB, or network behavior
regress, restore the previously active plan through the Recovery workflow.
