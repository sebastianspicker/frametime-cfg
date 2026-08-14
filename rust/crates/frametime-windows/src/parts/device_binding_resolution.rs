/// Retains only present, status-OK PCI display, network, and media devices.
/// Identical repeated records are collapsed; conflicting evidence for the same
/// PnP instance is an ambiguity and is never selected.
pub fn resolve_present_status_ok_pci(
    observations: impl IntoIterator<Item = PciDeviceObservation>,
) -> Result<Vec<(PciDeviceClass, CorePciDeviceBinding)>, DeviceBindingError> {
    let mut devices = BTreeMap::new();
    for observation in observations {
        if !observation.present || !observation.status_ok {
            continue;
        }
        observation
            .binding
            .validate()
            .map_err(|error| DeviceBindingError::InvalidPciBinding(error.to_string()))?;
        let Some(class) = PciDeviceClass::from_class_guid(&observation.binding.class_guid) else {
            continue;
        };
        let identity = observation.binding.instance_id.to_ascii_uppercase();
        match devices.get(&identity) {
            Some((existing_class, existing))
                if *existing_class == class && *existing == observation.binding => {}
            Some(_) => return Err(DeviceBindingError::AmbiguousPciIdentity(identity)),
            None => {
                devices.insert(identity, (class, observation.binding));
            }
        }
    }
    Ok(devices.into_values().collect())
}

pub fn enumerate_present_status_ok_pci(
    enumerator: &impl PciDeviceEnumerator,
) -> Result<Vec<(PciDeviceClass, CorePciDeviceBinding)>, DeviceBindingError> {
    resolve_present_status_ok_pci(enumerator.enumerate_pci_devices()?)
}

/// Validates the one network adapter that may receive interrupt-affinity policy.
pub fn resolve_active_physical_wired_adapter(
    observations: impl IntoIterator<Item = NetworkAdapterObservation>,
) -> Result<CoreNetworkAdapterBinding, DeviceBindingError> {
    let mut candidates = BTreeMap::new();
    for observation in observations {
        if !(observation.is_up && observation.is_physical && observation.is_wired) {
            continue;
        }
        observation
            .binding
            .validate()
            .map_err(|error| DeviceBindingError::InvalidNetworkBinding(error.to_string()))?;
        let identity = observation.binding.interface_guid.to_ascii_uppercase();
        match candidates.get(&identity) {
            Some(existing) if *existing == observation.binding => {}
            Some(_) => return Err(DeviceBindingError::AmbiguousNetworkIdentity(identity)),
            None => {
                candidates.insert(identity, observation.binding);
            }
        }
    }
    match candidates.len() {
        0 => Err(DeviceBindingError::NoEligibleNetworkAdapter),
        1 => Ok(candidates.into_values().next().expect("one candidate")),
        _ => Err(DeviceBindingError::MultipleEligibleNetworkAdapters),
    }
}
