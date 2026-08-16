use crate::{
    DeviceBindingError, InterruptRegistryChange, InterruptRegistryReader, InterruptRegistryValue,
    MsiDeviceBatch, NetworkAdapterEnumerator, NetworkAdapterObservation, NicAffinityBinding,
    PciDeviceClass, PciDeviceEnumerator, PciDeviceObservation, ProcessorGroup,
    ProcessorTopology, ProcessorTopologyProvider, discover_nic_affinity_binding,
    resolve_active_physical_wired_adapter, resolve_present_status_ok_pci,
};
use frametime_core::{NATIVE_BINDING_SCHEMA_VERSION, NetworkAdapterBinding, PciDeviceBinding};

fn device(class_guid: &str) -> PciDeviceBinding {
    PciDeviceBinding {
        schema_version: NATIVE_BINDING_SCHEMA_VERSION,
        instance_id: r"PCI\VEN_10DE&DEV_2684&SUBSYS_47101462&REV_A1\4&abc&0&0008".into(),
        container_id: "{01234567-89ab-cdef-0123-456789abcdef}".into(),
        class_guid: class_guid.into(),
        vendor_id: 0x10de,
        device_id: 0x2684,
        subsystem_vendor_id: 0x1462,
        subsystem_device_id: 0x4710,
        revision_id: 0xa1,
        driver_provider: "NVIDIA".into(),
        driver_version: "32.0.15.1234".into(),
        published_inf: "oem42.inf".into(),
        observed_at_utc: "2026-08-13T12:00:00Z".into(),
        unknown: BTreeMap::new(),
    }
}

fn adapter(device: PciDeviceBinding) -> NetworkAdapterBinding {
    NetworkAdapterBinding {
        schema_version: NATIVE_BINDING_SCHEMA_VERSION,
        adapter_name: "{aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee}".into(),
        interface_guid: "{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}".into(),
        interface_luid: 7,
        interface_index: 9,
        friendly_name: "Ethernet".into(),
        interface_description: "PCIe Ethernet Controller".into(),
        physical_address: vec![1, 2, 3, 4, 5, 6],
        device,
        observed_at_utc: "2026-08-13T12:00:00Z".into(),
        unknown: BTreeMap::new(),
    }
}

#[test]
fn setupapi_resolution_accepts_only_present_status_ok_supported_pci_devices() {
    let display = device(PciDeviceClass::Display.class_guid());
    let absent = PciDeviceObservation {
        binding: device(PciDeviceClass::Network.class_guid()),
        present: false,
        status_ok: true,
    };
    let selected = resolve_present_status_ok_pci([
        PciDeviceObservation {
            binding: display.clone(),
            present: true,
            status_ok: true,
        },
        PciDeviceObservation {
            binding: display.clone(),
            present: true,
            status_ok: true,
        },
        absent,
    ])
    .expect("exact duplicate is deduplicated");
    assert_eq!(selected, vec![(PciDeviceClass::Display, display)]);
}

#[test]
fn setupapi_resolution_refuses_conflicting_evidence_for_one_pnp_instance() {
    let display = device(PciDeviceClass::Display.class_guid());
    let mut conflicting = display.clone();
    conflicting.driver_version = "32.0.15.9999".into();
    let result = resolve_present_status_ok_pci([
        PciDeviceObservation {
            binding: display,
            present: true,
            status_ok: true,
        },
        PciDeviceObservation {
            binding: conflicting,
            present: true,
            status_ok: true,
        },
    ]);
    assert!(matches!(result, Err(DeviceBindingError::AmbiguousPciIdentity(_))));
}

#[test]
fn msi_batch_has_exact_gpu_and_non_gpu_registry_contracts() {
    let gpu = MsiDeviceBatch::build(PciDeviceClass::Display, device(PciDeviceClass::Display.class_guid()))
        .expect("GPU batch");
    assert_eq!(gpu.changes.len(), 2);
    assert_eq!(gpu.changes[0].name, "MSISupported");
    assert_eq!(gpu.changes[0].value, InterruptRegistryValue::Dword(1));
    assert_eq!(gpu.changes[1].name, "MessageNumberLimit");
    assert_eq!(gpu.changes[1].value, InterruptRegistryValue::Dword(16));
    let network = MsiDeviceBatch::build(PciDeviceClass::Network, device(PciDeviceClass::Network.class_guid()))
        .expect("network batch");
    assert_eq!(network.changes, vec![InterruptRegistryChange {
        key: network.changes[0].key.clone(),
        name: "MSISupported",
        value: InterruptRegistryValue::Dword(1),
    }]);
}

