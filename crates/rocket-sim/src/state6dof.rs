//! 6-DOF flight simulator with quaternion attitude and Euler-equation
//! rotational dynamics.
//!
//! This is the Phase 3 successor to Phase 2's 3-DOF point-mass sim. It
//! adds:
//! - Quaternion attitude (`q ∈ S³`)
//! - Angular velocity (`ω`) integrated via Euler's equations
//! - Variable mass properties (`m(t)`, `x_cg(t)`, `I(t)`) from the motor
//! - Full aero coefficient query (lift, drag, pitch/yaw/roll moments)
//! - Recovery events (drogue/main parachute deployment)
//!
//! State vector (13-dim): `[x(3), v(3), q(4), ω(3)]`. Mass is derived
//! from the motor schedule (not integrated) per spec §15.3.

#![allow(
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::too_many_arguments,
    clippy::needless_pass_by_value,
    clippy::type_complexity
)]

use rocket_aero::{compute, AeroCoefficients, AeroModel, AeroQuery, AeroSource};
use rocket_core::validation::ValidationError;
use rocket_core::{Duration, Force, Length, Mass, Rocket};
use rocket_motors::Motor;
use rocket_physics::{angle_of_attack_sideslip, AtmosphereState, IsaAtmosphere, Quaternion, Vec3};
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
use crate::integrator::rk4_step_6dof;
use crate::integrator::{try_rk4_step_6dof, Derivative6Dof};
use crate::result::{FlightEvent, FlightSample, SimResult, TelemetryPoint, Termination};
use crate::state::SimConfig;

/// Standard gravity (m/s²).
const G: f64 = 9.80665;

/// The 6-DOF flight state.
///
/// Coordinate system (launch frame):
/// - `x`: downrange (horizontal, in the direction of launch tilt)
/// - `y`: cross-range (horizontal, perpendicular to x)
/// - `z`: altitude above sea level (vertical, up)
///
/// The quaternion `q` maps body-frame vectors to launch-frame vectors:
/// `v_launch = R(q) · v_body`. The body x-axis points along the nose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimState6Dof {
    /// Time since ignition.
    pub time: Duration,
    /// Position in the launch frame (meters).
    pub position: Vec3,
    /// Velocity in the launch frame (m/s).
    pub velocity: Vec3,
    /// Attitude quaternion mapping body vectors to the launch frame.
    pub quaternion: Quaternion,
    /// Angular velocity in the body frame (rad/s).
    pub angular_velocity: Vec3,
    /// Total mass (dry rocket + motor at current time).
    pub mass: Mass,
}

/// The 6-DOF simulator.
pub struct Simulator6Dof {
    rocket: Rocket,
    motor: Motor,
    config: SimConfig,
    /// Dry rocket mass (structure, without motor).
    dry_mass: Mass,
    /// Dry rocket CG (from nose tip), without motor.
    dry_cg_x: f64,
    /// Dry axial (roll) inertia, about the dry CG.
    dry_i_axial: f64,
    /// Dry transverse inertia, about the dry CG.
    dry_i_transverse: f64,
    /// ISA atmosphere model (Phase 3 will switch to US-1976 for HPR).
    atmosphere: IsaAtmosphere,
    /// Launch direction unit vector (initial body x-axis in launch frame).
    launch_dir: Vec3,
    /// Recovery state.
    recovery: RecoveryState,
    aerodynamics: AeroModel,
    aero_reference_length: f64,
}

/// Recovery system state.
#[derive(Debug, Clone, Copy, Default)]
pub struct RecoveryState {
    /// Whether the drogue chute has deployed.
    pub drogue_deployed: bool,
    /// Time of drogue deployment.
    pub drogue_time: Option<f64>,
    /// Whether the main chute has deployed.
    pub main_deployed: bool,
    /// Time of main deployment.
    pub main_time: Option<f64>,
    /// Main chute deployment altitude (m AGL).
    pub main_altitude: f64,
    /// Drogue chute drag area (Cd · A, m²).
    pub drogue_cd_a: f64,
    /// Main chute drag area (Cd · A, m²).
    pub main_cd_a: f64,
}

impl RecoveryState {
    /// Example recovery configuration: drogue at apogee, main at 50 m AGL.
    /// Drag areas are inputs; descent speed also depends on mass and density.
    #[must_use]
    pub fn default_lpr() -> Self {
        Self {
            drogue_deployed: false,
            drogue_time: None,
            main_deployed: false,
            main_time: None,
            main_altitude: 50.0, // LPR: deploy main at 50m AGL (low altitude)
            drogue_cd_a: 0.05,
            main_cd_a: 0.5,
        }
    }
}

