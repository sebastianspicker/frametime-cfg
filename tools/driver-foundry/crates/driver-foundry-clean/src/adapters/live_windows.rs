use super::{
    driverstore, filesystem, mmdevices, pci, service, OemDriverPackage, OsEnvironment, ServiceProbe,
};
use driver_foundry_common::ActionJournal;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Live Windows adapter, intended to run only after the elevation gate succeeds.
pub struct LiveWindowsEnvironment;

impl OsEnvironment for LiveWindowsEnvironment {
    fn is_live(&self) -> bool {
        true
    }
    fn is_administrator(&self) -> bool {
        driver_foundry_common::elevation::is_administrator()
    }
    fn query_service(&self, name: &str) -> ServiceProbe {
        service::probe_service_windows(name)
    }
    fn stop_delete_service(&mut self, name: &str, journal: &mut ActionJournal) {
        journal.plan("Service", "stop_delete", name);
        let result =
            cleanup_command("sc", &["stop", name]).and(cleanup_command("sc", &["delete", name]));
        record_outcome(journal, "Service", "stop_delete", name, result);
    }
    fn kill_process_match(&mut self, token: &str, journal: &mut ActionJournal) {
        journal.plan("Process", "kill_matching", token);
        let result = cleanup_command("taskkill", &["/F", "/IM", &format!("{token}.exe")])
            .and(cleanup_command("taskkill", &["/F", "/IM", token]));
        record_outcome(journal, "Process", "kill_matching", token, result);
    }
    fn delete_file_match(&mut self, token: &str, journal: &mut ActionJournal) {
        journal.plan("File", "delete_match", token);
        let result = filesystem::file_match_candidates(token)
            .iter()
            .map(|path| remove_path(path))
            .find(Result::is_err)
            .unwrap_or(Ok(()));
        record_outcome(journal, "File", "delete_match", token, result);
    }
    fn wipe_path(&mut self, path: &Path, journal: &mut ActionJournal) {
        let target = path.display().to_string();
        journal.plan("File", "wipe_path", &target);
        record_outcome(journal, "File", "wipe_path", &target, remove_path(path));
    }
    fn registry_cleanup(&mut self, action: &str, token: &str, journal: &mut ActionJournal) {
        journal.plan("Registry", action, token);
        if token.contains('\\') {
            record_outcome(
                journal,
                "Registry",
                action,
                token,
                cleanup_command("reg", &["delete", token, "/f"]),
            );
        } else {
            journal.mark_failed(
                "Registry",
                action,
                token,
                "catalog token is not a registry path",
            );
        }
    }
    fn remove_appx_match(&mut self, package: &str, journal: &mut ActionJournal) {
        journal.plan("AppX", "remove_package_match", package);
        if !is_safe_catalog_token(package) {
            journal.mark_failed(
                "AppX",
                "remove_package_match",
                package,
                "unsafe package catalog token",
            );
            return;
        }
        // The script is constant; the catalog value crosses as process data, not PowerShell text.
        let script = "$ErrorActionPreference = 'Stop'; $needle = $env:DFOUNDRY_APPX_TOKEN; $pattern = '*' + $needle + '*'; $packages = @(Get-AppxPackage $pattern -ErrorAction SilentlyContinue); foreach ($item in $packages) { Remove-AppxPackage -Package $item.PackageFullName -ErrorAction Stop }; $provisioned = @(Get-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -like $pattern }); foreach ($item in $provisioned) { Remove-AppxProvisionedPackage -Online -PackageName $item.PackageName -ErrorAction Stop }";
        record_outcome(
            journal,
            "AppX",
            "remove_package_match",
            package,
            command_with_env(
                "powershell",
                &["-NoProfile", "-Command", script],
                "DFOUNDRY_APPX_TOKEN",
                package,
            ),
        );
    }
    fn uninstall_device(&mut self, hardware_id: &str, journal: &mut ActionJournal) {
        journal.plan("Device", "uninstall_device", hardware_id);
        let script = format!("$ErrorActionPreference = 'Stop'; $ids = @(Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {{ $_.InstanceId -like '*{hardware_id}*' -or $_.HardwareID -like '*{hardware_id}*' }}); foreach ($d in $ids) {{ Disable-PnpDevice -InstanceId $d.InstanceId -Confirm:$false -ErrorAction Stop; Remove-PnpDevice -InstanceId $d.InstanceId -Confirm:$false -ErrorAction Stop }}");
        record_outcome(
            journal,
            "Device",
            "uninstall_device",
            hardware_id,
            silent("powershell", &["-NoProfile", "-Command", &script]),
        );
    }
    fn delete_scheduled_task(&mut self, task: &str, journal: &mut ActionJournal) {
        journal.plan("Task", "delete_scheduled_task", task);
        record_outcome(
            journal,
            "Task",
            "delete_scheduled_task",
            task,
            cleanup_command("schtasks", &["/Delete", "/TN", task, "/F"]),
        );
    }
    fn set_block_driver_search(&mut self, enable: bool, journal: &mut ActionJournal) {
        let action = if enable {
            "block_driver_search"
        } else {
            "allow_driver_search"
        };
        let target = r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\DriverSearching";
        journal.plan("Policy", action, target);
        let result = silent(
            "reg",
            &[
                "add",
                target,
                "/v",
                "SearchOrderConfig",
                "/t",
                "REG_DWORD",
                "/d",
                if enable { "0" } else { "1" },
                "/f",
            ],
        );
        record_outcome(journal, "Policy", action, target, result);
    }
    fn create_restore_point(&mut self, description: &str, journal: &mut ActionJournal) {
        journal.plan("Policy", "create_restore_point", description);
        let script = format!("$ErrorActionPreference = 'Stop'; Checkpoint-Computer -Description '{}' -RestorePointType MODIFY_SETTINGS -ErrorAction Stop", description.replace('\'', "''"));
        record_outcome(
            journal,
            "Policy",
            "create_restore_point",
            description,
            silent("powershell", &["-NoProfile", "-Command", &script]),
        );
    }
    fn clean_driverstore(&mut self, folder: &str, ven_id: &str, journal: &mut ActionJournal) {
        journal.plan("File", "clean_driverstore", folder);
        let packages = match driverstore::try_enum_oem_driver_packages() {
            Ok(packages) => packages,
            Err(error) => {
                journal.mark_failed("File", "clean_driverstore", folder, &error);
                journal.plan("DriverStore", "enum_summary", folder);
                journal.mark_failed("DriverStore", "enum_summary", folder, error);
                return;
            }
        };
        let matched = driverstore::filter_packages_for_vendor(&packages, folder, ven_id);
        for package in &matched {
            journal.plan_detail(
                "DriverStore",
                "delete_oem_package",
                &package.published_name,
                format!(
                    "provider={} original={}",
                    package.provider, package.original_name
                ),
            );
            let result = silent(
                "pnputil",
                &[
                    "/delete-driver",
                    &package.published_name,
                    "/uninstall",
                    "/force",
                ],
            );
            record_outcome(
                journal,
                "DriverStore",
                "delete_oem_package",
                &package.published_name,
                result,
            );
        }
        journal.plan_detail(
            "DriverStore",
            "enum_summary",
            folder,
            format!(
                "pnputil_enum={} matched_vendor={} deleted_attempted={}",
                packages.len(),
                matched.len(),
                matched.len()
            ),
        );
        let failed = journal
            .entries
            .iter()
            .any(|entry| entry.action == "delete_oem_package" && !entry.executed);
        let result = if failed {
            Err("one or more pnputil removals failed".into())
        } else {
            Ok(())
        };
        record_outcome(journal, "File", "clean_driverstore", folder, result.clone());
        record_outcome(journal, "DriverStore", "enum_summary", folder, result);
    }
    fn pnp_lockdown_orphans(&mut self, folder: &str, journal: &mut ActionJournal) {
        journal.plan("File", "pnp_lockdown_orphans", folder);
        let prefixes = allowed_repository_prefixes(folder);
        remove_driverstore_orphans(&prefixes, journal);
        let target = r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Setup\PnpLockdownFiles";
        journal.plan("Registry", "fix_pnp_orphans", target);
        record_outcome(
            journal,
            "Registry",
            "fix_pnp_orphans",
            target,
            remove_lockdown_values(target, &prefixes),
        );
        record_outcome(journal, "File", "pnp_lockdown_orphans", folder, Ok(()));
    }
    fn clean_pci_root(&mut self, folder: &str, ven_id: &str, journal: &mut ActionJournal) {
        pci::plan_pci_root_entries(folder, ven_id, journal);
        pci::execute_pci_root_cleanup(folder, ven_id, journal);
    }
    fn clean_mmdevices(&mut self, tokens: &[String], journal: &mut ActionJournal) {
        mmdevices::plan_mmdevices_entries(tokens, journal);
        mmdevices::execute_mmdevices_cleanup(tokens, journal);
    }
}

