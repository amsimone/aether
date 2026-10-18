//! # aether-ui
//!
//! The Aether graphical application. Phase 1: design editor with a 3D
//! viewport showing the rocket, a component tree, and a properties panel.
//! Phase 2: flight simulation with trajectory plots.
//!
//! Run with: `cargo run -p rocket-ui`

mod app;
mod editing;
mod plots;
mod properties;
mod reference;
mod simulate;
mod stability_gauge;
mod tree;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([980.0, 640.0])
            .with_title("Aether — Rocket Simulator"),
        renderer: eframe::Renderer::Wgpu,
        depth_buffer: 32,
        ..Default::default()
    };

    eframe::run_native(
        "Aether",
        options,
        Box::new(|cc| Ok(Box::new(app::AetherApp::new(cc)))),
    )
}
