use super::{mmdevices, pci, OsEnvironment, ServiceProbe};
use driver_foundry_common::ActionJournal;
use std::path::Path;

/// Recording adapter for unit tests, using dry-run journal semantics.
#[derive(Debug, Default)]
pub struct RecordingEnvironment {
    pub calls: Vec<(String, String)>,
    pub probe_host: bool,
    pub fail_services: bool,
}

impl RecordingEnvironment {
    pub fn called(&self, method: &str) -> bool {
        self.calls.iter().any(|(name, _)| name == method)
    }
    pub fn call_count(&self, method: &str) -> usize {
        self.calls.iter().filter(|(name, _)| name == method).count()
    }
    fn record(&mut self, method: &str, detail: impl Into<String>) {
        self.calls.push((method.into(), detail.into()));
    }
}

impl OsEnvironment for RecordingEnvironment {
    fn is_live(&self) -> bool {
        false
    }
    fn is_administrator(&self) -> bool {
        false
    }
    fn query_service(&self, name: &str) -> ServiceProbe {
        ServiceProbe {
            name: name.into(),
            exists: false,
            state: "recording".into(),
            raw: String::new(),
        }
    }
    fn stop_delete_service(&mut self, value: &str, journal: &mut ActionJournal) {
        self.record("stop_delete_service", value);
        journal.plan("Service", "stop_delete", value);
        if self.fail_services {
            journal.mark_failed("Service", "stop_delete", value, "recording failure");
        }
    }
    fn kill_process_match(&mut self, value: &str, journal: &mut ActionJournal) {
        self.record("kill_process_match", value);
        journal.plan("Process", "kill_matching", value);
    }
    fn delete_file_match(&mut self, value: &str, journal: &mut ActionJournal) {
        self.record("delete_file_match", value);
        journal.plan("File", "delete_match", value);
    }
    fn wipe_path(&mut self, value: &Path, journal: &mut ActionJournal) {
        self.record("wipe_path", value.display().to_string());
        journal.plan("File", "wipe_path", value.display().to_string());
    }
    fn registry_cleanup(&mut self, action: &str, value: &str, journal: &mut ActionJournal) {
        self.record("registry_cleanup", format!("{action}:{value}"));
        journal.plan("Registry", action, value);
    }
    fn remove_appx_match(&mut self, value: &str, journal: &mut ActionJournal) {
        self.record("remove_appx_match", value);
        journal.plan("AppX", "remove_package_match", value);
    }
    fn uninstall_device(&mut self, value: &str, journal: &mut ActionJournal) {
        self.record("uninstall_device", value);
        journal.plan("Device", "uninstall_device", value);
    }
    fn delete_scheduled_task(&mut self, value: &str, journal: &mut ActionJournal) {
        self.record("delete_scheduled_task", value);
        journal.plan("Task", "delete_scheduled_task", value);
    }
    fn set_block_driver_search(&mut self, value: bool, journal: &mut ActionJournal) {
        self.record("set_block_driver_search", value.to_string());
        journal.plan(
            "Policy",
            if value {
                "block_driver_search"
            } else {
                "allow_driver_search"
            },
            "DriverSearching",
        );
    }
    fn create_restore_point(&mut self, value: &str, journal: &mut ActionJournal) {
        self.record("create_restore_point", value);
        journal.plan("Policy", "create_restore_point", value);
    }
    fn clean_driverstore(&mut self, folder: &str, ven_id: &str, journal: &mut ActionJournal) {
        self.record("clean_driverstore", format!("{folder}|{ven_id}"));
        journal.plan("File", "clean_driverstore", folder);
        journal.plan_detail(
            "DriverStore",
            "enum_summary",
            folder,
            format!("recording ven_id={ven_id}"),
        );
    }
    fn pnp_lockdown_orphans(&mut self, folder: &str, journal: &mut ActionJournal) {
        self.record("pnp_lockdown_orphans", folder);
        journal.plan("File", "pnp_lockdown_orphans", folder);
    }
    fn clean_pci_root(&mut self, folder: &str, ven_id: &str, journal: &mut ActionJournal) {
        self.record("clean_pci_root", format!("{folder}|{ven_id}"));
        pci::plan_pci_root_entries(folder, ven_id, journal);
    }
    fn clean_mmdevices(&mut self, tokens: &[String], journal: &mut ActionJournal) {
        self.record("clean_mmdevices", tokens.join(","));
        mmdevices::plan_mmdevices_entries(tokens, journal);
    }
}
