//! Interactive, unit-aware retained-run plots. Scroll to zoom, drag to pan, double-click to reset.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)]
use eframe::egui;
use rocket_sim::{RunRecord, TelemetryPoint};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Channel {
    #[default]
    Altitude,
    Trajectory,
    Speed,
    VerticalVelocity,
    Airspeed,
    Acceleration,
    Thrust,
    Drag,
    Mass,
    Mach,
    DynamicPressure,
    Alpha,
    Beta,
    RollRate,
    PitchRate,
    YawRate,
    Qw,
    Qx,
    Qy,
    Qz,
}
impl Channel {
    pub const ALL: [Self; 20] = [
        Self::Altitude,
        Self::Trajectory,
        Self::Speed,
        Self::VerticalVelocity,
        Self::Airspeed,
        Self::Acceleration,
        Self::Thrust,
        Self::Drag,
        Self::Mass,
        Self::Mach,
        Self::DynamicPressure,
        Self::Alpha,
        Self::Beta,
        Self::RollRate,
        Self::PitchRate,
        Self::YawRate,
        Self::Qw,
        Self::Qx,
        Self::Qy,
        Self::Qz,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Altitude => "Altitude AGL (m)",
            Self::Trajectory => "Trajectory X / height AGL (m)",
            Self::Speed => "Ground speed (m/s)",
            Self::VerticalVelocity => "Vertical velocity (m/s)",
            Self::Airspeed => "Airspeed (m/s)",
            Self::Acceleration => "Acceleration magnitude (m/s²)",
            Self::Thrust => "Thrust (N)",
            Self::Drag => "Drag (N)",
            Self::Mass => "Mass (kg)",
            Self::Mach => "Local Mach",
            Self::DynamicPressure => "Dynamic pressure (Pa)",
            Self::Alpha => "Angle of attack (deg)",
            Self::Beta => "Sideslip (deg)",
            Self::RollRate => "Body roll rate (rad/s)",
            Self::PitchRate => "Body pitch rate (rad/s)",
            Self::YawRate => "Body yaw rate (rad/s)",
            Self::Qw => "Attitude qw",
            Self::Qx => "Attitude qx",
            Self::Qy => "Attitude qy",
            Self::Qz => "Attitude qz",
        }
    }
    pub fn point(self, run: &RunRecord, p: &TelemetryPoint) -> Option<[f64; 2]> {
        let x = if self == Self::Trajectory {
            p.position.0
        } else {
            p.time.as_seconds()
        };
        let y = match self {
            Self::Altitude | Self::Trajectory => {
                p.position.2 - run.case.config.pad_altitude.as_meters()
            }
            Self::Speed => {
                (p.velocity.0.powi(2) + p.velocity.1.powi(2) + p.velocity.2.powi(2)).sqrt()
            }
            Self::VerticalVelocity => p.velocity.2,
            Self::Acceleration => p.acceleration,
            Self::Thrust => p.thrust.as_newtons(),
            Self::Drag => p.drag.as_newtons(),
            Self::Mass => p.mass.as_kilograms(),
            _ => {
                let f = p.flight?;
                match self {
                    Self::Airspeed => f.airspeed_m_s,
                    Self::Mach => f.mach,
                    Self::DynamicPressure => f.dynamic_pressure_pa,
                    Self::Alpha => f.alpha_rad.to_degrees(),
                    Self::Beta => f.beta_rad.to_degrees(),
                    Self::RollRate => f.angular_rate_rad_s[0],
                    Self::PitchRate => f.angular_rate_rad_s[1],
                    Self::YawRate => f.angular_rate_rad_s[2],
                    Self::Qw => f.quaternion_wxyz[0],
                    Self::Qx => f.quaternion_wxyz[1],
                    Self::Qy => f.quaternion_wxyz[2],
                    Self::Qz => f.quaternion_wxyz[3],
                    _ => return None,
                }
            }
        };
        (x.is_finite() && y.is_finite()).then_some([x, y])
    }
}
#[derive(Default)]
pub struct PlotState {
    pub channel: Channel,
    limits: Option<[f64; 4]>,
}
impl PlotState {
    pub fn reset(&mut self) {
        self.limits = None;
    }
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut PlotState,
    run: &RunRecord,
    baseline: Option<&RunRecord>,
) {
    let before = state.channel;
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_id_salt("result_channel")
            .width(250.0)
            .selected_text(state.channel.label())
            .show_ui(ui, |ui| {
                for channel in Channel::ALL {
                    ui.selectable_value(&mut state.channel, channel, channel.label());
                }
            });
        if ui.button("Reset view").clicked() {
            state.reset();
        }
        ui.label(
            egui::RichText::new("Scroll: zoom · Drag: pan · Hover: inspect")
                .small()
                .weak(),
        );
    });
    if before != state.channel {
        state.reset();
    }
    let data: Vec<[f64; 2]> = run
        .result
        .telemetry
        .iter()
        .filter_map(|p| state.channel.point(run, p))
        .collect();
    let base: Vec<[f64; 2]> = baseline
        .into_iter()
        .flat_map(|r| {
            r.result
                .telemetry
                .iter()
                .filter_map(|p| state.channel.point(r, p))
        })
        .collect();
    if data.is_empty() {
        ui.label("No samples for this output in the selected run.");
        return;
    }
    let mut auto = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in data.iter().chain(&base) {
        auto[0] = auto[0].min(p[0]);
        auto[1] = auto[1].max(p[0]);
        auto[2] = auto[2].min(p[1]);
        auto[3] = auto[3].max(p[1]);
    }
    for axis in [0, 2] {
        let pad = ((auto[axis + 1] - auto[axis]) * 0.06)
            .max(1e-6)
            .max(auto[axis].abs() * 0.001);
        auto[axis] -= pad;
        auto[axis + 1] += pad;
    }
    let mut limits = state.limits.unwrap_or(auto);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(100.0), 330.0),
        egui::Sense::click_and_drag(),
    );
    let plot = egui::Rect::from_min_max(
        rect.min + egui::vec2(68.0, 25.0),
        rect.max - egui::vec2(15.0, 35.0),
    );
    if response.double_clicked() {
        state.reset();
        limits = auto;
    }
    if response.dragged() {
        let delta = response.drag_delta();
        let dx = f64::from(delta.x / plot.width()) * (limits[1] - limits[0]);
        let dy = f64::from(delta.y / plot.height()) * (limits[3] - limits[2]);
        limits[0] -= dx;
        limits[1] -= dx;
        limits[2] += dy;
        limits[3] += dy;
        state.limits = Some(limits);
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.0 {
            if let Some(mouse) = response.hover_pos() {
                let wx = f64::from(((mouse.x - plot.left()) / plot.width()).clamp(0.0, 1.0));
                let wy = f64::from(((plot.bottom() - mouse.y) / plot.height()).clamp(0.0, 1.0));
                let factor = (-f64::from(scroll) / 300.0).exp().clamp(0.5, 2.0);
                for (axis, w) in [(0, wx), (2, wy)] {
                    let anchor = limits[axis] + w * (limits[axis + 1] - limits[axis]);
                    limits[axis] = anchor + (limits[axis] - anchor) * factor;
                    limits[axis + 1] = anchor + (limits[axis + 1] - anchor) * factor;
                }
                state.limits = Some(limits);
                ui.ctx().request_repaint();
                ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
            }
        }
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, egui::Color32::from_rgb(17, 24, 33));
    let color = egui::Color32::from_rgb(100, 210, 240);
    let base_color = egui::Color32::from_rgb(245, 180, 90);
    let map = |p: [f64; 2]| {
        egui::pos2(
            plot.left() + ((p[0] - limits[0]) / (limits[1] - limits[0])) as f32 * plot.width(),
            plot.bottom() - ((p[1] - limits[2]) / (limits[3] - limits[2])) as f32 * plot.height(),
        )
    };
    for i in 0..=5 {
        let f = f64::from(i) / 5.0;
        let px = plot.left() + f as f32 * plot.width();
        let py = plot.bottom() - f as f32 * plot.height();
        let stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(48));
        painter.line_segment(
            [egui::pos2(px, plot.top()), egui::pos2(px, plot.bottom())],
            stroke,
        );
        painter.line_segment(
            [egui::pos2(plot.left(), py), egui::pos2(plot.right(), py)],
            stroke,
        );
        painter.text(
            egui::pos2(px, plot.bottom() + 5.0),
            egui::Align2::CENTER_TOP,
            format_number(limits[0] + f * (limits[1] - limits[0])),
            egui::FontId::proportional(11.0),
            egui::Color32::GRAY,
        );
        painter.text(
            egui::pos2(plot.left() - 7.0, py),
            egui::Align2::RIGHT_CENTER,
            format_number(limits[2] + f * (limits[3] - limits[2])),
            egui::FontId::proportional(11.0),
            egui::Color32::GRAY,
        );
    }
    let clipped = painter.with_clip_rect(plot);
    for (points, c) in [(&base, base_color), (&data, color)] {
        if points.len() >= 2 {
            clipped.add(egui::Shape::line(
                points.iter().map(|p| map(*p)).collect(),
                egui::Stroke::new(1.7, c),
            ));
        } else if let Some(p) = points.first() {
            clipped.circle_filled(map(*p), 3.0, c);
        }
    }
    if state.channel != Channel::Trajectory {
        for (index, e) in run.result.events.iter().enumerate() {
            if (limits[0]..=limits[1]).contains(&e.time_s) {
                let x = map([e.time_s, limits[2]]).x;
                clipped.line_segment(
                    [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
                    egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
                );
                let near_right_edge = x > plot.right() - 110.0;
                clipped.text(
                    egui::pos2(
                        x + if near_right_edge { -3.0 } else { 3.0 },
                        plot.top() + 3.0 + (index % 2) as f32 * 14.0,
                    ),
                    if near_right_edge {
                        egui::Align2::RIGHT_TOP
                    } else {
                        egui::Align2::LEFT_TOP
                    },
                    &e.name,
                    egui::FontId::proportional(10.0),
                    egui::Color32::GRAY,
                );
            }
        }
    }
    painter.text(
        egui::pos2(plot.left(), rect.top() + 3.0),
        egui::Align2::LEFT_TOP,
        state.channel.label(),
        egui::FontId::proportional(12.0),
        color,
    );
    painter.text(
        egui::pos2(plot.center().x, rect.bottom() - 2.0),
        egui::Align2::CENTER_BOTTOM,
        if state.channel == Channel::Trajectory {
            "Downrange X (m)"
        } else {
            "Time (s)"
        },
        egui::FontId::proportional(11.0),
        egui::Color32::GRAY,
    );
    if let Some(mouse) = response.hover_pos().filter(|p| plot.contains(*p)) {
        let x =
            limits[0] + f64::from((mouse.x - plot.left()) / plot.width()) * (limits[1] - limits[0]);
        clipped.line_segment(
            [
                egui::pos2(mouse.x, plot.top()),
                egui::pos2(mouse.x, plot.bottom()),
            ],
            egui::Stroke::new(1.0, egui::Color32::WHITE),
        );
        response.on_hover_ui(|ui| {
            for (points, label) in [
                (&data, run.label.as_str()),
                (&base, baseline.map_or("Baseline", |r| r.label.as_str())),
            ] {
                if let Some([point_x, point_y]) = points
                    .iter()
                    .min_by(|a, b| (a[0] - x).abs().total_cmp(&(b[0] - x).abs()))
                {
                    ui.label(format!("{label}: x {point_x:.5}, value {point_y:.6}"));
                }
            }
        });
    }
    ui.horizontal(|ui| {
        ui.colored_label(color, format!("Selected: {}", run.label));
        if let Some(b) = baseline {
            ui.colored_label(base_color, format!("Baseline: {}", b.label));
        }
    });
}
fn format_number(v: f64) -> String {
    if v.abs() >= 10000.0 || (v != 0.0 && v.abs() < 0.01) {
        format!("{v:.1e}")
    } else {
        format!("{v:.2}")
    }
}
