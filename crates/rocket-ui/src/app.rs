//! The main Aether application.
//!
//! Phase 1 design editor: a user can build a complete model rocket
//! visually and see live CG/CP readouts. The 3D viewport updates in
//! real time as components are added, edited, or deleted.

#![allow(
    clippy::unwrap_used,
    clippy::useless_format,
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::similar_names,
    clippy::must_use_candidate,
    clippy::needless_return,
    clippy::redundant_closure,
    clippy::manual_map,
    clippy::unnecessary_unwrap,
    clippy::format_push_string,
    clippy::uninlined_format_args,
    clippy::map_unwrap_or,
    clippy::filter_map_next,
    clippy::redundant_closure_call,
    clippy::ignored_unit_patterns
)]

use eframe::egui;
use rocket_core::{
    BodyTube, Component, ComponentId, FinPlanform, FinSet, LaunchLug, Length, Mass, MassObject,
    MassObjectKind, NoseCone, NoseShape, Rocket,
};
use rocket_io::{load_from_json, save_to_json};
use rocket_render::RocketViewport;

use crate::properties::show_properties;
use crate::stability_gauge::show_stability_gauge;
use crate::tree::show_component_tree;

/// The main Aether eframe app.
pub struct AetherApp {
    /// The rocket being designed.
    pub rocket: Rocket,
    /// The 3D viewport.
    pub viewport: RocketViewport,
    /// Currently selected component ID (if any).
    pub selected: Option<ComponentId>,
    /// Counter for generating new component IDs.
    next_id: u64,
    /// Path of the currently-loaded file (for Save vs Save As).
    file_path: Option<std::path::PathBuf>,
    /// Whether the rocket has unsaved changes.
    dirty: bool,
    /// Pending file dialog operation (processed in `logic`).
    file_dialog: Option<FileDialog>,
}

/// Pending file dialog operation (processed in `logic`).
enum FileDialog {
    Open,
    Save,
    SaveAs,
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
        visuals.hyperlink_color = egui::Color32::from_rgb(0x4a, 0xa8, 0xff);
        cc.egui_ctx.set_visuals(visuals);

        let mut app = Self {
            rocket: Rocket::new("Untitled Rocket"),
            viewport: RocketViewport::default(),
            selected: None,
            next_id: 100,
            file_path: None,
            dirty: false,
            file_dialog: None,
        };

        // Add a default nose cone + body tube so the viewport isn't empty.
        let balsa = app.find_material("Balsa");
        let card = app.find_material("Cardboard (body tube)");
        app.rocket.root.push(Component::NoseCone(NoseCone {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: balsa,
            shape: NoseShape::TangentOgive,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(24.8),
            wall_thickness: Length::millimeters(1.0),
        }));
        app.rocket.root.push(Component::BodyTube(BodyTube {
            id: ComponentId::new(2),
            position: Length::millimeters(70.0),
            material: card,
            length: Length::millimeters(180.0),
            outer_diameter: Length::millimeters(24.8),
            inner_diameter: Length::millimeters(24.1),
        }));

        app.viewport.update_mesh(&app.rocket, cc);
        app
    }

    /// Find a material ID by name in the rocket's library.
    pub fn find_material(&self, name: &str) -> ComponentId {
        self.rocket
            .materials
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.id)
            .unwrap_or_else(|| {
                self.rocket
                    .materials
                    .iter()
                    .next()
                    .expect("material library should not be empty")
                    .id
            })
    }

    /// Generate a new unique component ID.
    pub fn new_id(&mut self) -> ComponentId {
        let id = ComponentId::new(self.next_id);
        self.next_id += 1;
        id
    }

    /// Mark the rocket as dirty and trigger a mesh rebuild on next frame.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.viewport.mark_dirty();
    }
}

impl eframe::App for AetherApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Rebuild the 3D mesh if the rocket has changed.
        // This uses the stored render state (captured during update_mesh).
        self.viewport.rebuild_if_changed(&self.rocket);

        // Handle pending file dialogs.
        if let Some(dialog) = self.file_dialog.take() {
            self.handle_file_dialog(dialog);
        }

        // Request repaint if there are unsaved changes or active editing.
        let _ = ctx;
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.show_menu_bar(ui, frame);

        // Right panel: component tree + properties + stability gauge.
        egui::Panel::right("right_panel")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    show_component_tree(self, ui);
                    ui.separator();
                    show_properties(self, ui);
                    ui.separator();
                    show_stability_gauge(self, ui);
                });
            });

        // Bottom panel: status bar.
        egui::Panel::bottom("status_bar")
            .resizable(false)
            .exact_size(28.0)
            .show_separator_line(false)
            .show(ui, |ui| {
                self.show_status_bar(ui);
            });

        // Central panel: the 3D viewport.
        egui::CentralPanel::default().show(ui, |ui| {
            self.viewport.show(ui, &self.rocket);
        });
    }
}