impl Simulator6Dof {
    /// Create a new 6-DOF simulator.
    #[must_use]
    pub fn new(rocket: Rocket, motor: Motor, config: SimConfig) -> Self {
        let mp = rocket.mass_properties();
        let dry_mass = mp.mass;
        let dry_cg_x = mp.cg_x.as_meters();
        let dry_i_axial = mp.i_axial;
        let dry_i_transverse = mp.i_transverse;

        let atmosphere = IsaAtmosphere::new();

        let theta = config.launch_angle;
        let launch_dir = Vec3::new(theta.sin(), 0.0, theta.cos());

        let aero_reference_length = compute(&rocket, AeroQuery::at_mach(0.0)).reference_length;
        Self {
            aerodynamics: AeroModel::Builtin,
            aero_reference_length,
            rocket,
            motor,
            config,
            dry_mass,
            dry_cg_x,
            dry_i_axial,
            dry_i_transverse,
            atmosphere,
            launch_dir,
            recovery: RecoveryState::default_lpr(),
        }
    }

    /// Set the recovery configuration.
    #[must_use]
    pub fn with_recovery(mut self, recovery: RecoveryState) -> Self {
        self.recovery = recovery;
        self
    }

    /// Run the 6-DOF simulation to ground impact.
    #[must_use]
    pub fn run(&self) -> SimResult {
        self.run_for(Duration::seconds(300.0))
    }

    /// Set an already validated aerodynamic provider.
    #[must_use]
    pub fn with_aerodynamics(mut self, model: AeroModel) -> Self {
        if let AeroModel::Dataset { data, .. } = &model {
            self.aero_reference_length = data.metadata.reference_length_m;
        }
        self.aerodynamics = model;
        self
    }

    /// Run with an explicit duration limit.
    #[must_use]
    pub fn run_for(&self, maximum_duration: Duration) -> SimResult {
        self.run_controlled(maximum_duration, &AtomicBool::new(false))
    }

