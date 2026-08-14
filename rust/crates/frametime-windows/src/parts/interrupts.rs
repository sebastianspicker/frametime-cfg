use frametime_core::{
    NetworkAdapterBinding as InterruptNetworkAdapterBinding,
    PciDeviceBinding as InterruptPciDeviceBinding,
};

const INTERRUPT_PARAMETERS_SUFFIX: &str =
    "Device Parameters\\Interrupt Management\\MessageSignaledInterruptProperties";
const AFFINITY_POLICY_SUFFIX: &str = "Device Parameters\\Interrupt Management\\Affinity Policy";
const MSI_SUPPORTED: &str = "MSISupported";
const MESSAGE_NUMBER_LIMIT: &str = "MessageNumberLimit";
const DEVICE_POLICY: &str = "DevicePolicy";
const ASSIGNMENT_SET_OVERRIDE: &str = "AssignmentSetOverride";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterruptRegistryValue {
    Dword(u32),
    Binary(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterruptRegistryChange {
    pub key: String,
    pub name: &'static str,
    pub value: InterruptRegistryValue,
}

/// Read-only seam used to prove the exact postcondition before later live
/// action wiring is allowed.
pub trait InterruptRegistryReader {
    fn read_interrupt_value(
        &self,
        key: &str,
        name: &'static str,
    ) -> Result<Option<InterruptRegistryValue>, DeviceBindingError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsiDeviceBatch {
    pub device: InterruptPciDeviceBinding,
    pub device_class: PciDeviceClass,
    pub changes: Vec<InterruptRegistryChange>,
}

impl MsiDeviceBatch {
    pub fn build(
        device_class: PciDeviceClass,
        device: InterruptPciDeviceBinding,
    ) -> Result<Self, DeviceBindingError> {
        device
            .validate()
            .map_err(|error| DeviceBindingError::InvalidPciBinding(error.to_string()))?;
        if !device.class_guid.eq_ignore_ascii_case(device_class.class_guid()) {
            return Err(DeviceBindingError::InvalidPciBinding(
                "device class GUID does not match its requested MSI class".into(),
            ));
        }
        let key = interrupt_parameters_key(&device);
        let mut changes = vec![InterruptRegistryChange {
            key: key.clone(),
            name: MSI_SUPPORTED,
            value: InterruptRegistryValue::Dword(1),
        }];
        if device_class == PciDeviceClass::Display {
            changes.push(InterruptRegistryChange {
                key,
                name: MESSAGE_NUMBER_LIMIT,
                value: InterruptRegistryValue::Dword(16),
            });
        }
        Ok(Self {
            device,
            device_class,
            changes,
        })
    }

    pub fn validate_readback(
        &self,
        registry: &impl InterruptRegistryReader,
    ) -> Result<(), DeviceBindingError> {
        validate_interrupt_readback(registry, &self.changes)
    }
}

/// Builds one exact, deterministically ordered MSI batch per supported device.
pub fn build_msi_device_batches(
    devices: impl IntoIterator<Item = (PciDeviceClass, InterruptPciDeviceBinding)>,
) -> Result<Vec<MsiDeviceBatch>, DeviceBindingError> {
    let mut batches = BTreeMap::new();
    for (class, device) in devices {
        let identity = device.instance_id.to_ascii_uppercase();
        let batch = MsiDeviceBatch::build(class, device)?;
        match batches.get(&identity) {
            Some(existing) if *existing == batch => {}
            Some(_) => return Err(DeviceBindingError::AmbiguousPciIdentity(identity)),
            None => {
                batches.insert(identity, batch);
            }
        }
    }
    Ok(batches.into_values().collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorGroup {
    pub group_number: u16,
    pub active_logical_processors: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorTopology {
    pub groups: Vec<ProcessorGroup>,
}

pub trait ProcessorTopologyProvider {
    fn processor_topology(&self) -> Result<ProcessorTopology, DeviceBindingError>;
}

impl ProcessorTopology {
    pub fn final_logical_processor(&self) -> Result<u8, DeviceBindingError> {
        let [group] = self.groups.as_slice() else {
            return Err(DeviceBindingError::UnsupportedProcessorTopology);
        };
        if group.group_number != 0 || !(1..=64).contains(&group.active_logical_processors) {
            return Err(DeviceBindingError::UnsupportedProcessorTopology);
        }
        Ok(group.active_logical_processors - 1)
    }

    pub fn assignment_set_override(&self) -> Result<[u8; 8], DeviceBindingError> {
        let processor = self.final_logical_processor()?;
        Ok((1_u64 << processor).to_le_bytes())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NicAffinityBinding {
    pub adapter: InterruptNetworkAdapterBinding,
    pub device: InterruptPciDeviceBinding,
    pub final_logical_processor: u8,
    pub assignment_set_override: [u8; 8],
    pub changes: Vec<InterruptRegistryChange>,
}

impl NicAffinityBinding {
    pub fn resolve(
        adapter: InterruptNetworkAdapterBinding,
        known_pci_devices: &[(PciDeviceClass, InterruptPciDeviceBinding)],
        topology: &ProcessorTopology,
    ) -> Result<Self, DeviceBindingError> {
        adapter
            .validate()
            .map_err(|error| DeviceBindingError::InvalidNetworkBinding(error.to_string()))?;
        let device = known_pci_devices
            .iter()
            .find_map(|(class, device)| {
                (*class == PciDeviceClass::Network
                    && same_pci_pnp_identity(device, &adapter.device))
                .then(|| device.clone())
            })
            .ok_or(DeviceBindingError::NetworkAdapterDoesNotMatchPciIdentity)?;
        let final_logical_processor = topology.final_logical_processor()?;
        let assignment_set_override = topology.assignment_set_override()?;
        let key = affinity_policy_key(&device);
        let changes = vec![
            InterruptRegistryChange {
                key: key.clone(),
                name: DEVICE_POLICY,
                value: InterruptRegistryValue::Dword(4),
            },
            InterruptRegistryChange {
                key,
                name: ASSIGNMENT_SET_OVERRIDE,
                value: InterruptRegistryValue::Binary(assignment_set_override.to_vec()),
            },
        ];
        Ok(Self {
            device,
            adapter,
            final_logical_processor,
            assignment_set_override,
            changes,
        })
    }

    pub fn validate_readback(
        &self,
        registry: &impl InterruptRegistryReader,
    ) -> Result<(), DeviceBindingError> {
        validate_interrupt_readback(registry, &self.changes)
    }
}

pub fn discover_nic_affinity_binding(
    pci_enumerator: &impl PciDeviceEnumerator,
    network_enumerator: &impl NetworkAdapterEnumerator,
    topology_provider: &impl ProcessorTopologyProvider,
) -> Result<NicAffinityBinding, DeviceBindingError> {
    let devices = enumerate_present_status_ok_pci(pci_enumerator)?;
    let adapter = resolve_active_physical_wired_adapter(network_enumerator.enumerate_network_adapters()?)?;
    NicAffinityBinding::resolve(adapter, &devices, &topology_provider.processor_topology()?)
}

pub fn validate_interrupt_readback(
    registry: &impl InterruptRegistryReader,
    changes: &[InterruptRegistryChange],
) -> Result<(), DeviceBindingError> {
    for change in changes {
        if registry.read_interrupt_value(&change.key, change.name)?.as_ref() != Some(&change.value) {
            return Err(DeviceBindingError::RegistryReadbackMismatch {
                key: change.key.clone(),
                name: change.name,
            });
        }
    }
    Ok(())
}

fn interrupt_parameters_key(device: &InterruptPciDeviceBinding) -> String {
    format!("SYSTEM\\CurrentControlSet\\Enum\\{}\\{INTERRUPT_PARAMETERS_SUFFIX}", device.instance_id)
}

fn affinity_policy_key(device: &InterruptPciDeviceBinding) -> String {
    format!("SYSTEM\\CurrentControlSet\\Enum\\{}\\{AFFINITY_POLICY_SUFFIX}", device.instance_id)
}

fn same_pci_pnp_identity(
    left: &InterruptPciDeviceBinding,
    right: &InterruptPciDeviceBinding,
) -> bool {
    left.instance_id.eq_ignore_ascii_case(&right.instance_id)
        && left.container_id.eq_ignore_ascii_case(&right.container_id)
        && left.class_guid.eq_ignore_ascii_case(&right.class_guid)
        && left.vendor_id == right.vendor_id
        && left.device_id == right.device_id
        && left.subsystem_vendor_id == right.subsystem_vendor_id
        && left.subsystem_device_id == right.subsystem_device_id
        && left.revision_id == right.revision_id
}