// ---- File operations ----

impl AetherApp {
    fn show_menu_bar(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.horizontal(|ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.rocket = Rocket::new("Untitled Rocket");
                    self.selected = None;
                    self.file_path = None;
                    self.dirty = false;
                    self.mark_dirty();
                    ui.close();
                }
                if ui.button("Open…").clicked() {
                    self.file_dialog = Some(FileDialog::Open);
                    ui.close();
                }
                if ui.button("Save").clicked() && self.file_path.is_some() {
                    self.file_dialog = Some(FileDialog::Save);
                    ui.close();
                }
                if ui.button("Save As…").clicked() {
                    self.file_dialog = Some(FileDialog::SaveAs);
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    ui.close();
                }
            });

            ui.menu_button("Add", |ui| {
                if ui.button("Nose Cone").clicked() {
                    self.add_nose_cone();
                    ui.close();
                }
                if ui.button("Body Tube").clicked() {
                    self.add_body_tube();
                    ui.close();
                }
                if ui.button("Fin Set").clicked() {
                    self.add_fin_set();
                    ui.close();
                }
                if ui.button("Launch Lug").clicked() {
                    self.add_launch_lug();
                    ui.close();
                }
                if ui.button("Mass Object (Ballast)").clicked() {
                    self.add_mass_object();
                    ui.close();
                }
            });

            ui.menu_button("Help", |ui| {
                ui.label("Aether — Rocket Simulator");
                ui.label(format!("Version: {}", env!("CARGO_PKG_VERSION")));
                ui.label("Phase 1: Design Editor");
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.dirty {
                    ui.label(
                        egui::RichText::new("● Unsaved changes")
                            .color(egui::Color32::from_rgb(0xf0, 0xa0, 0x20)),
                    );
                } else if self.file_path.is_some() {
                    ui.label(
                        egui::RichText::new("✓ Saved")
                            .color(egui::Color32::from_rgb(0x40, 0xc0, 0x60)),
                    );
                }
            });
        });
    }

    fn show_status_bar(&self, ui: &mut egui::Ui) {
        let mp = self.rocket.mass_properties();
        let component_count = self.rocket.root.components.len();

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(" {} components ", component_count))
                    .color(egui::Color32::from_gray(180)),
            );
            ui.separator();
            ui.label(format!(" Mass: {:.1} g ", mp.mass.as_grams()));
            ui.separator();
            ui.label(format!(" CG: {:.1} mm ", mp.cg_x.as_millimeters()));
            ui.separator();
            ui.label(
                egui::RichText::new(" CP: pending (Task 1.5) ")
                    .color(egui::Color32::from_rgb(0xf0, 0xa0, 0x20)),
            );
            ui.separator();
            if let Some(path) = &self.file_path {
                ui.label(format!(" {} ", path.display()));
            } else {
                ui.label(" (unsaved) ");
            }
        });
    }

    fn handle_file_dialog(&mut self, dialog: FileDialog) {
        match dialog {
            FileDialog::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Rocket design", &["rocket"])
                    .pick_file()
                {
                    match std::fs::read_to_string(&path) {
                        Ok(text) => match load_from_json(&text) {
                            Ok(rocket) => {
                                self.rocket = rocket;
                                self.selected = None;
                                self.file_path = Some(path);
                                self.dirty = false;
                                self.mark_dirty();
                            }
                            Err(e) => eprintln!("Failed to load: {e}"),
                        },
                        Err(e) => eprintln!("Failed to read file: {e}"),
                    }
                }
            }
            FileDialog::Save | FileDialog::SaveAs => {
                let path = if matches!(dialog, FileDialog::Save) {
                    self.file_path.clone()
                } else {
                    None
                };
                let path = path.or_else(|| {
                    rfd::FileDialog::new()
                        .add_filter("Rocket design", &["rocket"])
                        .set_file_name(format!("{}.rocket", self.rocket.name))
                        .save_file()
                });
                if let Some(path) = path {
                    match save_to_json(&self.rocket) {
                        Ok(text) => match std::fs::write(&path, text) {
                            Ok(_) => {
                                self.file_path = Some(path);
                                self.dirty = false;
                            }
                            Err(e) => eprintln!("Failed to write file: {e}"),
                        },
                        Err(e) => eprintln!("Failed to serialize: {e}"),
                    }
                }
            }
        }
    }
}

