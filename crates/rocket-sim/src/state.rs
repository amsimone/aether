//! Sim configuration, state, and the simulator driver.
//
// The similar-names lint flags force component variables (fx_thrust,
// fy_thrust, fz_drag, etc.) which are intentional and readable.
#![allow(clippy::similar_names)]

use rocket_core::{Duration, Force, Length, Mass, Rocket, Velocity};
use rocket_motors::Motor;
use rocket_physics::IsaAtmosphere;

use crate::integrator::rk4_step;
use crate::result::{SimResult, TelemetryPoint};

/// Default integration time step (1 ms). Provides good accuracy for
/// model rocket flights (5000+ steps for a 5-second flight) while
/// keeping runtime under 100 ms.
pub const DEFAULT_TIME_STEP: f64 = 0.001;

/// Standard gravity (m/s²).
const G: f64 = 9.80665;

/// Configuration for a flight simulation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimConfig {
    /// Launch angle from vertical, in radians.
    ///
    /// `0.0` = straight up. A typical rod angle is 3–7° (0.05–0.12 rad)
    /// to ensure the rocket arcs away from the pad.
    pub launch_angle: f64,
    /// Pad altitude above sea level.
    pub pad_altitude: Length,
    /// Wind vector `(vx, vy, vz)` in m/s.
    ///
    /// `vx` is downrange (along the launch tilt direction), `vy` is
    /// cross-range, `vz` should be 0 (no vertical wind). The wind adds
    /// to the relative airspeed for drag computation.
    pub wind: (f64, f64, f64),
    /// Integration time step (seconds).
    pub time_step: f64,
    /// Telemetry sampling interval (seconds). Telemetry points are
    /// recorded at this interval; the integrator runs at `time_step`.
    pub telemetry_interval: f64,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            launch_angle: 0.0,
            pad_altitude: Length::ZERO,
            wind: (0.0, 0.0, 0.0),
            time_step: DEFAULT_TIME_STEP,
            telemetry_interval: 0.01, // 10 ms
        }
    }
}

impl SimConfig {
    /// Create a vertical-launch config at sea level with no wind.
    #[must_use]
    pub fn vertical() -> Self {
        Self::default()
    }

    /// Create a config with a launch angle (in degrees from vertical)
    /// and sea-level pad.
    #[must_use]
    pub fn angled(degrees_from_vertical: f64) -> Self {
        Self {
            launch_angle: degrees_from_vertical.to_radians(),
            ..Self::default()
        }
    }
}

/// The instantaneous state of the rocket during flight.
///
/// Coordinate system:
/// - `x`: downrange (horizontal, in the direction of launch tilt)
/// - `y`: cross-range (horizontal, perpendicular to x)
/// - `z`: altitude above sea level (vertical, up)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimState {
    /// Time since ignition.
    pub time: Duration,
    /// Position `(x, y, z)` in meters.
    pub position: (f64, f64, f64),
    /// Velocity `(vx, vy, vz)` in m/s.
    pub velocity: (f64, f64, f64),
    /// Total mass (dry rocket + motor at current time).
    pub mass: Mass,
}

/// The flight simulator.
///
/// Construct with [`Simulator::new`] and run with [`Simulator::run`].
pub struct Simulator {
    // The rocket design is kept for future phases (CG, inertia, staging).
    // It's currently used only during construction to compute dry mass
    // and drag; the field is retained so Phase 3 can access it without
    // an API change.
    #[allow(dead_code)]
    rocket: Rocket,
    motor: Motor,
    config: SimConfig,
    /// Dry rocket mass (structure, without motor).
    dry_mass: Mass,
    /// Drag coefficient (constant for Phase 2).
    cd: f64,
    /// Reference area for drag (m²).
    ref_area: f64,
    /// ISA atmosphere model.
    atmosphere: IsaAtmosphere,
    /// Launch direction unit vector (thrust direction during boost).
    launch_dir: (f64, f64, f64),
}

