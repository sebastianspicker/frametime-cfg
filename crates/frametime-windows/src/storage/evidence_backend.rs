use crate::*;
pub(crate) fn backend_evidence_requirement(operation: Operation) -> EvidenceRequirement {
    descriptor_for(&operation.step)
        .map(|descriptor| descriptor.evidence_requirement)
        .unwrap_or(EvidenceRequirement::None)
}

pub(crate) fn capture_action_evidence(
    operation: Operation,
    msi_batches: Option<&[MsiDeviceBatch]>,
    nic_affinity: Option<&NicAffinityBinding>,
) -> Result<ObservationReceipt, String> {
    let key = operation.step.id.progress_key();
    let subject = match key.as_str() {
        "P1:18" => return capture_driver_cleanup_preparation_receipt(),
        "P1:20" => return capture_nvidia_drs_preparation_receipt(),
        "P1:21" => ObservationSubject::MsiDeviceSet {
            devices: sorted_msi_devices(
                msi_batches.ok_or("P1:21 evidence capture requires inspected MSI devices")?,
            ),
        },
        "P1:22" => {
            let binding = nic_affinity
                .ok_or("P1:22 evidence capture requires an inspected NIC affinity proposal")?;
            ObservationSubject::NicAffinityProposal {
                adapter: Box::new(binding.adapter.clone()),
                processor_group: 0,
                logical_processor_count: u16::from(binding.final_logical_processor) + 1,
                target_processor: u16::from(binding.final_logical_processor),
                assignment_mask: u64::from_le_bytes(binding.assignment_set_override),
            }
        }
        _ => {
            return Err(format!(
                "{key} does not expose a live prerequisite-evidence capture adapter"
            ));
        }
    };
    ObservationReceipt::new(timestamp(), None, None, subject)
        .map_err(|error| format!("capture {key} prerequisite evidence: {error}"))
}

pub(crate) fn verify_persisted_observation(
    trusted: &TrustedWorkDir,
    operation: Operation,
    receipt: &ObservationReceipt,
) -> Result<(), String> {
    let key = operation.step.id.progress_key();
    receipt
        .validate_for(&key)
        .map_err(|error| format!("validate {key} prerequisite evidence: {error}"))?;
    if load_observation_receipt(trusted, &key)?.as_ref() != Some(receipt) {
        return Err(format!(
            "{key} persisted prerequisite evidence does not match the captured receipt"
        ));
    }
    Ok(())
}

pub(crate) fn verify_preparation_observation(
    operation: Operation,
    receipt: &ObservationReceipt,
) -> Result<(), String> {
    let key = operation.step.id.progress_key();
    match (&*key, &receipt.subject) {
        (
            "P1:18",
            ObservationSubject::DriverCleanupPreparation {
                target_gpu,
                installed_packages,
            },
        ) => {
            let (current_target, current_packages) = inspect_driver_cleanup_preparation()?;
            if same_driver_cleanup_preparation(
                target_gpu,
                installed_packages,
                &current_target,
                &current_packages,
            ) {
                Ok(())
            } else {
                Err("P1:18 GPU or installed package binding changed after persistence".into())
            }
        }
        ("P1:20", ObservationSubject::NvidiaDrsPreparation { .. }) => {
            verify_selected_preparation(&key, receipt, PreparationCheck::Persisted)
        }
        ("P1:21", ObservationSubject::MsiDeviceSet { .. })
        | ("P1:22", ObservationSubject::NicAffinityProposal { .. }) => {
            verify_selected_preparation(&key, receipt, PreparationCheck::Persisted)
        }
        _ => Err(format!(
            "{key} prerequisite evidence has the wrong typed subject"
        )),
    }
}

pub(crate) fn capture_driver_cleanup_preparation_receipt() -> Result<ObservationReceipt, String> {
    let (target_gpu, installed_packages) = inspect_driver_cleanup_preparation()?;
    ObservationReceipt::new(
        timestamp(),
        None,
        None,
        ObservationSubject::DriverCleanupPreparation {
            target_gpu,
            installed_packages,
        },
    )
    .map_err(|error| format!("capture P1:18 prerequisite evidence: {error}"))
}

pub(crate) fn inspect_driver_cleanup_preparation() -> Result<
    (
        frametime_domain::binding::PciDeviceBinding,
        Vec<frametime_domain::binding::PciDeviceBinding>,
    ),
    String,
> {
    #[cfg(windows)]
    {
        WindowsDriverInspection::native()
            .inspect_driver_cleanup_preparation()
            .map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        Err("P1:18 requires Windows SetupAPI display-driver inspection".into())
    }
}

pub(crate) fn inspect_driver_cleanup_preparation_action() -> Result<Inspection, String> {
    let (target_gpu, _) = inspect_driver_cleanup_preparation()?;
    Ok(driver_cleanup_preparation_inspection(&target_gpu))
}

pub(crate) fn driver_cleanup_preparation_inspection(
    target_gpu: &frametime_domain::binding::PciDeviceBinding,
) -> Inspection {
    if target_gpu.vendor_id == 0x10de {
        Inspection::Satisfied
    } else {
        Inspection::Inapplicable
    }
}

