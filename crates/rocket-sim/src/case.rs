//! Persistable simulation inputs. Deployment flags belong to a run, not its case.

use crate::{RecoveryState, SimConfig, SimResult, Simulator6Dof};
use rocket_core::{
    validation::{validate_rocket, ValidationError},
    Duration, Rocket,
};
use rocket_motors::Motor;
use serde::{Deserialize, Serialize};

/// Explicit model implementation selection; new implementations need new variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelSet {
    /// Current rigid 6-DOF, built-in aero, ISA, constant gravity, fixed-step RK4.
    #[default]
    Builtin6DofRk4IsaV1,
}

/// Recovery inputs, separate from evolving deployment state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryConfig {
    /// Main deployment height above launch pad (m), after apogee.
    pub main_altitude_m: f64,
    /// Main drag coefficient times area (m²).
    pub main_cd_a: f64,
    /// Drogue drag coefficient times area (m²), deployed at apogee.
    pub drogue_cd_a: f64,
}
impl Default for RecoveryConfig {
    fn default() -> Self {
        let state = RecoveryState::default_lpr();
        Self {
            main_altitude_m: state.main_altitude,
            main_cd_a: state.main_cd_a,
            drogue_cd_a: state.drogue_cd_a,
        }
    }
}
impl RecoveryConfig {
    /// A fresh, stowed runtime state for each run.
    #[must_use]
    pub fn initial_state(&self) -> RecoveryState {
        RecoveryState {
            main_altitude: self.main_altitude_m,
            main_cd_a: self.main_cd_a,
            drogue_cd_a: self.drogue_cd_a,
            ..RecoveryState::default()
        }
    }
}

/// One named case, with all configurable inputs consumed by the current solver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationCase {
    /// Name displayed in the analysis panel.
    pub name: String,
    /// Resolved motor curve and mass data; no catalog-index dependency.
    pub motor: Option<Motor>,
    /// Launch, wind, integration, and telemetry settings in SI units.
    pub config: SimConfig,
    /// Recovery configuration.
    pub recovery: RecoveryConfig,
    /// Explicit implementation set.
    pub models: ModelSet,
    /// Stop after this many seconds if ground impact has not occurred.
    pub maximum_duration_s: f64,
}
impl Default for SimulationCase {
    fn default() -> Self {
        Self {
            name: "Nominal".into(),
            motor: None,
            config: SimConfig::default(),
            recovery: RecoveryConfig::default(),
            models: ModelSet::default(),
            maximum_duration_s: 300.0,
        }
    }
}
impl SimulationCase {
    /// Validate persisted inputs. An unresolved motor is allowed while editing.
    ///
    /// # Errors
    /// Rejects non-finite or out-of-scope settings and malformed motor data.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let config = &self.config;
        let valid = !self.name.trim().is_empty()
            && config.launch_angle.is_finite()
            && (0.0..std::f64::consts::FRAC_PI_2).contains(&config.launch_angle)
            && config.pad_altitude.as_meters().is_finite()
            && (-500.0..=20_000.0).contains(&config.pad_altitude.as_meters())
            && [config.wind.0, config.wind.1, config.wind.2]
                .iter()
                .all(|v| v.is_finite())
            && config.time_step.is_finite()
            && (0.0001..=0.1).contains(&config.time_step)
            && config.telemetry_interval.is_finite()
            && config.telemetry_interval >= config.time_step
            && self.maximum_duration_s.is_finite()
            && (config.time_step..=3600.0).contains(&self.maximum_duration_s)
            && self.maximum_duration_s / config.time_step <= 10_000_000.0
            && [
                self.recovery.main_altitude_m,
                self.recovery.main_cd_a,
                self.recovery.drogue_cd_a,
            ]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0);
        if !valid {
            return Err(ValidationError("Case needs a name, finite launch/wind/recovery values, angle below 90°, pad altitude −500…20000 m, step 0.0001…0.1 s, telemetry interval ≥ step, and duration ≤3600 s / 10 million steps".into()));
        }
        if let Some(motor) = &self.motor {
            validate_motor(motor)?;
        }
        Ok(())
    }

    /// Execute the same resolved case used by persistence and the UI.
    ///
    /// # Errors
    /// Rejects invalid inputs, absent motors, and vehicles with no dry mass.
    pub fn run(&self, rocket: &Rocket) -> Result<SimResult, ValidationError> {
        self.validate()?;
        validate_rocket(rocket)?;
        if rocket.root.upper_stage.is_some() {
            return Err(ValidationError("This model set supports a single rigid stage; upper-stage dynamics are not implemented".into()));
        }
        let motor = self
            .motor
            .as_ref()
            .ok_or_else(|| ValidationError("Select a motor before running this case".into()))?;
        let properties = rocket.mass_properties();
        if !properties.mass.as_kilograms().is_finite()
            || properties.mass.as_kilograms() <= 0.0
            || !properties.cg_x.as_meters().is_finite()
            || !properties.i_axial.is_finite()
            || !properties.i_transverse.is_finite()
        {
            return Err(ValidationError(
                "Vehicle needs finite positive dry mass and finite CG/inertia".into(),
            ));
        }
        match self.models {
            ModelSet::Builtin6DofRk4IsaV1 => {
                Ok(
                    Simulator6Dof::new(rocket.clone(), motor.clone(), self.config.clone())
                        .with_recovery(self.recovery.initial_state())
                        .run_for(Duration::seconds(self.maximum_duration_s)),
                )
            }
        }
    }
}

