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

use crate::editing::{Decision, DesignDocument, DocumentAction, ReplacementGuard};
use eframe::egui;
use rocket_core::{
    BodyTube, Component, ComponentId, Coupler, FinPlanform, FinSet, LaunchLug, Length, Mass,
    MassObject, MassObjectKind, NoseCone, NoseShape, Pod, Rocket, Transition,
};
use rocket_io::{load_project_from_path, save_project_to_path, Project};
use rocket_render::RocketViewport;

use crate::properties::show_properties;
use crate::reference::reference_rocket;
use crate::simulate::SimPanelState;
use crate::stability_gauge::show_stability_gauge;
use crate::tree::show_component_tree;

/// Which right-panel tab is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightPanelTab {
    Design,
    Simulate,
    Results,
    Aerodynamics,
}

/// The main Aether eframe app.
pub struct AetherApp {
    /// The engineering document and its edit history.
    pub document: DesignDocument,
    /// Rejection of a component edit, scoped to the edited component.
    pub edit_error: Option<(ComponentId, String)>,
    /// The 3D viewport.
    pub viewport: RocketViewport,
    /// Currently selected component ID (if any).
    pub selected: Option<ComponentId>,
    /// Counter for generating new component IDs.
    next_id: u64,
    /// Path of the currently-loaded file (for Save vs Save As).
    file_path: Option<std::path::PathBuf>,
    /// Pending file dialog operation (processed in `logic`).
    file_dialog: Option<FileDialog>,
    /// The simulation panel state (motor selector, config, results).
    pub sim_panel: SimPanelState,
    /// Which right-panel tab is active.
    right_tab: RightPanelTab,
    file_error: Option<String>,
    replacement: ReplacementGuard,
    allow_close: bool,
    pub results: crate::results::ResultsState,
    pub aero_panel: crate::aerodynamics::AeroPanelState,
}

/// Pending file dialog operation (processed in `logic`).
enum FileDialog {
    Open,
    Save,
    SaveAs,
    ExportCsv(String),
    ExportRun(String),
    ImportRun,
    ImportDataset,
    PrepareCfd(bool),
}

impl AetherApp {
    /// Create a new app, initializing the dark theme and 3D viewport.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(23, 29, 38);
        visuals.window_fill = egui::Color32::from_rgb(30, 38, 49);
        visuals.extreme_bg_color = egui::Color32::from_rgb(15, 21, 29);
        visuals.selection.bg_fill = egui::Color32::from_rgb(29, 85, 110);
        visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(115, 220, 242));
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(38, 48, 62);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(48, 65, 82);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(35, 97, 122);
        visuals.hyperlink_color = egui::Color32::from_rgb(115, 220, 242);
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        cc.egui_ctx.set_visuals_of(egui::Theme::Dark, visuals);
        cc.egui_ctx.style_mut_of(egui::Theme::Dark, |style| {
            style.spacing.item_spacing = egui::vec2(10.0, 10.0);
            style.spacing.button_padding = egui::vec2(12.0, 7.0);
            style.spacing.interact_size.y = 30.0;
            style
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
            style
                .text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
            style
                .text_styles
                .insert(egui::TextStyle::Heading, egui::FontId::proportional(20.0));
        });

        let mut app = Self {
            document: DesignDocument::from_project(crate::simulate::reference_project(
                reference_rocket(),
            )),
            edit_error: None,
            viewport: RocketViewport::default(),
            selected: Some(ComponentId::new(1)),
            next_id: 100,
            file_path: None,
            file_dialog: None,
            sim_panel: SimPanelState::default(),
            right_tab: RightPanelTab::Design,
            file_error: None,
            replacement: ReplacementGuard::default(),
            allow_close: false,
            results: crate::results::ResultsState::default(),
            aero_panel: crate::aerodynamics::AeroPanelState::default(),
        };

        app.viewport.update_mesh(app.document.rocket(), cc);
        app
    }

    /// Find a material ID by name in the rocket's library.
    pub fn find_material(&self, name: &str) -> ComponentId {
        self.document
            .rocket()
            .materials
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.id)
            .unwrap_or_else(|| {
                self.document
                    .rocket()
                    .materials
                    .iter()
                    .next()
                    .expect("material library should not be empty")
                    .id
            })
    }

    /// Generate a new unique component ID.
    pub fn new_id(&mut self) -> ComponentId {
        fn highest_id(stage: &rocket_core::Stage) -> u64 {
            stage
                .iter()
                .map(|c| c.id().as_u64())
                .max()
                .unwrap_or(0)
                .max(
                    stage
                        .upper_stage
                        .as_ref()
                        .map_or(0, |upper| highest_id(upper)),
                )
        }
        self.next_id = self
            .next_id
            .max(highest_id(&self.document.rocket().root).saturating_add(1));
        let id = ComponentId::new(self.next_id);
        self.next_id += 1;
        id
    }

    /// Apply one validated component edit, coalescing pointer updates.
    pub fn edit_component(&mut self, component: Component, pointer_gesture: bool) {
        let id = component.id();
        match self.document.edit_component(component, pointer_gesture) {
            Ok(changed) => {
                self.edit_error = None;
                if changed {
                    self.mark_dirty();
                }
            }
            Err(error) => self.edit_error = Some((id, error.to_string())),
        }
    }

    fn undo_edit(&mut self) {
        if self.document.undo() {
            self.edit_error = None;
            self.reconcile_selection();
            self.mark_dirty();
        }
    }

    fn redo_edit(&mut self) {
        if self.document.redo() {
            self.edit_error = None;
            self.reconcile_selection();
            self.mark_dirty();
        }
    }

    fn reconcile_selection(&mut self) {
        if self
            .selected
            .is_some_and(|id| self.document.rocket().root.find(id).is_none())
        {
            self.selected = None;
        }
    }

    /// Invalidate derived views after an accepted engineering input change.
    pub fn mark_dirty(&mut self) {
        self.viewport.mark_dirty();
    }
}

