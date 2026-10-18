//! The stability margin gauge — a bespoke widget showing CG and CP
//! positions along the rocket length.
//!
//! This is one of the "small bespoke widgets" (plan §11.5) that
//! separates a designed tool from a generic form. It renders a
//! horizontal bar representing the rocket body, with CG (●) and CP (◆)
//! markers, and a colored zone indicating stability margin.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::unnecessary_unwrap,
    clippy::manual_map,
    clippy::needless_pass_by_value,
    clippy::must_use_candidate,
    clippy::map_unwrap_or,
    clippy::uninlined_format_args,
    clippy::redundant_closure,
    clippy::filter_map_next
)]

use eframe::egui;

use crate::app::AetherApp;

/// Show the stability margin gauge.
pub fn show_stability_gauge(app: &mut AetherApp, ui: &mut egui::Ui) {
    ui.heading("Stability");
    ui.add_space(4.0);

    let mp = app.rocket.mass_properties();
    let rocket_length = app.rocket.root.length().as_millimeters().max(1.0);
    let cg_mm = mp.cg_x.as_millimeters();

    // CP is not yet computed (Task 1.5); show a placeholder.
    // For now, we display the CG position and a "CP pending" note.
    let cp_mm: Option<f64> = None;

    // Mass readouts.
    ui.horizontal(|ui| {
        ui.label("Total mass:");
        ui.label(
            egui::RichText::new(format!("{:.1} g", mp.mass.as_grams()))
                .strong()
                .color(egui::Color32::from_rgb(0x4a, 0xa8, 0xff)),
        );
    });

    ui.horizontal(|ui| {
        ui.label("CG (from nose):");
        ui.label(format!("{:.1} mm", cg_mm));
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

    // The gauge: a horizontal bar showing the rocket body with CG/CP markers.
    let (rect, _) = ui.allocate_exact_size(
        egui::Vec2::new(ui.available_width(), 40.0),
        egui::Sense::hover(),
    );

    let painter = ui.painter();
    let bar_height = 12.0;
    let bar_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.center().y - bar_height / 2.0),
        egui::Vec2::new(rect.width(), bar_height),
    );

    // Draw the rocket body bar.
    painter.rect_filled(bar_rect, 2.0, egui::Color32::from_rgb(0x30, 0x35, 0x40));
    // Draw the border as a filled rect slightly larger than the bar.
    let border_rect = egui::Rect::from_min_size(
        egui::pos2(
            bar_rect.left() - 1.0,
            bar_rect.center().y - bar_height / 2.0 - 1.0,
        ),
        egui::Vec2::new(bar_rect.width() + 2.0, bar_height + 2.0),
    );
    painter.rect_filled(border_rect, 2.0, egui::Color32::from_gray(100));

    // Tick marks every 25% of the rocket length.
    for i in 1..4 {
        let x = bar_rect.left() + (bar_rect.width() * i as f32 / 4.0);
        painter.line_segment(
            [
                egui::pos2(x, bar_rect.bottom()),
                egui::pos2(x, bar_rect.bottom() + 4.0),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
        );
    }

    // CG marker (blue circle).
    let cg_x = bar_rect.left() + (cg_mm as f32 / rocket_length as f32) * bar_rect.width();
    let cg_y = bar_rect.center().y;
    painter.circle_filled(
        egui::pos2(cg_x, cg_y),
        5.0,
        egui::Color32::from_rgb(0x4a, 0xa8, 0xff),
    );
    painter.text(
        egui::pos2(cg_x, cg_y - 12.0),
        egui::Align2::CENTER_CENTER,
        "CG",
        egui::FontId::proportional(10.0),
        egui::Color32::from_rgb(0x4a, 0xa8, 0xff),
    );

    // CP marker (orange diamond) — only if computed.
    if let Some(cp) = cp_mm {
        let cp_x = bar_rect.left() + (cp as f32 / rocket_length as f32) * bar_rect.width();
        let cp_y = bar_rect.center().y;
        // Draw a diamond.
        let size = 5.0_f32;
        let points = [
            egui::pos2(cp_x, cp_y - size),
            egui::pos2(cp_x + size, cp_y),
            egui::pos2(cp_x, cp_y + size),
            egui::pos2(cp_x - size, cp_y),
        ];
        painter.add(egui::Shape::convex_polygon(
            points.to_vec(),
            egui::Color32::from_rgb(0xf0, 0xa0, 0x20),
            egui::Stroke::NONE,
        ));
        painter.text(
            egui::pos2(cp_x, cp_y + 14.0),
            egui::Align2::CENTER_CENTER,
            "CP",
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(0xf0, 0xa0, 0x20),
        );

        // Stability margin in calibers.
        let ref_diameter = app
            .rocket
            .root
            .iter()
            .filter_map(|c| match c {
                rocket_core::Component::BodyTube(bt) => Some(bt.outer_diameter.as_millimeters()),
                rocket_core::Component::NoseCone(nc) => Some(nc.base_diameter.as_millimeters()),
                _ => None,
            })
            .next()
            .unwrap_or(25.0);
        let sm = (cp - cg_mm) / ref_diameter;

        let (sm_color, sm_label) = if sm >= 2.0 {
            (
                egui::Color32::from_rgb(0x40, 0xc0, 0x60),
                format!("SM: +{:.1} cal (very stable)", sm),
            )
        } else if sm >= 1.0 {
            (
                egui::Color32::from_rgb(0x40, 0xc0, 0x60),
                format!("SM: +{:.1} cal (stable)", sm),
            )
        } else if sm >= 0.0 {
            (
                egui::Color32::from_rgb(0xf0, 0xa0, 0x20),
                format!("SM: +{:.1} cal (marginal)", sm),
            )
        } else {
            (
                egui::Color32::from_rgb(0xe5, 0x48, 0x4d),
                format!("SM: {:.1} cal (UNSTABLE!)", sm),
            )
        };

        ui.add_space(4.0);
        ui.label(egui::RichText::new(sm_label).strong().color(sm_color));
    } else {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("CP: pending (Task 1.5 — design-time aero)")
                .color(egui::Color32::from_rgb(0xf0, 0xa0, 0x20)),
        );
    }

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(format!("Rocket length: {:.0} mm", rocket_length))
            .small()
            .color(egui::Color32::from_gray(140)),
    );
}
