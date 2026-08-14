/// Exact vendor identity used only for the P1:35 chipset-driver observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ChipsetVendor {
    Amd,
    Intel,
}

impl ChipsetVendor {
    const fn pci_vendor_id(self) -> &'static str {
        match self {
            Self::Amd => "1022",
            Self::Intel => "8086",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChipsetDriverRecord {
    instance_id: String,
    hardware_ids: Vec<String>,
    compatible_ids: Vec<String>,
    inf_path: String,
    provider: String,
    driver_version: String,
    driver_date_filetime: u64,
}

/// Immutable, in-memory evidence captured by inspection and repeated exactly
/// during verification.  It is intentionally never backed up or persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChipsetInventory {
    vendor: ChipsetVendor,
    records: Vec<ChipsetDriverRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawChipsetDriverRecord {
    instance_id: String,
    hardware_ids: Vec<String>,
    compatible_ids: Vec<String>,
    inf_path: String,
    provider: String,
    driver_version: String,
    driver_date_filetime: u64,
}

fn chipset_inventory_from_raw(
    vendor: ChipsetVendor,
    records: Vec<RawChipsetDriverRecord>,
) -> Result<Option<ChipsetInventory>, String> {
    let mut selected = Vec::new();
    for record in records {
        let bindings = chipset_vendor_bindings(&record.hardware_ids, &record.compatible_ids)?;
        if bindings.is_empty() {
            continue;
        }
        if bindings.len() != 1 {
            return Err(
                "P1:35 system device has conflicting AMD and Intel PCI vendor bindings".into(),
            );
        }
        let Some(binding) = bindings.into_iter().next() else {
            return Err("P1:35 system device has no PCI vendor binding".into());
        };
        if binding != vendor {
            return Err(format!(
                "P1:35 CPU vendor {} conflicts with system-device PCI vendor {}",
                vendor.pci_vendor_id(),
                binding.pci_vendor_id()
            ));
        }
        selected.push(validate_chipset_record(vendor, record)?);
    }
    if selected.is_empty() {
        return Ok(None);
    }
    selected.sort_by(|left, right| {
        left.instance_id
            .to_ascii_uppercase()
            .cmp(&right.instance_id.to_ascii_uppercase())
    });
    if selected.windows(2).any(|pair| {
        pair[0]
            .instance_id
            .eq_ignore_ascii_case(&pair[1].instance_id)
    }) {
        return Err("P1:35 system-device inventory has duplicate instance IDs".into());
    }
    Ok(Some(ChipsetInventory {
        vendor,
        records: selected,
    }))
}

fn chipset_vendor_bindings(
    hardware_ids: &[String],
    compatible_ids: &[String],
) -> Result<std::collections::BTreeSet<ChipsetVendor>, String> {
    if hardware_ids.is_empty() && compatible_ids.is_empty() {
        return Err("P1:35 system device has no hardware or compatible IDs".into());
    }
    hardware_ids
        .iter()
        .chain(compatible_ids)
        .map(|id| chipset_vendor_from_pci_id(id))
        .collect::<Result<std::collections::BTreeSet<_>, _>>()
        .map(|set| set.into_iter().flatten().collect())
}

fn chipset_vendor_from_pci_id(id: &str) -> Result<Option<ChipsetVendor>, String> {
    let id = checked_device_text(id, "device ID")?;
    let upper = id.to_ascii_uppercase();
    if !upper.starts_with("PCI\\") {
        return Ok(None);
    }
    let vendor = upper
        .split(['\\', '&'])
        .find_map(|component| component.strip_prefix("VEN_"));
    let Some(vendor) = vendor else {
        return Ok(None);
    };
    if vendor.len() != 4 || !vendor.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("P1:35 PCI device ID has a malformed VEN_ component".into());
    }
    Ok(match vendor {
        "1022" => Some(ChipsetVendor::Amd),
        "8086" => Some(ChipsetVendor::Intel),
        _ => None,
    })
}

fn validate_chipset_record(
    vendor: ChipsetVendor,
    record: RawChipsetDriverRecord,
) -> Result<ChipsetDriverRecord, String> {
    if !record.instance_id.to_ascii_uppercase().starts_with("PCI\\") {
        return Err("P1:35 chipset record has a non-PCI instance ID".into());
    }
    if record.hardware_ids.is_empty() {
        return Err("P1:35 chipset record has no hardware IDs".into());
    }
    for id in record.hardware_ids.iter().chain(&record.compatible_ids) {
        let _ = checked_device_text(id, "device ID")?;
    }
    let instance_id = checked_device_text(&record.instance_id, "instance ID")?;
    let inf_path = checked_device_text(&record.inf_path, "installed INF")?;
    if !valid_vendor_package_inf(&inf_path) {
        return Err("P1:35 installed INF is not an OEM vendor-package filename".into());
    }
    let provider = checked_device_text(&record.provider, "driver provider")?;
    if !provider_matches_vendor(vendor, &provider) {
        return Err("P1:35 driver provider does not match the exact CPU/PCI vendor binding".into());
    }
    if record.driver_date_filetime == 0 {
        return Err("P1:35 driver date is malformed".into());
    }
    let driver_version = checked_device_text(&record.driver_version, "driver version")?;
    if !valid_driver_version(&driver_version) {
        return Err("P1:35 driver version is not a bounded numeric dotted version".into());
    }
    Ok(ChipsetDriverRecord {
        instance_id,
        hardware_ids: record.hardware_ids,
        compatible_ids: record.compatible_ids,
        inf_path,
        provider,
        driver_version,
        driver_date_filetime: record.driver_date_filetime,
    })
}

fn checked_device_text(value: &str, label: &str) -> Result<String, String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 4096
        || value
            .chars()
            .any(|character| character.is_control() || character == '\0')
    {
        return Err(format!("P1:35 {label} is malformed"));
    }
    Ok(value.to_owned())
}

