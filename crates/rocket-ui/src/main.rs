//! # aether-ui — Phase-0 Task 2 Spike
//!
//! Proves the egui + wgpu integration (ADR-0002):
//! - A 3D cube rendered with wgpu, embedded in an egui panel.
//! - The cube shares egui's wgpu instance (no separate GPU context).
//! - A slider in a side panel controls the cube's rotation speed.
//! - Continuous redraw for smooth animation.
//!
//! Run with: `cargo run -p rocket-ui`

mod cube;

use cube::CubeViewport;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_title("Aether — Phase 0 Spike (egui + wgpu)"),
        ..Default::default()
    };

    eframe::run_native(
        "Aether Phase-0 Spike",
        options,
        Box::new(|cc| Ok(Box::new(SpikeApp::new(cc)))),
    )
}

struct SpikeApp {
    viewport: CubeViewport,
}

impl SpikeApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Dark theme matching the plan's §11.5 visual identity.
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(0x1a, 0x1d, 0x23);
        visuals.window_fill = egui::Color32::from_rgb(0x23, 0x28, 0x30);
        visuals.extreme_bg_color = egui::Color32::from_rgb(0x10, 0x12, 0x16);
        cc.egui_ctx.set_visuals(visuals);

        let mut viewport = CubeViewport::default();
        viewport.init(cc);

        Self { viewport }
    }
}

impl eframe::App for SpikeApp {
    /// Non-UI logic: advance the rotation angle.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i: &egui::InputState| i.stable_dt).max(0.016);
        self.viewport.angle += self.viewport.speed * dt;
        ctx.request_repaint();
    }

    /// UI: side panel (controls) + central panel (3D viewport).
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Right panel: properties / controls.
        egui::Panel::right("properties")
            .resizable(true)
            .default_size(280.0)
            .show(ui, |ui| {
                ui.heading("Properties");
                ui.separator();
                ui.add_space(8.0);

                ui.label("3D Viewport (Phase-0 spike)");
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Rotation speed:");
                    ui.add(
                        egui::Slider::new(&mut self.viewport.speed, -3.0..=3.0)
                            .suffix(" rad/s")
                            .fixed_decimals(2),
                    );
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Angle:");
                    ui.label(format!("{:.2} rad", self.viewport.angle));
                });

                ui.add_space(8.0);
                ui.label(format!(
                    "GPU resources: {}",
                    if self.viewport.initialized {
                        "initialized"
                    } else {
                        "not initialized"
                    }
                ));

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("This spike proves:")
                        .color(egui::Color32::from_rgb(0x4a, 0xa8, 0xff))
                        .strong(),
                );
                ui.label("• wgpu rendering embedded in egui");
                ui.label("• Shared wgpu instance (no 2nd GPU context)");
                ui.label("• Real-time UI → 3D communication");
                ui.label("• Continuous redraw for animation");
            });

        // Central panel: the 3D viewport.
        egui::CentralPanel::default().show(ui, |ui| {
            self.viewport.show(ui);
        });
    }
}
