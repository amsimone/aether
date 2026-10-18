//! The main Aether application.

#![allow(clippy::unwrap_used, clippy::useless_format)]

use eframe::egui;
use rocket_core::{BodyTube, Component, ComponentId, Length, NoseCone, NoseShape, Rocket};
use rocket_render::RocketViewport;

/// The main Aether eframe app.
pub struct AetherApp {
    /// The rocket being designed.
    rocket: Rocket,
    /// The 3D viewport.
    viewport: RocketViewport,
    /// Currently selected component ID (if any).
    selected: Option<ComponentId>,
}

impl AetherApp {
    /// Create a new app, initializing the dark theme and 3D viewport.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Dark theme matching the plan's §11.5 visual identity.
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(0x1a, 0x1d, 0x23);
        visuals.window_fill = egui::Color32::from_rgb(0x23, 0x28, 0x30);
        visuals.extreme_bg_color = egui::Color32::from_rgb(0x10, 0x12, 0x16);
        visuals.selection.bg_fill = egui::Color32::from_rgb(0x4a, 0xa8, 0xff);
        cc.egui_ctx.set_visuals(visuals);

        // Start with a sample rocket so the viewport isn't empty.
        let mut rocket = Rocket::new("Sample Rocket");
        let balsa_id = rocket
            .materials
            .iter()
            .find(|m| m.name == "Balsa")
            .expect("balsa should be in the built-in library")
            .id;
        let card_id = rocket
            .materials
            .iter()
            .find(|m| m.name == "Cardboard (body tube)")
            .expect("cardboard should be in the built-in library")
            .id;
        rocket.root.push(Component::NoseCone(NoseCone {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: balsa_id,
            shape: NoseShape::TangentOgive,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(24.8),
            wall_thickness: Length::millimeters(1.0),
        }));
        rocket.root.push(Component::BodyTube(BodyTube {
            id: ComponentId::new(2),
            position: Length::millimeters(70.0),
            material: card_id,
            length: Length::millimeters(180.0),
            outer_diameter: Length::millimeters(24.8),
            inner_diameter: Length::millimeters(24.1),
        }));

        let mut viewport = RocketViewport::default();
        viewport.update_mesh(&rocket, cc);

        Self {
            rocket,
            viewport,
            selected: None,
        }
    }
}

impl eframe::App for AetherApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // No per-frame logic yet; the viewport handles its own input.
        let _ = ctx;
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Right panel: component tree + properties.
        egui::Panel::right("properties")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| {
                self.show_component_tree(ui);
                ui.separator();
                self.show_properties(ui);
                ui.separator();
                self.show_mass_readouts(ui);
            });

        // Central panel: the 3D viewport.
        egui::CentralPanel::default().show(ui, |ui| {
            self.viewport.show(ui, &self.rocket);
        });
    }
}

impl AetherApp {
    fn show_component_tree(&mut self, ui: &mut egui::Ui) {
        ui.heading("Components");
        ui.add_space(4.0);

        let component_count = self.rocket.root.components.len();
        if component_count == 0 {
            ui.label(egui::RichText::new("(no components)").color(egui::Color32::from_gray(120)));
            return;
        }

        // Collect component summaries first (to avoid borrowing self.rocket
        // while also borrowing self.selected mutably below).
        let summaries: Vec<(ComponentId, String, String)> = self
            .rocket
            .root
            .iter()
            .map(|c| {
                let kind = match c {
                    Component::NoseCone(_) => "Nose Cone",
                    Component::BodyTube(_) => "Body Tube",
                    Component::Transition(_) => "Transition",
                    Component::Coupler(_) => "Coupler",
                    Component::FinSet(_) => "Fin Set",
                    Component::LaunchLug(_) => "Launch Lug",
                    Component::Pod(_) => "Pod",
                    Component::MassObject(_) => "Mass Object",
                };
                let detail = format!("{:.0} mm", c.length().as_millimeters());
                (c.id(), kind.to_string(), detail)
            })
            .collect();

        for (id, kind, detail) in &summaries {
            let selected = self.selected == Some(*id);
            let response = ui.selectable_label(selected, format!("{kind} ({detail})"));
            if response.clicked() {
                self.selected = Some(*id);
            }
        }
    }

