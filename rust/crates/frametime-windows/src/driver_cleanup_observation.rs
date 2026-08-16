//! P1:18 canonical GPU driver-cleanup observation.

use frametime_driver::{AdapterFailure, ExactGpuIdentity};

use crate::PciDeviceClass;
use frametime_core::PciDeviceBinding;

pub(crate) fn from_bindings(
    bindings: Vec<(PciDeviceClass, PciDeviceBinding)>,
    exact_gpu: impl Fn(&PciDeviceBinding) -> Result<ExactGpuIdentity, AdapterFailure>,
) -> Result<(PciDeviceBinding, Vec<PciDeviceBinding>), AdapterFailure> {
    let mut identities = Vec::new();
    for (class, binding) in &bindings {
        if *class != PciDeviceClass::Display {
            continue;
        }
        let identity = exact_gpu(binding)?;
        if !identities.contains(&identity) {
            identities.push(identity);
        }
    }
    let target = match identities.len() {
        1 => identities.pop().expect("one exact GPU identity"),
        0 => {
            return Err(AdapterFailure {
                operation: "inspect GPU",
                reason: "no active status-OK PCI display GPU was found".into(),
            });
        }
        _ => {
            return Err(AdapterFailure {
                operation: "inspect GPU",
                reason: "multiple display GPUs require an explicit PnP selection".into(),
            });
        }
    };

    let mut installed_packages = bindings
        .into_iter()
        .filter_map(|(class, binding)| {
            (class == PciDeviceClass::Display && exact_gpu(&binding).ok().as_ref() == Some(&target))
                .then_some(binding)
        })
        .collect::<Vec<_>>();
    installed_packages.sort_by_key(|binding| binding.instance_id.to_ascii_uppercase());
    if installed_packages.len() != 1 {
        return Err(AdapterFailure {
            operation: "inspect GPU",
            reason: "P1:18 requires one canonical display PnP binding".into(),
        });
    }
    Ok((installed_packages[0].clone(), installed_packages))
}
