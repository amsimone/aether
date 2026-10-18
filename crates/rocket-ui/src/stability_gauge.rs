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
use rocket_aero::{estimate, static_margin};

use crate::app::AetherApp;

/// Show the stability margin gauge.
pub fn show_stability_gauge(app: &mut AetherApp, ui: &mut egui::Ui) {
    ui.heading("Stability");
    ui.add_space(4.0);

    let mp = app.rocket.mass_properties();
    let aero = estimate(&app.rocket);
    let sm = static_margin(&app.rocket);
    let rocket_length = app.rocket.root.length().as_millimeters().max(1.0);
    let cg_mm = mp.cg_x.as_millimeters();

    // CP is now computed by the design-time aero estimate.
    let cp_mm = if aero.cn_alpha.abs() > 1e-9 {
        Some(aero.cp_x * 1000.0)
    } else {
        None
    };

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

    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("C_Nα:");
        ui.label(format!("{:.2} /rad", aero.cn_alpha));
    });

    if let Some(cp) = cp_mm {
        ui.horizontal(|ui| {
            ui.label("CP (from nose):");
            ui.label(format!("{:.1} mm", cp));
        });
    }

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
    // Re-draw the inner bar on top.
    painter.rect_filled(bar_rect, 2.0, egui::Color32::from_rgb(0x30, 0x35, 0x40));

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

    // CP marker (orange diamond) — now computed!
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
        if sm.is_finite() {
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
        }
    } else {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("CP: no aero components (add fins)")
                .color(egui::Color32::from_gray(120)),
        );
    }

    ui.add_space(4.0);

    // Design-estimate label.
    ui.label(
        egui::RichText::new(format!(
            "Design estimate at M={:.1} · Rocket length: {:.0} mm",
            aero.mach, rocket_length
        ))
        .small()
        .color(egui::Color32::from_gray(140)),
    );
}
