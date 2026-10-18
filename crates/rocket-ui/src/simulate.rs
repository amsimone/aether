//! The simulate panel: motor selection, sim config, run button, results.
//!
//! Phase 3: uses the 6-DOF simulator with quaternion attitude, recovery
//! events, and full aero coefficients.

#![allow(
    clippy::cast_precision_loss,
    clippy::similar_names,
    clippy::needless_pass_by_value,
    clippy::too_many_lines
)]

use eframe::egui;
use rocket_core::Length;
use rocket_motors::Motor;
use rocket_sim::{RecoveryState, SimConfig, SimResult, Simulator6Dof};

use crate::app::AetherApp;
use crate::plots;

/// Built-in motor fixtures available for simulation.
/// These are loaded from the tests/fixtures/motors/ directory.
const BUILTIN_MOTORS: &[&str] = &["estes_a8_3.eng", "estes_b6.eng", "estes_c6.eng"];

/// The sim panel state, stored in [`AetherApp`].
pub struct SimPanelState {
    /// Index of the selected motor in the available list.
    pub selected_motor: usize,
    /// Available motors loaded from fixtures.
    pub available_motors: Vec<(String, Motor)>,
    /// Launch angle in degrees from vertical.
    pub launch_angle_deg: f64,
    /// Pad altitude in meters.
    pub pad_altitude_m: f64,
    /// Wind speed in m/s.
    pub wind_speed: f64,
    /// Wind direction in degrees (0 = downrange, 90 = cross-range).
    pub wind_dir_deg: f64,
    /// Main chute deployment altitude (m AGL).
    pub main_altitude_m: f64,
    /// Main chute drag area (Cd · A, m²).
    pub main_cd_a: f64,
    /// Drogue chute drag area (Cd · A, m²).
    pub drogue_cd_a: f64,
    /// The last simulation result (if any).
    pub result: Option<SimResult>,
    /// Whether a sim is currently running.
    pub running: bool,
    /// Playback time slider position (seconds).
    pub playback_time: f64,
}

impl Default for SimPanelState {
    fn default() -> Self {
        let recovery = RecoveryState::default_lpr();
        Self {
            selected_motor: 0,
            available_motors: Vec::new(),
            launch_angle_deg: 0.0,
            pad_altitude_m: 0.0,
            wind_speed: 0.0,
            wind_dir_deg: 0.0,
            main_altitude_m: recovery.main_altitude,
            main_cd_a: recovery.main_cd_a,
            drogue_cd_a: recovery.drogue_cd_a,
            result: None,
            running: false,
            playback_time: 0.0,
        }
    }
}

impl SimPanelState {
    /// Load the built-in motor fixtures from the tests/fixtures/motors/ directory.
    pub fn load_builtin_motors(&mut self) {
        if !self.available_motors.is_empty() {
            return;
        }
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let fixtures_dir = std::path::Path::new(manifest_dir)
            .join("..")
            .join("..")
            .join("tests")
            .join("fixtures")
            .join("motors");

        for name in BUILTIN_MOTORS {
            let path = fixtures_dir.join(name);
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(motor) = rocket_motors::rasp::parse(&text) {
                    let display = format!(
                        "{} ({} {:.1}g, {:.1}Ns)",
                        motor.designation,
                        motor.manufacturer,
                        motor.total_mass.as_grams(),
                        motor.total_impulse()
                    );
                    self.available_motors.push((display, motor));
                }
            }
        }
    }

    /// Build a [`SimConfig`] from the panel state.
    fn build_config(&self) -> SimConfig {
        let wind_rad = self.wind_dir_deg.to_radians();
        SimConfig {
            launch_angle: self.launch_angle_deg.to_radians(),
            pad_altitude: Length::meters(self.pad_altitude_m),
            wind: (
                self.wind_speed * wind_rad.cos(),
                self.wind_speed * wind_rad.sin(),
                0.0,
            ),
            ..SimConfig::default()
        }
    }

    /// Build a [`RecoveryState`] from the panel state.
    fn build_recovery(&self) -> RecoveryState {
        RecoveryState {
            main_altitude: self.main_altitude_m,
            main_cd_a: self.main_cd_a,
            drogue_cd_a: self.drogue_cd_a,
            ..RecoveryState::default_lpr()
        }
    }
}

