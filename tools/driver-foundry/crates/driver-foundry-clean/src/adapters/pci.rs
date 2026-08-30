use driver_foundry_common::ActionJournal;
use std::process::{Command, Output};

/// Plan PCI root / Enum leftover cleanup without mutation.
pub fn plan_pci_root_entries(vendor_folder: &str, ven_id: &str, journal: &mut ActionJournal) {
    let ven_id = normalize_ven_id(ven_id);
    journal.plan("Device", "clean_pci_root", format!("PCI\\{ven_id}"));
    journal.plan(
        "Registry",
        "pci_enum_leftover",
        format!(r"HKLM\SYSTEM\CurrentControlSet\Enum\PCI\{ven_id}"),
    );
    for bus in ["PCI", "HDAUDIO", "SWD", "ROOT", "DISPLAY"] {
        journal.plan(
            "Registry",
            "enum_bus_scan",
            format!(r"HKLM\SYSTEM\CurrentControlSet\Enum\{bus}\*{ven_id}*"),
        );
    }
    journal.plan_detail(
        "Device",
        "pci_root_summary",
        vendor_folder,
        format!("ven_id={ven_id} targets=Enum\\PCI+Get-PnpDevice+pnputil"),
    );
    let filters = super::mmdevices::pci_filter_tokens(vendor_folder);
    for filter in &filters {
        journal.plan("Service", "pci_filter_stop_delete", *filter);
    }
    if !filters.is_empty() {
        journal.plan(
            "Registry",
            "StripFilterValues",
            format!(r"HKLM\SYSTEM\CurrentControlSet\Enum\PCI filters={} values=UpperFilters,LowerFilters", filters.join(",")),
        );
        if vendor_folder.eq_ignore_ascii_case("AMD") {
            journal.plan(
                "Registry",
                "StripFilterValues",
                r"HKLM\SYSTEM\CurrentControlSet\Enum\ACPI filters=amdkmpfd,amdkmafd values=UpperFilters,LowerFilters",
            );
        }
        for filter in &filters {
            journal.plan(
                "Registry",
                "pci_filter_strip",
                format!(
                    r"HKLM\SYSTEM\CurrentControlSet\Enum\PCI\* UpperFilters/LowerFilters:{filter}"
                ),
            );
        }
    }
}

pub(crate) fn execute_pci_root_cleanup(
    vendor_folder: &str,
    ven_id: &str,
    journal: &mut ActionJournal,
) {
    let ven_id = normalize_ven_id(ven_id);
    let script = format!("$ErrorActionPreference = 'Stop'; $ids = @(Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {{ $_.InstanceId -like '*{ven_id}*' -or $_.HardwareID -like '*{ven_id}*' }}); foreach ($device in $ids) {{ Disable-PnpDevice -InstanceId $device.InstanceId -Confirm:$false -ErrorAction Stop; Remove-PnpDevice -InstanceId $device.InstanceId -Confirm:$false -ErrorAction Stop }}");
    let device_result = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status()
        .map_err(|error| format!("powershell: {error}"))
        .and_then(status_result);
    let target = format!(r"HKLM\SYSTEM\CurrentControlSet\Enum\PCI\{ven_id}");
    let registry_result = cleanup_command("reg", &["delete", &target, "/f"]);
    record_result(
        journal,
        "Device",
        "clean_pci_root",
        &format!("PCI\\{ven_id}"),
        device_result,
    );
    record_result(
        journal,
        "Registry",
        "pci_enum_leftover",
        &target,
        registry_result,
    );
    let filters = super::mmdevices::pci_filter_tokens(vendor_folder);
    if filters.is_empty() {
        return;
    }
    let pci_base = r"HKLM\SYSTEM\CurrentControlSet\Enum\PCI";
    let pci_target = format!(
        r"{pci_base} filters={} values=UpperFilters,LowerFilters",
        filters.join(",")
    );
    let pci_result = execute_strip_filter_values(pci_base, &filters);
    record_result(
        journal,
        "Registry",
        "StripFilterValues",
        &pci_target,
        pci_result.clone().map(|_| ()),
    );
    if vendor_folder.eq_ignore_ascii_case("AMD") {
        let acpi_base = r"HKLM\SYSTEM\CurrentControlSet\Enum\ACPI";
        let acpi_target = r"HKLM\SYSTEM\CurrentControlSet\Enum\ACPI filters=amdkmpfd,amdkmafd values=UpperFilters,LowerFilters";
        record_result(
            journal,
            "Registry",
            "StripFilterValues",
            acpi_target,
            execute_strip_filter_values(acpi_base, &filters).map(|_| ()),
        );
    }
    for filter in filters {
        let service_result = cleanup_command("sc", &["stop", filter])
            .and(cleanup_command("sc", &["delete", filter]));
        record_result(
            journal,
            "Service",
            "pci_filter_stop_delete",
            filter,
            service_result,
        );
        let strip_target =
            format!(r"HKLM\SYSTEM\CurrentControlSet\Enum\PCI\* UpperFilters/LowerFilters:{filter}");
        record_result(
            journal,
            "Registry",
            "pci_filter_strip",
            &strip_target,
            pci_result.clone().map(|_| ()),
        );
    }
}

fn status_result(status: std::process::ExitStatus) -> Result<(), String> {
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("command exited with {status}"))
}

fn cleanup_command(program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("{program}: {error}"))?;
    if output.status.success() || is_expected_absence(&output) {
        Ok(())
    } else {
        Err(format!("{program} exited with {}", output.status))
    }
}