fn valid_vendor_package_inf(value: &str) -> bool {
    let value = value.as_bytes();
    value.len() > 7
        && value[..3].eq_ignore_ascii_case(b"oem")
        && value[value.len() - 4..].eq_ignore_ascii_case(b".inf")
        && value[3..value.len() - 4]
            .iter()
            .all(|byte| byte.is_ascii_digit())
}

fn provider_matches_vendor(vendor: ChipsetVendor, provider: &str) -> bool {
    let provider = provider.to_ascii_lowercase();
    match vendor {
        ChipsetVendor::Amd => provider == "amd" || provider.contains("advanced micro devices"),
        ChipsetVendor::Intel => provider == "intel" || provider.contains("intel corporation"),
    }
}

fn valid_driver_version(value: &str) -> bool {
    value.len() <= 32
        && value.split('.').count() == 4
        && value.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 5
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && part.parse::<u32>().is_ok()
        })
}

fn detected_chipset_vendor() -> Result<ChipsetVendor, String> {
    let value = registry_read_exact(&RegistryChange {
        hive: Hive::LocalMachine,
        key: "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0",
        name: "VendorIdentifier",
        value: RegValue::Dword(0),
    })?;
    match value {
        Some(RegValue::String("AuthenticAMD")) => Ok(ChipsetVendor::Amd),
        Some(RegValue::String("GenuineIntel")) => Ok(ChipsetVendor::Intel),
        Some(RegValue::String(_)) | Some(RegValue::Dword(_)) | Some(RegValue::Binary(_)) | None => {
            Err("P1:35 CPU vendor is unknown or malformed".into())
        }
    }
}

fn capture_chipset_inventory() -> Result<Option<ChipsetInventory>, String> {
    let vendor = detected_chipset_vendor()?;
    chipset_inventory_from_raw(vendor, enumerate_chipset_driver_records()?)
}

fn verify_chipset_inventory(captured: &Option<ChipsetInventory>) -> Result<(), String> {
    let observed = capture_chipset_inventory()?;
    if chipset_inventory_matches(captured, &observed) {
        Ok(())
    } else {
        Err("P1:35 installed chipset-driver records changed after inspection".into())
    }
}

fn chipset_inventory_matches(
    captured: &Option<ChipsetInventory>,
    observed: &Option<ChipsetInventory>,
) -> bool {
    captured == observed
}

#[cfg(not(windows))]
fn enumerate_chipset_driver_records() -> Result<Vec<RawChipsetDriverRecord>, String> {
    Err("P1:35 SetupAPI observation is supported only on Windows".into())
}