// ---- Component addition helpers ----

impl AetherApp {
    pub fn add_nose_cone(&mut self) {
        let id = self.new_id();
        let material = self.find_material("Balsa");
        let pos = self.rocket.root.length();
        self.rocket.root.push(Component::NoseCone(NoseCone {
            id,
            position: pos,
            material,
            shape: NoseShape::TangentOgive,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(25.0),
            wall_thickness: Length::millimeters(1.0),
        }));
        self.selected = Some(id);
        self.mark_dirty();
    }

    pub fn add_body_tube(&mut self) {
        let id = self.new_id();
        let material = self.find_material("Cardboard (body tube)");
        let pos = self.rocket.root.length();
        self.rocket.root.push(Component::BodyTube(BodyTube {
            id,
            position: pos,
            material,
            length: Length::millimeters(150.0),
            outer_diameter: Length::millimeters(25.0),
            inner_diameter: Length::millimeters(24.0),
        }));
        self.selected = Some(id);
        self.mark_dirty();
    }

    pub fn add_fin_set(&mut self) {
        let id = self.new_id();
        let material = self.find_material("Balsa");
        let mount = self
            .rocket
            .root
            .components
            .iter()
            .rev()
            .find(|c| matches!(c, Component::BodyTube(_)))
            .map(rocket_core::Component::id);
        let pos = self.rocket.root.length();
        self.rocket.root.push(Component::FinSet(FinSet {
            id,
            count: 3,
            planform: FinPlanform::Trapezoidal {
                root_chord: Length::millimeters(50.0),
                tip_chord: Length::millimeters(25.0),
                span: Length::millimeters(35.0),
                sweep: Length::millimeters(25.0),
            },
            thickness: Length::millimeters(3.0),
            airfoil: rocket_core::Airfoil::Square,
            mount: mount.unwrap_or(ComponentId::new(0)),
            position: pos,
            material,
        }));
        self.selected = Some(id);
        self.mark_dirty();
    }

    pub fn add_launch_lug(&mut self) {
        let id = self.new_id();
        let material = self.find_material("Cardboard (body tube)");
        self.rocket.root.push(Component::LaunchLug(LaunchLug {
            id,
            position: Length::millimeters(50.0),
            material,
            length: Length::millimeters(25.0),
            inner_diameter: Length::millimeters(5.0),
            outer_diameter: Length::millimeters(7.0),
        }));
        self.selected = Some(id);
        self.mark_dirty();
    }

    pub fn add_mass_object(&mut self) {
        let id = self.new_id();
        let pos = self.rocket.root.length();
        self.rocket.root.push(Component::MassObject(MassObject {
            id,
            position: pos,
            mass: Mass::grams(10.0),
            kind: MassObjectKind::Ballast,
        }));
        self.selected = Some(id);
        self.mark_dirty();
    }

    pub fn delete_selected(&mut self) {
        if let Some(id) = self.selected {
            self.rocket.root.components.retain(|c| c.id() != id);
            self.selected = None;
            self.mark_dirty();
        }
    }

    pub fn duplicate_selected(&mut self) {
        if let Some(id) = self.selected {
            if let Some(component) = self.rocket.root.find(id) {
                let mut new_component = component.clone();
                let new_id = self.new_id();
                match &mut new_component {
                    Component::NoseCone(c) => c.id = new_id,
                    Component::BodyTube(c) => c.id = new_id,
                    Component::Transition(c) => c.id = new_id,
                    Component::Coupler(c) => c.id = new_id,
                    Component::FinSet(c) => c.id = new_id,
                    Component::LaunchLug(c) => c.id = new_id,
                    Component::Pod(c) => c.id = new_id,
                    Component::MassObject(c) => c.id = new_id,
                }
                self.rocket.root.push(new_component);
                self.selected = Some(new_id);
                self.mark_dirty();
            }
        }
    }
}
