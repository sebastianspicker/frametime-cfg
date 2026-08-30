use super::OemDriverPackage;
pub use driver_foundry_common::pnputil::parse_pnputil_enum_drivers;
use std::process::Command;

/// Run `pnputil /enum-drivers` and parse OEM packages.
pub fn enum_oem_driver_packages() -> Vec<OemDriverPackage> {
    try_enum_oem_driver_packages().unwrap_or_default()
}

pub(crate) fn try_enum_oem_driver_packages() -> Result<Vec<OemDriverPackage>, String> {
    Command::new("pnputil")
        .args(["/enum-drivers"])
        .output()
        .map_err(|error| format!("pnputil /enum-drivers: {error}"))
        .and_then(|output| {
            if output.status.success() {
                Ok(parse_pnputil_enum_drivers(&String::from_utf8_lossy(
                    &output.stdout,
                )))
            } else {
                Err(format!(
                    "pnputil /enum-drivers exited with {}",
                    output.status
                ))
            }
        })
}

/// Match OEM packages to a vendor folder / PCI VEN id.
pub fn filter_packages_for_vendor(
    packages: &[OemDriverPackage],
    vendor_folder: &str,
    ven_id: &str,
) -> Vec<OemDriverPackage> {
    let mut needles = vec![
        vendor_folder.to_ascii_lowercase(),
        ven_id.to_ascii_lowercase(),
        ven_id.to_ascii_lowercase().replace("ven_", ""),
    ];
    match vendor_folder.to_ascii_uppercase().as_str() {
        "NVIDIA" => needles.extend(["nvidia", "nv_", "nvdia", "10de"].map(String::from)),
        "AMD" => {
            needles.extend(["amd", "ati", "radeon", "1002", "advanced micro"].map(String::from))
        }
        "INTEL" => needles.extend(["intel", "8086", "igfx"].map(String::from)),
        "LISUAN" => needles.extend(["lisuan", "4c54"].map(String::from)),
        "REALTEK" => needles.extend(["realtek", "10ec", "hdudio"].map(String::from)),
        _ => {}
    }
    packages
        .iter()
        .filter(|package| {
            let details = format!(
                "{} {} {} {}",
                package.published_name, package.original_name, package.provider, package.class_name
            )
            .to_ascii_lowercase();
            needles.iter().any(|needle| details.contains(needle))
                && is_cleanup_class(vendor_folder, package)
        })
        .cloned()
        .collect()
}

fn is_cleanup_class(vendor_folder: &str, package: &OemDriverPackage) -> bool {
    let class = package.class_name.to_ascii_lowercase();
    let inf = package.original_name.to_ascii_lowercase();
    let display_class = ["display", "graphics", "video"]
        .iter()
        .any(|needle| class.contains(needle));
    let audio_class = ["media", "audio", "sound"]
        .iter()
        .any(|needle| class.contains(needle));
    match vendor_folder.to_ascii_uppercase().as_str() {
        "NVIDIA" => {
            display_class
                || ["nv_disp", "nvd", "nv4"]
                    .iter()
                    .any(|needle| inf.contains(needle))
        }
        "AMD" => {
            display_class
                || ["amdkmd", "ati", "radeon"]
                    .iter()
                    .any(|needle| inf.contains(needle))
        }
        "INTEL" => display_class || ["igdl", "igfx"].iter().any(|needle| inf.contains(needle)),
        "LISUAN" => display_class || inf.contains("lisuan"),
        "REALTEK" => {
            audio_class
                || ["hdaudio", "rtkvhd", "realtek_audio"]
                    .iter()
                    .any(|needle| inf.contains(needle))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(provider: &str, original_name: &str, class_name: &str) -> OemDriverPackage {
        OemDriverPackage {
            published_name: "oem1.inf".into(),
            original_name: original_name.into(),
            provider: provider.into(),
            class_name: class_name.into(),
        }
    }

    #[test]
    fn keeps_only_vendor_packages_in_cleanup_classes() {
        let intel = vec![
            package("Intel", "netwtw.inf", "Net"),
            package("Intel", "igdlh64.inf", "Display adapters"),
        ];
        assert_eq!(
            filter_packages_for_vendor(&intel, "INTEL", "VEN_8086").len(),
            1
        );
        let realtek = vec![
            package("Realtek", "rt640x64.inf", "Net"),
            package("Realtek", "hdaudio.inf", "Media"),
        ];
        assert_eq!(
            filter_packages_for_vendor(&realtek, "REALTEK", "VEN_10EC").len(),
            1
        );
    }
}
