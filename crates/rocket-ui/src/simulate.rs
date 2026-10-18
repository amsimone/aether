//! Case editing and execution; widgets do not own engineering settings.
#![allow(clippy::too_many_lines)]

use crate::{app::AetherApp, plots};
use eframe::egui;
use rocket_core::{Length, Rocket};
use rocket_io::Project;
use rocket_motors::Motor;
use rocket_sim::SimResult;

const BUILTIN_MOTORS: &[&str] = &[
    include_str!("../../../tests/fixtures/motors/estes_a8_3.eng"),
    include_str!("../../../tests/fixtures/motors/estes_b6.eng"),
    include_str!("../../../tests/fixtures/motors/estes_c6.eng"),
];

/// Transient presentation state. All run inputs live in the document's case.
#[derive(Default)]
pub struct SimPanelState {
    pub available_motors: Vec<(String, Motor)>,
    pub result: Option<SimResult>,
    pub playback_time: f64,
    pub error: Option<String>,
}
impl SimPanelState {
    pub fn load_builtin_motors(&mut self) {
        if !self.available_motors.is_empty() {
            return;
        }
        for text in BUILTIN_MOTORS {
            if let Ok(motor) = rocket_motors::rasp::parse(text) {
                self.available_motors.push((
                    format!(
                        "{} · {} · {:.1} N·s",
                        motor.designation,
                        motor.manufacturer,
                        motor.total_impulse()
                    ),
                    motor,
                ));
            }
        }
    }
}

/// New editor documents explicitly resolve the first reference motor.
/// Legacy imports use the I/O migration with an unresolved motor instead.
pub fn reference_project(rocket: Rocket) -> Project {
    let mut project = Project::from_rocket(rocket);
    project.case.motor = rocket_motors::rasp::parse(BUILTIN_MOTORS[0]).ok();
    project
}

pub fn show_simulate_panel(app: &mut AetherApp, ui: &mut egui::Ui) {
    ui.heading("Flight case");
    ui.label(
        egui::RichText::new("Case settings and the motor curve are saved with this project.")
            .small()
            .weak(),
    );
    app.sim_panel.load_builtin_motors();
    let mut case = app.document.case().clone();
    ui.label("Case name");
    let name_edit = ui.text_edit_singleline(&mut case.name);
    ui.label("Motor");
    let selected = case.motor.as_ref().map_or_else(
        || "Select a motor".into(),
        |m| format!("{} · {}", m.designation, m.manufacturer),
    );
    egui::ComboBox::from_id_salt("motor_selector")
        .width(ui.available_width() - 16.0)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (label, motor) in &app.sim_panel.available_motors {
                let chosen = case.motor.as_ref() == Some(motor);
                if ui.selectable_label(chosen, label).clicked() {
                    case.motor = Some(motor.clone());
                    ui.close();
                }
            }
        });
    if case.motor.is_none() {
        ui.label("Imported designs need a motor selection before running.");
    }
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Launch and environment").strong());
    let mut angle = case.config.launch_angle.to_degrees();
    if number(ui, "Angle from vertical", &mut angle, 0.0..=89.0, "°") {
        case.config.launch_angle = angle.to_radians();
    }
    let mut altitude = case.config.pad_altitude.as_meters();
    if number(
        ui,
        "Pad altitude (MSL)",
        &mut altitude,
        -500.0..=20_000.0,
        " m",
    ) {
        case.config.pad_altitude = Length::meters(altitude);
    }
    number(
        ui,
        "Wind X (downrange)",
        &mut case.config.wind.0,
        -200.0..=200.0,
        " m/s",
    );
    number(
        ui,
        "Wind Y (cross-range)",
        &mut case.config.wind.1,
        -200.0..=200.0,
        " m/s",
    );
    number(
        ui,
        "Wind Z (up)",
        &mut case.config.wind.2,
        -200.0..=200.0,
        " m/s",
    );
    ui.collapsing("Recovery", |ui| {
        ui.label("Drogue at apogee; main on descent.");
        number(
            ui,
            "Main deploy (AGL)",
            &mut case.recovery.main_altitude_m,
            0.0..=10_000.0,
            " m",
        );
        number(
            ui,
            "Main Cd·A",
            &mut case.recovery.main_cd_a,
            0.0..=100.0,
            " m²",
        );
        number(
            ui,
            "Drogue Cd·A",
            &mut case.recovery.drogue_cd_a,
            0.0..=100.0,
            " m²",
        );
    });
    ui.collapsing("Models and numerics", |ui| {
        ui.label("Rigid 6-DOF · built-in aero v1");
        ui.label("ISA · constant gravity · fixed-step RK4");
        number(
            ui,
            "Integration step",
            &mut case.config.time_step,
            0.0001..=0.1,
            " s",
        );
        number(
            ui,
            "Telemetry interval",
            &mut case.config.telemetry_interval,
            0.0001..=10.0,
            " s",
        );
        number(
            ui,
            "Maximum duration",
            &mut case.maximum_duration_s,
            0.1..=3600.0,
            " s",
        );
    });
    if case != *app.document.case() {
        match app.document.edit_case(
            case,
            name_edit.has_focus() || ui.input(|i| i.pointer.primary_down()),
        ) {
            Ok(true) => {
                app.mark_dirty();
                app.sim_panel.error = None;
            }
            Ok(false) => {}
            Err(error) => app.sim_panel.error = Some(format!("Edit rejected: {error}")),
        }
        ui.ctx().request_repaint();
    }
    if let Some(error) = &app.sim_panel.error {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
    }
    ui.add_space(8.0);
    if ui
        .add_enabled(
            app.document.case().motor.is_some(),
            egui::Button::new("Run case").min_size(egui::vec2(ui.available_width(), 32.0)),
        )
        .clicked()
    {
        match app.document.case().run(app.document.rocket()) {
            Ok(result) => {
                app.sim_panel.result = Some(result);
                app.sim_panel.playback_time = 0.0;
                app.sim_panel.error = None;
            }
            Err(error) => app.sim_panel.error = Some(error.to_string()),
        }
    }
    let result = app.sim_panel.result.clone();
    if let Some(result) = result {
        ui.add_space(8.0);
        show_results(ui, &result);
        ui.add(
            egui::Slider::new(
                &mut app.sim_panel.playback_time,
                0.0..=result.landing_time.as_seconds(),
            )
            .text("Time")
            .suffix(" s"),
        );
        plots::show_altitude_plot(ui, &result);
        plots::show_velocity_plot(ui, &result);
        plots::show_thrust_plot(ui, &result);
    }
}