pub(crate) fn same_driver_cleanup_preparation(
    target: &frametime_domain::binding::PciDeviceBinding,
    packages: &[frametime_domain::binding::PciDeviceBinding],
    current_target: &frametime_domain::binding::PciDeviceBinding,
    current_packages: &[frametime_domain::binding::PciDeviceBinding],
) -> bool {
    same_driver_cleanup_binding(target, current_target)
        && packages.len() == current_packages.len()
        && packages
            .iter()
            .zip(current_packages)
            .all(|(expected, current)| same_driver_cleanup_binding(expected, current))
}

pub(crate) fn same_driver_cleanup_binding(
    expected: &frametime_domain::binding::PciDeviceBinding,
    current: &frametime_domain::binding::PciDeviceBinding,
) -> bool {
    expected.same_pnp_device(current)
        && expected.driver_provider == current.driver_provider
        && expected.driver_version == current.driver_version
        && expected
            .published_inf
            .eq_ignore_ascii_case(&current.published_inf)
}

pub(crate) fn require_stored_preparation(
    trusted: &TrustedWorkDir,
    step: &str,
) -> Result<(), String> {
    let receipt = load_observation_receipt(trusted, step)?
        .ok_or_else(|| format!("{step} durable prerequisite evidence is missing"))?;
    receipt
        .validate_for(step)
        .map_err(|error| format!("validate {step} prerequisite evidence: {error}"))?;
    verify_selected_preparation(step, &receipt, PreparationCheck::Durable)
}

#[derive(Clone, Copy)]
enum PreparationCheck {
    Persisted,
    Durable,
}

fn verify_selected_preparation(
    step: &str,
    receipt: &ObservationReceipt,
    check: PreparationCheck,
) -> Result<(), String> {
    let mismatch = match (step, &receipt.subject) {
        ("P1:20", ObservationSubject::NvidiaDrsPreparation { .. }) => {
            !nvidia_drs_preparation_matches(receipt)?
        }
        ("P1:21", ObservationSubject::MsiDeviceSet { devices }) => {
            let compare = match check {
                PreparationCheck::Persisted => same_device_observation_set,
                PreparationCheck::Durable => same_stable_device_set,
            };
            !msi_device_set_matches(devices, compare)?
        }
        (
            "P1:22",
            ObservationSubject::NicAffinityProposal {
                adapter,
                processor_group,
                logical_processor_count,
                target_processor,
                assignment_mask,
            },
        ) => !nic_affinity_proposal_matches(
            adapter,
            *processor_group,
            *logical_processor_count,
            *target_processor,
            *assignment_mask,
        )?,
        _ => {
            let qualifier = match check {
                PreparationCheck::Persisted => "prerequisite evidence",
                PreparationCheck::Durable => "durable prerequisite evidence",
            };
            return Err(format!("{step} {qualifier} has the wrong typed subject"));
        }
    };
    if !mismatch {
        return Ok(());
    }
    let error = match (step, check) {
        ("P1:20", PreparationCheck::Persisted) => {
            "P1:20 NVIDIA DRS evidence changed after persistence"
        }
        ("P1:21", PreparationCheck::Persisted) => {
            "P1:21 PCI device evidence changed after persistence"
        }
        ("P1:22", PreparationCheck::Persisted) => {
            "P1:22 NIC or processor-topology evidence changed after persistence"
        }
        ("P1:20", PreparationCheck::Durable) => {
            "P1:20 durable NVIDIA DRS evidence changed before P3:4"
        }
        ("P1:21", PreparationCheck::Durable) => {
            "P1:21 durable PCI device identities changed before P3:2"
        }
        ("P1:22", PreparationCheck::Durable) => {
            "P1:22 durable NIC or processor-topology evidence changed before P3:3"
        }
        _ => unreachable!("typed preparation check has a supported step"),
    };
    Err(error.into())
}

fn nvidia_drs_preparation_matches(receipt: &ObservationReceipt) -> Result<bool, String> {
    Ok(nvidia_drs_subjects_match(
        &receipt.subject,
        &capture_nvidia_drs_preparation_receipt()?.subject,
    ))
}

/// Compares the GPU by stable PnP identity: `observed_at_utc` differs on every capture.
fn nvidia_drs_subjects_match(expected: &ObservationSubject, current: &ObservationSubject) -> bool {
    match (expected, current) {
        (
            ObservationSubject::NvidiaDrsPreparation {
                target_gpu: expected_gpu,
                driver_version: expected_driver,
                nvapi_module_sha256: expected_sha,
                nvapi_interface_version: expected_interface,
                profile_name: expected_profile,
                application_name: expected_application,
            },
            ObservationSubject::NvidiaDrsPreparation {
                target_gpu: current_gpu,
                driver_version: current_driver,
                nvapi_module_sha256: current_sha,
                nvapi_interface_version: current_interface,
                profile_name: current_profile,
                application_name: current_application,
            },
        ) => {
            expected_gpu.same_pnp_device(current_gpu)
                && expected_driver == current_driver
                && expected_sha == current_sha
                && expected_interface == current_interface
                && expected_profile == current_profile
                && expected_application == current_application
        }
        _ => false,
    }
}

