//! Flight simulation plots.
//!
//! Simple line plots drawn directly on an `egui` canvas (no `egui_plot`
//! dependency). Each plot shows a telemetry variable vs time.

#![allow(
    clippy::cast_precision_loss,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::cast_possible_truncation,
    clippy::cast_lossless
)]

use eframe::egui;
use rocket_sim::SimResult;

/// Color palette for the plots.
const COLOR_ALT: egui::Color32 = egui::Color32::from_rgb(0x4a, 0xa8, 0xff); // blue
const COLOR_VEL: egui::Color32 = egui::Color32::from_rgb(0x40, 0xc0, 0x60); // green
const COLOR_THRUST: egui::Color32 = egui::Color32::from_rgb(0xf0, 0xa0, 0x20); // orange
const COLOR_AXIS: egui::Color32 = egui::Color32::from_gray(100);
const COLOR_GRID: egui::Color32 = egui::Color32::from_gray(50);
const COLOR_TEXT: egui::Color32 = egui::Color32::from_gray(180);

/// Show the altitude vs time plot.
pub fn show_altitude_plot(ui: &mut egui::Ui, result: &SimResult) {
    let points: Vec<(f64, f64)> = result
        .telemetry
        .iter()
        .map(|p| (p.time.as_seconds(), p.position.2))
        .collect();
    let max_y = result.max_altitude.as_meters().max(10.0);
    let max_x = result.landing_time.as_seconds().max(1.0);
    line_plot(
        ui,
        "Altitude",
        &points,
        max_x,
        0.0,
        max_y * 1.1,
        "t (s)",
        "h (m)",
        COLOR_ALT,
    );
}

/// Show the velocity vs time plot.
pub fn show_velocity_plot(ui: &mut egui::Ui, result: &SimResult) {
    let points: Vec<(f64, f64)> = result
        .telemetry
        .iter()
        .map(|p| {
            (
                p.time.as_seconds(),
                (p.velocity.0 * p.velocity.0
                    + p.velocity.1 * p.velocity.1
                    + p.velocity.2 * p.velocity.2)
                    .sqrt(),
            )
        })
        .collect();
    let max_y = result.max_velocity.as_meters_per_second().max(10.0);
    let max_x = result.landing_time.as_seconds().max(1.0);
    line_plot(
        ui,
        "Speed",
        &points,
        max_x,
        0.0,
        max_y * 1.1,
        "t (s)",
        "v (m/s)",
        COLOR_VEL,
    );
}

/// Show the thrust vs time plot.
pub fn show_thrust_plot(ui: &mut egui::Ui, result: &SimResult) {
    let points: Vec<(f64, f64)> = result
        .telemetry
        .iter()
        .map(|p| (p.time.as_seconds(), p.thrust.as_newtons()))
        .collect();
    let max_y = result
        .telemetry
        .iter()
        .map(|p| p.thrust.as_newtons())
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let max_x = result.landing_time.as_seconds().max(1.0);
    line_plot(
        ui,
        "Thrust",
        &points,
        max_x,
        0.0,
        max_y * 1.15,
        "t (s)",
        "T (N)",
        COLOR_THRUST,
    );
}

/// Draw a simple line plot on an egui canvas.
fn line_plot(
    ui: &mut egui::Ui,
    title: &str,
    points: &[(f64, f64)],
    max_x: f64,
    min_y: f64,
    max_y: f64,
    x_label: &str,
    y_label: &str,
    color: egui::Color32,
) {
    ui.label(egui::RichText::new(title).strong().color(color));
    let (rect, _resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 120.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);

    // Background.
    painter.rect_filled(rect, 0.0, egui::Color32::from_gray(30));

    let plot_rect = rect.shrink(4.0);
    let plot_rect = egui::Rect::from_min_max(
        plot_rect.left_top() + egui::vec2(30.0, 10.0),
        plot_rect.right_bottom() - egui::vec2(10.0, 15.0),
    );

    // Grid lines (4 horizontal, 4 vertical).
    for i in 0..=4 {
        let frac = i as f32 / 4.0;
        let y = plot_rect.top() + frac * plot_rect.height();
        painter.line_segment(
            [
                egui::pos2(plot_rect.left(), y),
                egui::pos2(plot_rect.right(), y),
            ],
            egui::Stroke::new(1.0, COLOR_GRID),
        );
        let x = plot_rect.left() + frac * plot_rect.width();
        painter.line_segment(
            [
                egui::pos2(x, plot_rect.top()),
                egui::pos2(x, plot_rect.bottom()),
            ],
            egui::Stroke::new(1.0, COLOR_GRID),
        );
    }

    // Axis labels.
    painter.text(
        egui::pos2(plot_rect.center().x, rect.bottom() - 4.0),
        egui::Align2::CENTER_BOTTOM,
        format!("{x_label} →"),
        egui::FontId::proportional(10.0),
        COLOR_TEXT,
    );
    painter.text(
        egui::pos2(rect.left() + 4.0, plot_rect.center().y),
        egui::Align2::LEFT_CENTER,
        y_label,
        egui::FontId::proportional(10.0),
        COLOR_TEXT,
    );

    // Y-axis tick labels.
    for i in 0..=4 {
        let frac = i as f64 / 4.0;
        let val = max_y - frac * (max_y - min_y);
        let y = plot_rect.top() + frac as f32 * plot_rect.height();
        painter.text(
            egui::pos2(plot_rect.left() - 2.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{val:.0}"),
            egui::FontId::proportional(9.0),
            COLOR_TEXT,
        );
    }

    // X-axis tick labels.
    for i in 0..=4 {
        let frac = i as f64 / 4.0;
        let val = frac * max_x;
        let x = plot_rect.left() + frac as f32 * plot_rect.width();
        painter.text(
            egui::pos2(x, plot_rect.bottom() + 2.0),
            egui::Align2::CENTER_TOP,
            format!("{val:.1}"),
            egui::FontId::proportional(9.0),
            COLOR_TEXT,
        );
    }

    // Axes.
    painter.line_segment(
        [
            egui::pos2(plot_rect.left(), plot_rect.top()),
            egui::pos2(plot_rect.left(), plot_rect.bottom()),
        ],
        egui::Stroke::new(1.5, COLOR_AXIS),
    );
    painter.line_segment(
        [
            egui::pos2(plot_rect.left(), plot_rect.bottom()),
            egui::pos2(plot_rect.right(), plot_rect.bottom()),
        ],
        egui::Stroke::new(1.5, COLOR_AXIS),
    );

    // Data line.
    if points.len() < 2 || (max_y - min_y).abs() < 1e-9 || max_x < 1e-9 {
        return;
    }

    let screen_points: Vec<egui::Pos2> = points
        .iter()
        .map(|(x, y)| {
            let px = plot_rect.left() + (x / max_x) as f32 * plot_rect.width();
            let py =
                plot_rect.bottom() - ((y - min_y) / (max_y - min_y)) as f32 * plot_rect.height();
            egui::pos2(px, py)
        })
        .collect();

    painter.add(egui::Shape::line(
        screen_points,
        egui::Stroke::new(2.0, color),
    ));
}