impl eframe::App for AetherApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::from(egui::Color32::from_rgb(23, 29, 38)).to_array()
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs(ctx);
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested
            && !self.allow_close
            && (self.document.is_dirty()
                || self.replacement.pending().is_some()
                || self.analysis_running())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_action(DocumentAction::Quit, ctx);
        }
        // Handle pending file dialogs.
        if let Some(dialog) = self.file_dialog.take() {
            self.handle_file_dialog(dialog, ctx);
        }

        if !ctx.input(|i| i.pointer.primary_down()) && !ctx.text_edit_focused() {
            self.document.end_gesture();
        }

        if self.replacement.pending().is_some() {
            return;
        }
        for action in shortcut_actions(ctx) {
            match action {
                Shortcut::Undo => self.undo_edit(),
                Shortcut::Redo => self.redo_edit(),
                Shortcut::Save => {
                    self.file_dialog = Some(if self.file_path.is_some() {
                        FileDialog::Save
                    } else {
                        FileDialog::SaveAs
                    });
                }
                Shortcut::New => self.request_action(DocumentAction::New, ctx),
                Shortcut::Open => self.request_action(DocumentAction::Open, ctx),
                Shortcut::Delete if self.right_tab == RightPanelTab::Design => {
                    self.delete_selected();
                }
                Shortcut::Duplicate if self.right_tab == RightPanelTab::Design => {
                    self.duplicate_selected();
                }
                Shortcut::Delete | Shortcut::Duplicate => {}
            }
        }
        if self.file_dialog.is_some() {
            ctx.request_repaint();
        }
        if self.analysis_running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.show_menu_bar(ui, frame);

        egui::Panel::bottom("status_bar")
            .resizable(false)
            .exact_size(32.0)
            .show_separator_line(true)
            .show(ui, |ui| {
                self.show_status_bar(ui);
            });

        if matches!(
            self.right_tab,
            RightPanelTab::Design | RightPanelTab::Simulate
        ) {
            egui::Panel::left("parts_panel").resizable(true).default_size(235.0)
            .size_range(210.0..=320.0)
            .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(16.0))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("parts_scroll").show(ui, |ui| {
                    show_component_tree(self, ui);
                    ui.add_space(20.0);
                    ui.separator();
                    ui.label(egui::RichText::new("REFERENCE DESIGN").small().weak());
                    ui.label("Aether One");
                    ui.label(egui::RichText::new("250 mm · 3 fins · 18 mm motor\nA simple starting point for testing.").small().weak());
                    if ui.button("Load reference design").clicked() {
                        self.request_action(DocumentAction::Reference, ui.ctx());
                    }
                });
            });

            egui::Panel::right("inspector_panel")
                .resizable(true)
                .default_size(310.0)
                .size_range(280.0..=460.0)
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(18.0))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("inspector_scroll")
                        .show(ui, |ui| match self.right_tab {
                            RightPanelTab::Design => {
                                ui.label(egui::RichText::new("COMPONENT INSPECTOR").small().weak());
                                show_properties(self, ui);
                                ui.add_space(16.0);
                                ui.separator();
                                show_stability_gauge(self, ui);
                            }
                            RightPanelTab::Simulate => {
                                crate::simulate::show_simulate_panel(self, ui);
                            }
                            _ => {}
                        });
                });
        } else if self.right_tab == RightPanelTab::Results {
            egui::Panel::left("runs_panel")
                .default_size(230.0)
                .size_range(190.0..=350.0)
                .resizable(true)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| crate::results::sidebar(self, ui));
                });
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(15, 21, 29))
                    .inner_margin(20.0),
            )
            .show(ui, |ui| {
                if self.right_tab == RightPanelTab::Results {
                    crate::results::show(self, ui);
                    return;
                }
                if self.right_tab == RightPanelTab::Aerodynamics {
                    crate::aerodynamics::show(self, ui);
                    return;
                }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("ASSEMBLY").small().weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Fit view")
                            .on_hover_text(
                                "Frame the visible parts. Double-click the canvas to fit.",
                            )
                            .clicked()
                        {
                            self.viewport.fit_requested = true;
                        }
                        if ui
                            .add_enabled(
                                self.selected.is_some(),
                                egui::Checkbox::new(
                                    &mut self.viewport.isolate_selected,
                                    "Isolate part",
                                ),
                            )
                            .changed()
                        {
                            self.viewport.fit_requested = true;
                        }
                    });
                });
                ui.add_space(8.0);
                ui.heading(self.document.rocket().name.as_str());
                let selection_label = self
                    .selected
                    .and_then(|id| self.document.rocket().root.find(id))
                    .map_or("Select a part in the list or the model".to_string(), |c| {
                        format!("{} selected", crate::tree::component_kind_label(c))
                    });
                ui.label(
                    egui::RichText::new(selection_label)
                        .color(egui::Color32::from_rgb(115, 220, 242)),
                );
                if self
                    .selected
                    .and_then(|id| self.document.rocket().root.find(id))
                    .is_some_and(|c| matches!(c, Component::MassObject(_) | Component::Coupler(_)))
                    && !self.viewport.isolate_selected
                {
                    ui.label(
                        egui::RichText::new("Internal part · Enable Isolate part to view it")
                            .small()
                            .weak(),
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Drag to orbit   ·   Scroll to zoom   ·   Shift + drag to pan",
                    )
                    .small()
                    .weak(),
                );
                if self.document.rocket().root.components.is_empty() {
                    ui.add_space(50.0);
                    ui.heading("Start your next rocket");
                    ui.label(
                        "Add a nose and body from the parts panel, or load the reference design.",
                    );
                    if ui.button("Load Aether One").clicked() {
                        self.request_action(DocumentAction::Reference, ui.ctx());
                    }
                }
                self.viewport
                    .show(ui, self.document.rocket(), &mut self.selected);
            });

        if self.file_dialog.is_some() {
            ui.ctx().request_repaint();
        }
        self.show_unsaved_dialog(ui.ctx());
        if let Some(message) = self
            .file_error
            .clone()
            .filter(|_| self.replacement.pending().is_none())
        {
            egui::Window::new("Could not complete action")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.label(message);
                    if ui.button("Dismiss").clicked() {
                        self.file_error = None;
                    }
                });
        }
    }
}