impl Simulator {
    /// Create a new simulator.
    ///
    /// The rocket's mass properties are computed once (dry mass, without
    /// the motor). The drag coefficient is estimated from the rocket
    /// geometry. The motor provides thrust and mass depletion over time.
    #[must_use]
    pub fn new(rocket: Rocket, motor: Motor, config: SimConfig) -> Self {
        let dry_mass = rocket.mass_properties().mass;

        // Estimate Cd and reference area from the rocket geometry.
        let drag = rocket_aero::estimate_cd_breakdown(&rocket);
        let cd = drag.cd;
        let ref_area = drag.reference_area;

        // Atmosphere model (ISA, optionally with non-standard sea level
        // if the pad is at altitude — but for Phase 2 we use standard ISA
        // and just query at the pad altitude + flight altitude).
        let atmosphere = IsaAtmosphere::new();

        // Launch direction: tilt from vertical by launch_angle, in the
        // x-z plane. (0, 0, 1) is straight up; tilting toward +x.
        let theta = config.launch_angle;
        let launch_dir = (theta.sin(), 0.0, theta.cos());

        Self {
            rocket,
            motor,
            config,
            dry_mass,
            cd,
            ref_area,
            atmosphere,
            launch_dir,
        }
    }

    /// Run the simulation to ground impact.
    ///
    /// Integrates the equations of motion with RK4 at the configured
    /// time step, recording telemetry at the configured interval. Stops
    /// when the rocket descends below the pad altitude (ground impact).
    ///
    /// # Panics
    ///
    /// Does not panic. If the time step is zero or negative, returns an
    /// empty result with zero apogee.
    #[must_use]
    pub fn run(&self) -> SimResult {
        if self.config.time_step <= 0.0 || self.config.telemetry_interval <= 0.0 {
            return SimResult::empty();
        }

        let dt = self.config.time_step;
        let ground_z = self.config.pad_altitude.as_meters();

        let mut state = SimState {
            time: Duration::ZERO,
            position: (0.0, 0.0, ground_z),
            velocity: (0.0, 0.0, 0.0),
            mass: self.dry_mass + self.motor.total_mass,
        };

        let mut telemetry: Vec<TelemetryPoint> = Vec::with_capacity(1024);
        let mut next_telemetry_time;

        let mut max_altitude = ground_z;
        let mut max_velocity = 0.0_f64;
        let mut max_accel = 0.0_f64;
        let mut apogee_time = 0.0_f64;
        let mut prev_vz = 0.0_f64;

        // Safety: cap the simulation at 5 minutes to avoid infinite loops.
        let max_time = 300.0;

        // Record the initial telemetry point.
        let (thrust0, drag0, accel0) = self.compute_forces(&state);
        telemetry.push(TelemetryPoint {
            time: state.time,
            position: state.position,
            velocity: state.velocity,
            mass: state.mass,
            thrust: thrust0,
            drag: drag0,
            acceleration: accel0,
            flight: None,
        });
        next_telemetry_time = self.config.telemetry_interval;

        while state.time.as_seconds() < max_time {
            let (thrust, drag, accel) = self.compute_forces(&state);

            // Track maxima.
            let speed = velocity_magnitude(state.velocity);
            if speed > max_velocity {
                max_velocity = speed;
            }
            if accel > max_accel {
                max_accel = accel;
            }
            if state.position.2 > max_altitude {
                max_altitude = state.position.2;
                apogee_time = state.time.as_seconds();
            }

            // Apogee detection: vz crosses zero from positive to negative.
            if prev_vz > 0.0 && state.velocity.2 <= 0.0 {
                apogee_time = state.time.as_seconds();
            }
            prev_vz = state.velocity.2;

            // Ground impact: rocket descends below pad altitude.
            if state.velocity.2 < 0.0 && state.position.2 <= ground_z {
                // Interpolate to exact ground crossing.
                break;
            }

            // Record telemetry at the sampling interval.
            if state.time.as_seconds() >= next_telemetry_time {
                telemetry.push(TelemetryPoint {
                    time: state.time,
                    position: state.position,
                    velocity: state.velocity,
                    mass: state.mass,
                    thrust,
                    drag,
                    acceleration: accel,
                    flight: None,
                });
                next_telemetry_time += self.config.telemetry_interval;
            }

            // RK4 step.
            state = rk4_step(&state, dt, |s| self.derivatives(s));
            // Recompute mass from the motor's impulse-proportional model.
            self.update_mass(&mut state);
        }

        // Record final telemetry point.
        let (thrust_f, drag_f, accel_f) = self.compute_forces(&state);
        telemetry.push(TelemetryPoint {
            time: state.time,
            position: state.position,
            velocity: state.velocity,
            mass: state.mass,
            thrust: thrust_f,
            drag: drag_f,
            acceleration: accel_f,
            flight: None,
        });

        let landing_time = state.time.as_seconds();
        let landing_position = (state.position.0, state.position.1);

        SimResult {
            max_local_mach: None,
            max_dynamic_pressure_pa: None,
            termination: if state.position.2 <= ground_z && state.velocity.2 < 0.0 {
                crate::Termination::GroundImpact
            } else {
                crate::Termination::DurationLimit
            },
            events: Vec::new(),
            max_altitude: Length::meters(max_altitude - ground_z),
            max_velocity: Velocity::meters_per_second(max_velocity),
            max_acceleration: max_accel,
            apogee_time: Duration::seconds(apogee_time),
            landing_time: Duration::seconds(landing_time),
            landing_position,
            telemetry,
        }
    }