fn number(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::DragValue::new(value)
                    .speed(0.01)
                    .range(range)
                    .suffix(suffix)
                    .max_decimals(4)
                    .update_while_editing(false),
            )
            .changed()
        })
        .inner
    })
    .inner
}

/// Show the simulation results summary.
fn show_results(ui: &mut egui::Ui, result: &SimResult) {
    ui.label(egui::RichText::new("Results").strong());
    ui.add_space(2.0);

    result_row(
        ui,
        "Apogee:",
        &format!("{:.1} m", result.max_altitude.as_meters()),
    );
    result_row(
        ui,
        "Max velocity:",
        &format!("{:.1} m/s", result.max_velocity.as_meters_per_second()),
    );
    result_row(ui, "Max Mach:", &format!("{:.2}", result.max_mach()));
    result_row(
        ui,
        "Max accel:",
        &format!("{:.1} m/s²", result.max_acceleration),
    );
    result_row(
        ui,
        "Apogee time:",
        &format!("{:.2} s", result.apogee_time.as_seconds()),
    );
    result_row(
        ui,
        "Run end time:",
        &format!("{:.2} s", result.landing_time.as_seconds()),
    );
    result_row(
        ui,
        "End distance:",
        &format!(
            "{:.1} m",
            (result.landing_position.0.powi(2) + result.landing_position.1.powi(2)).sqrt()
        ),
    );
}

/// A label + value row for the results table.
fn result_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(egui::Color32::from_gray(160)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value)
                    .strong()
                    .color(egui::Color32::from_gray(220)),
            );
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_case_is_portable_and_can_run_offline() {
        let mut panel = SimPanelState::default();
        panel.load_builtin_motors();
        panel.load_builtin_motors();
        assert_eq!(panel.available_motors.len(), 3);
        let example = rocket_io::load_project_from_json(include_str!(
            "../../../examples/aether-one-project.rocket"
        ))
        .expect("portable example");
        let reference = reference_project(crate::reference::reference_rocket());
        assert_eq!(example, reference);
        let result = example.case.run(&example.rocket).expect("reference run");
        assert!(result.max_altitude.as_meters().is_finite());
        assert!(result.max_altitude.as_meters() > 10.0);
        assert!(result.landing_time.as_seconds() > result.apogee_time.as_seconds());
    }
}
