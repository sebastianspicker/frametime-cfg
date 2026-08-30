use driver_foundry_clean::{
    options_from_selection, run_clean, CleanOptions, CleanVendor, RemoveScopes,
};

pub(crate) const VENDORS: &[&str] = &["nvidia", "amd", "intel", "lisuan", "realtek"];

/// Clean panel UI state, mapped directly to the shared clean engine options.
#[derive(Debug, Clone)]
pub struct CleanUiState {
    pub vendor_idx: usize,
    pub dry_run: bool,
    pub remove_gfe: bool,
    pub remove_nv_broadcast: bool,
    pub remove_amd_kmpfd: bool,
    pub remove_intel_igs: bool,
    pub remove_intel_npu: bool,
    pub remove_oneapi: bool,
    pub remove_endurance: bool,
    pub remove_vulkan: bool,
    pub remove_physx: bool,
    pub remove_audiobus: bool,
    pub remove_monitors: bool,
    pub remove_unpack_nvidia: bool,
    pub remove_unpack_amd: bool,
    pub remove_install_cache: bool,
    pub cache_only: bool,
    pub block_driver_search: bool,
    pub no_setup_api: bool,
    pub no_restore_point: bool,
    pub prepare_safeboot: bool,
    pub clear_safeboot: bool,
    pub safeboot_network: bool,
    pub restart: bool,
    pub shutdown: bool,
    pub last_log: String,
    pub last_planned: usize,
    pub last_executed: usize,
}

impl Default for CleanUiState {
    fn default() -> Self {
        Self {
            vendor_idx: 0,
            dry_run: true,
            remove_gfe: false,
            remove_nv_broadcast: false,
            remove_amd_kmpfd: false,
            remove_intel_igs: false,
            remove_intel_npu: false,
            remove_oneapi: false,
            remove_endurance: false,
            remove_vulkan: false,
            remove_physx: false,
            remove_audiobus: false,
            remove_monitors: false,
            remove_unpack_nvidia: false,
            remove_unpack_amd: false,
            remove_install_cache: false,
            cache_only: false,
            block_driver_search: false,
            no_setup_api: false,
            no_restore_point: false,
            prepare_safeboot: false,
            clear_safeboot: false,
            safeboot_network: false,
            restart: false,
            shutdown: false,
            last_log: String::new(),
            last_planned: 0,
            last_executed: 0,
        }
    }
}

impl CleanUiState {
    pub fn vendor(&self) -> CleanVendor {
        CleanVendor::parse(VENDORS[self.vendor_idx.min(VENDORS.len() - 1)])
            .unwrap_or(CleanVendor::Nvidia)
    }

    pub fn scopes(&self) -> RemoveScopes {
        RemoveScopes {
            remove_gfe: self.remove_gfe,
            remove_nv_broadcast: self.remove_nv_broadcast,
            remove_amd_kmpfd: self.remove_amd_kmpfd,
            remove_intel_igs: self.remove_intel_igs,
            remove_intel_npu: self.remove_intel_npu,
            remove_oneapi: self.remove_oneapi,
            remove_endurance: self.remove_endurance,
            remove_vulkan: self.remove_vulkan,
            remove_physx: self.remove_physx,
            remove_audiobus: self.remove_audiobus,
            remove_monitors: self.remove_monitors,
            remove_unpack_nvidia: self.remove_unpack_nvidia,
            remove_unpack_amd: self.remove_unpack_amd,
            remove_install_cache: self.remove_install_cache,
        }
    }

    pub fn to_options(&self) -> CleanOptions {
        let mut options = options_from_selection(self.vendor(), self.dry_run, self.scopes(), None);
        options.cache_only = self.cache_only;
        options.block_driver_search = self.block_driver_search;
        options.no_setup_api = self.no_setup_api;
        options.no_restore_point = self.no_restore_point;
        options.prepare_safeboot = self.prepare_safeboot;
        options.clear_safeboot = self.clear_safeboot;
        options.safeboot_network = self.safeboot_network;
        options.restart = self.restart;
        options.shutdown = self.shutdown;
        options.attempt_elevation = true;
        options
    }

    pub fn run(&mut self) {
        match run_clean(&self.to_options()) {
            Ok(result) => {
                self.last_planned = result.planned;
                self.last_executed = result.executed;
                self.last_log = result.messages.join("\n");
            }
            Err(error) => {
                self.last_log = format!("error: {error}");
                self.last_planned = 0;
                self.last_executed = 0;
            }
        }
    }
}

pub(crate) fn view(state: &mut CleanUiState, ui: &mut egui::Ui) {
    ui.heading("GPU / Audio Cleanup");
    ui.horizontal(|ui| {
        ui.label("Vendor:");
        egui::ComboBox::from_id_salt("vendor")
            .selected_text(VENDORS[state.vendor_idx.min(VENDORS.len() - 1)])
            .show_ui(ui, |ui| {
                for (index, vendor) in VENDORS.iter().enumerate() {
                    ui.selectable_value(&mut state.vendor_idx, index, *vendor);
                }
            });
        ui.checkbox(&mut state.dry_run, "Dry-run (safe default)");
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.cache_only, "Cache-only");
        ui.checkbox(&mut state.block_driver_search, "Block driver search");
        ui.checkbox(&mut state.no_setup_api, "No SetupAPI");
        ui.checkbox(&mut state.no_restore_point, "No restore point");
    });
    ui.collapsing("Remove scopes", |ui| {
        for (value, label) in [
            (&mut state.remove_gfe, "Remove GFE"),
            (&mut state.remove_nv_broadcast, "Remove NV Broadcast"),
            (&mut state.remove_amd_kmpfd, "Remove AMD KMPFD"),
            (&mut state.remove_intel_igs, "Remove Intel IGS"),
            (&mut state.remove_intel_npu, "Remove Intel NPU"),
            (&mut state.remove_oneapi, "Remove oneAPI"),
            (&mut state.remove_endurance, "Remove Endurance Gaming"),
            (&mut state.remove_vulkan, "Remove Vulkan leftovers"),
            (&mut state.remove_physx, "Remove PhysX"),
            (&mut state.remove_audiobus, "Remove audiobus"),
            (&mut state.remove_monitors, "Remove monitors"),
            (&mut state.remove_unpack_nvidia, "Remove unpack NVIDIA"),
            (&mut state.remove_unpack_amd, "Remove unpack AMD"),
            (&mut state.remove_install_cache, "Remove install cache"),
        ] {
            ui.checkbox(value, label);
        }
    });
    ui.collapsing("Safe Mode", |ui| {
        ui.checkbox(&mut state.prepare_safeboot, "Prepare SafeBoot");
        ui.checkbox(&mut state.clear_safeboot, "Clear SafeBoot");
        ui.checkbox(&mut state.safeboot_network, "SafeBoot network");
    });
    ui.collapsing("Power (journal-only flags)", |ui| {
        ui.checkbox(&mut state.restart, "Restart after clean");
        ui.checkbox(&mut state.shutdown, "Shutdown after clean");
        ui.label("Flags are passed to CleanOptions; dry-run journals only.");
    });
    if ui.button("Run clean").clicked() {
        state.run();
    }
    ui.separator();
    ui.label(format!(
        "planned={} executed={}",
        state.last_planned, state.last_executed
    ));
    egui::ScrollArea::vertical()
        .max_height(400.0)
        .show(ui, |ui| {
            ui.monospace(&state.last_log);
        });
}