// ---- File operations ----

impl AetherApp {
    fn show_menu_bar(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("AETHER")
                    .size(19.0)
                    .strong()
                    .color(egui::Color32::from_rgb(115, 220, 242)),
            );
            ui.separator();
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.request_action(DocumentAction::New, ui.ctx());
                    ui.close();
                }
                if ui.button("Load reference design").clicked() {
                    self.request_action(DocumentAction::Reference, ui.ctx());
                    ui.close();
                }
                if ui.button("Open…").clicked() {
                    self.request_action(DocumentAction::Open, ui.ctx());
                    ui.close();
                }
                if ui.button("Save").clicked() {
                    self.file_dialog = Some(if self.file_path.is_some() {
                        FileDialog::Save
                    } else {
                        FileDialog::SaveAs
                    });
                    ui.close();
                }
                if ui.button("Save As…").clicked() {
                    self.file_dialog = Some(FileDialog::SaveAs);
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    self.request_action(DocumentAction::Quit, ui.ctx());
                    ui.close();
                }
            });

            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(self.document.can_undo(), egui::Button::new("Undo"))
                    .clicked()
                {
                    self.undo_edit();
                    ui.close();
                }
                if ui
                    .add_enabled(self.document.can_redo(), egui::Button::new("Redo"))
                    .clicked()
                {
                    self.redo_edit();
                    ui.close();
                }
            });

            ui.menu_button("Add", |ui| {
                if ui.button("Transition").clicked() {
                    self.add_transition();
                    ui.close();
                }
                if ui.button("Coupler").clicked() {
                    self.add_coupler();
                    ui.close();
                }
                if ui.button("Pod").clicked() {
                    self.add_pod();
                    ui.close();
                }
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
                ui.label("Click a part to inspect it. Isolate reveals internal parts.");
                ui.label("Cmd/Ctrl + S: Save · O: Open · D: Duplicate");
            });

            ui.separator();
            if ui
                .add(egui::Button::new("Save").fill(egui::Color32::from_rgb(29, 85, 110)))
                .on_hover_text("Save project, case, and retained results · Cmd/Ctrl+S")
                .clicked()
            {
                self.queue_save();
            }
            let filename = self
                .file_path
                .as_ref()
                .and_then(|p| p.file_name())
                .map_or("Untitled project".into(), |s| {
                    s.to_string_lossy().into_owned()
                });
            ui.add(egui::Label::new(filename).truncate());
        });
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.selectable_value(&mut self.right_tab, RightPanelTab::Design, "Design");
            ui.selectable_value(&mut self.right_tab, RightPanelTab::Simulate, "Flight case");
            ui.selectable_value(&mut self.right_tab, RightPanelTab::Results, "Results");
            ui.selectable_value(
                &mut self.right_tab,
                RightPanelTab::Aerodynamics,
                "Aerodynamics",
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.document.is_dirty() {
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
        ui.add_space(8.0);
        ui.separator();
    }

    fn replace_project(&mut self, project: Project) {
        self.document = DesignDocument::from_project(project);
        self.edit_error = None;
        self.selected = self.document.rocket().root.iter().next().map(Component::id);
        self.file_path = None;
        self.file_error = None;
        self.sim_panel = SimPanelState::default();
        self.results = crate::results::ResultsState::default();
        self.aero_panel.settings = None;
        self.aero_panel.preview = None;
        self.aero_panel.completed = None;
        self.aero_panel.artifact = None;
        self.aero_panel.error = None;
        self.viewport.isolate_selected = false;
        self.viewport.fit_requested = true;
        self.viewport.mark_dirty();
    }

    fn request_action(&mut self, action: DocumentAction, ctx: &egui::Context) {
        if self.analysis_running() {
            self.file_error=Some("An analysis is still running. Wait for it to finish or cancel it before replacing or closing this project.".into());
            ctx.request_repaint();
            return;
        }
        if let Some(ready) = self.replacement.request(action, self.document.is_dirty()) {
            self.execute_action(ready, ctx);
        }
        ctx.request_repaint();
    }
    fn execute_action(&mut self, action: DocumentAction, ctx: &egui::Context) {
        match action {
            DocumentAction::New => self.replace_project(crate::simulate::reference_project(
                Rocket::new("Untitled Rocket"),
            )),
            DocumentAction::Reference => {
                self.replace_project(crate::simulate::reference_project(reference_rocket()));
            }
            DocumentAction::Open => self.file_dialog = Some(FileDialog::Open),
            DocumentAction::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    fn show_unsaved_dialog(&mut self, ctx: &egui::Context) {
        if self.replacement.pending().is_none() {
            return;
        }
        let mut decision = None;
        let modal_id = egui::Id::unique("unsaved_changes");
        egui::Modal::new(modal_id)
            .area(egui::Modal::default_area(modal_id).fade_in(false))
            .show(ctx, |ui| {
                ui.set_max_width(430.0);
                ui.heading("Save changes before continuing?");
                ui.label("The project has unsaved vehicle, case, or result changes.");
                if let Some(error) = &self.file_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                ui.horizontal(|ui| {
                    if ui.button("Save and continue").clicked() {
                        decision = Some(Decision::Save);
                    }
                    if ui.button("Discard changes").clicked() {
                        decision = Some(Decision::Discard);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(Decision::Cancel);
                    }
                });
            });
        if let Some(decision) = decision {
            ctx.request_repaint();
            if let Some(action) = self.replacement.decide(decision) {
                self.execute_action(action, ctx);
            }
            if matches!(decision, Decision::Save) {
                self.file_dialog = Some(if self.file_path.is_some() {
                    FileDialog::Save
                } else {
                    FileDialog::SaveAs
                });
                ctx.request_repaint();
            }
        }
    }

    fn show_status_bar(&self, ui: &mut egui::Ui) {
        let mp = self.document.rocket().mass_properties();
        let aero = rocket_aero::estimate(self.document.rocket());
        let sm = rocket_aero::static_margin(self.document.rocket());
        let component_count = self.document.rocket().root.components.len();

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    " {} components · rev {} ",
                    component_count,
                    self.document.revision()
                ))
                .color(egui::Color32::from_gray(180)),
            );
            ui.separator();
            ui.label(format!(" Mass: {:.1} g ", mp.mass.as_grams()));
            ui.separator();
            ui.label(format!(" CG: {:.1} mm ", mp.cg_x.as_millimeters()));
            ui.separator();
            if aero.cn_alpha.abs() > 1e-9 {
                ui.label(format!(" CP: {:.1} mm ", aero.cp_x * 1000.0));
                ui.separator();
                if sm.is_finite() {
                    let sm_color = if sm >= 1.0 {
                        egui::Color32::from_rgb(0x40, 0xc0, 0x60)
                    } else if sm >= 0.0 {
                        egui::Color32::from_rgb(0xf0, 0xa0, 0x20)
                    } else {
                        egui::Color32::from_rgb(0xe5, 0x48, 0x4d)
                    };
                    ui.label(
                        egui::RichText::new(format!(" SM: {:+.1} cal ", sm))
                            .strong()
                            .color(sm_color),
                    );
                }
            } else {
                ui.label(
                    egui::RichText::new(" CP: — (no fins) ").color(egui::Color32::from_gray(120)),
                );
            }
            ui.separator();
            if let Some(path) = &self.file_path {
                ui.label(format!(" {} ", path.display()));
            } else {
                ui.label(" (unsaved) ");
            }
        });
    }

    fn handle_file_dialog(&mut self, dialog: FileDialog, ctx: &egui::Context) {
        let exporting_csv = matches!(&dialog, FileDialog::ExportCsv(_));
        match dialog {
            FileDialog::ExportCsv(id) | FileDialog::ExportRun(id) => {
                let csv = exporting_csv;
                let run = self
                    .document
                    .project()
                    .runs
                    .iter()
                    .find(|r| r.id == id)
                    .cloned();
                if let Some(run) = run {
                    let extension = if csv { "csv" } else { "json" };
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(format!("run-{}.{}", run.id, extension))
                        .add_filter("Run output", &[extension])
                        .save_file()
                    {
                        let result = if csv {
                            rocket_io::save_telemetry_csv(&run, path)
                        } else {
                            rocket_io::save_run_to_path(&run, path)
                        };
                        if let Err(error) = result {
                            self.file_error = Some(error.to_string());
                        }
                    }
                }
            }
            FileDialog::ImportRun => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Run record", &["json"])
                    .pick_file()
                {
                    match rocket_io::load_run_from_path(path) {
                        Ok(run) => {
                            let id = run.id.clone();
                            match self.document.retain_run(run) {
                                Ok(()) => {
                                    self.results.selected = Some(id);
                                    self.right_tab = RightPanelTab::Results;
                                }
                                Err(e) => self.file_error = Some(e.to_string()),
                            }
                        }
                        Err(e) => self.file_error = Some(e.to_string()),
                    }
                }
            }
            FileDialog::ImportDataset => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Aerodynamic dataset", &["json"])
                    .pick_file()
                {
                    match rocket_io::load_aero_dataset(path) {
                        Ok(data) => self.aero_panel.set_preview(std::sync::Arc::new(data)),
                        Err(e) => self.aero_panel.error = Some(e.to_string()),
                    }
                }
            }
            FileDialog::PrepareCfd(run) => {
                if let Some(parent) = rfd::FileDialog::new()
                    .set_title("Choose study output folder")
                    .pick_folder()
                {
                    self.prepare_cfd_in(parent, run);
                }
            }
            FileDialog::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Aether project or design", &["rocket"])
                    .pick_file()
                {
                    match load_project_from_path(&path) {
                        Ok(project) => {
                            self.replace_project(project);
                            self.file_path = Some(path);
                        }
                        Err(error) => {
                            self.file_error = Some(format!("Could not open this design: {error}"));
                        }
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
                        .add_filter("Aether project or design", &["rocket"])
                        .set_file_name(format!("{}.rocket", self.document.rocket().name))
                        .save_file()
                });
                if let Some(path) = path {
                    match save_project_to_path(self.document.project(), &path) {
                        Ok(()) => {
                            self.file_path = Some(path);
                            self.document.mark_saved();
                            self.file_error = None;
                            if let Some(action) = self.replacement.saved(true) {
                                self.execute_action(action, ctx);
                            }
                        }
                        Err(error) => {
                            self.file_error = Some(format!("Could not save the file: {error}"));
                            self.replacement.saved(false);
                        }
                    }
                }
            }
        }
    }
}

