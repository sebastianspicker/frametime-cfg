use super::{mmdevices, pci, OsEnvironment, ServiceProbe};
use driver_foundry_common::ActionJournal;
use std::path::Path;

/// Dry-run adapter: journals only. It deliberately performs no host-process probes.
pub struct DryRunEnvironment {
    pub probe_host: bool,
}

impl OsEnvironment for DryRunEnvironment {
    fn is_live(&self) -> bool {
        false
    }
    fn is_administrator(&self) -> bool {
        false
    }
    fn query_service(&self, name: &str) -> ServiceProbe {
        let _ = self.probe_host;
        ServiceProbe {
            name: name.into(),
            exists: false,
            state: "not-probed-dry-run".into(),
            raw: String::new(),
        }
    }
    fn stop_delete_service(&mut self, name: &str, journal: &mut ActionJournal) {
        journal.plan("Service", "stop_delete", name);
    }
    fn kill_process_match(&mut self, token: &str, journal: &mut ActionJournal) {
        journal.plan("Process", "kill_matching", token);
    }
    fn delete_file_match(&mut self, token: &str, journal: &mut ActionJournal) {
        journal.plan("File", "delete_match", token);
    }
    fn wipe_path(&mut self, path: &Path, journal: &mut ActionJournal) {
        journal.plan_detail(
            "File",
            "wipe_path",
            path.display().to_string(),
            if path.exists() {
                "exists=true"
            } else {
                "exists=false"
            },
        );
    }
    fn registry_cleanup(&mut self, action: &str, token: &str, journal: &mut ActionJournal) {
        journal.plan("Registry", action, token);
    }
    fn remove_appx_match(&mut self, package: &str, journal: &mut ActionJournal) {
        journal.plan("AppX", "remove_package_match", package);
    }
    fn uninstall_device(&mut self, hardware_id: &str, journal: &mut ActionJournal) {
        journal.plan("Device", "uninstall_device", hardware_id);
    }
    fn delete_scheduled_task(&mut self, task: &str, journal: &mut ActionJournal) {
        journal.plan("Task", "delete_scheduled_task", task);
    }
    fn set_block_driver_search(&mut self, enable: bool, journal: &mut ActionJournal) {
        journal.plan(
            "Policy",
            if enable {
                "block_driver_search"
            } else {
                "allow_driver_search"
            },
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\DriverSearching",
        );
    }
    fn create_restore_point(&mut self, description: &str, journal: &mut ActionJournal) {
        journal.plan("Policy", "create_restore_point", description);
    }
    fn clean_driverstore(
        &mut self,
        vendor_folder: &str,
        ven_id: &str,
        journal: &mut ActionJournal,
    ) {
        journal.plan("File", "clean_driverstore", vendor_folder);
        let _ = (self.probe_host, ven_id);
        journal.plan_detail(
            "DriverStore",
            "enum_summary",
            vendor_folder,
            "dry-run: host pnputil enumeration disabled",
        );
    }
    fn pnp_lockdown_orphans(&mut self, vendor_folder: &str, journal: &mut ActionJournal) {
        journal.plan("File", "pnp_lockdown_orphans", vendor_folder);
        journal.plan(
            "Registry",
            "fix_pnp_orphans",
            format!(r"HKLM\SYSTEM\CurrentControlSet\Enum\*\{vendor_folder}"),
        );
    }
    fn clean_pci_root(&mut self, vendor_folder: &str, ven_id: &str, journal: &mut ActionJournal) {
        pci::plan_pci_root_entries(vendor_folder, ven_id, journal);
    }
    fn clean_mmdevices(&mut self, tokens: &[String], journal: &mut ActionJournal) {
        mmdevices::plan_mmdevices_entries(tokens, journal);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dry_run_canary_has_no_process_backed_host_probes() {
        let source = include_str!("dry_run.rs");
        let command_new = ["Command", "::new"].concat();
        let process_module = ["std", "::process"].concat();
        assert!(!source.contains(&command_new));
        assert!(!source.contains(&process_module));
    }
}
