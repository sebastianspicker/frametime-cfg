use driver_foundry_install::{options_from_wizard, run_install, InstallOptions};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstallPage {
    #[default]
    Source,
    Components,
    Tweaks,
    Output,
    Run,
    Results,
}

/// Install wizard state, mapped directly to the shared install engine options.
#[derive(Debug, Clone)]
pub struct InstallUiState {
    pub page: InstallPage,
    pub preset: String,
    pub package_root: String,
    pub package_archive: String,
    pub package_url: String,
    pub work_dir: String,
    pub dry_run: bool,
    pub force_install: bool,
    pub select_extra: String,
    pub deselect_extra: String,
    pub disable_telemetry: bool,
    pub disable_installer_telemetry: bool,
    pub disable_nvcontainer: bool,
    pub disable_nvcamera: bool,
    pub disable_hdcp: bool,
    pub disable_mpo: bool,
    pub disable_hdaudio_sleep: bool,
    pub enable_msi: bool,
    pub clean_install: bool,
    pub unattended: bool,
    pub deep_inf: bool,
    pub try_sign: bool,
    pub uninstall_drivers: bool,
    pub export_path: String,
    pub archive_path: String,
    pub archive_format: String,
    pub last_log: String,
    pub last_kept: Vec<String>,
    pub last_stripped: Vec<String>,
    pub last_report: String,
}

impl Default for InstallUiState {
    fn default() -> Self {
        let work = std::env::temp_dir().join(format!("driver-foundry-gui-{}", std::process::id()));
        Self {
            page: InstallPage::Source,
            preset: "clean".into(),
            package_root: String::new(),
            package_archive: String::new(),
            package_url: String::new(),
            work_dir: work.display().to_string(),
            dry_run: true,
            force_install: false,
            select_extra: String::new(),
            deselect_extra: String::new(),
            disable_telemetry: true,
            disable_installer_telemetry: true,
            disable_nvcontainer: true,
            disable_nvcamera: true,
            disable_hdcp: false,
            disable_mpo: false,
            disable_hdaudio_sleep: false,
            enable_msi: true,
            clean_install: true,
            unattended: true,
            deep_inf: false,
            try_sign: false,
            uninstall_drivers: false,
            export_path: String::new(),
            archive_path: String::new(),
            archive_format: "zip".into(),
            last_log: String::new(),
            last_kept: Vec::new(),
            last_stripped: Vec::new(),
            last_report: String::new(),
        }
    }
}

fn optional_path(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    (!value.is_empty()).then(|| PathBuf::from(value))
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

impl InstallUiState {
    pub fn to_options(&self) -> InstallOptions {
        // The GUI must never enter live mode merely because a presentation
        // checkbox was cleared. Only the explicit force-install control can
        // disable the engine's dry-run safety boundary.
        let dry_run = !self.force_install;
        let mut options = options_from_wizard(
            PathBuf::from(&self.work_dir),
            &self.preset,
            optional_path(&self.package_root),
            dry_run,
            optional_path(&self.export_path),
            optional_path(&self.archive_path),
        );
        options.package_archive = optional_path(&self.package_archive);
        options.package_url =
            (!self.package_url.trim().is_empty()).then(|| self.package_url.trim().to_owned());
        options.select = split_csv(&self.select_extra);
        options.deselect = split_csv(&self.deselect_extra);
        options.disable_telemetry = self.disable_telemetry;
        options.disable_installer_telemetry = self.disable_installer_telemetry;
        options.disable_nvcontainer = self.disable_nvcontainer;
        options.disable_nvcamera = self.disable_nvcamera;
        options.disable_hdcp = self.disable_hdcp;
        options.disable_mpo = self.disable_mpo;
        options.disable_hdaudio_sleep = self.disable_hdaudio_sleep;
        options.enable_msi = self.enable_msi;
        options.clean_install = self.clean_install;
        options.unattended = self.unattended;
        options.deep_inf = self.deep_inf;
        options.try_sign = self.try_sign;
        options.uninstall_drivers = self.uninstall_drivers;
        if !self.archive_format.trim().is_empty() {
            options.archive_format = self.archive_format.trim().to_owned();
        }
        options
    }

    pub fn run(&mut self) {
        match run_install(&self.to_options()) {
            Ok(result) => {
                self.last_kept = result.kept_components;
                self.last_stripped = result.stripped_components;
                self.last_log = result.messages.join("\n");
                self.last_report = result
                    .run_report_path
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
            }
            Err(error) => self.last_log = format!("error: {error}"),
        }
        self.page = InstallPage::Results;
    }
}

pub(crate) fn view(state: &mut InstallUiState, ui: &mut egui::Ui) {
    ui.heading("Customize / Install");
    page_tabs(state, ui);
    ui.separator();
    match state.page {
        InstallPage::Source => source_page(state, ui),
        InstallPage::Components => components_page(state, ui),
        InstallPage::Tweaks => tweaks_page(state, ui),
        InstallPage::Output => output_page(state, ui),
        InstallPage::Run => run_page(state, ui),
        InstallPage::Results => results_page(state, ui),
    }
    navigation(state, ui);
}

fn page_tabs(state: &mut InstallUiState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for (page, label) in [
            (InstallPage::Source, "1.Source"),
            (InstallPage::Components, "2.Components"),
            (InstallPage::Tweaks, "3.Tweaks"),
            (InstallPage::Output, "4.Output"),
            (InstallPage::Run, "5.Run"),
            (InstallPage::Results, "6.Results"),
        ] {
            ui.selectable_value(&mut state.page, page, label);
        }
    });
}

