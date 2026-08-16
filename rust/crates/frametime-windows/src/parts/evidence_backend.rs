fn backend_evidence_requirement(operation: Operation) -> EvidenceRequirement {
    descriptor_for(operation.step.phase as u8, operation.step.number)
        .map(|descriptor| descriptor.evidence_requirement)
        .unwrap_or(EvidenceRequirement::None)
}

fn capture_action_evidence(
    operation: Operation,
    msi_batches: Option<&[MsiDeviceBatch]>,
    nic_affinity: Option<&NicAffinityBinding>,
) -> Result<ObservationReceipt, String> {
    let key = Progress::key(operation.step.phase as u8, operation.step.number);
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

fn verify_persisted_observation(
    trusted: &TrustedWorkDir,
    operation: Operation,
    receipt: &ObservationReceipt,
) -> Result<(), String> {
    let key = Progress::key(operation.step.phase as u8, operation.step.number);
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

fn verify_preparation_observation(
    operation: Operation,
    receipt: &ObservationReceipt,
) -> Result<(), String> {
    let key = Progress::key(operation.step.phase as u8, operation.step.number);
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
            let current = capture_nvidia_drs_preparation_receipt()?;
            if current.subject == receipt.subject {
                Ok(())
            } else {
                Err("P1:20 NVIDIA DRS evidence changed after persistence".into())
            }
        }
        ("P1:21", ObservationSubject::MsiDeviceSet { devices }) => {
            let current = sorted_msi_devices(&discover_native_msi_batches()?);
            if same_device_observation_set(devices, &current) {
                Ok(())
            } else {
                Err("P1:21 PCI device evidence changed after persistence".into())
            }
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
        ) => {
            let current = discover_native_nic_affinity()?;
            let same = *processor_group == 0
                && *logical_processor_count == u16::from(current.final_logical_processor) + 1
                && *target_processor == u16::from(current.final_logical_processor)
                && *assignment_mask == u64::from_le_bytes(current.assignment_set_override)
                && adapter.device.same_pnp_device(&current.adapter.device)
                && adapter
                    .adapter_name
                    .eq_ignore_ascii_case(&current.adapter.adapter_name)
                && adapter
                    .interface_guid
                    .eq_ignore_ascii_case(&current.adapter.interface_guid)
                && adapter.interface_luid == current.adapter.interface_luid
                && adapter.interface_index == current.adapter.interface_index
                && adapter.physical_address == current.adapter.physical_address;
            if same {
                Ok(())
            } else {
                Err("P1:22 NIC or processor-topology evidence changed after persistence".into())
            }
        }
        _ => Err(format!(
            "{key} prerequisite evidence has the wrong typed subject"
        )),
    }
}

fn capture_driver_cleanup_preparation_receipt() -> Result<ObservationReceipt, String> {
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

fn inspect_driver_cleanup_preparation(
) -> Result<(frametime_core::PciDeviceBinding, Vec<frametime_core::PciDeviceBinding>), String> {
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

fn inspect_driver_cleanup_preparation_action() -> Result<Inspection, String> {
    let (target_gpu, _) = inspect_driver_cleanup_preparation()?;
    Ok(driver_cleanup_preparation_inspection(&target_gpu))
}

fn driver_cleanup_preparation_inspection(
    target_gpu: &frametime_core::PciDeviceBinding,
) -> Inspection {
    if target_gpu.vendor_id == 0x10de {
        Inspection::Satisfied
    } else {
        Inspection::Inapplicable
    }
}

fn same_driver_cleanup_preparation(
    target: &frametime_core::PciDeviceBinding,
    packages: &[frametime_core::PciDeviceBinding],
    current_target: &frametime_core::PciDeviceBinding,
    current_packages: &[frametime_core::PciDeviceBinding],
) -> bool {
    same_driver_cleanup_binding(target, current_target)
        && packages.len() == current_packages.len()
        && packages
            .iter()
            .zip(current_packages)
            .all(|(expected, current)| same_driver_cleanup_binding(expected, current))
}

fn same_driver_cleanup_binding(
    expected: &frametime_core::PciDeviceBinding,
    current: &frametime_core::PciDeviceBinding,
) -> bool {
    expected.same_pnp_device(current)
        && expected.driver_provider == current.driver_provider
        && expected.driver_version == current.driver_version
        && expected
            .published_inf
            .eq_ignore_ascii_case(&current.published_inf)
}

fn require_stored_preparation(trusted: &TrustedWorkDir, step: &str) -> Result<(), String> {
    let receipt = load_observation_receipt(trusted, step)?
        .ok_or_else(|| format!("{step} durable prerequisite evidence is missing"))?;
    receipt
        .validate_for(step)
        .map_err(|error| format!("validate {step} prerequisite evidence: {error}"))?;
    match (step, &receipt.subject) {
        ("P1:20", ObservationSubject::NvidiaDrsPreparation { .. }) => {
            let current = capture_nvidia_drs_preparation_receipt()?;
            if current.subject == receipt.subject {
                Ok(())
            } else {
                Err("P1:20 durable NVIDIA DRS evidence changed before P3:4".into())
            }
        }
        ("P1:21", ObservationSubject::MsiDeviceSet { devices }) => {
            let current = sorted_msi_devices(&discover_native_msi_batches()?);
            if same_stable_device_set(devices, &current) {
                Ok(())
            } else {
                Err("P1:21 durable PCI device identities changed before P3:2".into())
            }
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
        ) => {
            let current = discover_native_nic_affinity()?;
            let same = *processor_group == 0
                && *logical_processor_count == u16::from(current.final_logical_processor) + 1
                && *target_processor == u16::from(current.final_logical_processor)
                && *assignment_mask == u64::from_le_bytes(current.assignment_set_override)
                && adapter.device.same_pnp_device(&current.adapter.device)
                && adapter
                    .adapter_name
                    .eq_ignore_ascii_case(&current.adapter.adapter_name)
                && adapter
                    .interface_guid
                    .eq_ignore_ascii_case(&current.adapter.interface_guid)
                && adapter.interface_luid == current.adapter.interface_luid
                && adapter.interface_index == current.adapter.interface_index
                && adapter.physical_address == current.adapter.physical_address;
            if same {
                Ok(())
            } else {
                Err("P1:22 durable NIC or processor-topology evidence changed before P3:3".into())
            }
        }
        _ => Err(format!(
            "{step} durable prerequisite evidence has the wrong typed subject"
        )),
    }
}

fn sorted_msi_devices(batches: &[MsiDeviceBatch]) -> Vec<frametime_core::PciDeviceBinding> {
    let mut devices = batches
        .iter()
        .map(|batch| batch.device.clone())
        .collect::<Vec<_>>();
    devices.sort_by_key(|device| device.instance_id.to_ascii_uppercase());
    devices.dedup_by(|left, right| left.instance_id.eq_ignore_ascii_case(&right.instance_id));
    devices
}

fn same_device_observation_set(
    left: &[frametime_core::PciDeviceBinding],
    right: &[frametime_core::PciDeviceBinding],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.same_pnp_device(right)
                && left.driver_provider == right.driver_provider
                && left.driver_version == right.driver_version
                && left.published_inf == right.published_inf
        })
}

fn same_stable_device_set(
    left: &[frametime_core::PciDeviceBinding],
    right: &[frametime_core::PciDeviceBinding],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.same_pnp_device(right))
}