impl AetherApp {
    fn analysis_running(&self) -> bool {
        self.sim_panel.job.is_some() || self.aero_panel.job.is_some()
    }
    pub fn queue_save(&mut self) {
        self.file_dialog = Some(if self.file_path.is_some() {
            FileDialog::Save
        } else {
            FileDialog::SaveAs
        });
    }
    pub fn show_results(&mut self) {
        self.right_tab = RightPanelTab::Results;
    }
    pub fn import_run(&mut self) {
        self.file_dialog = Some(FileDialog::ImportRun);
    }
    pub fn import_dataset(&mut self) {
        self.file_dialog = Some(FileDialog::ImportDataset);
    }
    pub fn export_csv(&mut self, id: String) {
        self.file_dialog = Some(FileDialog::ExportCsv(id));
    }
    pub fn export_run(&mut self, id: String) {
        self.file_dialog = Some(FileDialog::ExportRun(id));
    }
    pub fn prepare_cfd(&mut self, run: bool) {
        self.file_dialog = Some(FileDialog::PrepareCfd(run));
    }
    pub fn cancel_run(&self) {
        if let Some(job) = &self.sim_panel.job {
            job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    pub fn cancel_cfd(&self) {
        if let Some(job) = &self.aero_panel.job {
            job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    pub fn start_run(&mut self) {
        if self.sim_panel.job.is_some() {
            return;
        }
        let rocket = self.document.rocket().clone();
        let case = self.document.case().clone();
        let revision = self.document.revision();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let token = cancel.clone();
        match std::thread::Builder::new()
            .name("aether-flight".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    case.run_controlled(&rocket, &token)
                        .map(|result| {
                            rocket_sim::RunRecord::capture(
                                rocket.clone(),
                                case.clone(),
                                result,
                                revision,
                            )
                        })
                        .map_err(|e| e.to_string())
                }))
                .unwrap_or_else(|_| Err("Flight worker failed unexpectedly".into()));
                let _ = sender.send(result);
            }) {
            Ok(_) => {
                self.sim_panel.job = Some(crate::simulate::FlightJob { receiver, cancel });
                self.sim_panel.error = None;
                self.right_tab = RightPanelTab::Results;
            }
            Err(error) => self.sim_panel.error = Some(error.to_string()),
        }
    }
    fn poll_jobs(&mut self, ctx: &egui::Context) {
        fn take<T>(
            receiver: &std::sync::mpsc::Receiver<Result<T, String>>,
        ) -> Option<Result<T, String>> {
            match receiver.try_recv() {
                Ok(value) => Some(value),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Analysis worker disconnected".into()))
                }
            }
        }
        if let Some(done) = self
            .sim_panel
            .job
            .as_ref()
            .and_then(|job| take(&job.receiver))
        {
            self.sim_panel.job = None;
            match done {
                Ok(run) => {
                    let id = run.id.clone();
                    match self.document.retain_run(run) {
                        Ok(()) => {
                            self.results.selected = Some(id);
                            self.results.plot.reset();
                        }
                        Err(e) => self.sim_panel.error = Some(e.to_string()),
                    }
                }
                Err(e) => self.sim_panel.error = Some(e),
            }
        }
        if let Some(done) = self
            .aero_panel
            .job
            .as_ref()
            .and_then(|job| take(&job.receiver))
        {
            let job = self.aero_panel.job.take().expect("completed CFD job");
            match done {
                Ok(outcome) => {
                    if let Some(data) = &outcome.dataset {
                        self.aero_panel
                            .set_preview(std::sync::Arc::new(data.clone()));
                    }
                    self.aero_panel.completed = Some((job.directory, outcome));
                }
                Err(e) => self.aero_panel.error = Some(e),
            }
        }
        if self.analysis_running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
    fn prepare_cfd_in(&mut self, parent: std::path::PathBuf, run: bool) {
        let settings = self
            .aero_panel
            .settings
            .clone()
            .unwrap_or_else(|| rocket_cfd::CfdSettings::for_vehicle(self.document.rocket()));
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = parent.join(format!("aether-cfd-{stamp}"));
        match rocket_cfd::prepare_case(self.document.rocket(), &settings, &directory) {
            Ok(_) => {
                self.aero_panel.artifact = Some(directory.clone());
                self.aero_panel.completed = None;
                self.aero_panel.error = None;
            }
            Err(error) => {
                self.aero_panel.error = Some(error.to_string());
                return;
            }
        }
        if !run {
            return;
        }
        let python = std::path::PathBuf::from(&self.aero_panel.python);
        let foam = std::path::PathBuf::from(&self.aero_panel.foam_bashrc);
        let (sender, receiver) = std::sync::mpsc::channel();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let token = cancel.clone();
        let path = directory.clone();
        match std::thread::Builder::new()
            .name("aether-cfd".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    rocket_cfd::run_case(&path, &python, &foam, &token).map_err(|e| e.to_string())
                }))
                .unwrap_or_else(|_| Err("CFD worker failed unexpectedly".into()));
                let _ = sender.send(result);
            }) {
            Ok(_) => {
                self.aero_panel.job = Some(crate::aerodynamics::CfdJob {
                    receiver,
                    cancel,
                    directory,
                });
            }
            Err(e) => self.aero_panel.error = Some(e.to_string()),
        }
    }
}

