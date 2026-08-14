#![forbid(unsafe_code)]

use eframe::egui;
use northclock_core::{
    ApplicationCommand, ApplicationService, ApplyReceipt, CommandEnvelope, MemoryTestConfig,
    OperationPlan, OperationRequest, RISK_ACKNOWLEDGEMENT,
};
use northclock_platform_windows::WindowsPlatform;
use std::time::{Duration, Instant};

pub fn run() -> Result<(), String> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Northclock")
            .with_inner_size([920.0, 680.0]),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "Northclock",
        options,
        Box::new(|_context| Ok(Box::new(NorthclockApp::default()))),
    )
    .map_err(|error| error.to_string())
}

pub fn run_overlay() -> Result<(), String> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Northclock measurements")
            .with_inner_size([380.0, 180.0])
            .with_transparent(true)
            .with_decorations(false)
            .with_always_on_top(),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "Northclock measurements",
        options,
        Box::new(|_context| Ok(Box::new(MeasurementOverlay::default()))),
    )
    .map_err(|error| error.to_string())
}

struct MeasurementOverlay {
    service: ApplicationService<WindowsPlatform>,
    cpu: Option<CommandEnvelope>,
    gpu: Option<CommandEnvelope>,
    last_refresh: Option<Instant>,
}

impl Default for MeasurementOverlay {
    fn default() -> Self {
        Self {
            service: ApplicationService::new(WindowsPlatform::new()),
            cpu: None,
            gpu: None,
            last_refresh: None,
        }
    }
}

impl MeasurementOverlay {
    fn refresh(&mut self) {
        self.cpu = Some(self.service.execute(ApplicationCommand::CpuMeasurements));
        self.gpu = Some(
            self.service
                .execute(ApplicationCommand::GpuMeasurements { stable_id: None }),
        );
        self.last_refresh = Some(Instant::now());
    }
}

impl eframe::App for MeasurementOverlay {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self
            .last_refresh
            .is_none_or(|last| last.elapsed() >= Duration::from_secs(1))
        {
            self.refresh();
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(egui::Color32::from_black_alpha(190))
                    .corner_radius(10.0)
                    .inner_margin(12.0),
            )
            .show_inside(ui, |ui| {
                ui.heading("Northclock measurements");
                render_overlay_result(ui, "CPU", self.cpu.as_ref());
                render_overlay_result(ui, "GPU", self.gpu.as_ref());
                ui.small("Read-only; every displayed value includes its backend source.");
            });
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }
}

fn render_overlay_result(ui: &mut egui::Ui, label: &str, result: Option<&CommandEnvelope>) {
    let Some(result) = result else {
        ui.label(format!("{label}: waiting"));
        return;
    };
    if let Some(values) = result.data.as_ref().and_then(serde_json::Value::as_array) {
        for value in values {
            let measurement = value.get("value").and_then(serde_json::Value::as_f64);
            let unit = value.get("unit").and_then(serde_json::Value::as_str);
            let source = value.get("source").and_then(serde_json::Value::as_str);
            if let (Some(measurement), Some(unit), Some(source)) = (measurement, unit, source) {
                ui.label(format!("{label}: {measurement:.1} {unit} · {source}"));
            }
        }
    } else if let Some(error) = &result.error {
        ui.label(format!("{label}: {}", error.message));
    }
}

struct NorthclockApp {
    service: ApplicationService<WindowsPlatform>,
    result: Option<CommandEnvelope>,
    experimental_session: bool,
    confirmation: String,
    curve_offset: i64,
    previewed_plan: Option<OperationPlan>,
    apply_receipt: Option<ApplyReceipt>,
}

impl Default for NorthclockApp {
    fn default() -> Self {
        let platform = WindowsPlatform::new();
        let service = match WindowsPlatform::local_app_data_dir() {
            Ok(root) => {
                ApplicationService::with_storage(platform, northclock_core::Storage::new(root))
            }
            Err(_) => ApplicationService::new(platform),
        };
        Self {
            service,
            result: None,
            experimental_session: false,
            confirmation: String::new(),
            curve_offset: -10,
            previewed_plan: None,
            apply_receipt: None,
        }
    }
}

impl NorthclockApp {
    fn execute(&mut self, command: ApplicationCommand) {
        self.result = Some(self.service.execute(command));
    }

    fn preview_curve_optimizer(&mut self) {
        let envelope = self.service.execute(ApplicationCommand::OperationPreview(
            OperationRequest::cpu_curve_optimizer(self.curve_offset),
        ));
        self.previewed_plan = envelope
            .data
            .as_ref()
            .and_then(|data| serde_json::from_value(data.clone()).ok());
        self.result = Some(envelope);
    }

    fn apply_preview(&mut self) {
        let Some(plan) = self.previewed_plan.clone() else {
            return;
        };
        let acknowledgement = (!self.confirmation.is_empty()).then(|| self.confirmation.clone());
        let envelope = self.service.execute(ApplicationCommand::OperationApply {
            plan,
            experimental: self.experimental_session,
            apply: true,
            risk_acknowledgement: acknowledgement,
        });
        self.apply_receipt = envelope
            .data
            .as_ref()
            .and_then(|data| serde_json::from_value(data.clone()).ok());
        self.result = Some(envelope);
        self.confirmation.clear();
    }