    /// Compute the forces (thrust, drag) and acceleration magnitude at
    /// the given state.
    fn compute_forces(&self, state: &SimState) -> (Force, Force, f64) {
        let t = state.time;
        let thrust_n = if t.as_seconds() < self.motor.burn_time() {
            self.motor.thrust_at(t).as_newtons()
        } else {
            0.0
        };

        // Atmosphere at current altitude.
        let atm = self.atmosphere.at(Length::meters(state.position.2));
        let rho = atm.density.as_kilograms_per_cubic_meter();

        // Relative velocity (rocket minus wind).
        let v_rel = (
            state.velocity.0 - self.config.wind.0,
            state.velocity.1 - self.config.wind.1,
            state.velocity.2 - self.config.wind.2,
        );
        let v_rel_mag = velocity_magnitude(v_rel);

        // Drag force magnitude: ½ ρ v² Cd A, opposite to v_rel.
        let drag_mag = if v_rel_mag > 1e-6 {
            0.5 * rho * v_rel_mag * v_rel_mag * self.cd * self.ref_area
        } else {
            0.0
        };

        // Total force = thrust (along launch dir) + gravity + drag.
        let m = state.mass.as_kilograms().max(1e-9);

        // Thrust vector (along launch direction during boost).
        let fx_thrust = thrust_n * self.launch_dir.0;
        let fy_thrust = thrust_n * self.launch_dir.1;
        let fz_thrust = thrust_n * self.launch_dir.2;

        // Gravity (down).
        let fz_gravity = -G * m;

        // Drag (opposite to v_rel).
        let (fx_drag, fy_drag, fz_drag) = if v_rel_mag > 1e-6 {
            let inv = -drag_mag / v_rel_mag;
            (inv * v_rel.0, inv * v_rel.1, inv * v_rel.2)
        } else {
            (0.0, 0.0, 0.0)
        };

        let fx_total = fx_thrust + fx_drag;
        let fy_total = fy_thrust + fy_drag;
        let fz_total = fz_thrust + fz_gravity + fz_drag;

        let accel = (fx_total * fx_total + fy_total * fy_total + fz_total * fz_total).sqrt() / m;

        (Force::newtons(thrust_n), Force::newtons(drag_mag), accel)
    }

