use crate::*;
use frametime_domain::{
    backup::{InterruptPolicyBackup, InterruptPolicyKind, InterruptPolicyValue},
    binding::PciDeviceBinding,
};

pub(crate) trait InterruptRegistryStore: InterruptRegistryReader {
    fn write_interrupt_value(
        &self,
        key: &str,
        name: &'static str,
        value: &InterruptRegistryValue,
    ) -> Result<(), DeviceBindingError>;

    fn delete_interrupt_value(
        &self,
        key: &str,
        name: &'static str,
    ) -> Result<(), DeviceBindingError>;

    fn key_exists(&self, key: &str) -> Result<bool, DeviceBindingError>;

    /// Creates the missing levels of `key` below an existing `Device Parameters` key.
    fn create_key_levels(&self, key: &str) -> Result<(), DeviceBindingError>;

    /// Deletes `key` only when it has no subkeys and no values.
    fn delete_key_if_empty(&self, key: &str) -> Result<(), DeviceBindingError>;
}

/// The keys from the first level below `Device Parameters` down to `key`, shallowest first.
fn key_levels_below_parameters(key: &str) -> Vec<String> {
    const MARKER: &str = "\\Device Parameters\\";
    let Some(start) = key.find(MARKER).map(|index| index + MARKER.len()) else {
        return Vec::new();
    };
    key[start..]
        .match_indices('\\')
        .map(|(index, _)| &key[..start + index])
        .chain(Some(key))
        .map(str::to_owned)
        .collect()
}

fn missing_key_levels(
    store: &impl InterruptRegistryStore,
    key: &str,
) -> Result<Vec<String>, DeviceBindingError> {
    let mut missing = Vec::new();
    for level in key_levels_below_parameters(key) {
        if !store.key_exists(&level)? {
            missing.push(level);
        }
    }
    Ok(missing)
}

pub(crate) fn policy_for_change(
    device: &PciDeviceBinding,
    change: &InterruptRegistryChange,
) -> Result<InterruptPolicyKind, DeviceBindingError> {
    let policy = match change.name {
        MSI_SUPPORTED => InterruptPolicyKind::MsiSupported,
        MESSAGE_NUMBER_LIMIT => InterruptPolicyKind::MessageNumberLimit,
        DEVICE_POLICY => InterruptPolicyKind::DevicePolicy,
        ASSIGNMENT_SET_OVERRIDE => InterruptPolicyKind::AssignmentSetOverride,
        _ => {
            return Err(DeviceBindingError::RegistryAccess(
                "interrupt value name is not allowlisted".into(),
            ));
        }
    };
    let expected = format!(
        "SYSTEM\\CurrentControlSet\\Enum\\{}\\{}",
        device.instance_id,
        policy.registry_suffix()
    );
    if change.key != expected
        || matches!(
            (&policy, &change.value),
            (
                InterruptPolicyKind::AssignmentSetOverride,
                InterruptRegistryValue::Dword(_)
            ) | (
                InterruptPolicyKind::MsiSupported
                    | InterruptPolicyKind::MessageNumberLimit
                    | InterruptPolicyKind::DevicePolicy,
                InterruptRegistryValue::Binary(_)
            )
        )
    {
        return Err(DeviceBindingError::RegistryAccess(
            "interrupt registry change is not bound to its device and policy".into(),
        ));
    }
    Ok(policy)
}

pub(crate) fn backup_value(value: InterruptRegistryValue) -> InterruptPolicyValue {
    match value {
        InterruptRegistryValue::Dword(value) => InterruptPolicyValue::Dword(value),
        InterruptRegistryValue::Binary(value) => InterruptPolicyValue::Binary(value),
    }
}

pub(crate) fn runtime_value(value: &InterruptPolicyValue) -> InterruptRegistryValue {
    match value {
        InterruptPolicyValue::Dword(value) => InterruptRegistryValue::Dword(*value),
        InterruptPolicyValue::Binary(value) => InterruptRegistryValue::Binary(value.clone()),
    }
}