fn validate_motor(motor: &Motor) -> Result<(), ValidationError> {
    let positive = [
        motor.diameter.as_meters(),
        motor.length.as_meters(),
        motor.total_mass.as_kilograms(),
    ];
    let propellant = motor.propellant_mass.as_kilograms();
    if motor.designation.trim().is_empty()
        || positive.iter().any(|v| !v.is_finite() || *v <= 0.0)
        || !propellant.is_finite()
        || propellant < 0.0
        || propellant >= motor.total_mass.as_kilograms()
        || motor.delays.iter().any(|v| !v.is_finite() || *v < -1.0)
        || motor.thrust_curve.len() < 2
        || motor.thrust_curve.iter().any(|(t, f)| {
            !t.as_seconds().is_finite()
                || t.as_seconds() < 0.0
                || !f.as_newtons().is_finite()
                || f.as_newtons() < 0.0
        })
        || motor
            .thrust_curve
            .windows(2)
            .any(|p| p[0].0.as_seconds() >= p[1].0.as_seconds())
        || !motor.thrust_curve.iter().any(|(_, f)| f.as_newtons() > 0.0)
    {
        return Err(ValidationError("Motor needs positive dimensions/casing mass, valid propellant mass, and a finite, nonnegative, strictly time-ordered thrust curve".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_numerics_and_motor_curves_are_rejected_before_execution() {
        for step in [0.0, f64::NAN, f64::INFINITY, -0.01, 0.2] {
            let mut case = SimulationCase::default();
            case.config.time_step = step;
            assert!(case.validate().is_err());
        }
        let mut case = SimulationCase::default();
        case.config.telemetry_interval = 0.0001;
        assert!(case.validate().is_err());
        case = SimulationCase::default();
        case.recovery.main_cd_a = -1.0;
        assert!(case.validate().is_err());
        case = SimulationCase::default();
        case.maximum_duration_s = f64::INFINITY;
        assert!(case.validate().is_err());
        case = SimulationCase::default();
        case.config.wind.1 = f64::NAN;
        assert!(case.validate().is_err());
        let motor = rocket_motors::rasp::parse(include_str!(
            "../../../tests/fixtures/motors/estes_a8_3.eng"
        ))
        .expect("motor");
        case = SimulationCase {
            motor: Some(motor.clone()),
            ..SimulationCase::default()
        };
        case.validate().expect("valid motor");
        case.motor.as_mut().expect("motor").thrust_curve[1].0 = Duration::ZERO;
        assert!(case.validate().is_err());
        case.motor = Some(motor);
        case.motor.as_mut().expect("motor").propellant_mass = rocket_core::Mass::kilograms(99.0);
        assert!(case.validate().is_err());
    }
    #[test]
    fn each_case_run_starts_with_stowed_recovery() {
        let config = RecoveryConfig::default();
        let mut first = config.initial_state();
        first.main_deployed = true;
        let second = config.initial_state();
        assert!(!second.main_deployed && !second.drogue_deployed);
        assert!(first.main_deployed);
        assert_eq!(second.main_cd_a, config.main_cd_a);
    }
}