fn is_expected_absence(output: &Output) -> bool {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();
    [
        "1060",
        "1062",
        "does not exist",
        "cannot find",
        "not found",
        "not running",
        "existiert nicht",
        "nicht gefunden",
        "nicht vorhanden",
        "angegebene datei nicht finden",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn execute_strip_filter_values(base_key: &str, filters: &[&str]) -> Result<usize, String> {
    let script = build_strip_filter_script(base_key, filters);
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|error| format!("powershell: {error}"))?;
    if !output.status.success() {
        return Err(format!("powershell exited with {}", output.status));
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .find_map(|line| line.trim().parse().ok())
        .ok_or_else(|| "powershell did not report a modified-key count".into())
}

fn build_strip_filter_script(base_key: &str, filters: &[&str]) -> String {
    let filters_ps = filters
        .iter()
        .map(|filter| format!("'{}'", filter.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    let base = reg_path_to_ps(base_key);
    format!(
        r#"
$ErrorActionPreference = 'Stop'
$filters = @({filters_ps})
$base = 'Registry::{base}'
$modified = 0
if (Test-Path -LiteralPath $base) {{
  Get-ChildItem -LiteralPath $base -Recurse -ErrorAction SilentlyContinue | ForEach-Object {{
    foreach ($name in @('UpperFilters','LowerFilters')) {{
      $value = $_.GetValue($name, $null, 'DoNotExpandEnvironmentNames')
      if ($null -eq $value) {{ continue }}
      $parts = @($value | ForEach-Object {{ $_.ToString() }})
      $kept = @($parts | Where-Object {{ $part = $_; -not ($filters | Where-Object {{ $part -ieq $_ -or $part -ilike "*$_*" }}) }})
      if ($kept.Count -eq $parts.Count) {{ continue }}
      if ($kept.Count -eq 0) {{ Remove-ItemProperty -LiteralPath $_.PSPath -Name $name -Force }}
      else {{ Set-ItemProperty -LiteralPath $_.PSPath -Name $name -Value ([string[]]$kept) -Force }}
      $modified++
    }}
  }}
}}
Write-Output $modified
"#
    )
}

fn reg_path_to_ps(reg_path: &str) -> String {
    if let Some(suffix) = reg_path.strip_prefix("HKLM\\") {
        format!("HKEY_LOCAL_MACHINE\\{suffix}")
    } else if let Some(suffix) = reg_path.strip_prefix("HKCU\\") {
        format!("HKEY_CURRENT_USER\\{suffix}")
    } else {
        reg_path.into()
    }
}

fn record_result(
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

fn normalize_ven_id(ven_id: &str) -> String {
    let upper = ven_id.trim().to_ascii_uppercase();
    if upper.starts_with("VEN_") {
        upper
    } else {
        format!("VEN_{upper}")
    }
}

#[cfg(test)]
mod tests {
    use super::{build_strip_filter_script, status_result};
    use std::process::Command;

    #[test]
    fn nonzero_status_is_not_success() {
        let status = if cfg!(windows) {
            Command::new("cmd").args(["/C", "exit 7"]).status().unwrap()
        } else {
            Command::new("sh").args(["-c", "exit 7"]).status().unwrap()
        };
        assert!(status_result(status).is_err());
    }

    #[test]
    fn strip_script_updates_existing_multi_sz_without_invalid_type_parameter() {
        let script = build_strip_filter_script(
            r"HKCU\Software\DriverFoundryTests\FilterScript",
            &["nvpciflt"],
        );
        assert!(script.contains("Set-ItemProperty"));
        assert!(script.contains("([string[]]$kept)"));
        assert!(!script.contains("-Type MultiString"));
    }

    #[cfg(windows)]
    #[test]
    fn live_strip_retains_neighbor_filter_values() {
        use super::execute_strip_filter_values;

        let key = format!(
            r"HKCU\Software\DriverFoundryTests\FilterScript-{}",
            std::process::id()
        );
        let ps_key = key.replacen("HKCU\\", "HKCU:\\", 1);
        let child = format!(r"{ps_key}\Device");
        let setup = format!(
            "$p='{child}'; New-Item -Path $p -Force | Out-Null; New-ItemProperty -Path $p -Name UpperFilters -PropertyType MultiString -Value @('mouclass','nvpciflt','kbdclass') -Force | Out-Null"
        );
        let setup_status = Command::new("powershell")
            .args(["-NoProfile", "-Command", &setup])
            .status()
            .expect("create registry fixture");
        assert!(setup_status.success());

        let modified = execute_strip_filter_values(&key, &["nvpciflt"])
            .expect("strip filter from registry fixture");
        let read =
            format!("((Get-Item -LiteralPath '{child}').GetValue('UpperFilters')) -join '|'");
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", &read])
            .output()
            .expect("read registry fixture");
        let _ = Command::new("reg").args(["delete", &key, "/f"]).status();

        assert_eq!(modified, 1);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "mouclass|kbdclass"
        );
    }

    #[test]
    fn pci_mutation_script_stops_on_errors() {
        let source = include_str!("pci.rs");
        assert!(source.contains("$ErrorActionPreference = 'Stop'"));
        assert!(source.contains(
            "Remove-PnpDevice -InstanceId $device.InstanceId -Confirm:$false -ErrorAction Stop"
        ));
    }
}