pub(crate) fn capture_interrupt_backups(
    store: &impl InterruptRegistryStore,
    step: &str,
    batches: &[MsiDeviceBatch],
    timestamp: &str,
) -> Result<Vec<BackupEntry>, DeviceBindingError> {
    let mut entries = Vec::new();
    for batch in batches {
        for change in &batch.changes {
            let policy = policy_for_change(&batch.device, change)?;
            if policy.expected_step() != step {
                return Err(DeviceBindingError::RegistryAccess(
                    "MSI batch was captured for the wrong workflow step".into(),
                ));
            }
            let original_value = store
                .read_interrupt_value(&change.key, change.name)?
                .map(backup_value);
            // Only P3:3 may create its `Affinity Policy` key; P3:2 still refuses when it is absent.
            let created_key_levels = if step == "P3:3" {
                missing_key_levels(store, &change.key)?
            } else {
                Vec::new()
            };
            let backup = InterruptPolicyBackup {
                step: step.into(),
                timestamp: timestamp.into(),
                device: batch.device.clone(),
                policy,
                existed: original_value.is_some(),
                original_value,
                created_key_levels,
                unknown: BTreeMap::new(),
            };
            backup
                .validate()
                .map_err(|error| DeviceBindingError::RegistryAccess(error.to_string()))?;
            entries.push(BackupEntry::InterruptPolicy {
                backup: Box::new(backup),
            });
        }
    }
    Ok(entries)
}

pub(crate) fn capture_nic_affinity_backups(
    store: &impl InterruptRegistryStore,
    binding: &NicAffinityBinding,
    timestamp: &str,
) -> Result<Vec<BackupEntry>, DeviceBindingError> {
    let batch = MsiDeviceBatch {
        device: binding.device.clone(),
        device_class: PciDeviceClass::Network,
        changes: binding.changes.clone(),
    };
    capture_interrupt_backups(store, "P3:3", &[batch], timestamp)
}

pub(crate) fn apply_interrupt_changes(
    store: &impl InterruptRegistryStore,
    device: &PciDeviceBinding,
    changes: &[InterruptRegistryChange],
) -> Result<(), DeviceBindingError> {
    for change in changes {
        if policy_for_change(device, change)?.expected_step() == "P3:3" {
            store.create_key_levels(&change.key)?;
        }
        store.write_interrupt_value(&change.key, change.name, &change.value)?;
    }
    Ok(())
}

pub(crate) fn interrupt_changes_satisfied(
    store: &impl InterruptRegistryStore,
    device: &PciDeviceBinding,
    changes: &[InterruptRegistryChange],
) -> Result<bool, DeviceBindingError> {
    let mut satisfied = true;
    for change in changes {
        policy_for_change(device, change)?;
        satisfied &= store
            .read_interrupt_value(&change.key, change.name)?
            .as_ref()
            == Some(&change.value);
    }
    Ok(satisfied)
}

pub(crate) fn reobserve_device(
    expected: &PciDeviceBinding,
    observed: &[(PciDeviceClass, PciDeviceBinding)],
) -> Result<PciDeviceBinding, DeviceBindingError> {
    let matches = observed
        .iter()
        .filter(|(_, candidate)| expected.same_pnp_device(candidate))
        .map(|(_, candidate)| candidate.clone())
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [device] => Ok(device.clone()),
        [] => Err(DeviceBindingError::InvalidPciBinding(format!(
            "device {} is no longer present",
            expected.instance_id
        ))),
        _ => Err(DeviceBindingError::AmbiguousPciIdentity(
            expected.instance_id.clone(),
        )),
    }
}

