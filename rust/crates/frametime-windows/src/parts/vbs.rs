const DEVICE_GUARD_HVCI_KEY: &str =
    "SYSTEM\\CurrentControlSet\\Control\\DeviceGuard\\Scenarios\\HypervisorEnforcedCodeIntegrity";
const DEVICE_GUARD_HVCI_ENABLED: &str = "Enabled";

fn vbs_hvci_batch() -> Vec<RegistryChange> {
    vec![registry_change(
        Hive::LocalMachine,
        DEVICE_GUARD_HVCI_KEY,
        DEVICE_GUARD_HVCI_ENABLED,
        RegValue::Dword(0),
    )]
}

fn vbs_hvci_inspection(status: u32, hvci: Option<&RegValue>) -> Inspection {
    // Win32_DeviceGuard defines 0 as disabled, 1 as configured but not running,
    // and 2 or greater as running. This matches the legacy Phase 3 contract.
    if status < 2 || hvci == Some(&RegValue::Dword(0)) {
        Inspection::Satisfied
    } else {
        Inspection::NeedsApply
    }
}

fn inspect_vbs_hvci(changes: &[RegistryChange]) -> Result<Inspection, String> {
    let change = vbs_hvci_change(changes)?;
    let status = native_device_guard_status()?;
    let hvci = registry_read_exact(change)?;
    Ok(vbs_hvci_inspection(status, hvci.as_ref()))
}

fn vbs_hvci_change(changes: &[RegistryChange]) -> Result<&RegistryChange, String> {
    let expected = vbs_hvci_batch();
    if changes == expected.as_slice() {
        Ok(&changes[0])
    } else {
        Err("P3:7 registry batch is not the exact HVCI contract".into())
    }
}