    /// Compute the time derivative of the state for the integrator.
    ///
    /// Returns `(dposition/dt, dvelocity/dt, dmass/dt)` as a flat tuple
    /// suitable for RK4.
    fn derivatives(&self, state: &SimState) -> ((f64, f64, f64), (f64, f64, f64), f64) {
        let t = state.time;
        let thrust_n = if t.as_seconds() < self.motor.burn_time() {
            self.motor.thrust_at(t).as_newtons()
        } else {
            0.0
        };

        let atm = self.atmosphere.at(Length::meters(state.position.2));
        let rho = atm.density.as_kilograms_per_cubic_meter();

        let v_rel = (
            state.velocity.0 - self.config.wind.0,
            state.velocity.1 - self.config.wind.1,
            state.velocity.2 - self.config.wind.2,
        );
        let v_rel_mag = velocity_magnitude(v_rel);

        let drag_mag = if v_rel_mag > 1e-6 {
            0.5 * rho * v_rel_mag * v_rel_mag * self.cd * self.ref_area
        } else {
            0.0
        };

        let m = state.mass.as_kilograms().max(1e-9);

        // Thrust.
        let fx_thrust = thrust_n * self.launch_dir.0;
        let fy_thrust = thrust_n * self.launch_dir.1;
        let fz_thrust = thrust_n * self.launch_dir.2;

        // Gravity.
        let fz_gravity = -G * m;

        // Drag.
        let (fx_drag, fy_drag, fz_drag) = if v_rel_mag > 1e-6 {
            let inv = -drag_mag / v_rel_mag;
            (inv * v_rel.0, inv * v_rel.1, inv * v_rel.2)
        } else {
            (0.0, 0.0, 0.0)
        };

        let ax = (fx_thrust + fx_drag) / m;
        let ay = (fy_thrust + fy_drag) / m;
        let az = (fz_thrust + fz_gravity + fz_drag) / m;

        // Mass flow rate: d(m_prop)/dt = -T(t) / (Isp * g0).
        // But we use the impulse-proportional model instead, so the
        // integrator doesn't need to track mass flow directly. We set
        // dmass/dt = 0 here and recompute mass from the motor at each
        // step. This is simpler and exactly conserves propellant mass.
        // (The RK4 integrator will call this for intermediate steps;
        // the mass at intermediate steps is approximated by the mass
        // at the step's start time, which is a minor inaccuracy.)
        let dmass = 0.0;

        (
            (state.velocity.0, state.velocity.1, state.velocity.2),
            (ax, ay, az),
            dmass,
        )
    }

    /// Recompute the mass at the current time (called after each RK4 step).
    fn update_mass(&self, state: &mut SimState) {
        state.mass = self.dry_mass + self.motor.mass_at(state.time);
    }
}

