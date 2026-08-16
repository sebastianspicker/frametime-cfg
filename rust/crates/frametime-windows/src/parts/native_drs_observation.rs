fn inspect_nvidia_drs_preparation() -> Result<Inspection, String> {
    #[cfg(windows)]
    {
        let mut api = NativeNvapiDrs::load().map_err(|error| error.to_string())?;
        prepare_cs2_profile(&mut api).map_err(|error| error.to_string())?;
        Ok(Inspection::Satisfied)
    }
    #[cfg(not(windows))]
    {
        Err("P1:20 requires NVIDIA NVAPI on Windows".into())
    }
}

fn capture_nvidia_drs_preparation_receipt() -> Result<ObservationReceipt, String> {
    #[cfg(windows)]
    {
        let target_gpu = nvidia_display_binding()?;
        let mut api = NativeNvapiDrs::load().map_err(|error| error.to_string())?;
        let profile_name = match prepare_cs2_profile(&mut api).map_err(|error| error.to_string())? {
            DrsPreparation::ExistingProfile { profile } => profile,
            DrsPreparation::DedicatedProfileWillBeCreated => "Counter-Strike 2".into(),
        };
        ObservationReceipt::new(
            timestamp(),
            None,
            None,
            ObservationSubject::NvidiaDrsPreparation {
                driver_version: target_gpu.driver_version.clone(),
                target_gpu,
                nvapi_module_sha256: api.module_sha256().into(),
                nvapi_interface_version: NativeNvapiDrs::interface_version().into(),
                profile_name,
                application_name: "cs2.exe".into(),
            },
        )
        .map_err(|error| format!("capture P1:20 prerequisite evidence: {error}"))
    }
    #[cfg(not(windows))]
    {
        Err("P1:20 requires NVIDIA NVAPI on Windows".into())
    }
}

#[cfg(windows)]
fn nvidia_display_binding() -> Result<frametime_core::PciDeviceBinding, String> {
    let candidates = enumerate_present_status_ok_pci(&WindowsSetupApiEnumerator)
        .map_err(|error| format!("enumerate NVIDIA display adapters: {error}"))?
        .into_iter()
        .filter_map(|(class, binding)| {
            (class == PciDeviceClass::Display && binding.vendor_id == 0x10de).then_some(binding)
        })
        .collect::<Vec<_>>();
    match candidates.len() {
        1 => Ok(candidates
            .into_iter()
            .next()
            .expect("one NVIDIA display adapter")),
        0 => Err("P1:20 requires one present NVIDIA display adapter".into()),
        _ => Err("P1:20 refuses an ambiguous NVIDIA display-adapter set".into()),
    }
}