/// Show the simulate panel in the right sidebar.
pub fn show_simulate_panel(app: &mut AetherApp, ui: &mut egui::Ui) {
    ui.heading("🚀 Simulate");
    ui.add_space(4.0);

    let sim_state = &mut app.sim_panel;
    sim_state.load_builtin_motors();

    // ---- Motor selector ----
    ui.label("Motor:");
    if sim_state.available_motors.is_empty() {
        ui.label(
            egui::RichText::new("No motors found (fixtures missing)")
                .color(egui::Color32::from_rgb(0xf0, 0x60, 0x60)),
        );
    } else {
        egui::ComboBox::from_id_salt("motor_selector")
            .selected_text(
                sim_state
                    .available_motors
                    .get(sim_state.selected_motor)
                    .map_or("Select motor", |(name, _)| name.as_str()),
            )
            .show_ui(ui, |ui| {
                for (i, (name, _)) in sim_state.available_motors.iter().enumerate() {
                    ui.selectable_value(&mut sim_state.selected_motor, i, name);
                }
            });
    }
    ui.add_space(4.0);

    // ---- Launch configuration ----
    ui.label("Launch Configuration:");
    ui.horizontal(|ui| {
        ui.label("Angle from vertical:");
        ui.add(
            egui::Slider::new(&mut sim_state.launch_angle_deg, 0.0..=15.0)
                .suffix("°")
                .fixed_decimals(1),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Pad altitude:");
        ui.add(
            egui::Slider::new(&mut sim_state.pad_altitude_m, 0.0..=3000.0)
                .suffix(" m")
                .fixed_decimals(0),
        );
    });
    ui.add_space(4.0);

    // ---- Wind ----
    ui.label("Wind:");
    ui.horizontal(|ui| {
        ui.label("Speed:");
        ui.add(
            egui::Slider::new(&mut sim_state.wind_speed, 0.0..=20.0)
                .suffix(" m/s")
                .fixed_decimals(1),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Direction:");
        ui.add(
            egui::Slider::new(&mut sim_state.wind_dir_deg, 0.0..=360.0)
                .suffix("°")
                .fixed_decimals(0),
        );
    });
    ui.add_space(8.0);

    // ---- Recovery config ----
    ui.collapsing("🪂 Recovery", |ui| {
        ui.horizontal(|ui| {
            ui.label("Main deploy altitude:");
            ui.add(
                egui::Slider::new(&mut sim_state.main_altitude_m, 10.0..=500.0)
                    .suffix(" m")
                    .fixed_decimals(0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Main chute Cd·A:");
            ui.add(egui::Slider::new(&mut sim_state.main_cd_a, 0.1..=5.0).fixed_decimals(2));
        });
        ui.horizontal(|ui| {
            ui.label("Drogue Cd·A:");
            ui.add(egui::Slider::new(&mut sim_state.drogue_cd_a, 0.01..=1.0).fixed_decimals(2));
        });
    });
    ui.add_space(8.0);

    // ---- Run button ----
    // Collect what we need before the mutable borrow of sim_panel ends.
    let run_clicked = !sim_state.available_motors.is_empty()
        && !sim_state.running
        && ui
            .add(
                egui::Button::new("▶ Run Simulation")
                    .fill(egui::Color32::from_rgb(0x2a, 0x6a, 0x3a))
                    .min_size(egui::vec2(ui.available_width(), 32.0)),
            )
            .clicked();

    if run_clicked {
        run_simulation(app);
    }

    ui.add_space(8.0);

    // ---- Results ----
    // Clone what we need to avoid holding a borrow of app.sim_panel.
    let result_opt = app.sim_panel.result.clone();
    if let Some(result) = &result_opt {
        show_results(ui, result);
        ui.add_space(8.0);

        // ---- Playback slider ----
        let max_t = result.landing_time.as_seconds();
        if max_t > 0.0 {
            let mut playback = app.sim_panel.playback_time;
            ui.horizontal(|ui| {
                ui.label("Playback:");
                ui.add(
                    egui::Slider::new(&mut playback, 0.0..=max_t)
                        .suffix(" s")
                        .fixed_decimals(2),
                );
            });
            app.sim_panel.playback_time = playback;

            // Show the telemetry at the playback time.
            if let Some(tp) = result.telemetry_at(rocket_core::Duration::seconds(playback)) {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "t={:.2}s  z={:.1}m  v={:.1}m/s  T={:.1}N",
                        tp.time.as_seconds(),
                        tp.position.2,
                        (tp.velocity.0.powi(2) + tp.velocity.1.powi(2) + tp.velocity.2.powi(2))
                            .sqrt(),
                        tp.thrust.as_newtons()
                    ));
                });
            }
        }

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);

        // ---- Plots ----
        ui.label(egui::RichText::new("Flight Profile").strong());
        plots::show_altitude_plot(ui, result);
        ui.add_space(4.0);
        plots::show_velocity_plot(ui, result);
        ui.add_space(4.0);
        plots::show_thrust_plot(ui, result);
    }
}

/// Run the simulation and store the result.
fn run_simulation(app: &mut AetherApp) {
    let sim_state = &mut app.sim_panel;
    let motor = match sim_state.available_motors.get(sim_state.selected_motor) {
        Some((_, m)) => m.clone(),
        None => return,
    };
    let config = sim_state.build_config();
    let recovery = sim_state.build_recovery();

    sim_state.running = true;
    let sim = Simulator6Dof::new(app.rocket.clone(), motor, config).with_recovery(recovery);
    let result = sim.run();
    sim_state.running = false;
    sim_state.result = Some(result);
    sim_state.playback_time = 0.0;
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
        "Landing time:",
        &format!("{:.2} s", result.landing_time.as_seconds()),
    );
    result_row(
        ui,
        "Landing distance:",
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