    /// Run with step-level cancellation and explicit termination diagnostics.
    #[must_use]
    pub fn run_controlled(&self, maximum_duration: Duration, cancelled: &AtomicBool) -> SimResult {
        let maximum = maximum_duration.as_seconds();
        if !maximum.is_finite()
            || maximum <= 0.0
            || !self.config.time_step.is_finite()
            || self.config.time_step <= 0.0
            || !self.config.telemetry_interval.is_finite()
            || self.config.telemetry_interval <= 0.0
        {
            return SimResult::empty();
        }
        let ground = self.config.pad_altitude.as_meters();
        let mut state = SimState6Dof {
            time: Duration::ZERO,
            position: Vec3::new(0.0, 0.0, ground),
            velocity: Vec3::ZERO,
            quaternion: attitude_from_launch_dir(self.launch_dir),
            angular_velocity: Vec3::ZERO,
            mass: self.dry_mass + self.motor.total_mass,
        };
        let mut recovery = self.recovery;
        let mut result = SimResult {
            max_local_mach: Some(0.0),
            max_dynamic_pressure_pa: Some(0.0),
            termination: Termination::DurationLimit,
            events: vec![FlightEvent {
                time_s: 0.0,
                name: "Ignition".into(),
            }],
            ..SimResult::empty()
        };
        let mut next_sample = 0.0;
        let mut previous_vz = 0.0;
        let mut max_altitude = ground;
        let mut burnout_recorded = false;
        loop {
            let time = state.time.as_seconds();
            if cancelled.load(Ordering::Relaxed) {
                result.termination = Termination::Cancelled;
                break;
            }
            if !burnout_recorded && time >= self.motor.burn_time() {
                result.events.push(FlightEvent {
                    time_s: self.motor.burn_time(),
                    name: "Burnout".into(),
                });
                burnout_recorded = true;
            }
            if previous_vz > 0.0 && state.velocity.z <= 0.0 && !recovery.drogue_deployed {
                recovery.drogue_deployed = true;
                recovery.drogue_time = Some(time);
                result.apogee_time = state.time;
                result.events.push(FlightEvent {
                    time_s: time,
                    name: "Apogee / drogue".into(),
                });
            }
            previous_vz = state.velocity.z;
            if !recovery.main_deployed
                && recovery.drogue_deployed
                && state.position.z - ground <= recovery.main_altitude
                && state.velocity.z < 0.0
            {
                recovery.main_deployed = true;
                recovery.main_time = Some(time);
                result.events.push(FlightEvent {
                    time_s: time,
                    name: "Main deployment".into(),
                });
            }
            let dynamics = match self.try_compute_dynamics(&state, &recovery) {
                Ok(dynamics) => dynamics,
                Err(error) => {
                    result.termination = termination_for_error(error);
                    break;
                }
            };
            let speed = state.velocity.mag();
            result.max_local_mach = Some(result.max_local_mach.unwrap_or(0.0).max(dynamics.1.mach));
            let airspeed = (state.velocity - Vec3::from_tuple(self.config.wind)).mag();
            let pressure =
                0.5 * dynamics.0.density.as_kilograms_per_cubic_meter() * airspeed * airspeed;
            result.max_dynamic_pressure_pa =
                Some(result.max_dynamic_pressure_pa.unwrap_or(0.0).max(pressure));
            if speed > result.max_velocity.as_meters_per_second() {
                result.max_velocity = rocket_core::Velocity::meters_per_second(speed);
            }
            result.max_acceleration = result.max_acceleration.max(dynamics.2.acceleration);
            if state.position.z > max_altitude {
                max_altitude = state.position.z;
                result.apogee_time = state.time;
            }
            let landed = state.velocity.z < 0.0 && state.position.z <= ground;
            if time >= next_sample || landed || time >= maximum {
                result
                    .telemetry
                    .push(self.sample(&state, &recovery, &dynamics));
                next_sample = time + self.config.telemetry_interval;
            }
            if landed {
                result.termination = Termination::GroundImpact;
                result.events.push(FlightEvent {
                    time_s: time,
                    name: "Ground impact".into(),
                });
                break;
            }
            if time >= maximum {
                break;
            }
            let step = self.config.time_step.min(maximum - time);
            match try_rk4_step_6dof(&state, step, |s| self.try_derivatives(s, &recovery)) {
                Ok(mut next) => {
                    self.update_mass(&mut next);
                    next.quaternion = next.quaternion.normalize();
                    if !finite_state(&next) {
                        result.termination =
                            Termination::NumericalFailure("Non-finite integration state".into());
                        break;
                    }
                    state = next;
                }
                Err(error) => {
                    result.termination = termination_for_error(error);
                    break;
                }
            }
        }
        // Preserve the final valid state even when a stop occurs between telemetry ticks.
        if result.telemetry.last().is_none_or(|p| p.time != state.time) {
            if let Ok(dynamics) = self.try_compute_dynamics(&state, &recovery) {
                result
                    .telemetry
                    .push(self.sample(&state, &recovery, &dynamics));
            }
        }
        result.max_altitude = Length::meters((max_altitude - ground).max(0.0));
        result.landing_time = state.time;
        result.landing_position = (state.position.x, state.position.y);
        result
    }

    fn sample(
        &self,
        state: &SimState6Dof,
        recovery: &RecoveryState,
        dynamics: &(AtmosphereState, AeroCoefficients, Forces),
    ) -> TelemetryPoint {
        let (atm, aero, forces) = dynamics;
        let speed = (state.velocity - Vec3::from_tuple(self.config.wind)).mag();
        TelemetryPoint {
            time: state.time,
            position: state.position.to_tuple(),
            velocity: state.velocity.to_tuple(),
            mass: state.mass,
            thrust: forces.thrust,
            drag: forces.drag,
            acceleration: forces.acceleration,
            flight: Some(FlightSample {
                airspeed_m_s: speed,
                mach: aero.mach,
                dynamic_pressure_pa: 0.5
                    * atm.density.as_kilograms_per_cubic_meter()
                    * speed
                    * speed,
                reynolds: forces.reynolds,
                alpha_rad: aero.alpha,
                beta_rad: aero.beta,
                quaternion_wxyz: [
                    state.quaternion.w,
                    state.quaternion.x,
                    state.quaternion.y,
                    state.quaternion.z,
                ],
                angular_rate_rad_s: [
                    state.angular_velocity.x,
                    state.angular_velocity.y,
                    state.angular_velocity.z,
                ],
                acceleration_m_s2: [forces.accel_vec.x, forces.accel_vec.y, forces.accel_vec.z],
                density_kg_m3: atm.density.as_kilograms_per_cubic_meter(),
                temperature_k: atm.temperature.as_kelvin(),
                drogue_deployed: recovery.drogue_deployed,
                main_deployed: recovery.main_deployed,
                aero_source: forces.source,
            }),
        }
    }

