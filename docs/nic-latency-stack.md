# NIC Configuration and Latency Diagnostics

This document covers Phase 1 Step 16 and Phase 3 Step 3. The implementation
changes Ethernet adapter properties, selected Windows network-offload state,
QoS policy, and optionally interrupt affinity.

## Evidence boundary

The repository has no committed packet capture, ETW trace, LatencyMon result, or
route dataset that establishes a latency improvement from these settings. NIC
property names and behavior vary by adapter, firmware, driver, Windows build,
switch, router, and ISP.

The suite records which operations were requested and skips properties a driver
does not expose. A successful property write does not prove a useful DPC or
network result. Measure the target adapter before and after application, and
restore values that worsen throughput, loss, power, wake behavior, or latency.

## Phase 1 adapter properties

The Ethernet path requests these values from `config.env.ps1`:

| Logical property | Requested value | Main tradeoff |
|---|---|---|
| Energy Efficient Ethernet | Disabled | Higher idle adapter and link power. |
| Flow Control | Disabled | Removes Ethernet PAUSE handling but can increase loss under congestion. |
| Interrupt Moderation | Medium | Coalesces some interrupt work; exact driver behavior is adapter-specific. |
| Receive Buffers | 512, or 2048 for detected 5 Gbit/s and faster adapters | Larger rings use more memory and can tolerate larger bursts. They are not a fixed latency guarantee. |
| Transmit Buffers | 512, or 2048 for detected 5 Gbit/s and faster adapters | Same driver-specific tradeoff as receive rings. |

Intel and Realtek drivers expose different display names. The implementation
tries the configured primary name and then the corresponding alternate name.
It also attempts to disable `*GreenEthernet` and `*PowerSavingMode` when the
driver exposes those registry keywords.

Unsupported or absent properties are skipped. The suite does not install a
different NIC driver to obtain them.

### Interrupt moderation

The repository chooses Medium for all profiles that run Step 16. This is a
community-derived default, not a Microsoft or adapter-vendor universal
recommendation. Driver implementations can map the label to different batching
behavior.

Compare available driver modes with the same background traffic and workload.
A mode that reduces interrupt count can add batching delay; a mode that handles
every packet separately can increase CPU interrupt work. Keep the mode that
produces the better measured result on the target system.

### RSS state

Receive Side Scaling distributes network receive processing across processors.
The helper adds expected RSS registry values only when they are absent. It does
not overwrite existing vendor or administrator values.

Restart the adapter or reboot before evaluating the effective RSS state. Use
Windows network cmdlets to confirm the active configuration rather than relying
only on the registry entries.

## UDP Receive Offload

Where the Windows network stack supports URO, Step 16 records the current global
state and requests that URO be disabled. URO can coalesce UDP datagrams before
delivery to higher network layers. Whether that helps or harms a game workload
depends on driver and stack behavior.

The operation is skipped when the command is not supported. The documentation
does not convert potential batching into a fixed millisecond delay or assume a
fixed CS2 packet interval.

Check the current Windows state with the applicable `netsh int udp show global`
command on the target build. Use the Recovery workflow to restore the recorded
state.

## QoS policy

Step 16 creates two repository-owned QoS policies with DSCP value 46:

| Policy | Match |
|---|---|
| `CS2_UDP_Ports` | UDP ports 27015 through 27036. |
| `CS2_App` | The detected `cs2.exe` application path. |

It also writes the policy value named `Do not use NLA` so the Windows QoS policy
can apply across network profile classification. This changes global QoS policy
behavior and can conflict with organization-managed settings.

A DSCP mark is only a label. A local switch or router must honor it for queuing
to change, and upstream networks can rewrite or discard it. The repository does
not claim that the policy changes Internet routing or Valve server behavior.

## TCP acknowledgment and Nagle values

Phase 1 Step 25 writes `TcpNoDelay=1` and `TcpAckFrequency=1` below the
detected wired interface's TCP/IP registry key. These values request different
TCP coalescing and acknowledgment behavior for applications using that
interface. They do not control UDP game datagrams, and this repository has no
application-level latency benchmark for the change.

The values can affect TCP packet frequency outside CS2. Recovery restores the
recorded prior values. Compare relevant TCP applications and network behavior
before retaining the change.

## IPv6

The suite leaves IPv6 enabled. It does not claim that IPv6 is always faster than
IPv4. Keeping both stacks available allows Windows, Steam, and the live network
to select an available path and avoids a global protocol change without a
target-specific diagnosis.

Do not disable IPv6 solely to reduce background traffic. If a repeatable
IPv6-specific failure exists, diagnose name resolution, address selection,
router advertisements, path MTU, and routing before making a system-wide change.

## Phase 3 interrupt affinity

Phase 3 Step 3 is a higher-risk operation that writes NIC interrupt-affinity
policy. It is intended for a system where NIC DPC work has been measured and CPU
topology is known.

The helper sets `DevicePolicy` and `AssignmentSetOverride` for the selected NIC
device. An incorrect processor mask can place interrupts on a busy core, an
inappropriate logical processor, or a processor outside the expected topology.
Do not infer a suitable mask from core count alone on hybrid or multi-CCD CPUs.

Use an authoritative logical-processor topology source and compare trace data
before retaining this step. Recovery removes or restores the recorded policy
values.

## Wi-Fi scope

The adapter property and affinity work targets the selected Ethernet adapter.
The workflow can still change global URO and QoS policy when Wi-Fi is active.
Wi-Fi latency is also affected by radio interference, channel use, roaming,
power policy, access-point buffering, and signal quality, none of which these
registry settings can correct.

## Bufferbloat

Bufferbloat is queueing delay caused by saturated links. NIC property changes do
not configure router queue management or increase ISP capacity. Test latency
while the connection is loaded in both upload and download directions. Address
the bottleneck at the router or traffic source when saturation is the cause.

## Settings deliberately left alone

The suite does not apply every advanced-property recommendation found in NIC
tuning guides. In particular:

- TCP RSC and LSO do not configure CS2 UDP game datagrams;
- checksum offload changes shift work between the NIC and CPU and require local
  evidence;
- wake-on-LAN settings concern sleep or powered-down behavior;
- jumbo-frame configuration must match the entire path and is not required for
  normal CS2 datagrams;
- arbitrary buffer reductions can overflow under background traffic;
- disabling IPv6 globally changes routing and application compatibility.

## Verification

After Step 16 or Phase 3 Step 3:

1. Export or record the effective adapter advanced properties.
2. Confirm link speed, RSS state, URO state, QoS policies, and the selected
   adapter.
3. Test throughput, packet loss, jitter, DNS, sleep and resume, Wake-on-LAN if
   used, VPNs, and organization network access.
4. Capture DPC and ISR behavior with an appropriate Windows trace tool under the
   same traffic and game workload used for the baseline.
5. Confirm the interrupt-affinity mask against current logical-processor
   topology if Phase 3 Step 3 was applied.
6. Restore the previous values if the result is neutral, negative, or unstable.

LatencyMon can identify drivers associated with long DPC or ISR execution, but
its summary alone does not prove that a specific NIC setting caused the result.
Retain raw traces and repeat the comparison.
