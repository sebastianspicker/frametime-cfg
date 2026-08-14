use frametime_core::{
    InterruptPolicyBackup, InterruptPolicyKind, InterruptPolicyValue, PciDeviceBinding,
};

trait InterruptRegistryStore: InterruptRegistryReader {
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
}

fn policy_for_change(
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

fn backup_value(value: InterruptRegistryValue) -> InterruptPolicyValue {
    match value {
        InterruptRegistryValue::Dword(value) => InterruptPolicyValue::Dword(value),
        InterruptRegistryValue::Binary(value) => InterruptPolicyValue::Binary(value),
    }
}

fn runtime_value(value: &InterruptPolicyValue) -> InterruptRegistryValue {
    match value {
        InterruptPolicyValue::Dword(value) => InterruptRegistryValue::Dword(*value),
        InterruptPolicyValue::Binary(value) => InterruptRegistryValue::Binary(value.clone()),
    }
}

fn capture_interrupt_backups(
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
            let backup = InterruptPolicyBackup {
                step: step.into(),
                timestamp: timestamp.into(),
                device: batch.device.clone(),
                policy,
                existed: original_value.is_some(),
                original_value,
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

fn capture_nic_affinity_backups(
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

fn apply_interrupt_changes(
    store: &impl InterruptRegistryStore,
    device: &PciDeviceBinding,
    changes: &[InterruptRegistryChange],
) -> Result<(), DeviceBindingError> {
    for change in changes {
        policy_for_change(device, change)?;
        store.write_interrupt_value(&change.key, change.name, &change.value)?;
    }
    Ok(())
}

fn interrupt_changes_satisfied(
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

fn reobserve_device(
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

fn reobserve_msi_batches(
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

fn apply_msi_batches(
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

fn verify_msi_batches(
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

fn restore_interrupt_backup(
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
    Ok(())
}

#[cfg(windows)]
fn discover_native_msi_batches() -> Result<Vec<MsiDeviceBatch>, String> {
    let devices = enumerate_present_status_ok_pci(&WindowsSetupApiEnumerator)
        .map_err(|error| error.to_string())?;
    if devices.is_empty() {
        return Err("P1:21 found no present, status-OK display/network/media PCI devices".into());
    }
    build_msi_device_batches(devices).map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn discover_native_msi_batches() -> Result<Vec<MsiDeviceBatch>, String> {
    Err("interrupt device discovery requires Windows SetupAPI".into())
}

#[cfg(windows)]
fn discover_native_nic_affinity() -> Result<NicAffinityBinding, String> {
    discover_nic_affinity_binding(
        &WindowsSetupApiEnumerator,
        &WindowsIpHelperNetworkAdapterEnumerator,
        &WindowsProcessorTopologyProvider,
    )
    .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn discover_native_nic_affinity() -> Result<NicAffinityBinding, String> {
    Err("NIC affinity discovery requires Windows SetupAPI and IP Helper".into())
}

#[cfg(windows)]
fn native_interrupt_batches_satisfied(batches: &[MsiDeviceBatch]) -> Result<bool, String> {
    let store = WindowsInterruptRegistry;
    let mut satisfied = true;
    for batch in batches {
        satisfied &= interrupt_changes_satisfied(&store, &batch.device, &batch.changes)
            .map_err(|error| error.to_string())?;
    }
    Ok(satisfied)
}

#[cfg(not(windows))]
fn native_interrupt_batches_satisfied(batches: &[MsiDeviceBatch]) -> Result<bool, String> {
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
fn capture_native_msi_backups(
    batches: &[MsiDeviceBatch],
    timestamp: &str,
) -> Result<Vec<BackupEntry>, String> {
    capture_interrupt_backups(&WindowsInterruptRegistry, "P3:2", batches, timestamp)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn capture_native_msi_backups(
    batches: &[MsiDeviceBatch],
    timestamp: &str,
) -> Result<Vec<BackupEntry>, String> {
    capture_interrupt_backups(&UnavailableInterruptRegistry, "P3:2", batches, timestamp)
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn capture_native_nic_backups(
    binding: &NicAffinityBinding,
    timestamp: &str,
) -> Result<Vec<BackupEntry>, String> {
    capture_nic_affinity_backups(&WindowsInterruptRegistry, binding, timestamp)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn capture_native_nic_backups(
    binding: &NicAffinityBinding,
    timestamp: &str,
) -> Result<Vec<BackupEntry>, String> {
    capture_nic_affinity_backups(&UnavailableInterruptRegistry, binding, timestamp)
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn apply_native_msi_batches(batches: &[MsiDeviceBatch]) -> Result<(), String> {
    let observed = enumerate_present_status_ok_pci(&WindowsSetupApiEnumerator)
        .map_err(|error| error.to_string())?;
    apply_msi_batches(&WindowsInterruptRegistry, batches, &observed)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn apply_native_msi_batches(batches: &[MsiDeviceBatch]) -> Result<(), String> {
    apply_msi_batches(&UnavailableInterruptRegistry, batches, &[])
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn verify_native_msi_batches(batches: &[MsiDeviceBatch]) -> Result<(), String> {
    let observed = enumerate_present_status_ok_pci(&WindowsSetupApiEnumerator)
        .map_err(|error| error.to_string())?;
    verify_msi_batches(&WindowsInterruptRegistry, batches, &observed)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn verify_native_msi_batches(batches: &[MsiDeviceBatch]) -> Result<(), String> {
    verify_msi_batches(&UnavailableInterruptRegistry, batches, &[])
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn apply_native_nic_affinity(binding: &NicAffinityBinding) -> Result<(), String> {
    let current = discover_native_nic_affinity()?;
    if !same_nic_affinity_subject(binding, &current) {
        return Err("P3:3 NIC or processor-topology evidence changed before apply".into());
    }
    apply_interrupt_changes(&WindowsInterruptRegistry, &binding.device, &binding.changes)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn apply_native_nic_affinity(binding: &NicAffinityBinding) -> Result<(), String> {
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
fn verify_native_nic_affinity(binding: &NicAffinityBinding) -> Result<(), String> {
    let current = discover_native_nic_affinity()?;
    if !same_nic_affinity_subject(binding, &current) {
        return Err("P3:3 NIC or processor-topology evidence changed before verification".into());
    }
    binding
        .validate_readback(&WindowsInterruptRegistry)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn verify_native_nic_affinity(binding: &NicAffinityBinding) -> Result<(), String> {
    let current = discover_native_nic_affinity()?;
    if !same_nic_affinity_subject(binding, &current) {
        return Err("P3:3 NIC or processor-topology evidence changed before verification".into());
    }
    binding
        .validate_readback(&UnavailableInterruptRegistry)
        .map_err(|error| error.to_string())
}

fn same_nic_affinity_subject(left: &NicAffinityBinding, right: &NicAffinityBinding) -> bool {
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
fn restore_native_interrupt_policy(entry: &BackupEntry) -> Result<(), String> {
    let BackupEntry::InterruptPolicy { backup } = entry else {
        return Err("interrupt restore received a different backup type".into());
    };
    let observed = enumerate_present_status_ok_pci(&WindowsSetupApiEnumerator)
        .map_err(|error| error.to_string())?;
    restore_interrupt_backup(&WindowsInterruptRegistry, &observed, backup)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn restore_native_interrupt_policy(entry: &BackupEntry) -> Result<(), String> {
    let BackupEntry::InterruptPolicy { backup } = entry else {
        return Err("interrupt restore received a different backup type".into());
    };
    restore_interrupt_backup(&UnavailableInterruptRegistry, &[], backup)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
struct UnavailableInterruptRegistry;

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
}