fn msi_device_set_matches(
    devices: &[frametime_domain::binding::PciDeviceBinding],
    compare: fn(
        &[frametime_domain::binding::PciDeviceBinding],
        &[frametime_domain::binding::PciDeviceBinding],
    ) -> bool,
) -> Result<bool, String> {
    let current = sorted_msi_devices(&discover_native_msi_batches()?);
    Ok(compare(devices, &current))
}

fn nic_affinity_proposal_matches(
    adapter: &frametime_domain::binding::NetworkAdapterBinding,
    processor_group: u16,
    logical_processor_count: u16,
    target_processor: u16,
    assignment_mask: u64,
) -> Result<bool, String> {
    Ok(same_nic_affinity_proposal(
        adapter,
        processor_group,
        logical_processor_count,
        target_processor,
        assignment_mask,
        &discover_native_nic_affinity()?,
    ))
}

fn same_nic_affinity_proposal(
    expected: &frametime_domain::binding::NetworkAdapterBinding,
    processor_group: u16,
    logical_processor_count: u16,
    target_processor: u16,
    assignment_mask: u64,
    observed: &NicAffinityBinding,
) -> bool {
    processor_group == 0
        && logical_processor_count == u16::from(observed.final_logical_processor) + 1
        && target_processor == u16::from(observed.final_logical_processor)
        && assignment_mask == u64::from_le_bytes(observed.assignment_set_override)
        && expected.device.same_pnp_device(&observed.adapter.device)
        && expected
            .adapter_name
            .eq_ignore_ascii_case(&observed.adapter.adapter_name)
        && expected
            .interface_guid
            .eq_ignore_ascii_case(&observed.adapter.interface_guid)
        && expected.interface_luid == observed.adapter.interface_luid
        && expected.interface_index == observed.adapter.interface_index
        && expected.physical_address == observed.adapter.physical_address
}

pub(crate) fn sorted_msi_devices(
    batches: &[MsiDeviceBatch],
) -> Vec<frametime_domain::binding::PciDeviceBinding> {
    let mut devices = batches
        .iter()
        .map(|batch| batch.device.clone())
        .collect::<Vec<_>>();
    devices.sort_by_key(|device| device.instance_id.to_ascii_uppercase());
    devices.dedup_by(|left, right| left.instance_id.eq_ignore_ascii_case(&right.instance_id));
    devices
}

pub(crate) fn same_device_observation_set(
    left: &[frametime_domain::binding::PciDeviceBinding],
    right: &[frametime_domain::binding::PciDeviceBinding],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.same_pnp_device(right)
                && left.driver_provider == right.driver_provider
                && left.driver_version == right.driver_version
                && left.published_inf == right.published_inf
        })
}

pub(crate) fn same_stable_device_set(
    left: &[frametime_domain::binding::PciDeviceBinding],
    right: &[frametime_domain::binding::PciDeviceBinding],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.same_pnp_device(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(observed_at_utc: &str) -> frametime_domain::binding::PciDeviceBinding {
        frametime_domain::binding::PciDeviceBinding {
            schema_version: frametime_domain::binding::NATIVE_BINDING_SCHEMA_VERSION,
            instance_id: "PCI\\VEN_10DE&DEV_2684&SUBSYS_16F110DE&REV_A1\\4&1&0&0008".into(),
            container_id: "{00000000-0000-0000-0000-000000000000}".into(),
            class_guid: "{4d36e968-e325-11ce-bfc1-08002be10318}".into(),
            vendor_id: 0x10de,
            device_id: 0x2684,
            subsystem_vendor_id: 0x10de,
            subsystem_device_id: 0x16f1,
            revision_id: 0xa1,
            driver_provider: "NVIDIA".into(),
            driver_version: "32.0.15.1".into(),
            published_inf: "oem1.inf".into(),
            observed_at_utc: observed_at_utc.into(),
            unknown: Default::default(),
        }
    }

    fn subject(observed_at_utc: &str, driver_version: &str) -> ObservationSubject {
        ObservationSubject::NvidiaDrsPreparation {
            target_gpu: gpu(observed_at_utc),
            driver_version: driver_version.into(),
            nvapi_module_sha256: "00".repeat(32),
            nvapi_interface_version: "1".into(),
            profile_name: "Counter-Strike 2".into(),
            application_name: "cs2.exe".into(),
        }
    }

    #[test]
    fn drs_subject_ignores_observation_time_only() {
        let expected = subject("2026-11-01T10:00:00", "1.0");
        assert!(nvidia_drs_subjects_match(
            &expected,
            &subject("2026-11-01T11:30:00", "1.0")
        ));
        assert!(!nvidia_drs_subjects_match(
            &expected,
            &subject("2026-11-01T10:00:00", "2.0")
        ));
    }
}
