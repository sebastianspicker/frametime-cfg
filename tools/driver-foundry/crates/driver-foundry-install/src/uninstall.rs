//! PnPUtil uninstall plan generation and guarded deletion execution.

use std::fs;
use std::path::Path;

use crate::pipeline::note;
use crate::InstallError;
pub(crate) use driver_foundry_common::pnputil::{
    parse_pnputil_enum_drivers as parse_pnputil_enum_drivers_simple,
    OemDriverPackage as OemDriverRow,
};

pub(crate) fn run_uninstall_stage(
    work: &Path,
    dry_run: bool,
    log: &mut Vec<String>,
) -> Result<(), InstallError> {
    let (enum_text, enum_ok) = enumerate_drivers(dry_run, log);
    let packages = parse_pnputil_enum_drivers_simple(&enum_text);
    let matched = filter_display_gpu_oems(&packages);
    let plan_path = write_uninstall_plan(work, enum_ok, &packages, &matched)?;
    note_enumeration(log, enum_ok, &packages, &matched, &plan_path);
    require_live_enumeration(dry_run, enum_ok)?;
    complete_uninstall(dry_run, &matched, &plan_path, log)
}

fn enumerate_drivers(dry_run: bool, log: &mut Vec<String>) -> (String, bool) {
    if dry_run {
        return (String::new(), false);
    }
    match std::process::Command::new("pnputil")
        .args(["/enum-drivers"])
        .output()
    {
        Ok(output) => (
            String::from_utf8_lossy(&output.stdout).to_string(),
            output.status.success(),
        ),
        Err(error) => {
            note(
                log,
                "S5d-UninstallDrivers",
                &format!("pnputil /enum-drivers failed to spawn: {error}"),
            );
            (String::new(), false)
        }
    }
}

fn write_uninstall_plan(
    work: &Path,
    enum_ok: bool,
    packages: &[OemDriverRow],
    matched: &[OemDriverRow],
) -> Result<std::path::PathBuf, InstallError> {
    let path = work.join("uninstall-drivers-plan.txt");
    let mut plan = String::from(
        "# uninstall-drivers plan (Driver Foundry)\n\
         # pnputil /enum-drivers → match NVIDIA/AMD/Intel display-related OEM packages\n\
         # Live mass-delete only when DFOUNDRY_UNINSTALL_DELETE=1\n\n",
    );
    plan.push_str(&format!(
        "# enum_ok={enum_ok} total_oem={} matched_display_gpu={}\n\n",
        packages.len(),
        matched.len()
    ));
    if matched.is_empty() {
        plan.push_str("# (no NVIDIA/AMD/Intel display-related OEM packages matched)\n");
        plan.push_str("pnputil /enum-drivers\n");
        plan.push_str("# Would delete: pnputil /delete-driver oemXX.inf /uninstall /force\n");
    } else {
        append_matched_commands(&mut plan, matched);
    }
    fs::write(&path, plan)?;
    Ok(path)
}

fn append_matched_commands(plan: &mut String, matched: &[OemDriverRow]) {
    for package in matched {
        plan.push_str(&format!(
            "# provider={} class={} original={}\n",
            package.provider, package.class_name, package.original_name
        ));
        plan.push_str(&format!(
            "pnputil /delete-driver {} /uninstall /force\n",
            package.published_name
        ));
    }
}

fn note_enumeration(
    log: &mut Vec<String>,
    enum_ok: bool,
    packages: &[OemDriverRow],
    matched: &[OemDriverRow],
    plan_path: &Path,
) {
    note(
        log,
        "S5d-UninstallDrivers",
        &format!(
            "pnputil enum: ok={enum_ok} total={} matched_gpu_display={} plan={}",
            packages.len(),
            matched.len(),
            plan_path.display()
        ),
    );
}

