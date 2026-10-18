//! # aether-ui
//!
//! The Aether graphical application. Phase 1: design editor with a 3D
//! viewport showing the rocket, a component tree, and a properties panel.
//!
//! Run with: `cargo run -p rocket-ui`

mod app;
mod properties;
mod stability_gauge;
mod tree;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Aether — Rocket Simulator"),
        ..Default::default()
    };

    eframe::run_native(
        "Aether",
        options,
        Box::new(|cc| Ok(Box::new(app::AetherApp::new(cc)))),
    )
}