#[test]
fn nic_affinity_requires_one_physical_wired_adapter_exact_pci_match_and_single_group_topology() {
    let nic = device(PciDeviceClass::Network.class_guid());
    let known = vec![(PciDeviceClass::Network, nic.clone())];
    let selected = resolve_active_physical_wired_adapter([NetworkAdapterObservation {
        binding: adapter(nic.clone()),
        is_up: true,
        is_physical: true,
        is_wired: true,
    }])
    .expect("one usable adapter");
    let binding = NicAffinityBinding::resolve(
        selected,
        &known,
        &ProcessorTopology {
            groups: vec![ProcessorGroup {
                group_number: 0,
                active_logical_processors: 64,
            }],
        },
    )
    .expect("exact NIC binding");
    assert_eq!(binding.final_logical_processor, 63);
    assert_eq!(binding.assignment_set_override, [0, 0, 0, 0, 0, 0, 0, 128]);
    assert_eq!(binding.changes[0].value, InterruptRegistryValue::Dword(4));
    assert_eq!(binding.changes[1].value, InterruptRegistryValue::Binary(vec![0, 0, 0, 0, 0, 0, 0, 128]));
}

#[test]
fn nic_affinity_refuses_ambiguity_unmatched_pnp_identity_and_multi_group_topology() {
    let nic = device(PciDeviceClass::Network.class_guid());
    let observation = NetworkAdapterObservation {
        binding: adapter(nic.clone()),
        is_up: true,
        is_physical: true,
        is_wired: true,
    };
    let mut second = observation.clone();
    second.binding.interface_guid = "{bbbbbbbb-bbbb-cccc-dddd-eeeeeeeeeeee}".into();
    second.binding.adapter_name = "{bbbbbbbb-bbbb-cccc-dddd-eeeeeeeeeeee}".into();
    assert!(matches!(
        resolve_active_physical_wired_adapter([observation.clone(), second]),
        Err(DeviceBindingError::MultipleEligibleNetworkAdapters)
    ));
    assert!(matches!(
        NicAffinityBinding::resolve(
            observation.binding,
            &[],
            &ProcessorTopology { groups: vec![ProcessorGroup { group_number: 0, active_logical_processors: 8 }] },
        ),
        Err(DeviceBindingError::NetworkAdapterDoesNotMatchPciIdentity)
    ));
    assert!(ProcessorTopology {
        groups: vec![
            ProcessorGroup { group_number: 0, active_logical_processors: 32 },
            ProcessorGroup { group_number: 1, active_logical_processors: 32 },
        ],
    }
    .assignment_set_override()
    .is_err());
    assert!(ProcessorTopology {
        groups: vec![ProcessorGroup {
            group_number: 0,
            active_logical_processors: 0,
        }],
    }
    .assignment_set_override()
    .is_err());
}

struct PciFixture(Vec<PciDeviceObservation>);

impl PciDeviceEnumerator for PciFixture {
    fn enumerate_pci_devices(&self) -> Result<Vec<PciDeviceObservation>, DeviceBindingError> {
        Ok(self.0.clone())
    }
}

struct NetworkFixture(Vec<NetworkAdapterObservation>);

impl NetworkAdapterEnumerator for NetworkFixture {
    fn enumerate_network_adapters(
        &self,
    ) -> Result<Vec<NetworkAdapterObservation>, DeviceBindingError> {
        Ok(self.0.clone())
    }
}

struct TopologyFixture(ProcessorTopology);

impl ProcessorTopologyProvider for TopologyFixture {
    fn processor_topology(&self) -> Result<ProcessorTopology, DeviceBindingError> {
        Ok(self.0.clone())
    }
}

#[test]
fn nic_affinity_discovery_is_host_testable_through_injected_observers() {
    let nic = device(PciDeviceClass::Network.class_guid());
    let binding = discover_nic_affinity_binding(
        &PciFixture(vec![PciDeviceObservation {
            binding: nic.clone(),
            present: true,
            status_ok: true,
        }]),
        &NetworkFixture(vec![NetworkAdapterObservation {
            binding: adapter(nic),
            is_up: true,
            is_physical: true,
            is_wired: true,
        }]),
        &TopologyFixture(ProcessorTopology {
            groups: vec![ProcessorGroup {
                group_number: 0,
                active_logical_processors: 8,
            }],
        }),
    )
    .expect("injected observers satisfy the exact contract");
    assert_eq!(binding.final_logical_processor, 7);
}

struct Readback(BTreeMap<(String, &'static str), InterruptRegistryValue>);

impl InterruptRegistryReader for Readback {
    fn read_interrupt_value(
        &self,
        key: &str,
        name: &'static str,
    ) -> Result<Option<InterruptRegistryValue>, DeviceBindingError> {
        Ok(self.0.get(&(key.into(), name)).cloned())
    }
}

#[test]
fn registry_readback_requires_every_exact_typed_value() {
    let batch = MsiDeviceBatch::build(PciDeviceClass::Display, device(PciDeviceClass::Display.class_guid()))
        .expect("batch");
    let values = batch
        .changes
        .iter()
        .map(|change| ((change.key.clone(), change.name), change.value.clone()))
        .collect();
    batch.validate_readback(&Readback(values)).expect("exact readback");
    assert!(batch.validate_readback(&Readback(BTreeMap::new())).is_err());
}