// ---- Component addition helpers ----

impl AetherApp {
    pub fn add_nose_cone(&mut self) {
        let id = self.new_id();
        let material = self.find_material("Balsa");
        let pos = self.document.rocket().root.length();
        self.insert_component(Component::NoseCone(NoseCone {
            id,
            position: pos,
            material,
            shape: NoseShape::TangentOgive,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(25.0),
            wall_thickness: Length::millimeters(1.0),
        }));
    }

    pub fn add_body_tube(&mut self) {
        let id = self.new_id();
        let material = self.find_material("Cardboard (body tube)");
        let pos = self.document.rocket().root.length();
        self.insert_component(Component::BodyTube(BodyTube {
            id,
            position: pos,
            material,
            length: Length::millimeters(150.0),
            outer_diameter: Length::millimeters(25.0),
            inner_diameter: Length::millimeters(24.0),
        }));
    }

    pub fn add_fin_set(&mut self) {
        let id = self.new_id();
        let material = self.find_material("Balsa");
        let mount = self
            .document
            .rocket()
            .root
            .components
            .iter()
            .rev()
            .find(|c| matches!(c, Component::BodyTube(_)))
            .map(rocket_core::Component::id);
        if mount.is_none() {
            self.file_error = Some("Add a body tube before adding fins".into());
            return;
        }
        let pos = mount
            .and_then(|id| self.document.rocket().root.find(id))
            .map_or(Length::ZERO, |body| {
                Length::meters(
                    (body.position().as_meters() + body.length().as_meters() - 0.05).max(0.0),
                )
            });
        self.insert_component(Component::FinSet(FinSet {
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
    }

    pub fn add_launch_lug(&mut self) {
        let id = self.new_id();
        let material = self.find_material("Cardboard (body tube)");
        self.insert_component(Component::LaunchLug(LaunchLug {
            id,
            position: Length::millimeters(50.0),
            material,
            length: Length::millimeters(25.0),
            inner_diameter: Length::millimeters(5.0),
            outer_diameter: Length::millimeters(7.0),
        }));
    }

    pub fn add_mass_object(&mut self) {
        let id = self.new_id();
        let pos = self.document.rocket().root.length();
        self.insert_component(Component::MassObject(MassObject {
            id,
            position: pos,
            mass: Mass::grams(10.0),
            kind: MassObjectKind::Ballast,
        }));
    }

    fn insert_component(&mut self, component: Component) {
        let id = component.id();
        match self.document.add_component(component) {
            Ok(true) => {
                self.selected = Some(id);
                self.edit_error = None;
                self.mark_dirty();
            }
            Ok(false) => {}
            Err(error) => self.file_error = Some(error.to_string()),
        }
    }

    pub fn add_transition(&mut self) {
        let c = Transition {
            id: self.new_id(),
            position: self.document.rocket().root.length(),
            material: self.find_material("Balsa"),
            length: Length::millimeters(30.0),
            fore_diameter: Length::millimeters(25.0),
            aft_diameter: Length::millimeters(35.0),
            wall_thickness: Length::millimeters(1.0),
        };
        self.insert_component(Component::Transition(c));
    }
    pub fn add_coupler(&mut self) {
        let c = Coupler {
            id: self.new_id(),
            position: Length::ZERO,
            material: self.find_material("Cardboard (body tube)"),
            length: Length::millimeters(30.0),
            outer_diameter: Length::millimeters(24.0),
        };
        self.insert_component(Component::Coupler(c));
    }
    pub fn add_pod(&mut self) {
        let c = Pod {
            id: self.new_id(),
            position: Length::millimeters(100.0),
            material: self.find_material("Balsa"),
            length: Length::millimeters(40.0),
            outer_diameter: Length::millimeters(10.0),
            radial_offset: Length::millimeters(20.0),
            angle: 0.0,
        };
        self.insert_component(Component::Pod(c));
    }

    pub fn delete_selected(&mut self) {
        if let Some(id) = self.selected {
            match self.document.delete_component(id) {
                Ok(true) => {
                    self.selected = None;
                    self.edit_error = None;
                    self.mark_dirty();
                }
                Ok(false) => {}
                Err(error) => self.edit_error = Some((id, error.to_string())),
            }
        }
    }
    pub fn duplicate_selected(&mut self) {
        if let Some(id) = self.selected {
            let new_id = self.new_id();
            match self.document.duplicate_component(id, new_id) {
                Ok(true) => {
                    self.selected = Some(new_id);
                    self.edit_error = None;
                    self.mark_dirty();
                }
                Ok(false) => {}
                Err(error) => self.edit_error = Some((id, error.to_string())),
            }
        }
    }
}

/// Read input while holding only one egui lock, then apply actions outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shortcut {
    Undo,
    Redo,
    Save,
    New,
    Open,
    Delete,
    Duplicate,
}

fn shortcut_actions(ctx: &egui::Context) -> Vec<Shortcut> {
    // Context access inside ctx.input would recursively lock the context.
    let text_focused = ctx.text_edit_focused();
    ctx.input(|input| {
        let mut actions = Vec::new();
        let command = input.modifiers.ctrl || input.modifiers.command;
        if command && !text_focused {
            if input.key_pressed(egui::Key::Z) {
                actions.push(if input.modifiers.shift {
                    Shortcut::Redo
                } else {
                    Shortcut::Undo
                });
            } else if input.key_pressed(egui::Key::Y) {
                actions.push(Shortcut::Redo);
            }
            if input.key_pressed(egui::Key::D) {
                actions.push(Shortcut::Duplicate);
            }
        }
        if command {
            for (key, action) in [
                (egui::Key::S, Shortcut::Save),
                (egui::Key::N, Shortcut::New),
                (egui::Key::O, Shortcut::Open),
            ] {
                if input.key_pressed(key) {
                    actions.push(action);
                }
            }
        }
        if !text_focused
            && (input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace))
        {
            actions.push(Shortcut::Delete);
        }
        actions
    })
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;

    fn input(key: egui::Key, shift: bool) -> egui::RawInput {
        let modifiers = egui::Modifiers {
            ctrl: true,
            shift,
            ..egui::Modifiers::default()
        };
        egui::RawInput {
            events: vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
            ],
            ..egui::RawInput::default()
        }
    }