    fn show_properties(&mut self, ui: &mut egui::Ui) {
        ui.heading("Properties");
        ui.add_space(4.0);

        let Some(selected_id) = self.selected else {
            ui.label(
                egui::RichText::new("(no component selected)").color(egui::Color32::from_gray(120)),
            );
            return;
        };

        // Find the selected component and show its properties.
        // We need to clone the relevant info to avoid borrowing self.rocket
        // while also mutating it via the slider.
        let component = self.rocket.root.find(selected_id);
        if component.is_none() {
            ui.label("(component not found)");
            return;
        }
        let c = component.unwrap();

        match c {
            Component::NoseCone(nc) => {
                ui.label(format!("Type: Nose Cone"));
                ui.label(format!("Shape: {:?}", nc.shape));
                ui.label(format!("Length: {:.1} mm", nc.length.as_millimeters()));
                ui.label(format!(
                    "Base diameter: {:.1} mm",
                    nc.base_diameter.as_millimeters()
                ));
            }
            Component::BodyTube(bt) => {
                ui.label("Type: Body Tube");
                ui.label(format!("Length: {:.1} mm", bt.length.as_millimeters()));
                ui.label(format!(
                    "Outer diameter: {:.1} mm",
                    bt.outer_diameter.as_millimeters()
                ));
                ui.label(format!(
                    "Inner diameter: {:.1} mm",
                    bt.inner_diameter.as_millimeters()
                ));
            }
            Component::FinSet(fs) => {
                ui.label("Type: Fin Set");
                ui.label(format!("Fin count: {}", fs.count));
                ui.label(format!(
                    "Thickness: {:.1} mm",
                    fs.thickness.as_millimeters()
                ));
                ui.label(format!("Airfoil: {:?}", fs.airfoil));
            }
            Component::Transition(t) => {
                ui.label("Type: Transition");
                ui.label(format!("Length: {:.1} mm", t.length.as_millimeters()));
                ui.label(format!(
                    "Fore diameter: {:.1} mm",
                    t.fore_diameter.as_millimeters()
                ));
                ui.label(format!(
                    "Aft diameter: {:.1} mm",
                    t.aft_diameter.as_millimeters()
                ));
            }
            Component::LaunchLug(ll) => {
                ui.label("Type: Launch Lug");
                ui.label(format!("Length: {:.1} mm", ll.length.as_millimeters()));
                ui.label(format!(
                    "Inner diameter: {:.1} mm",
                    ll.inner_diameter.as_millimeters()
                ));
            }
            Component::Pod(p) => {
                ui.label("Type: Pod");
                ui.label(format!("Length: {:.1} mm", p.length.as_millimeters()));
                ui.label(format!(
                    "Outer diameter: {:.1} mm",
                    p.outer_diameter.as_millimeters()
                ));
            }
            Component::Coupler(c) => {
                ui.label("Type: Coupler");
                ui.label(format!("Length: {:.1} mm", c.length.as_millimeters()));
                ui.label(format!(
                    "Outer diameter: {:.1} mm",
                    c.outer_diameter.as_millimeters()
                ));
            }
            Component::MassObject(mo) => {
                ui.label("Type: Mass Object");
                ui.label(format!("Mass: {:.1} g", mo.mass.as_grams()));
                ui.label(format!("Position: {:.1} mm", mo.position.as_millimeters()));
                ui.label(format!("Kind: {:?}", mo.kind));
            }
        }
    }

    fn show_mass_readouts(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mass & Stability");
        ui.add_space(4.0);

        let mp = self.rocket.mass_properties();

        ui.horizontal(|ui| {
            ui.label("Total mass:");
            ui.label(format!("{:.1} g", mp.mass.as_grams()));
        });
        ui.horizontal(|ui| {
            ui.label("CG (from nose):");
            ui.label(format!("{:.1} mm", mp.cg_x.as_millimeters()));
        });
        ui.horizontal(|ui| {
            ui.label("I_axial:");
            ui.label(format!("{:.4} kg·m²", mp.i_axial));
        });
        ui.horizontal(|ui| {
            ui.label("I_transverse:");
            ui.label(format!("{:.4} kg·m²", mp.i_transverse));
        });

        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("CP: (not yet computed — Task 1.5)")
                .color(egui::Color32::from_rgb(0xf0, 0xa0, 0x20)),
        );
    }
}
