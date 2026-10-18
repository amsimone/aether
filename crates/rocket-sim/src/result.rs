//! Simulation result and telemetry types.

use rocket_core::{Duration, Force, Length, Mass, Velocity};

/// A single telemetry sample from the flight simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
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
}

/// The result of a flight simulation.
#[derive(Debug, Clone)]
pub struct SimResult {
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