    fn rollback_apply(&mut self) {
        let Some(receipt) = self.apply_receipt.clone() else {
            return;
        };
        let acknowledgement = (!self.confirmation.is_empty()).then(|| self.confirmation.clone());
        let envelope = self.service.execute(ApplicationCommand::OperationRollback {
            receipt,
            experimental: self.experimental_session,
            apply: true,
            risk_acknowledgement: acknowledgement,
        });
        if envelope.status == northclock_core::CommandStatus::Success {
            self.apply_receipt = None;
            self.previewed_plan = None;
        }
        self.result = Some(envelope);
        self.confirmation.clear();
    }
}

impl eframe::App for NorthclockApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("header").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Northclock");
                ui.label("read-only by default");
            });
        });

        egui::Panel::left("actions")
            .resizable(false)
            .default_size(260.0)
            .show_inside(ui, |ui| {
                ui.heading("Diagnostics");
                if ui.button("Hardware doctor").clicked() {
                    self.execute(ApplicationCommand::Doctor);
                }
                if ui.button("CPU identity").clicked() {
                    self.execute(ApplicationCommand::CpuIdentity);
                }
                if ui.button("CPU measurements").clicked() {
                    self.execute(ApplicationCommand::CpuMeasurements);
                }
                if ui.button("CPU workload (5 seconds)").clicked() {
                    self.execute(ApplicationCommand::CpuWorkload {
                        duration_ms: 5_000,
                        threads: 1,
                    });
                }
                if ui.button("GPU inventory").clicked() {
                    self.execute(ApplicationCommand::GpuDevices);
                }
                if ui.button("GPU measurements").clicked() {
                    self.execute(ApplicationCommand::GpuMeasurements { stable_id: None });
                }
                if ui.button("Power plans").clicked() {
                    self.execute(ApplicationCommand::PowerPlans);
                }
                if ui.button("Windows system status").clicked() {
                    self.execute(ApplicationCommand::SystemStatus);
                }
                if ui.button("System memory test").clicked() {
                    self.execute(ApplicationCommand::SystemMemoryTest(
                        MemoryTestConfig::default(),
                    ));
                }
                if ui.button("VRAM test").clicked() {
                    self.execute(ApplicationCommand::VramTest {
                        adapter: None,
                        bytes: 256 * 1024 * 1024,
                        timeout_ms: 30_000,
                    });
                }
                if ui.button("Frame capture (2 seconds)").clicked() {
                    self.execute(ApplicationCommand::FrameCapture { duration_ms: 2_000 });
                }
                if ui.button("Recent WHEA events").clicked() {
                    self.execute(ApplicationCommand::WheaEvents {
                        duration_ms: 60_000,
                    });
                }
                if ui.button("Settings").clicked() {
                    self.execute(ApplicationCommand::SettingsShow);
                }
                if ui.button("Profiles").clicked() {
                    self.execute(ApplicationCommand::ProfilesList);
                }

                ui.separator();
                ui.heading("Experimental writes");
                ui.checkbox(
                    &mut self.experimental_session,
                    "Enable for this session only",
                );
                ui.add(
                    egui::Slider::new(&mut self.curve_offset, -50..=50)
                        .text("Curve Optimizer"),
                );
                if ui.button("Preview captured state").clicked() {
                    self.preview_curve_optimizer();
                }
                ui.label("Type the full acknowledgement for this operation:");
                ui.text_edit_singleline(&mut self.confirmation);
                let confirmed = self.experimental_session
                    && self.confirmation == RISK_ACKNOWLEDGEMENT
                    && self.previewed_plan.is_some();
                if ui
                    .add_enabled(confirmed, egui::Button::new("Apply previewed operation"))
                    .clicked()
                {
                    self.apply_preview();
                }
                let rollback_confirmed = self.experimental_session
                    && self.confirmation == RISK_ACKNOWLEDGEMENT
                    && self.apply_receipt.is_some();
                if ui
                    .add_enabled(rollback_confirmed, egui::Button::new("Roll back last apply"))
                    .clicked()
                {
                    self.rollback_apply();
                }
                ui.small("Apply requires every safety gate and readback. Failed validation attempts rollback and reports any rollback failure. This UI does not imply hardware validation.");
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Result");
            if let Some(result) = &self.result {
                ui.horizontal(|ui| {
                    ui.label(format!("{}: {:?}", result.command, result.status));
                    if let Some(capability) = &result.capability {
                        ui.label(format!("{:?}", capability.state));
                    }
                });
                if let Some(capability) = &result.capability {
                    ui.label(format!("Backend: {}", capability.backend));
                    ui.label(&capability.detail);
                    ui.label(format!(
                        "Physical hardware verified: {}",
                        capability.hardware_verified
                    ));
                }
                let rendered = serde_json::to_string_pretty(result)
                    .unwrap_or_else(|error| format!("result serialization failed: {error}"));
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.monospace(rendered);
                });
            } else {
                ui.label("Choose a diagnostic. Missing data is reported as unavailable.");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experimental_state_is_session_local() {
        let app = NorthclockApp::default();
        assert!(!app.experimental_session);
        assert!(app.confirmation.is_empty());
        assert!(app.previewed_plan.is_none());
        assert!(app.apply_receipt.is_none());
    }
}