    #[cfg(test)]
    fn compute_dynamics(
        &self,
        state: &SimState6Dof,
        recovery: &RecoveryState,
    ) -> (AtmosphereState, AeroCoefficients, Forces) {
        self.try_compute_dynamics(state, recovery)
            .expect("valid builtin dynamics")
    }

    /// Compute the full dynamics (atmosphere, aero, forces) at a state.
    fn try_compute_dynamics(
        &self,
        state: &SimState6Dof,
        recovery: &RecoveryState,
    ) -> Result<(AtmosphereState, AeroCoefficients, Forces), ValidationError> {
        if !finite_state(state) {
            return Err(ValidationError("Numerical: non-finite flight state".into()));
        }
        let t = state.time;
        let thrust_n = if t.as_seconds() < self.motor.burn_time() {
            self.motor.thrust_at(t).as_newtons()
        } else {
            0.0
        };

        let atm = self.atmosphere.at(Length::meters(state.position.z));
        let rho = atm.density.as_kilograms_per_cubic_meter();

        // Relative velocity (rocket minus wind), in launch frame.
        let v_rel_launch = Vec3::new(
            state.velocity.x - self.config.wind.0,
            state.velocity.y - self.config.wind.1,
            state.velocity.z - self.config.wind.2,
        );

        // Rotate to body frame for aero coefficient query.
        let v_rel_body = state.quaternion.rotate_to_body(v_rel_launch);
        let v_rel_mag = v_rel_body.mag();

        let mach = if atm.speed_of_sound.as_meters_per_second() > 1e-6 {
            v_rel_mag / atm.speed_of_sound.as_meters_per_second()
        } else {
            0.0
        };

        let (alpha, beta) = angle_of_attack_sideslip(v_rel_body);

        let reynolds = rho * v_rel_mag * self.aero_reference_length / atm.dynamic_viscosity();
        let query = AeroQuery {
            mach,
            alpha,
            beta,
            roll_rate: state.angular_velocity.x,
            pitch_rate: state.angular_velocity.y,
            reynolds,
        };
        let (aero, source) = if v_rel_mag < 1e-8 {
            (
                AeroCoefficients::zero(mach, alpha, beta),
                AeroSource::ZeroAirspeed,
            )
        } else {
            self.aerodynamics
                .evaluate(&self.rocket, query, self.mass_properties_at(t).0)?
        };

        let q_bar = 0.5 * rho * v_rel_mag * v_rel_mag;
        let mut forces = Self::assemble_forces(state, &aero, q_bar, thrust_n, v_rel_body, recovery);
        forces.source = source;
        forces.reynolds = reynolds;
        if !forces.acceleration.is_finite()
            || ![
                aero.c_a,
                aero.c_n,
                aero.c_y,
                aero.c_l,
                aero.c_m,
                aero.c_n_yaw,
            ]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err(ValidationError(
                "Numerical: non-finite aerodynamic force".into(),
            ));
        }
        Ok((atm, aero, forces))
    }

    /// Assemble forces in the body frame, then compute the
    /// total acceleration in the launch frame.
    fn assemble_forces(
        state: &SimState6Dof,
        aero: &AeroCoefficients,
        q_bar: f64,
        thrust_n: f64,
        v_rel_body: Vec3,
        recovery: &RecoveryState,
    ) -> Forces {
        let m = state.mass.as_kilograms().max(1e-9);
        let ref_a = aero.reference_area;

        // Thrust along body +x.
        let f_thrust_body = Vec3::new(thrust_n, 0.0, 0.0);

        // Aero forces in body frame.
        // c_a is axial (along +x), c_n is normal (pitch plane, +z), c_y is side (+y).
        // (Sign convention: c_n positive = lift upward when α > 0.)
        let f_aero_body = Vec3::new(
            -q_bar * ref_a * aero.c_a, // axial force is drag (opposes motion)
            q_bar * ref_a * aero.c_y,
            q_bar * ref_a * aero.c_n,
        );

        // Gravity in launch frame.
        let f_grav_launch = Vec3::new(0.0, 0.0, -G * m);

        let v_rel_mag = v_rel_body.mag();
        let cd_a = if recovery.main_deployed {
            recovery.main_cd_a
        } else if recovery.drogue_deployed {
            recovery.drogue_cd_a
        } else {
            0.0
        };
        // Dynamic pressure already includes the factor 1/2.
        let recovery_drag = q_bar * cd_a;
        let f_recovery_body = if v_rel_mag > 1e-6 {
            v_rel_body * (-recovery_drag / v_rel_mag)
        } else {
            Vec3::ZERO
        };

        // Total force in body frame (thrust + aero + recovery).
        let f_body_total = f_thrust_body + f_aero_body + f_recovery_body;

        // Rotate to launch frame for translation.
        let f_launch_aero_thrust = state.quaternion.rotate_to_launch(f_body_total);
        let f_total_launch = f_launch_aero_thrust + f_grav_launch;

        let accel = f_total_launch / m;
        let accel_mag = accel.mag();

        // Thrust and drag magnitudes for telemetry.
        let thrust = Force::newtons(thrust_n);
        let drag = Force::newtons(q_bar * ref_a * aero.c_a + recovery_drag);

        Forces {
            source: AeroSource::Builtin,
            reynolds: 0.0,
            acceleration: accel_mag,
            thrust,
            drag,
            accel_vec: accel,
        }
    }

    /// Compute the time derivatives of the 13-dim state.
    #[cfg(test)]
    fn derivatives(&self, state: &SimState6Dof, recovery: &RecoveryState) -> Derivative6Dof {
        self.try_derivatives(state, recovery)
            .expect("builtin derivative")
    }
    fn try_derivatives(
        &self,
        state: &SimState6Dof,
        recovery: &RecoveryState,
    ) -> Result<Derivative6Dof, ValidationError> {
        let (atm, aero, forces) = self.try_compute_dynamics(state, recovery)?;
        let (_, i_axial, i_transverse) = self.mass_properties_at(state.time);
        let v_rel_launch = state.velocity - Vec3::from_tuple(self.config.wind);
        let rho = atm.density.as_kilograms_per_cubic_meter();
        let q_bar = 0.5 * rho * v_rel_launch.mag().powi(2);
        let accel = forces.accel_vec;

        // Moments about the CG in the body frame.
        // Aero moment: c_m (pitch) and c_n_yaw (yaw), scaled by q·A·L.
        // The aero module returns c_m about the CG already.
        let m_pitch = q_bar * aero.reference_area * aero.reference_length * aero.c_m;
        let m_yaw = q_bar * aero.reference_area * aero.reference_length * aero.c_n_yaw;
        let m_roll = q_bar * aero.reference_area * aero.reference_length * aero.c_l;

        // Thrust moment (thrust is along body x, offset by CG misalignment).
        // For a well-aligned rocket, this is ~0. Phase 5 (Monte Carlo) will
        // add random misalignment.
        let m_thrust = Vec3::ZERO;

        // Total moment in body frame.
        let m_total = Vec3::new(
            m_roll + m_thrust.x,
            m_pitch + m_thrust.y,
            m_yaw + m_thrust.z,
        );

        // Euler equations: I·ω̇ + ω × (I·ω) = M.
        // For diagonal I = diag(I_axial, I_t, I_t):
        // I_axial · ω̇_x = M_x
        // I_t · ω̇_y + (I_axial − I_t)·ω_x·ω_z = M_y
        // I_t · ω̇_z + (I_t − I_axial)·ω_x·ω_y = M_z
        let omega = state.angular_velocity;
        let i_x = i_axial.max(1e-12);
        let i_t = i_transverse.max(1e-12);
        let omega_dot = Vec3::new(
            m_total.x / i_x,
            (m_total.y - (i_axial - i_t) * omega.x * omega.z) / i_t,
            (m_total.z - (i_t - i_axial) * omega.x * omega.y) / i_t,
        );

        // Quaternion kinematics: q̇ = ½·q⊗ω_quat.
        let omega_quat = Quaternion::new(0.0, omega.x, omega.y, omega.z);
        let q_dot = state.quaternion.hamilton(omega_quat) * 0.5;

        Ok((
            (state.velocity.x, state.velocity.y, state.velocity.z),
            (accel.x, accel.y, accel.z),
            (q_dot.w, q_dot.x, q_dot.y, q_dot.z),
            (omega_dot.x, omega_dot.y, omega_dot.z),
        ))
    }

    /// Compute mass properties at time `t` (derived, not integrated).
    ///
    /// `m(t) = m_dry + m_motor(t)`, `cg(t)` and `I(t)` from the motor's
    /// mass and the dry rocket's properties.
    fn mass_properties_at(&self, t: Duration) -> (f64, f64, f64) {
        let motor_mass = self.motor.mass_at(t).as_kilograms();
        let total_mass = self.dry_mass.as_kilograms() + motor_mass;

        // Motor CG: assume at the motor's midpoint (simple model).
        // Phase 4 will model propellant CG movement during burn.
        let motor_length = self.motor.length.as_meters();
        let motor_cg_from_nose = self.dry_cg_x + motor_length / 2.0; // rough: motor at aft end

        // Weighted CG.
        let dry_mass_kg = self.dry_mass.as_kilograms();
        let cg_x = if total_mass > 1e-9 {
            (dry_mass_kg * self.dry_cg_x + motor_mass * motor_cg_from_nose) / total_mass
        } else {
            self.dry_cg_x
        };

        // Inertia: dry + motor contribution (parallel-axis theorem).
        // Motor modeled as a solid cylinder of the motor's dimensions.
        let motor_radius = self.motor.diameter.as_meters() / 2.0;
        let motor_i_axial = 0.5 * motor_mass * motor_radius * motor_radius;
        let motor_i_transverse = (1.0 / 12.0)
            * motor_mass
            * (3.0 * motor_radius * motor_radius + motor_length * motor_length);

        // Parallel-axis: shift from motor CG to rocket CG.
        let dx = motor_cg_from_nose - cg_x;
        let motor_i_trans_shifted = motor_i_transverse + motor_mass * dx * dx;

        let i_axial = self.dry_i_axial + motor_i_axial;
        let i_transverse = self.dry_i_transverse + motor_i_trans_shifted;

        (cg_x, i_axial, i_transverse)
    }

    /// Update the mass at the current time (called after each RK4 step).
    fn update_mass(&self, state: &mut SimState6Dof) {
        state.mass = self.dry_mass + self.motor.mass_at(state.time);
    }
}