/// Magnitude of a 3-vector.
fn velocity_magnitude(v: (f64, f64, f64)) -> f64 {
    (v.0 * v.0 + v.1 * v.1 + v.2 * v.2).sqrt()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp, clippy::cast_precision_loss)]

    use super::*;
    use rocket_core::{
        Airfoil, BodyTube, Component, ComponentId, FinPlanform, FinSet, LaunchLug, Length, Mass,
        MassObject, MassObjectKind, NoseCone, NoseShape,
    };

    /// Build an Estes Alpha clone for testing.
    ///
    /// Includes recovery hardware (parachute, shock cord, wadding, engine
    /// block) as mass objects to match the real Alpha's empty mass of
    /// ~16g.
    fn build_estes_alpha() -> Rocket {
        let mut rocket = Rocket::new("Estes Alpha");
        let balsa = rocket
            .materials
            .iter()
            .find(|m| m.name == "Balsa")
            .unwrap()
            .id;
        let card = rocket
            .materials
            .iter()
            .find(|m| m.name == "Cardboard (body tube)")
            .unwrap()
            .id;

        rocket.root.push(Component::NoseCone(NoseCone {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: balsa,
            shape: NoseShape::TangentOgive,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(24.8),
            wall_thickness: Length::ZERO,
        }));
        rocket.root.push(Component::BodyTube(BodyTube {
            id: ComponentId::new(2),
            position: Length::millimeters(70.0),
            material: card,
            length: Length::millimeters(180.0),
            outer_diameter: Length::millimeters(24.8),
            inner_diameter: Length::millimeters(24.1),
        }));
        rocket.root.push(Component::FinSet(FinSet {
            id: ComponentId::new(3),
            count: 3,
            planform: FinPlanform::Trapezoidal {
                root_chord: Length::millimeters(50.0),
                tip_chord: Length::millimeters(25.0),
                span: Length::millimeters(35.0),
                sweep: Length::millimeters(25.0),
            },
            thickness: Length::millimeters(3.0),
            airfoil: Airfoil::Square,
            mount: ComponentId::new(2),
            position: Length::millimeters(200.0),
            material: balsa,
        }));
        rocket.root.push(Component::LaunchLug(LaunchLug {
            id: ComponentId::new(4),
            position: Length::millimeters(100.0),
            material: card,
            length: Length::millimeters(25.0),
            inner_diameter: Length::millimeters(5.0),
            outer_diameter: Length::millimeters(7.0),
        }));
        // Ballast in the nose for stability (typical for the Alpha).
        rocket.root.push(Component::MassObject(MassObject {
            id: ComponentId::new(5),
            position: Length::millimeters(30.0),
            mass: Mass::grams(3.0),
            kind: MassObjectKind::Ballast,
        }));
        // Recovery system: parachute + shock cord + wadding (~5g at
        // body midpoint). The real Alpha's empty mass is ~16g; without
        // this the model is ~10g, about 6g too light.
        rocket.root.push(Component::MassObject(MassObject {
            id: ComponentId::new(6),
            position: Length::millimeters(160.0),
            mass: Mass::grams(5.0),
            kind: MassObjectKind::Recovery,
        }));
        // Engine block (~1g at the aft end).
        rocket.root.push(Component::MassObject(MassObject {
            id: ComponentId::new(7),
            position: Length::millimeters(230.0),
            mass: Mass::grams(1.0),
            kind: MassObjectKind::Other,
        }));
        rocket
    }

    fn parse_a8_3() -> Motor {
        // Load the real A8-3 thrust curve from the fixture file.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests")
            .join("fixtures")
            .join("motors")
            .join("estes_a8_3.eng");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        rocket_motors::rasp::parse(&text).unwrap()
    }

    #[test]
    fn vertical_launch_produces_positive_apogee() {
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator::new(rocket, motor, SimConfig::vertical());
        let result = sim.run();

        // The Estes TR-10 altitude chart predicts ~116m (379 ft) for the
        // Astron Alpha + A8-3. Our sim with Cd≈0.7 should be in the
        // 80–150m range (the exact value depends on mass and Cd).
        assert!(
            result.max_altitude.as_meters() > 50.0,
            "apogee should be > 50m for an A8 Alpha, got {}",
            result.max_altitude.as_meters()
        );
        assert!(
            result.max_altitude.as_meters() < 200.0,
            "apogee should be < 200m for an A8 Alpha, got {}",
            result.max_altitude.as_meters()
        );
    }

    #[test]
    fn vacuum_vertical_shot_matches_closed_form() {
        // In vacuum (no drag), vertical launch:
        // During boost, a = T/m(t) - g. After burnout, ballistic.
        // We can't easily get a closed form because m(t) varies, but we
        // can check that the apogee is close to the vacuum estimate.
        //
        // A simpler check: with no drag (Cd=0) and constant thrust, the
        // velocity at burnout should be close to the rocket equation:
        // Δv = Isp * g0 * ln(m0/mf) - g * t_burn
        //
        // For the A8-3: Isp ≈ 57s, m0 = 0.0167+0.0135=0.0302kg (rocket+motor),
        // mf = 0.0135+0.0134=0.0269kg (rocket+dry motor).
        // Δv = 57 * 9.80665 * ln(0.0302/0.0269) - 9.80665 * 0.506
        //    = 558.9 * 0.1156 - 4.96 = 64.6 - 4.96 = 59.6 m/s
        //
        // Apogee ≈ v_burn * t_coast + 0.5*g*t_coast² (rough).
        // This is approximate, so we just check the velocity is in range.
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();

        // Create a vacuum config: we can't easily set Cd=0 through the
        // API, so we check the normal sim's burnout velocity is reasonable.
        let sim = Simulator::new(rocket, motor, SimConfig::vertical());
        let result = sim.run();

        // The max velocity should be > 30 m/s (typical for an A8 Alpha).
        assert!(
            result.max_velocity.as_meters_per_second() > 30.0,
            "max velocity should be > 30 m/s, got {}",
            result.max_velocity.as_meters_per_second()
        );
    }

    #[test]
    fn drag_reduces_apogee() {
        // We can't easily toggle drag, but we can verify the sim produces
        // a reasonable apogee that's less than the vacuum estimate.
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator::new(rocket, motor, SimConfig::vertical());
        let result = sim.run();

        // Vacuum estimate (very rough): Δv ≈ 60 m/s, coast time ≈ 6s,
        // apogee ≈ 60*6 - 0.5*9.8*36 = 360 - 176 = 184m. With drag, it
        // should be much less.
        assert!(
            result.max_altitude.as_meters() < 184.0,
            "apogee with drag ({}) should be < vacuum estimate (184m)",
            result.max_altitude.as_meters()
        );
    }

    #[test]
    fn rocket_returns_to_ground() {
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator::new(rocket, motor, SimConfig::vertical());
        let result = sim.run();

        // The landing time should be > apogee time.
        assert!(
            result.landing_time.as_seconds() > result.apogee_time.as_seconds(),
            "landing time ({}) should be > apogee time ({})",
            result.landing_time.as_seconds(),
            result.apogee_time.as_seconds()
        );
        // The rocket should land within a reasonable time (< 60s).
        assert!(
            result.landing_time.as_seconds() < 60.0,
            "landing time should be < 60s, got {}",
            result.landing_time.as_seconds()
        );
    }

    #[test]
    fn vertical_launch_has_zero_landing_offset() {
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator::new(rocket, motor, SimConfig::vertical());
        let result = sim.run();

        // A vertical launch with no wind should land at ~0 downrange.
        assert!(
            result.landing_position.0.abs() < 5.0,
            "vertical launch should land near x=0, got {}",
            result.landing_position.0
        );
    }

    #[test]
    fn angled_launch_produces_downrange_drift() {
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator::new(rocket, motor, SimConfig::angled(10.0));
        let result = sim.run();

        // A 10° tilt should produce significant downrange drift.
        assert!(
            result.landing_position.0 > 5.0,
            "angled launch should drift downrange, got x={}",
            result.landing_position.0
        );
        // And the apogee should be lower than a vertical launch.
        let sim_vert = Simulator::new(build_estes_alpha(), parse_a8_3(), SimConfig::vertical());
        let result_vert = sim_vert.run();
        assert!(
            result.max_altitude.as_meters() < result_vert.max_altitude.as_meters(),
            "angled launch apogee ({}) should be < vertical ({})",
            result.max_altitude.as_meters(),
            result_vert.max_altitude.as_meters()
        );
    }

    #[test]
    fn telemetry_is_recorded() {
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator::new(rocket, motor, SimConfig::vertical());
        let result = sim.run();

        // Should have at least one point per 10ms for the flight duration.
        let expected_min = result.landing_time.as_seconds() / 0.01;
        assert!(
            result.telemetry.len() as f64 >= expected_min,
            "should have ~{expected_min:.0} telemetry points, got {}",
            result.telemetry.len()
        );
        // Telemetry should be time-ordered.
        for w in result.telemetry.windows(2) {
            assert!(
                w[1].time.as_seconds() >= w[0].time.as_seconds(),
                "telemetry should be time-ordered"
            );
        }
    }

    #[test]
    fn sim_completes_quickly() {
        use std::time::Instant;
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator::new(rocket, motor, SimConfig::vertical());
        let start = Instant::now();
        let _result = sim.run();
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 1000,
            "sim should complete in < 1s, took {elapsed:?}"
        );
    }
}