fn is_safe_catalog_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 260
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'.' | b'_' | b'-'))
}

fn silent(program: &str, args: &[&str]) -> Result<(), String> {
    command_output(program, args).and_then(|output| {
        output
            .status
            .success()
            .then_some(())
            .ok_or_else(|| format!("{program} exited with {}", output.status))
    })
}

fn cleanup_command(program: &str, args: &[&str]) -> Result<(), String> {
    let output = command_output(program, args)?;
    if output.status.success() || is_expected_absence(&output) {
        Ok(())
    } else {
        Err(format!(
            "{program} exited with {}: {}",
            output.status,
            command_text(&output)
        ))
    }
}

fn command_output(program: &str, args: &[&str]) -> Result<Output, String> {
    Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("{program}: {error}"))
}

fn command_with_env(program: &str, args: &[&str], key: &str, value: &str) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .env(key, value)
        .output()
        .map_err(|error| format!("{program}: {error}"))?;
    output
        .status
        .success()
        .then_some(())
        .ok_or_else(|| format!("{program} exited with {}", output.status))
}

fn command_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn is_expected_absence(output: &Output) -> bool {
    is_expected_absence_text(&command_text(output))
}

fn is_expected_absence_text(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    [
        "1060",
        "1062",
        "does not exist",
        "cannot find",
        "not found",
        "not running",
        "no running instance",
        "existiert nicht",
        "nicht gefunden",
        "nicht vorhanden",
        "angegebene datei nicht finden",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn remove_path(path: &Path) -> Result<(), String> {
    if path.is_file() {
        fs::remove_file(path).map_err(|error| format!("{}: {error}", path.display()))?;
    } else if path.is_dir() {
        fs::remove_dir_all(path).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}
fn record_outcome(
    journal: &mut ActionJournal,
    surface: &str,
    action: &str,
    target: &str,
    result: Result<(), String>,
) {
    match result {
        Ok(()) => journal.mark_executed(surface, action, target),
        Err(detail) => journal.mark_failed(surface, action, target, detail),
    }
}
fn allowed_repository_prefixes(folder: &str) -> Vec<String> {
    let ven_id = match folder.to_ascii_uppercase().as_str() {
        "NVIDIA" => "VEN_10DE",
        "AMD" => "VEN_1002",
        "INTEL" => "VEN_8086",
        "LISUAN" => "VEN_4C54",
        "REALTEK" => "VEN_10EC",
        _ => return Vec::new(),
    };
    let packages = driverstore::filter_packages_for_vendor(
        &driverstore::enum_oem_driver_packages(),
        folder,
        ven_id,
    );
    repository_prefixes(&packages)
}

fn repository_prefixes(packages: &[OemDriverPackage]) -> Vec<String> {
    packages
        .iter()
        .filter_map(|package| {
            std::path::Path::new(&package.original_name)
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
        })
        .filter(|prefix| !prefix.is_empty())
        .collect()
}

fn remove_driverstore_orphans(prefixes: &[String], journal: &mut ActionJournal) {
    if prefixes.is_empty() {
        return;
    }
    let Ok(system_root) = std::env::var("SystemRoot") else {
        return;
    };
    let repository = PathBuf::from(system_root)
        .join("System32")
        .join("DriverStore")
        .join("FileRepository");
    let Ok(entries) = fs::read_dir(repository) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if prefixes.iter().any(|prefix| name.starts_with(prefix)) {
            let target = entry.path().display().to_string();
            journal.plan("File", "driverstore_repo_orphan", &target);
            record_outcome(
                journal,
                "File",
                "driverstore_repo_orphan",
                &target,
                remove_path(&entry.path()),
            );
        }
    }
}
fn remove_lockdown_values(target: &str, prefixes: &[String]) -> Result<(), String> {
    let output = command_output("reg", &["query", target])?;
    if !output.status.success() && !is_expected_absence(&output) {
        return Err(format!("reg query exited with {}", output.status));
    }
    for line in String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| {
            prefixes
                .iter()
                .any(|prefix| line.to_ascii_lowercase().contains(prefix))
        })
    {
        if let Some(value) = line.split_whitespace().next() {
            cleanup_command("reg", &["delete", target, "/v", value, "/f"])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_expected_absence_text, is_safe_catalog_token, repository_prefixes};
    use crate::adapters::OemDriverPackage;

    #[test]
    fn recognizes_idempotent_absence_but_not_access_denied() {
        assert!(is_expected_absence_text(
            "FAILED 1060: The specified service does not exist."
        ));
        assert!(is_expected_absence_text(
            "Das System kann die angegebene Datei nicht finden."
        ));
        assert!(!is_expected_absence_text("Access is denied."));
    }

    #[test]
    fn repository_prefixes_exclude_unselected_network_packages() {
        let display = OemDriverPackage {
            published_name: "oem1.inf".into(),
            original_name: "igdlh64.inf".into(),
            provider: "Intel".into(),
            class_name: "Display".into(),
        };
        let prefixes = repository_prefixes(&[display]);
        assert!(prefixes.iter().any(|prefix| prefix == "igdlh64"));
        assert!(!prefixes
            .iter()
            .any(|prefix| "netwtw.inf".starts_with(prefix)));
        assert!(!prefixes
            .iter()
            .any(|prefix| "rt640x64.inf".starts_with(prefix)));
    }

    #[test]
    fn live_powershell_mutations_are_not_silently_swallowed() {
        let source = include_str!("live_windows.rs");
        assert!(source.contains("$ErrorActionPreference = 'Stop'"));
        assert!(
            source.contains("Remove-AppxPackage -Package $item.PackageFullName -ErrorAction Stop")
        );
        assert!(source.contains(
            "Remove-PnpDevice -InstanceId $d.InstanceId -Confirm:$false -ErrorAction Stop"
        ));
    }

    #[test]
    fn appx_token_rejects_powershell_metacharacters_and_newlines() {
        for token in [
            "NVIDIA; Remove-Item C:\\",
            "NVIDIA'",
            "NVIDIA\nRemove-AppxPackage",
            "..\\Windows",
        ] {
            assert!(
                !is_safe_catalog_token(token),
                "unsafe token accepted: {token:?}"
            );
        }
        assert!(is_safe_catalog_token("NVIDIAControlPanel"));
    }
}