fn complete_uninstall(
    dry_run: bool,
    matched: &[OemDriverRow],
    plan_path: &Path,
    log: &mut Vec<String>,
) -> Result<(), InstallError> {
    if dry_run {
        note(
            log,
            "S5d-UninstallDrivers",
            &format!(
                "Dry-run uninstall plan written: {} (no pnputil enumeration or delete)",
                plan_path.display()
            ),
        );
        return Ok(());
    }
    if !delete_is_enabled() || matched.is_empty() {
        note(log, "S5d-UninstallDrivers", "Live: enumerated drivers via pnputil; plan written; mass OEM delete skipped (set DFOUNDRY_UNINSTALL_DELETE=1 to enable)");
        return Ok(());
    }
    let (deleted, failures) = delete_matched_packages(matched);
    note(log, "S5d-UninstallDrivers", &format!("Live: DFOUNDRY_UNINSTALL_DELETE=1; delete attempted for {} OEM package(s), success={deleted}", matched.len()));
    if failures.is_empty() {
        return Ok(());
    }
    Err(InstallError::Other(format!(
        "pnputil failed to delete {} of {} matched package(s): {}",
        failures.len(),
        matched.len(),
        failures.join("; ")
    )))
}

fn delete_is_enabled() -> bool {
    std::env::var("DFOUNDRY_UNINSTALL_DELETE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn delete_matched_packages(matched: &[OemDriverRow]) -> (usize, Vec<String>) {
    let mut deleted = 0;
    let mut failures = Vec::new();
    for package in matched {
        match std::process::Command::new("pnputil")
            .args([
                "/delete-driver",
                &package.published_name,
                "/uninstall",
                "/force",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
        {
            Ok(status) if status.success() => deleted += 1,
            Ok(status) => failures.push(format!("{} exited with {status}", package.published_name)),
            Err(error) => failures.push(format!("{}: {error}", package.published_name)),
        }
    }
    (deleted, failures)
}

fn require_live_enumeration(dry_run: bool, enum_ok: bool) -> Result<(), InstallError> {
    if !dry_run && !enum_ok {
        return Err(InstallError::Other(
            "pnputil /enum-drivers failed; refusing live uninstall without an authoritative package list".into(),
        ));
    }
    Ok(())
}

/// Match NVIDIA / AMD / Intel display-related OEM packages.
pub(crate) fn filter_display_gpu_oems(packages: &[OemDriverRow]) -> Vec<OemDriverRow> {
    packages
        .iter()
        .filter(|package| is_display_gpu_package(package))
        .cloned()
        .collect()
}

fn is_display_gpu_package(package: &OemDriverRow) -> bool {
    let provider = package.provider.to_ascii_lowercase();
    let class = package.class_name.to_ascii_lowercase();
    let original = package.original_name.to_ascii_lowercase();
    let blob = format!("{provider} {class} {original}");
    vendor_matches(&provider, &original, &blob)
        && (display_matches(&class, &original) || class.is_empty())
}

fn vendor_matches(provider: &str, original: &str, blob: &str) -> bool {
    provider.contains("nvidia")
        || provider.contains("advanced micro devices")
        || provider.contains("ati technologies")
        || (provider.contains("amd") && !provider.contains("adam"))
        || provider.contains("intel")
        || original.contains("nvidia")
        || original.starts_with("nv")
        || original.contains("atikmd")
        || original.contains("amd")
        || original.contains("igdlh")
        || original.contains("iigd")
        || blob.contains("10de")
        || blob.contains("1002")
        || blob.contains("8086")
}

fn display_matches(class: &str, original: &str) -> bool {
    class.contains("display")
        || class.contains("graphics")
        || class.contains("video")
        || original.contains("disp")
        || original.contains("graphics")
        || original.contains("nvlddmkm")
        || original.contains("atikmdag")
        || original.contains("igdkmd")
}

#[cfg(test)]
mod tests {
    use super::{require_live_enumeration, run_uninstall_stage};

    #[test]
    fn live_uninstall_requires_successful_driver_enumeration() {
        assert!(require_live_enumeration(false, false).is_err());
        assert!(require_live_enumeration(false, true).is_ok());
        assert!(require_live_enumeration(true, false).is_ok());
    }

    #[test]
    fn dry_run_uninstall_canary_skips_pnputil_enumeration() {
        let work =
            std::env::temp_dir().join(format!("dfoundry-uninstall-dry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).unwrap();
        let mut log = Vec::new();
        run_uninstall_stage(&work, true, &mut log).unwrap();
        assert!(log
            .iter()
            .any(|entry| entry.contains("no pnputil enumeration")));
        let plan = std::fs::read_to_string(work.join("uninstall-drivers-plan.txt")).unwrap();
        assert!(plan.contains("enum_ok=false"));
        let _ = std::fs::remove_dir_all(work);
    }
}
