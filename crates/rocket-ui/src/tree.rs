//! The component tree panel.
//!
//! Shows a hierarchical list of the rocket's components with add/delete/
//! duplicate actions. Clicking a component selects it (syncing the
//! properties panel and 3D view).

use eframe::egui;

use crate::app::AetherApp;
use rocket_core::Component;

/// Show the component tree panel.
pub fn show_component_tree(app: &mut AetherApp, ui: &mut egui::Ui) {
    ui.heading("Components");
    ui.add_space(4.0);

    // Toolbar: add buttons.
    ui.horizontal_wrapped(|ui| {
        let add_button = |ui: &mut egui::Ui, label: &str| -> bool {
            ui.add(egui::Button::new(label).small()).clicked()
        };
        if add_button(ui, "+ Nose") {
            app.add_nose_cone();
        }
        if add_button(ui, "+ Body") {
            app.add_body_tube();
        }
        if add_button(ui, "+ Fins") {
            app.add_fin_set();
        }
        if add_button(ui, "+ Lug") {
            app.add_launch_lug();
        }
        if add_button(ui, "+ Mass") {
            app.add_mass_object();
        }
    });

    ui.add_space(6.0);

    // Selected-component actions.
    if app.selected.is_some() {
        ui.horizontal(|ui| {
            if ui.button("Duplicate").clicked() {
                app.duplicate_selected();
            }
            if ui
                .add(egui::Button::new("Delete").fill(egui::Color32::from_rgb(0x80, 0x20, 0x20)))
                .clicked()
            {
                app.delete_selected();
            }
        });
        ui.add_space(4.0);
    }

    // Component list.
    let component_count = app.rocket.root.components.len();
    if component_count == 0 {
        ui.label(
            egui::RichText::new("(no components — use the buttons above to add)")
                .color(egui::Color32::from_gray(120)),
        );
        return;
    }

    // Collect component summaries (to avoid borrow issues).
    let summaries: Vec<(rocket_core::ComponentId, String, String)> = app
        .rocket
        .root
        .iter()
        .map(|c| {
            let kind = component_kind_label(c).to_string();
            let detail = component_detail_label(c);
            (c.id(), kind, detail)
        })
        .collect();

    let mut new_selection = None;
    for (id, kind, detail) in &summaries {
        let selected = app.selected == Some(*id);
        let response = ui
            .selectable_label(selected, egui::RichText::new(format!("{kind} — {detail}")))
            .on_hover_text(format!(
                "Click to select · Right-click for actions\nID: {id}"
            ));
        if response.clicked() {
            new_selection = Some(*id);
        }
        response.context_menu(|ui| {
            if ui.button("Duplicate").clicked() {
                app.selected = Some(*id);
                app.duplicate_selected();
                ui.close();
            }
            if ui
                .add(egui::Button::new("Delete").fill(egui::Color32::from_rgb(0x80, 0x20, 0x20)))
                .clicked()
            {
                app.selected = Some(*id);
                app.delete_selected();
                ui.close();
            }
        });
    }

    if let Some(id) = new_selection {
        app.selected = Some(id);
    }
}

fn component_kind_label(c: &Component) -> &'static str {
    match c {
        Component::NoseCone(_) => "Nose Cone",
        Component::BodyTube(_) => "Body Tube",
        Component::Transition(_) => "Transition",
        Component::Coupler(_) => "Coupler",
        Component::FinSet(_) => "Fin Set",
        Component::LaunchLug(_) => "Launch Lug",
        Component::Pod(_) => "Pod",
        Component::MassObject(_) => "Mass Object",
    }
}

fn component_detail_label(c: &Component) -> String {
    match c {
        Component::NoseCone(nc) => format!("{:.0}mm, {:?}", nc.length.as_millimeters(), nc.shape),
        Component::BodyTube(bt) => {
            format!(
                "{:.0}mm × {:.0}mm",
                bt.length.as_millimeters(),
                bt.outer_diameter.as_millimeters()
            )
        }
        Component::Transition(t) => {
            format!(
                "{:.0}mm, {:.0}→{:.0}",
                t.length.as_millimeters(),
                t.fore_diameter.as_millimeters(),
                t.aft_diameter.as_millimeters()
            )
        }
        Component::Coupler(c) => format!("{:.0}mm", c.length.as_millimeters()),
        Component::FinSet(fs) => format!("{} fins, {:.0}mm²", fs.count, fs.planform.area() * 1e6),
        Component::LaunchLug(ll) => format!("{:.0}mm", ll.length.as_millimeters()),
        Component::Pod(p) => format!("{:.0}mm", p.length.as_millimeters()),
        Component::MassObject(mo) => format!("{:.1}g, {:?}", mo.mass.as_grams(), mo.kind),
    }
}