pub(crate) fn reobserve_msi_batches(
    batches: &[MsiDeviceBatch],
    observed: &[(PciDeviceClass, PciDeviceBinding)],
) -> Result<(), DeviceBindingError> {
    for batch in batches {
        let current = reobserve_device(&batch.device, observed)?;
        if !current
            .class_guid
            .eq_ignore_ascii_case(batch.device_class.class_guid())
        {
            return Err(DeviceBindingError::InvalidPciBinding(
                "reobserved interrupt device changed class".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn apply_msi_batches(
    store: &impl InterruptRegistryStore,
    batches: &[MsiDeviceBatch],
    observed: &[(PciDeviceClass, PciDeviceBinding)],
) -> Result<(), DeviceBindingError> {
    reobserve_msi_batches(batches, observed)?;
    for batch in batches {
        apply_interrupt_changes(store, &batch.device, &batch.changes)?;
    }
    Ok(())
}

pub(crate) fn verify_msi_batches(
    store: &impl InterruptRegistryStore,
    batches: &[MsiDeviceBatch],
    observed: &[(PciDeviceClass, PciDeviceBinding)],
) -> Result<(), DeviceBindingError> {
    reobserve_msi_batches(batches, observed)?;
    for batch in batches {
        batch.validate_readback(store)?;
    }
    Ok(())
}

pub(crate) fn restore_interrupt_backup(
    store: &impl InterruptRegistryStore,
    observed: &[(PciDeviceClass, PciDeviceBinding)],
    backup: &InterruptPolicyBackup,
) -> Result<(), DeviceBindingError> {
    backup
        .validate()
        .map_err(|error| DeviceBindingError::RegistryAccess(error.to_string()))?;
    reobserve_device(&backup.device, observed)?;
    let key = backup.registry_key();
    let name = backup.policy.value_name();
    if let Some(value) = &backup.original_value {
        store.write_interrupt_value(&key, name, &runtime_value(value))?;
        if store.read_interrupt_value(&key, name)?.as_ref() != Some(&runtime_value(value)) {
            return Err(DeviceBindingError::RegistryReadbackMismatch { key, name });
        }
    } else {
        store.delete_interrupt_value(&key, name)?;
        if store.read_interrupt_value(&key, name)?.is_some() {
            return Err(DeviceBindingError::RegistryReadbackMismatch { key, name });
        }
    }
    // Deepest first; a level still holding the sibling policy's value is removed by that restore.
    for level in backup.created_key_levels.iter().rev() {
        store.delete_key_if_empty(level)?;
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn discover_native_msi_batches() -> Result<Vec<MsiDeviceBatch>, String> {
    let devices = enumerate_present_status_ok_pci(&WindowsSetupApiEnumerator)
        .map_err(|error| error.to_string())?;
    if devices.is_empty() {
        return Err("P1:21 found no present, status-OK display/network/media PCI devices".into());
    }
    build_msi_device_batches(devices).map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub(crate) fn discover_native_msi_batches() -> Result<Vec<MsiDeviceBatch>, String> {
    Err("interrupt device discovery requires Windows SetupAPI".into())
}

#[cfg(windows)]
pub(crate) fn discover_native_nic_affinity() -> Result<NicAffinityBinding, String> {
    discover_nic_affinity_binding(
        &WindowsSetupApiEnumerator,
        &WindowsIpHelperNetworkAdapterEnumerator,
        &WindowsProcessorTopologyProvider,
    )
    .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub(crate) fn discover_native_nic_affinity() -> Result<NicAffinityBinding, String> {
    Err("NIC affinity discovery requires Windows SetupAPI and IP Helper".into())
}

#[cfg(windows)]
pub(crate) fn native_interrupt_batches_satisfied(
    batches: &[MsiDeviceBatch],
) -> Result<bool, String> {
    let store = WindowsInterruptRegistry;
    let mut satisfied = true;
    for batch in batches {
        satisfied &= interrupt_changes_satisfied(&store, &batch.device, &batch.changes)
            .map_err(|error| error.to_string())?;
    }
    Ok(satisfied)
}

#[cfg(not(windows))]
pub(crate) fn native_interrupt_batches_satisfied(
    batches: &[MsiDeviceBatch],
) -> Result<bool, String> {
    let mut satisfied = true;
    for batch in batches {
        satisfied &= interrupt_changes_satisfied(
            &UnavailableInterruptRegistry,
            &batch.device,
            &batch.changes,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(satisfied)
}

#[cfg(windows)]
pub(crate) fn capture_native_msi_backups(
    batches: &[MsiDeviceBatch],
    timestamp: &str,
) -> Result<Vec<BackupEntry>, String> {
    capture_interrupt_backups(&WindowsInterruptRegistry, "P3:2", batches, timestamp)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub(crate) fn capture_native_msi_backups(
    batches: &[MsiDeviceBatch],
    timestamp: &str,
) -> Result<Vec<BackupEntry>, String> {
    capture_interrupt_backups(&UnavailableInterruptRegistry, "P3:2", batches, timestamp)
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
pub(crate) fn capture_native_nic_backups(
    binding: &NicAffinityBinding,
    timestamp: &str,
) -> Result<Vec<BackupEntry>, String> {
    capture_nic_affinity_backups(&WindowsInterruptRegistry, binding, timestamp)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub(crate) fn capture_native_nic_backups(
    binding: &NicAffinityBinding,
    timestamp: &str,
) -> Result<Vec<BackupEntry>, String> {
    capture_nic_affinity_backups(&UnavailableInterruptRegistry, binding, timestamp)
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
pub(crate) fn apply_native_msi_batches(batches: &[MsiDeviceBatch]) -> Result<(), String> {
    let observed = enumerate_present_status_ok_pci(&WindowsSetupApiEnumerator)
        .map_err(|error| error.to_string())?;
    apply_msi_batches(&WindowsInterruptRegistry, batches, &observed)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub(crate) fn apply_native_msi_batches(batches: &[MsiDeviceBatch]) -> Result<(), String> {
    apply_msi_batches(&UnavailableInterruptRegistry, batches, &[])
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
pub(crate) fn verify_native_msi_batches(batches: &[MsiDeviceBatch]) -> Result<(), String> {
    let observed = enumerate_present_status_ok_pci(&WindowsSetupApiEnumerator)
        .map_err(|error| error.to_string())?;
    verify_msi_batches(&WindowsInterruptRegistry, batches, &observed)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub(crate) fn verify_native_msi_batches(batches: &[MsiDeviceBatch]) -> Result<(), String> {
    verify_msi_batches(&UnavailableInterruptRegistry, batches, &[])
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
pub(crate) fn apply_native_nic_affinity(binding: &NicAffinityBinding) -> Result<(), String> {
    let current = discover_native_nic_affinity()?;
    if !same_nic_affinity_subject(binding, &current) {
        return Err("P3:3 NIC or processor-topology evidence changed before apply".into());
    }
    apply_interrupt_changes(&WindowsInterruptRegistry, &binding.device, &binding.changes)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub(crate) fn apply_native_nic_affinity(binding: &NicAffinityBinding) -> Result<(), String> {
    let current = discover_native_nic_affinity()?;
    if !same_nic_affinity_subject(binding, &current) {
        return Err("P3:3 NIC or processor-topology evidence changed before apply".into());
    }
    apply_interrupt_changes(
        &UnavailableInterruptRegistry,
        &binding.device,
        &binding.changes,
    )
    .map_err(|error| error.to_string())
}

#[cfg(windows)]
pub(crate) fn verify_native_nic_affinity(binding: &NicAffinityBinding) -> Result<(), String> {
    let current = discover_native_nic_affinity()?;
    if !same_nic_affinity_subject(binding, &current) {
        return Err("P3:3 NIC or processor-topology evidence changed before verification".into());
    }
    binding
        .validate_readback(&WindowsInterruptRegistry)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub(crate) fn verify_native_nic_affinity(binding: &NicAffinityBinding) -> Result<(), String> {
    let current = discover_native_nic_affinity()?;
    if !same_nic_affinity_subject(binding, &current) {
        return Err("P3:3 NIC or processor-topology evidence changed before verification".into());
    }
    binding
        .validate_readback(&UnavailableInterruptRegistry)
        .map_err(|error| error.to_string())
}

pub(crate) fn same_nic_affinity_subject(
    left: &NicAffinityBinding,
    right: &NicAffinityBinding,
) -> bool {
    left.device.same_pnp_device(&right.device)
        && left
            .adapter
            .adapter_name
            .eq_ignore_ascii_case(&right.adapter.adapter_name)
        && left
            .adapter
            .interface_guid
            .eq_ignore_ascii_case(&right.adapter.interface_guid)
        && left.adapter.interface_luid == right.adapter.interface_luid
        && left.adapter.interface_index == right.adapter.interface_index
        && left.adapter.physical_address == right.adapter.physical_address
        && left.final_logical_processor == right.final_logical_processor
        && left.assignment_set_override == right.assignment_set_override
}

#[cfg(windows)]
pub(crate) fn restore_native_interrupt_policy(entry: &BackupEntry) -> Result<(), String> {
    let BackupEntry::InterruptPolicy { backup } = entry else {
        return Err("interrupt restore received a different backup type".into());
    };
    let observed = enumerate_present_status_ok_pci(&WindowsSetupApiEnumerator)
        .map_err(|error| error.to_string())?;
    restore_interrupt_backup(&WindowsInterruptRegistry, &observed, backup)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub(crate) fn restore_native_interrupt_policy(entry: &BackupEntry) -> Result<(), String> {
    let BackupEntry::InterruptPolicy { backup } = entry else {
        return Err("interrupt restore received a different backup type".into());
    };
    restore_interrupt_backup(&UnavailableInterruptRegistry, &[], backup)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub(crate) struct UnavailableInterruptRegistry;

#[cfg(not(windows))]
impl InterruptRegistryReader for UnavailableInterruptRegistry {
    fn read_interrupt_value(
        &self,
        _: &str,
        _: &'static str,
    ) -> Result<Option<InterruptRegistryValue>, DeviceBindingError> {
        Err(DeviceBindingError::PlatformAdapterUnavailable(
            "interrupt registry",
        ))
    }
}

#[cfg(not(windows))]
impl InterruptRegistryStore for UnavailableInterruptRegistry {
    fn write_interrupt_value(
        &self,
        _: &str,
        _: &'static str,
        _: &InterruptRegistryValue,
    ) -> Result<(), DeviceBindingError> {
        Err(DeviceBindingError::PlatformAdapterUnavailable(
            "interrupt registry",
        ))
    }

    fn delete_interrupt_value(&self, _: &str, _: &'static str) -> Result<(), DeviceBindingError> {
        Err(DeviceBindingError::PlatformAdapterUnavailable(
            "interrupt registry",
        ))
    }

    fn key_exists(&self, _: &str) -> Result<bool, DeviceBindingError> {
        Err(DeviceBindingError::PlatformAdapterUnavailable(
            "interrupt registry",
        ))
    }

    fn create_key_levels(&self, _: &str) -> Result<(), DeviceBindingError> {
        Err(DeviceBindingError::PlatformAdapterUnavailable(
            "interrupt registry",
        ))
    }

    fn delete_key_if_empty(&self, _: &str) -> Result<(), DeviceBindingError> {
        Err(DeviceBindingError::PlatformAdapterUnavailable(
            "interrupt registry",
        ))
    }
}

#[cfg(test)]
mod created_key_tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeSet;

    const PARAMETERS: &str = "SYSTEM\\CurrentControlSet\\Enum\\PCI\\VEN_8086&DEV_15F3&SUBSYS_00008086&REV_03\\1\\Device Parameters";

    #[derive(Default)]
    struct FakeStore {
        keys: RefCell<BTreeSet<String>>,
        values: RefCell<BTreeMap<String, InterruptRegistryValue>>,
    }

    impl InterruptRegistryReader for FakeStore {
        fn read_interrupt_value(
            &self,
            key: &str,
            _: &'static str,
        ) -> Result<Option<InterruptRegistryValue>, DeviceBindingError> {
            Ok(self.values.borrow().get(key).cloned())
        }
    }

    impl InterruptRegistryStore for FakeStore {
        fn write_interrupt_value(
            &self,
            key: &str,
            _: &'static str,
            value: &InterruptRegistryValue,
        ) -> Result<(), DeviceBindingError> {
            self.values.borrow_mut().insert(key.into(), value.clone());
            Ok(())
        }

        fn delete_interrupt_value(
            &self,
            key: &str,
            _: &'static str,
        ) -> Result<(), DeviceBindingError> {
            self.values.borrow_mut().remove(key);
            Ok(())
        }

        fn key_exists(&self, key: &str) -> Result<bool, DeviceBindingError> {
            Ok(self.keys.borrow().contains(key))
        }

        fn create_key_levels(&self, key: &str) -> Result<(), DeviceBindingError> {
            self.keys
                .borrow_mut()
                .extend(key_levels_below_parameters(key));
            Ok(())
        }

        fn delete_key_if_empty(&self, key: &str) -> Result<(), DeviceBindingError> {
            let occupied = self.values.borrow().contains_key(key)
                || self
                    .keys
                    .borrow()
                    .iter()
                    .any(|other| other.starts_with(&format!("{key}\\")));
            if !occupied {
                self.keys.borrow_mut().remove(key);
            }
            Ok(())
        }
    }

    #[test]
    fn levels_are_listed_shallowest_first_and_only_below_device_parameters() {
        let key = format!("{PARAMETERS}\\Interrupt Management\\Affinity Policy");
        assert_eq!(
            key_levels_below_parameters(&key),
            [format!("{PARAMETERS}\\Interrupt Management"), key.clone()]
        );
        assert!(key_levels_below_parameters("SYSTEM\\Other").is_empty());
    }

    #[test]
    fn missing_levels_are_recorded_and_removed_deepest_first() {
        let key = format!("{PARAMETERS}\\Interrupt Management\\Affinity Policy");
        let store = FakeStore::default();
        store.keys.borrow_mut().insert(PARAMETERS.into());
        let created = missing_key_levels(&store, &key).unwrap();
        assert_eq!(created.len(), 2);
        store.create_key_levels(&key).unwrap();
        store
            .write_interrupt_value(&key, "DevicePolicy", &InterruptRegistryValue::Dword(4))
            .unwrap();
        assert!(missing_key_levels(&store, &key).unwrap().is_empty());

        store.delete_interrupt_value(&key, "DevicePolicy").unwrap();
        for level in created.iter().rev() {
            store.delete_key_if_empty(level).unwrap();
        }
        assert_eq!(
            *store.keys.borrow(),
            BTreeSet::from([PARAMETERS.to_owned()])
        );
    }
}
