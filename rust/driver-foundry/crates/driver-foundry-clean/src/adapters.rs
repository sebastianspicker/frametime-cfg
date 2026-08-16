//! OS environment adapters: dry-run journal vs live Windows mutation.
//!
//! The public adapter surface stays flat for CLI/GUI consumers while the
//! implementation is organized by host interaction domain.

mod driverstore;
mod dry_run;
mod filesystem;
mod live_windows;
mod mmdevices;
mod pci;
mod recording;
mod service;

pub use driver_foundry_common::pnputil::OemDriverPackage;
pub use driverstore::{
    enum_oem_driver_packages, filter_packages_for_vendor, parse_pnputil_enum_drivers,
};
pub use dry_run::DryRunEnvironment;
pub use filesystem::{apply_planned_entry, file_match_candidates};
pub(crate) use live_windows::LiveWindowsEnvironment;
pub use mmdevices::{
    parse_multi_sz_filters, pci_filter_tokens, plan_mmdevices_entries, strip_filters_from_multi_sz,
    would_strip_filter_value,
};
pub use pci::plan_pci_root_entries;
pub use recording::RecordingEnvironment;
pub use service::probe_service_windows;

use driver_foundry_common::ActionJournal;
use std::path::Path;

/// Snapshot of a Windows service query (host-probe / live).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceProbe {
    pub name: String,
    pub exists: bool,
    pub state: String,
    pub raw: String,
}

/// Host environment used by the clean orchestrator.
pub trait OsEnvironment {
    fn is_live(&self) -> bool;
    fn is_administrator(&self) -> bool;
    fn query_service(&self, name: &str) -> ServiceProbe;
    fn stop_delete_service(&mut self, name: &str, journal: &mut ActionJournal);
    fn kill_process_match(&mut self, token: &str, journal: &mut ActionJournal);
    fn delete_file_match(&mut self, token: &str, journal: &mut ActionJournal);
    fn wipe_path(&mut self, path: &Path, journal: &mut ActionJournal);
    fn registry_cleanup(&mut self, action: &str, token: &str, journal: &mut ActionJournal);
    fn remove_appx_match(&mut self, package: &str, journal: &mut ActionJournal);
    fn uninstall_device(&mut self, hardware_id: &str, journal: &mut ActionJournal);
    fn delete_scheduled_task(&mut self, task: &str, journal: &mut ActionJournal);
    fn set_block_driver_search(&mut self, enable: bool, journal: &mut ActionJournal);
    fn create_restore_point(&mut self, description: &str, journal: &mut ActionJournal);
    fn clean_driverstore(&mut self, vendor_folder: &str, ven_id: &str, journal: &mut ActionJournal);
    fn pnp_lockdown_orphans(&mut self, vendor_folder: &str, journal: &mut ActionJournal);
    fn clean_pci_root(&mut self, vendor_folder: &str, ven_id: &str, journal: &mut ActionJournal);
    fn clean_mmdevices(&mut self, tokens: &[String], journal: &mut ActionJournal);
}

#[cfg(test)]
mod tests;
