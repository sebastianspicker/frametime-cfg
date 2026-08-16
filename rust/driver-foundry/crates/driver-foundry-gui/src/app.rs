use crate::{clean, install, CleanUiState, InstallUiState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppMode {
    #[default]
    Clean,
    Install,
}

pub struct DriverFoundryApp {
    pub mode: AppMode,
    pub clean: CleanUiState,
    pub install: InstallUiState,
}

impl Default for DriverFoundryApp {
    fn default() -> Self {
        Self {
            mode: AppMode::Clean,
            clean: CleanUiState::default(),
            install: InstallUiState::default(),
        }
    }
}

impl eframe::App for DriverFoundryApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.horizontal(|ui| {
            ui.heading("Driver Foundry");
            ui.separator();
            ui.selectable_value(&mut self.mode, AppMode::Clean, "Clean");
            ui.selectable_value(&mut self.mode, AppMode::Install, "Install");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(driver_foundry_common::version_line());
            });
        });
        ui.label("Native Rust · shares CLI engines · default dry-run (no host mutation)");
        ui.separator();
        match self.mode {
            AppMode::Clean => clean::view(&mut self.clean, ui),
            AppMode::Install => install::view(&mut self.install, ui),
        }
    }
}

pub fn run_gui() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 720.0])
            .with_title("Driver Foundry"),
        ..Default::default()
    };
    eframe::run_native(
        "Driver Foundry",
        options,
        Box::new(|_context| Ok(Box::new(DriverFoundryApp::default()))),
    )
}
