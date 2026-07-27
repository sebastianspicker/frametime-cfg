# MSI Interrupts and NIC Affinity

This document covers Phase 3 Steps 2 and 3 and
`helpers/msi-interrupts.ps1`.

## Evidence boundary

The repository can verify device discovery, registry paths, requested values,
backup handling, and dry-run control flow. It does not contain committed ETW,
DPC, ISR, or input-latency traces showing that enabling MSI or changing affinity
improves a particular device.

Interrupt support and negotiation depend on firmware, the PCI device, driver,
Windows, and processor topology. An `MSISupported` registry value of 1 does not
prove that the device initialized in MSI or MSI-X mode. Verify the effective
device state after a reboot and restore the prior value if stability regresses.

## Interrupt mechanisms

Legacy line-based interrupts can share an IRQ line. Message Signaled Interrupts
allow a capable PCI device to signal an interrupt through a message. MSI-X can
provide multiple vectors where the device and driver support them.

These mechanisms concern interrupt delivery. They do not by themselves decide
which CPU handles all deferred work, how a driver batches events, or whether the
application receives lower latency.

## Phase 3 Step 2

The suite writes the following value under each selected PCI device instance:

```text
HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\<device-instance>\
  Device Parameters\Interrupt Management\MessageSignaledInterruptProperties

MSISupported = 1
```

The single T2 step considers eligible display, network, and media-class devices
together when the profile permits the step and the operator accepts it. There
is no per-class profile selection. The GPU path can also request a
`MessageNumberLimit` value. The driver and Windows determine whether MSI or
MSI-X is actually negotiated and how many vectors are used.

Do not copy the value to arbitrary PCI devices. A driver that does not support
the mode can fail to initialize the device.

## Reboot requirement

The interrupt mode is negotiated during device initialization. Reboot after the
change before evaluating it. Phase 1 Step 23 disables Windows Fast Startup so a
subsequent shutdown does not reuse the hybrid kernel session.

A registry value can persist while the effective device mode remains unchanged.
Use an effective-state check after reboot rather than registry presence alone.

## Verification

### System Information

Open `msinfo32` and inspect the device's IRQ information. Large synthetic IRQ
values can indicate a message-signaled mode, while a conventional small IRQ can
indicate line-based routing. Treat this as an indicator and correlate it with
device and driver state.

### Registry inspection

```powershell
$gpu = Get-PnpDevice -Class Display |
    Where-Object { $_.InstanceId -match '^PCI\\' } |
    Select-Object -First 1

$msiPath = "HKLM:\SYSTEM\CurrentControlSet\Enum\$($gpu.InstanceId)\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties"
(Get-ItemProperty $msiPath -ErrorAction SilentlyContinue).MSISupported
```

This confirms the requested registry state, not the negotiated runtime mode.

### Trace comparison

Use the same Windows trace method, workload, power state, and background traffic
before and after the change. Check device errors, disconnects, audio dropout,
display recovery, network loss, and DPC or ISR behavior. A short LatencyMon run
can identify suspect drivers, but retain raw traces and repeat the result before
attributing it to MSI.

## Phase 3 Step 3: NIC interrupt affinity

The NIC affinity step writes policy under the selected adapter instance:

```text
HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\<nic-instance>\
  Device Parameters\Interrupt Management\Affinity Policy

DevicePolicy = 4
AssignmentSetOverride = <processor mask>
```

`DevicePolicy = 4` requests an explicit processor set. The assignment is a
logical-processor bitmask. Processor numbers are not interchangeable with
physical-core numbers on SMT, hybrid, multi-group, or multi-CCD systems.

The current helper derives a target from detected topology. Treat that result as
experimental. Confirm it against an authoritative logical-processor topology
source and a trace from the target machine. An unsuitable mask can concentrate
work on a busy processor or cross a topology boundary.

## RSS relationship

Receive Side Scaling controls distribution of receive processing and is
separate from MSI and explicit interrupt affinity. Phase 1 Step 16 adds selected
RSS values only when they are absent:

```text
*RSS               = 1
*RSSProfile        = 1
*RssBaseProcNumber = 2
*MaxRssProcessors  = 4
*NumRssQueues      = 4
```

For a detected link speed of 5 Gbit/s or more, the implementation can request 8
processors and queues, capped by available processors. Existing vendor values
are not overwritten.

Registry entries do not prove the effective RSS configuration. Confirm it with
the applicable Windows network cmdlets after the adapter restarts.

## Recovery

The suite records supported MSI and affinity values before mutation. Use the
Recovery workflow, reboot, and verify the effective device state. If a device
fails to initialize, use Safe Mode or Device Manager as needed to restore the
recorded values or reinstall the device driver.
