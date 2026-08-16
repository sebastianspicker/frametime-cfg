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
    let plan_path = work.join("uninstall-drivers-plan.txt");

    // A dry-run is planning only: never execute PATH tools, including read-only probes.
    let (enum_text, enum_ok) = if dry_run {
        (String::new(), false)
    } else {
        let enum_output = std::process::Command::new("pnputil")
            .args(["/enum-drivers"])
            .output();
        match enum_output {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout).to_string();
                (text, o.status.success())
            }
            Err(e) => {
                note(
                    log,
                    "S5d-UninstallDrivers",
                    &format!("pnputil /enum-drivers failed to spawn: {e}"),
                );
                (String::new(), false)
            }
        }
    };

    let pkgs = parse_pnputil_enum_drivers_simple(&enum_text);
    let matched = filter_display_gpu_oems(&pkgs);

    let mut plan = String::from(
        "# uninstall-drivers plan (Driver Foundry)\n\
         # pnputil /enum-drivers → match NVIDIA/AMD/Intel display-related OEM packages\n\
         # Live mass-delete only when DFOUNDRY_UNINSTALL_DELETE=1\n\n",
    );
    plan.push_str(&format!(
        "# enum_ok={enum_ok} total_oem={} matched_display_gpu={}\n\n",
        pkgs.len(),
        matched.len()
    ));
    if matched.is_empty() {
        plan.push_str("# (no NVIDIA/AMD/Intel display-related OEM packages matched)\n");
        plan.push_str("pnputil /enum-drivers\n");
        plan.push_str("# Would delete: pnputil /delete-driver oemXX.inf /uninstall /force\n");
    } else {
        for p in &matched {
            plan.push_str(&format!(
                "# provider={} class={} original={}\n",
                p.provider, p.class_name, p.original_name
            ));
            plan.push_str(&format!(
                "pnputil /delete-driver {} /uninstall /force\n",
                p.published_name
            ));
        }
    }
    fs::write(&plan_path, &plan)?;

    note(
        log,
        "S5d-UninstallDrivers",
        &format!(
            "pnputil enum: ok={enum_ok} total={} matched_gpu_display={} plan={}",
            pkgs.len(),
            matched.len(),
            plan_path.display()
        ),
    );

    require_live_enumeration(dry_run, enum_ok)?;

    let allow_delete = std::env::var("DFOUNDRY_UNINSTALL_DELETE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if dry_run {
        note(
            log,
            "S5d-UninstallDrivers",
            &format!(
                "Dry-run uninstall plan written: {} (no pnputil enumeration or delete)",
                plan_path.display()
            ),
        );
    } else if allow_delete && !matched.is_empty() {
        let mut deleted = 0usize;
        let mut failures = Vec::new();
        for p in &matched {
            let status = std::process::Command::new("pnputil")
                .args(["/delete-driver", &p.published_name, "/uninstall", "/force"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            match status {
                Ok(status) if status.success() => deleted += 1,
                Ok(status) => failures.push(format!("{} exited with {}", p.published_name, status)),
                Err(error) => failures.push(format!("{}: {error}", p.published_name)),
            }
        }
        note(
            log,
            "S5d-UninstallDrivers",
            &format!(
                "Live: DFOUNDRY_UNINSTALL_DELETE=1 — delete attempted for {} OEM package(s), success={deleted}",
                matched.len()
            ),
        );
        if !failures.is_empty() {
            return Err(InstallError::Other(format!(
                "pnputil failed to delete {} of {} matched package(s): {}",
                failures.len(),
                matched.len(),
                failures.join("; ")
            )));
        }
    } else {
        note(
            log,
            "S5d-UninstallDrivers",
            "Live: enumerated drivers via pnputil; plan written; mass OEM delete skipped (set DFOUNDRY_UNINSTALL_DELETE=1 to enable)",
        );
    }
    Ok(())
}

fn require_live_enumeration(dry_run: bool, enum_ok: bool) -> Result<(), InstallError> {
    if !dry_run && !enum_ok {
        return Err(InstallError::Other(
            "pnputil /enum-drivers failed; refusing live uninstall without an authoritative package list"
                .into(),
        ));
    }
    Ok(())
}

/// Match NVIDIA / AMD / Intel display-related OEM packages.
pub(crate) fn filter_display_gpu_oems(pkgs: &[OemDriverRow]) -> Vec<OemDriverRow> {
    pkgs.iter()
        .filter(|p| {
            let provider = p.provider.to_ascii_lowercase();
            let class = p.class_name.to_ascii_lowercase();
            let original = p.original_name.to_ascii_lowercase();
            let blob = format!("{provider} {class} {original}");

            let vendor = provider.contains("nvidia")
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
                || blob.contains("8086");

            let displayish = class.contains("display")
                || class.contains("graphics")
                || class.contains("video")
                || original.contains("disp")
                || original.contains("graphics")
                || original.contains("nvlddmkm")
                || original.contains("atikmdag")
                || original.contains("igdkmd");

            // GPU vendor + display class/name; empty class still matches strong GPU INF names.
            vendor && (displayish || class.is_empty())
        })
        .cloned()
        .collect()
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