fn source_page(state: &mut InstallUiState, ui: &mut egui::Ui) {
    for (label, value) in [
        (
            "Package root (empty = synthetic fixture):",
            &mut state.package_root,
        ),
        (
            "Package archive path (zip/7z/exe):",
            &mut state.package_archive,
        ),
        ("Package URL (HTTPS):", &mut state.package_url),
        ("Work directory:", &mut state.work_dir),
    ] {
        ui.label(label);
        ui.text_edit_singleline(value);
    }
    ui.checkbox(
        &mut state.force_install,
        "Force install (explicitly launches setup.exe; refuses synthetic)",
    );
    state.dry_run = !state.force_install;
    ui.label(if state.dry_run {
        "Mode: dry-run (setup.exe will not launch)"
    } else {
        "Mode: force-install"
    });
}

fn components_page(state: &mut InstallUiState, ui: &mut egui::Ui) {
    ui.label("Preset:");
    for preset in [
        "minimal",
        "clean",
        "recommended",
        "notebook",
        "gaming",
        "full",
    ] {
        ui.radio_value(&mut state.preset, preset.to_owned(), preset);
    }
    ui.separator();
    ui.label("Extra select (comma-separated component ids):");
    ui.text_edit_singleline(&mut state.select_extra);
    ui.label("Deselect (comma-separated component ids):");
    ui.text_edit_singleline(&mut state.deselect_extra);
}

fn tweaks_page(state: &mut InstallUiState, ui: &mut egui::Ui) {
    for (value, label) in [
        (&mut state.disable_telemetry, "Disable telemetry"),
        (
            &mut state.disable_installer_telemetry,
            "Disable installer telemetry",
        ),
        (&mut state.disable_nvcontainer, "Disable NvContainer"),
        (&mut state.disable_nvcamera, "Disable NvCamera"),
        (&mut state.disable_hdcp, "Disable HDCP"),
        (&mut state.disable_mpo, "Disable MPO"),
        (&mut state.disable_hdaudio_sleep, "Disable HDAudio sleep"),
        (&mut state.enable_msi, "Enable MSI"),
        (&mut state.clean_install, "Clean install flag"),
        (&mut state.unattended, "Unattended setup.cfg"),
        (&mut state.deep_inf, "Deep INF"),
        (&mut state.try_sign, "Try sign"),
        (&mut state.uninstall_drivers, "Uninstall drivers stage"),
    ] {
        ui.checkbox(value, label);
    }
}

fn output_page(state: &mut InstallUiState, ui: &mut egui::Ui) {
    ui.label("Export path (optional):");
    ui.text_edit_singleline(&mut state.export_path);
    ui.label("Portable archive path (optional):");
    ui.text_edit_singleline(&mut state.archive_path);
    ui.label("Archive format:");
    for format in ["zip", "7z", "sfx"] {
        ui.radio_value(&mut state.archive_format, format.to_owned(), format);
    }
}

fn run_page(state: &mut InstallUiState, ui: &mut egui::Ui) {
    ui.label("Ready to run install pipeline with shared engine.");
    ui.label(format!(
        "preset={} dry_run={} force_install={} deep_inf={}",
        state.preset, !state.force_install, state.force_install, state.deep_inf
    ));
    if ui.button("Run install").clicked() {
        state.run();
    }
}

fn results_page(state: &InstallUiState, ui: &mut egui::Ui) {
    ui.label(format!("kept: {:?}", state.last_kept));
    ui.label(format!("stripped count: {}", state.last_stripped.len()));
    if !state.last_report.is_empty() {
        ui.label(format!("report: {}", state.last_report));
    }
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .show(ui, |ui| {
            ui.monospace(&state.last_log);
        });
}

fn navigation(state: &mut InstallUiState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if ui.button("◀ Back").clicked() {
            state.page = previous_page(state.page);
        }
        if ui.button("Next ▶").clicked() {
            state.page = next_page(state.page);
        }
    });
}

fn previous_page(page: InstallPage) -> InstallPage {
    match page {
        InstallPage::Source | InstallPage::Components => InstallPage::Source,
        InstallPage::Tweaks => InstallPage::Components,
        InstallPage::Output => InstallPage::Tweaks,
        InstallPage::Run => InstallPage::Output,
        InstallPage::Results => InstallPage::Run,
    }
}

fn next_page(page: InstallPage) -> InstallPage {
    match page {
        InstallPage::Source => InstallPage::Components,
        InstallPage::Components => InstallPage::Tweaks,
        InstallPage::Tweaks => InstallPage::Output,
        InstallPage::Output => InstallPage::Run,
        InstallPage::Run => InstallPage::Run,
        InstallPage::Results => InstallPage::Results,
    }
}