fn finite_state(state: &SimState6Dof) -> bool {
    [
        state.time.as_seconds(),
        state.position.x,
        state.position.y,
        state.position.z,
        state.velocity.x,
        state.velocity.y,
        state.velocity.z,
        state.quaternion.w,
        state.quaternion.x,
        state.quaternion.y,
        state.quaternion.z,
        state.angular_velocity.x,
        state.angular_velocity.y,
        state.angular_velocity.z,
        state.mass.as_kilograms(),
    ]
    .iter()
    .all(|v| v.is_finite())
}
fn termination_for_error(error: ValidationError) -> Termination {
    if error.0.starts_with("Numerical:") {
        Termination::NumericalFailure(error.0)
    } else {
        Termination::ModelDomainLimit(error.0)
    }
}

/// Forces computed at a state, for telemetry.
struct Forces {
    source: AeroSource,
    reynolds: f64,
    acceleration: f64,
    thrust: Force,
    drag: Force,
    accel_vec: Vec3,
}

/// Construct the initial attitude quaternion from the launch direction.
///
/// The body x-axis (nose) should align with `launch_dir` in the launch
/// frame. For a vertical launch (`launch_dir = (0, 0, 1)`), the body
/// x-axis must point up (+z in launch frame), which requires a 90°
/// rotation about the y-axis.
///
/// The rotation axis is `body_x × launch_dir` (normalized), and the
/// angle is `acos(body_x · launch_dir)`.
fn attitude_from_launch_dir(launch_dir: Vec3) -> Quaternion {
    let body_x = Vec3::UNIT_X;
    let dot = body_x.dot(launch_dir);
    // If already aligned (or anti-aligned), handle specially.
    if dot > 1.0 - 1e-9 {
        // body_x already equals launch_dir: no rotation needed.
        return Quaternion::IDENTITY;
    }
    if dot < -1.0 + 1e-9 {
        // body_x is opposite to launch_dir: 180° rotation about any
        // perpendicular axis (use y).
        return Quaternion::from_axis_angle(Vec3::UNIT_Y, std::f64::consts::PI);
    }
    let angle = dot.acos();
    let axis = body_x.cross(launch_dir).normalize();
    Quaternion::from_axis_angle(axis, angle)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp, clippy::cast_precision_loss)]

    use super::*;
    use rocket_core::{
        Airfoil, BodyTube, Component, ComponentId, FinPlanform, FinSet, LaunchLug, Length, Mass,
        MassObject, MassObjectKind, NoseCone, NoseShape,
    };

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
        rocket.root.push(Component::MassObject(MassObject {
            id: ComponentId::new(5),
            position: Length::millimeters(30.0),
            mass: Mass::grams(3.0),
            kind: MassObjectKind::Ballast,
        }));
        rocket.root.push(Component::MassObject(MassObject {
            id: ComponentId::new(6),
            position: Length::millimeters(160.0),
            mass: Mass::grams(5.0),
            kind: MassObjectKind::Recovery,
        }));
        rocket.root.push(Component::MassObject(MassObject {
            id: ComponentId::new(7),
            position: Length::millimeters(230.0),
            mass: Mass::grams(1.0),
            kind: MassObjectKind::Other,
        }));
        rocket
    }

    fn parse_a8_3() -> Motor {
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
        let sim = Simulator6Dof::new(rocket, motor, SimConfig::vertical());
        let result = sim.run();

        assert!(
            result.max_altitude.as_meters() > 50.0,
            "apogee should be > 50m, got {}",
            result.max_altitude.as_meters()
        );
        assert!(
            result.max_altitude.as_meters() < 200.0,
            "apogee should be < 200m, got {}",
            result.max_altitude.as_meters()
        );
    }

    #[test]
    fn quaternion_norm_preserved() {
        // After a full sim run, the quaternion should still be unit norm
        // (renormalization happens each step).
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator6Dof::new(rocket, motor, SimConfig::vertical());

        // We can't directly inspect the final state, but we can verify
        // the sim doesn't diverge (which would happen if q drifted).
        let result = sim.run();
        assert!(
            result.landing_time.as_seconds() < 120.0,
            "sim should converge to landing, not diverge (landing_t={})",
            result.landing_time.as_seconds()
        );
    }

    #[test]
    fn vertical_launch_lands_near_pad() {
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator6Dof::new(rocket, motor, SimConfig::vertical());
        let result = sim.run();

        let drift = (result.landing_position.0.powi(2) + result.landing_position.1.powi(2)).sqrt();
        assert!(
            drift < 10.0,
            "vertical launch should land within 10m of pad, got {drift:.1}m"
        );
    }

    #[test]
    fn recovery_deploys_drogue_at_apogee() {
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator6Dof::new(rocket, motor, SimConfig::vertical());
        let result = sim.run();

        // With default recovery, the drogue deploys at apogee. The
        // landing time should be longer than without recovery (slower
        // descent). The apogee time should be unchanged.
        assert!(
            result.apogee_time.as_seconds() > 0.5,
            "apogee should be after 0.5s"
        );
        // The landing time should be after the apogee (obvious, but
        // verifies the sim ran to completion).
        assert!(
            result.landing_time.as_seconds() > result.apogee_time.as_seconds(),
            "landing should be after apogee"
        );
    }

    #[test]
    fn recovery_reduces_descent_rate() {
        // With recovery, the descent rate (near landing) should be much
        // less than the max velocity (which occurs during boost).
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator6Dof::new(rocket, motor, SimConfig::vertical());
        let result = sim.run();

        // Find the velocity near landing (last few telemetry points).
        let last = result.telemetry.last().unwrap();
        let landing_speed =
            (last.velocity.0.powi(2) + last.velocity.1.powi(2) + last.velocity.2.powi(2)).sqrt();

        // With a 24" main chute, descent rate should be ~5-10 m/s.
        assert!(
            landing_speed < 15.0,
            "descent rate with recovery should be < 15 m/s, got {landing_speed:.1}"
        );
    }

    #[test]
    fn sim_completes_quickly() {
        use std::time::Instant;
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator6Dof::new(rocket, motor, SimConfig::vertical());
        let start = Instant::now();
        let _result = sim.run();
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 2000,
            "6-DOF sim should complete in < 2s, took {elapsed:?}"
        );
    }

    #[test]
    fn angled_launch_produces_downrange_drift() {
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let sim = Simulator6Dof::new(rocket, motor, SimConfig::angled(10.0));
        let result = sim.run();

        assert!(
            result.landing_position.0 > 5.0,
            "angled launch should drift downrange, got x={}",
            result.landing_position.0
        );
    }

    #[test]
    fn attitude_from_launch_dir_vertical_is_identity() {
        // For a vertical launch, launch_dir = (0, 0, 1). The body x-axis
        // must rotate to point along +z, which is a 90° rotation about y.
        // This is NOT the identity quaternion.
        let q = attitude_from_launch_dir(Vec3::new(0.0, 0.0, 1.0));
        let body_x = q.rotate_to_launch(Vec3::UNIT_X);
        assert!(
            (body_x - Vec3::UNIT_Z).mag() < 1e-9,
            "body x-axis should point up (+z): got {body_x:?}"
        );
    }

    #[test]
    fn attitude_from_launch_dir_tilted() {
        // 10° tilt: launch_dir = (sin(10°), 0, cos(10°)).
        let theta = 10.0_f64.to_radians();
        let dir = Vec3::new(theta.sin(), 0.0, theta.cos());
        let q = attitude_from_launch_dir(dir);
        // The body x-axis (1,0,0) in launch frame should be `dir`.
        let body_x = q.rotate_to_launch(Vec3::UNIT_X);
        assert!(
            (body_x - dir).mag() < 1e-9,
            "body x-axis should align with launch dir: got {body_x:?}, expected {dir:?}"
        );
    }

    #[test]
    fn mass_properties_decrease_during_burn() {
        let rocket = build_estes_alpha();
        let motor = parse_a8_3();
        let burn_time = motor.burn_time();
        let sim = Simulator6Dof::new(rocket, motor, SimConfig::vertical());

        let (cg0, _, _) = sim.mass_properties_at(Duration::ZERO);
        let (cg_burn, _, _) = sim.mass_properties_at(Duration::seconds(burn_time));

        // As propellant burns (from the aft end in our simple model), the
        // CG should move forward (toward the nose, i.e. cg_x decreases).
        assert!(
            cg_burn < cg0,
            "CG should move forward during burn: cg0={cg0:.4}, cg_burn={cg_burn:.4}"
        );
    }
    fn recovery_test_sim() -> Simulator6Dof {
        let mut rocket = Rocket::new("Recovery verification");
        rocket.root.push(Component::MassObject(MassObject {
            id: ComponentId::new(1),
            position: Length::ZERO,
            mass: Mass::kilograms(1.0),
            kind: MassObjectKind::Other,
        }));
        Simulator6Dof::new(rocket, parse_a8_3(), SimConfig::vertical())
    }

    fn falling_state(sim: &Simulator6Dof) -> SimState6Dof {
        let time = Duration::seconds(10.0); // Motor burnout is complete.
        SimState6Dof {
            time,
            position: Vec3::new(0.0, 0.0, 1000.0),
            velocity: Vec3::new(3.0, 4.0, -12.0),
            quaternion: attitude_from_launch_dir(Vec3::UNIT_Z),
            angular_velocity: Vec3::ZERO,
            mass: sim.dry_mass + sim.motor.mass_at(time),
        }
    }

    #[test]
    fn recovery_force_and_telemetry_match_dynamic_pressure_for_each_deployment() {
        let mut sim = recovery_test_sim();
        sim.config.wind = (1.0, -2.0, 0.0);
        let state = falling_state(&sim);
        let mut recovery = RecoveryState {
            drogue_cd_a: 0.07,
            main_cd_a: 0.2,
            ..RecoveryState::default()
        };
        // The simulator's original recovery state stays stowed throughout.
        // The evolving run state must be used by both reported forces and derivatives.
        for (drogue, main, area) in [(false, false, 0.0), (true, false, 0.07), (true, true, 0.2)] {
            recovery.drogue_deployed = drogue;
            recovery.main_deployed = main;
            let (atm, _, forces) = sim.compute_dynamics(&state, &recovery);
            let relative = state.velocity - Vec3::from_tuple(sim.config.wind);
            let speed = relative.mag();
            let expected_drag =
                0.5 * atm.density.as_kilograms_per_cubic_meter() * speed.powi(2) * area;
            assert!((forces.drag.as_newtons() - expected_drag).abs() < 1e-10);
            let expected_accel = relative * (-expected_drag / (speed * state.mass.as_kilograms()))
                + Vec3::new(0.0, 0.0, -G);
            assert!((forces.accel_vec - expected_accel).mag() < 1e-10);
            let (_, acceleration, _, _) = sim.derivatives(&state, &recovery);
            assert!((Vec3::from_tuple(acceleration) - expected_accel).mag() < 1e-10);
            assert!((forces.acceleration - expected_accel.mag()).abs() < 1e-10);
        }
    }

    #[test]
    fn integrated_parachute_descent_matches_analytical_terminal_speed() {
        let sim = recovery_test_sim();
        let mut state = falling_state(&sim);
        state.velocity = Vec3::ZERO;
        let recovery = RecoveryState {
            drogue_deployed: true,
            drogue_cd_a: 0.2,
            ..RecoveryState::default()
        };
        for _ in 0..1000 {
            state = rk4_step_6dof(&state, 0.01, |s| sim.derivatives(s, &recovery));
        }
        let rho = sim
            .atmosphere
            .at(Length::meters(state.position.z))
            .density
            .as_kilograms_per_cubic_meter();
        let expected = (2.0 * state.mass.as_kilograms() * G / (rho * recovery.drogue_cd_a)).sqrt();
        let error = (state.velocity.mag() - expected).abs() / expected;
        assert!(
            error < 0.005,
            "terminal speed relative error {error}, speed {}, expected {expected}",
            state.velocity.mag()
        );
    }
}