    #[test]
    fn command_shortcuts_can_read_focus_without_recursively_locking_egui() {
        for (key, shift, expected) in [
            (egui::Key::Z, false, vec![Shortcut::Undo]),
            (egui::Key::Z, true, vec![Shortcut::Redo]),
            (egui::Key::Y, false, vec![Shortcut::Redo]),
            (egui::Key::S, false, vec![Shortcut::Save]),
            (egui::Key::A, false, vec![]),
        ] {
            let context = egui::Context::default();
            let mut output = context.run_pass(input(key, shift), |ctx| {
                assert_eq!(shortcut_actions(ctx), expected);
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    fn editing_text_keeps_document_shortcuts_out_of_the_text_field() {
        for key in [
            egui::Key::Z,
            egui::Key::Y,
            egui::Key::D,
            egui::Key::Backspace,
            egui::Key::Delete,
        ] {
            let context = egui::Context::default();
            let mut output = context.run_pass(input(key, false), |ctx| {
                let id = egui::Id::unique("dimension_input");
                egui::text_edit::TextEditState::default().store(ctx, id);
                ctx.memory_mut(|memory| memory.request_focus(id));
                assert!(ctx.text_edit_focused());
                assert_eq!(shortcut_actions(ctx), Vec::<Shortcut>::new());
            });
            output.textures_delta.clear();
        }
    }
}
