//! Self-contained retained run records, shared by GUI and headless workflows.
use crate::{SimResult, SimulationCase};
use rocket_core::{
    validation::{validate_rocket, ValidationError},
    Rocket,
};
use serde::{Deserialize, Serialize};

/// Immutable inputs and outputs of a completed, interrupted, or failed propagation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub version: u32,
    pub id: String,
    pub label: String,
    pub created_unix_ms: u128,
    pub source_document_revision: u64,
    pub solver_version: String,
    pub source_revision: String,
    pub rocket: Rocket,
    pub case: SimulationCase,
    pub result: SimResult,
}
impl RunRecord {
    /// Capture resolved inputs and the actual propagation outcome.
    #[must_use]
    pub fn capture(rocket: Rocket, case: SimulationCase, result: SimResult, revision: u64) -> Self {
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Self {
            version: 1,
            id: format!("{}-{}", time.as_nanos(), std::process::id()),
            label: case.name.clone(),
            created_unix_ms: time.as_millis(),
            source_document_revision: revision,
            solver_version: format!("aether-6dof-rk4-isa-v2/{}", env!("CARGO_PKG_VERSION")),
            source_revision: env!("AETHER_SOURCE_REVISION").into(),
            rocket,
            case,
            result,
        }
    }
    /// Check persisted record identity, inputs, finite outputs and time ordering.
    /// # Errors
    /// Rejects invalid or structurally inconsistent records.
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_rocket(&self.rocket)?;
        self.case.validate()?;
        self.case.aerodynamics.validate_vehicle(&self.rocket)?;
        if self.version != 1
            || self.id.trim().is_empty()
            || self.label.trim().is_empty()
            || self.solver_version.trim().is_empty()
            || self.source_revision.trim().is_empty()
        {
            return Err(ValidationError(
                "Invalid run-record identity or schema".into(),
            ));
        }
        let r = &self.result;
        if [
            r.max_altitude.as_meters(),
            r.max_velocity.as_meters_per_second(),
            r.max_acceleration,
            r.apogee_time.as_seconds(),
            r.landing_time.as_seconds(),
            r.landing_position.0,
            r.landing_position.1,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || r.max_local_mach.is_some_and(|v| !v.is_finite() || v < 0.0)
            || r.max_dynamic_pressure_pa
                .is_some_and(|v| !v.is_finite() || v < 0.0)
            || r.apogee_time.as_seconds() < 0.0
            || r.apogee_time > r.landing_time
            || r.landing_time.as_seconds() < 0.0
            || r.landing_time.as_seconds()
                > self.case.maximum_duration_s + self.case.config.time_step
        {
            return Err(ValidationError(
                "Run summary contains invalid values".into(),
            ));
        }
        let mut previous = -1.0;
        for p in &r.telemetry {
            let values = [
                p.time.as_seconds(),
                p.position.0,
                p.position.1,
                p.position.2,
                p.velocity.0,
                p.velocity.1,
                p.velocity.2,
                p.mass.as_kilograms(),
                p.thrust.as_newtons(),
                p.drag.as_newtons(),
                p.acceleration,
            ];
            if values.iter().any(|v| !v.is_finite())
                || p.time.as_seconds() < previous
                || p.time.as_seconds() < 0.0
                || p.time.as_seconds() > r.landing_time.as_seconds() + 1e-9
                || p.mass.as_kilograms() <= 0.0
            {
                return Err(ValidationError(
                    "Run telemetry contains invalid values or sample ordering".into(),
                ));
            }
            if let Some(f) = p.flight {
                let mut values = vec![
                    f.airspeed_m_s,
                    f.mach,
                    f.dynamic_pressure_pa,
                    f.reynolds,
                    f.alpha_rad,
                    f.beta_rad,
                    f.density_kg_m3,
                    f.temperature_k,
                ];
                values.extend(f.quaternion_wxyz);
                values.extend(f.angular_rate_rad_s);
                values.extend(f.acceleration_m_s2);
                if values.iter().any(|v| !v.is_finite()) {
                    return Err(ValidationError("Non-finite 6-DOF telemetry".into()));
                }
            }
            previous = p.time.as_seconds();
        }
        for e in &r.events {
            if !e.time_s.is_finite()
                || e.time_s < 0.0
                || e.time_s > r.landing_time.as_seconds() + 1e-8
                || e.name.trim().is_empty()
            {
                return Err(ValidationError("Invalid run event".into()));
            }
        }
        Ok(())
    }
}
