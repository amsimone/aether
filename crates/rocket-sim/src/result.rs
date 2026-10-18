//! Simulation result and telemetry types.

use rocket_core::{Duration, Force, Length, Mass, Velocity};
use serde::{Deserialize, Serialize};

/// Additional 6-DOF telemetry, with SI units and body-to-launch quaternion order w,x,y,z.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightSample {
    pub airspeed_m_s: f64,
    pub mach: f64,
    pub dynamic_pressure_pa: f64,
    pub reynolds: f64,
    pub alpha_rad: f64,
    pub beta_rad: f64,
    pub quaternion_wxyz: [f64; 4],
    pub angular_rate_rad_s: [f64; 3],
    pub acceleration_m_s2: [f64; 3],
    pub density_kg_m3: f64,
    pub temperature_k: f64,
    pub drogue_deployed: bool,
    pub main_deployed: bool,
    pub aero_source: rocket_aero::AeroSource,
}

/// A run's actual outcome, independent of its last recorded position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum Termination {
    GroundImpact,
    DurationLimit,
    Cancelled,
    NumericalFailure(String),
    ModelDomainLimit(String),
    InvalidInput,
}

/// Timestamped event; crossing times currently have fixed-step resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightEvent {
    pub time_s: f64,
    pub name: String,
}

/// A single telemetry sample from the flight simulation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TelemetryPoint {
    /// Time since ignition.
    pub time: Duration,
    /// Position `(x, y, z)` in meters. z is altitude above sea level.
    pub position: (f64, f64, f64),
    /// Velocity `(vx, vy, vz)` in m/s.
    pub velocity: (f64, f64, f64),
    /// Total mass at this time (dry rocket + motor).
    pub mass: Mass,
    /// Thrust force at this time.
    pub thrust: Force,
    /// Drag force magnitude at this time.
    pub drag: Force,
    /// Acceleration magnitude at this time (m/s²).
    pub acceleration: f64,
    pub flight: Option<FlightSample>,
}

/// The result of a flight simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimResult {
    /// Local-air maxima evaluated at integration states (not just exported sample times).
    pub max_local_mach: Option<f64>,
    pub max_dynamic_pressure_pa: Option<f64>,
    pub termination: Termination,
    pub events: Vec<FlightEvent>,
    /// Maximum altitude above the pad (apogee height).
    pub max_altitude: Length,
    /// Maximum speed reached during the flight.
    pub max_velocity: Velocity,
    /// Maximum acceleration magnitude reached during the flight.
    pub max_acceleration: f64,
    /// Time of apogee (when vertical velocity crosses zero).
    pub apogee_time: Duration,
    /// Time of ground impact (landing).
    pub landing_time: Duration,
    /// Landing position `(x, y)` in meters (downrange, cross-range).
    pub landing_position: (f64, f64),
    /// Full telemetry time series.
    pub telemetry: Vec<TelemetryPoint>,
}

impl SimResult {
    /// An empty result (for degenerate configs).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            max_local_mach: None,
            max_dynamic_pressure_pa: None,
            termination: Termination::InvalidInput,
            events: Vec::new(),
            max_altitude: Length::ZERO,
            max_velocity: Velocity::ZERO,
            max_acceleration: 0.0,
            apogee_time: Duration::ZERO,
            landing_time: Duration::ZERO,
            landing_position: (0.0, 0.0),
            telemetry: Vec::new(),
        }
    }

    /// Apogee altitude above the pad (alias for `max_altitude`).
    #[must_use]
    pub fn apogee(&self) -> Length {
        self.max_altitude
    }

    /// Maximum Mach number reached during the flight.
    ///
    /// Uses the speed of sound at sea level (340.3 m/s) as a rough
    /// approximation. Phase 3 will compute the local Mach at each
    /// telemetry point using the atmosphere model.
    #[must_use]
    pub fn max_mach(&self) -> f64 {
        const A_SEA_LEVEL: f64 = 340.3;
        if let Some(value) = self.max_local_mach {
            return value;
        }
        if self.telemetry.iter().any(|p| p.flight.is_some()) {
            return self
                .telemetry
                .iter()
                .filter_map(|p| p.flight.map(|f| f.mach))
                .fold(0.0, f64::max);
        }
        self.max_velocity.as_meters_per_second() / A_SEA_LEVEL
    }

    /// Burnout time (time when thrust drops to zero).
    ///
    /// This is the time of the last telemetry point with nonzero thrust.
    #[must_use]
    pub fn burnout_time(&self) -> Option<Duration> {
        self.telemetry
            .iter()
            .rev()
            .find(|p| p.thrust.as_newtons() > 0.0)
            .map(|p| p.time)
    }

    /// Get the telemetry point closest to the given time.
    #[must_use]
    pub fn telemetry_at(&self, time: Duration) -> Option<&TelemetryPoint> {
        let target = time.as_seconds();
        self.telemetry.iter().min_by(|a, b| {
            let da = (a.time.as_seconds() - target).abs();
            let db = (b.time.as_seconds() - target).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn empty_result_has_zero_values() {
        let r = SimResult::empty();
        assert_eq!(r.max_altitude, Length::ZERO);
        assert_eq!(r.max_velocity, Velocity::ZERO);
        assert_eq!(r.apogee_time, Duration::ZERO);
        assert!(r.telemetry.is_empty(), "telemetry should be empty");
    }

    #[test]
    fn max_mach_uses_sea_level_speed_of_sound() {
        let r = SimResult {
            max_local_mach: None,
            max_dynamic_pressure_pa: None,
            termination: Termination::DurationLimit,
            events: Vec::new(),
            max_altitude: Length::ZERO,
            max_velocity: Velocity::meters_per_second(340.3),
            max_acceleration: 0.0,
            apogee_time: Duration::ZERO,
            landing_time: Duration::ZERO,
            landing_position: (0.0, 0.0),
            telemetry: vec![],
        };
        assert!((r.max_mach() - 1.0).abs() < 1e-9);
    }
}
