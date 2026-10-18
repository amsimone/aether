//! Static aerodynamic data contract. SI units, explicit signs and moment reference.
// Grid knots require exact equality; interpolated queries use bounded weights.
#![allow(clippy::too_many_lines, clippy::float_cmp)]

use crate::{compute, AeroCoefficients, AeroQuery};
use rocket_core::{validation::ValidationError, Rocket};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Stable fingerprint of the vehicle input, independent of map insertion order.
/// The display name is excluded; geometry/material/mount changes invalidate the binding.
/// # Panics
/// Panics if the core vehicle type cannot be serialized as JSON.
#[must_use]
pub fn vehicle_fingerprint(rocket: &Rocket) -> String {
    let mut vehicle = rocket.clone();
    vehicle.name.clear();
    let mut value = serde_json::to_value(vehicle).expect("finite, validated vehicle");
    value.sort_all_objects();
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value).expect("JSON value"))
    )
}

/// Provenance and reference quantities for a static body-axis coefficient grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetMetadata {
    pub name: String,
    pub source: String,
    pub vehicle_sha256: String,
    /// Body axes: +x toward nose, +y right, +z completes the right-handed frame.
    /// Forces are qA*(-CA, CY, CN); moments qAL*(Cl, Cm, Cn).
    pub axes: String,
    pub reference_area_m2: f64,
    pub reference_length_m: f64,
    /// Axial station of the moment origin, measured aft from nose tip (m).
    pub moment_reference_station_m: f64,
    /// Reynolds validity interval based on `reference_length_m` (inclusive).
    pub reynolds_min: f64,
    pub reynolds_max: f64,
    /// Evidence/limitations, such as mesh study, turbulence model and wall treatment.
    pub evidence: String,
}
/// One static condition, with angles in radians and moment coefficients at the declared origin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoefficientRow {
    pub mach: f64,
    pub alpha_rad: f64,
    pub beta_rad: f64,
    pub ca: f64,
    pub cy: f64,
    pub cn: f64,
    pub cl: f64,
    pub cm: f64,
    pub cn_yaw: f64,
}
/// Complete rectangular Mach × alpha × beta grid. Linear interpolation; no implicit extrapolation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AeroDataset {
    pub version: u32,
    pub metadata: DatasetMetadata,
    pub rows: Vec<CoefficientRow>,
}

/// Explicit behavior outside tabulated conditions; fallback is recorded in telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DomainPolicy {
    #[default]
    Stop,
    BuiltinFallback,
}
/// Resolved aerodynamic provider; imported data is embedded in the flight case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AeroModel {
    #[default]
    Builtin,
    Dataset {
        data: Arc<AeroDataset>,
        outside_domain: DomainPolicy,
    },
}
/// Actual coefficient source at a sampled state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AeroSource {
    Builtin,
    Dataset,
    BuiltinOutsideDataset,
    ZeroAirspeed,
}

impl AeroDataset {
    /// Check reference quantities, all coefficients, uniqueness, and grid completeness.
    /// # Errors
    /// Rejects malformed, ambiguous or incomplete datasets.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let m = &self.metadata;
        if self.version != 1
            || m.name.trim().is_empty()
            || m.source.trim().is_empty()
            || m.axes != "body_x_nose_y_right_z_rh"
            || m.vehicle_sha256.len() != 64
            || !m.vehicle_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || !m.reference_area_m2.is_finite()
            || m.reference_area_m2 <= 0.0
            || !m.reference_length_m.is_finite()
            || m.reference_length_m <= 0.0
            || !m.moment_reference_station_m.is_finite()
            || m.moment_reference_station_m < 0.0
            || !m.reynolds_min.is_finite()
            || !m.reynolds_max.is_finite()
            || m.reynolds_min < 0.0
            || m.reynolds_max < m.reynolds_min
            || m.evidence.trim().is_empty()
            || self.rows.is_empty()
            || self.rows.len() > 100_000
        {
            return Err(ValidationError("Dataset needs schema 1, provenance/evidence, a vehicle fingerprint, body_x_nose_y_right_z_rh axes, positive reference area/length, a finite moment station, and Reynolds bounds".into()));
        }
        for row in &self.rows {
            if [
                row.mach,
                row.alpha_rad,
                row.beta_rad,
                row.ca,
                row.cy,
                row.cn,
                row.cl,
                row.cm,
                row.cn_yaw,
            ]
            .iter()
            .any(|v| !v.is_finite())
                || row.mach < 0.0
                || row.ca < 0.0
                || row.alpha_rad.abs() > std::f64::consts::PI
                || row.beta_rad.abs() > std::f64::consts::FRAC_PI_2
            {
                return Err(ValidationError("Dataset rows need finite coefficients, nonnegative Mach/CA, and angles in radians".into()));
            }
        }
        let (mach, alpha, beta) = self.axes();
        if mach
            .len()
            .checked_mul(alpha.len())
            .and_then(|n| n.checked_mul(beta.len()))
            != Some(self.rows.len())
        {
            return Err(ValidationError(
                "Dataset must cover a complete rectangular Mach/alpha/beta grid".into(),
            ));
        }
        let mut keys = std::collections::HashSet::new();
        for row in &self.rows {
            let key = [
                mach.iter().position(|v| *v == row.mach),
                alpha.iter().position(|v| *v == row.alpha_rad),
                beta.iter().position(|v| *v == row.beta_rad),
            ];
            if !keys.insert(key) {
                return Err(ValidationError(
                    "Duplicate aerodynamic grid condition".into(),
                ));
            }
        }
        Ok(())
    }
    fn axes(&self) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        fn axis(values: impl Iterator<Item = f64>) -> Vec<f64> {
            let mut v: Vec<_> = values.collect();
            v.sort_by(f64::total_cmp);
            v.dedup();
            v
        }
        (
            axis(self.rows.iter().map(|r| r.mach)),
            axis(self.rows.iter().map(|r| r.alpha_rad)),
            axis(self.rows.iter().map(|r| r.beta_rad)),
        )
    }
    /// Interpolate static coefficients and translate moments to the current loaded CG.
    /// # Errors
    /// Rejects out-of-domain conditions. Call validate once before repeated queries.
    pub fn evaluate(
        &self,
        query: AeroQuery,
        cg_station_m: f64,
    ) -> Result<AeroCoefficients, ValidationError> {
        if !query.reynolds.is_finite()
            || query.reynolds < self.metadata.reynolds_min
            || query.reynolds > self.metadata.reynolds_max
        {
            return Err(ValidationError(format!(
                "Reynolds {} is outside dataset [{}, {}]",
                query.reynolds, self.metadata.reynolds_min, self.metadata.reynolds_max
            )));
        }
        let (mach_axis, alpha_axis, beta_axis) = self.axes();
        let mut coefficients = [0.0; 6];
        for (mach, wm) in bracket(&mach_axis, query.mach, "Mach")? {
            for (alpha, wa) in bracket(&alpha_axis, query.alpha, "Alpha")? {
                for (beta, wb) in bracket(&beta_axis, query.beta, "Beta")? {
                    let row = self
                        .rows
                        .iter()
                        .find(|r| r.mach == mach && r.alpha_rad == alpha && r.beta_rad == beta)
                        .ok_or_else(|| ValidationError("Incomplete coefficient grid".into()))?;
                    for (out, value) in coefficients
                        .iter_mut()
                        .zip([row.ca, row.cy, row.cn, row.cl, row.cm, row.cn_yaw])
                    {
                        *out += wm * wa * wb * value;
                    }
                }
            }
        }
        let arm = (cg_station_m - self.metadata.moment_reference_station_m)
            / self.metadata.reference_length_m;
        Ok(AeroCoefficients {
            c_a: coefficients[0],
            c_y: coefficients[1],
            c_n: coefficients[2],
            c_l: coefficients[3],
            c_m: coefficients[4] - arm * coefficients[2],
            c_n_yaw: coefficients[5] + arm * coefficients[1],
            cp_x: 0.0,
            reference_area: self.metadata.reference_area_m2,
            reference_length: self.metadata.reference_length_m,
            mach: query.mach,
            alpha: query.alpha,
            beta: query.beta,
        })
    }
}
impl AeroModel {
    /// Validate once when accepting a case or loading a file.
    /// # Errors
    /// Rejects malformed datasets.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if let Self::Dataset { data, .. } = self {
            data.validate()?;
        }
        Ok(())
    }
    /// Reject using a dataset on a different vehicle revision.
    /// # Errors
    /// Reports a stale vehicle binding.
    pub fn validate_vehicle(&self, rocket: &Rocket) -> Result<(), ValidationError> {
        if let Self::Dataset { data, .. } = self {
            if data.metadata.vehicle_sha256 != vehicle_fingerprint(rocket) {
                return Err(ValidationError("Aerodynamic dataset belongs to different vehicle inputs; import matching data or select the built-in model".into()));
            }
        }
        Ok(())
    }
    /// Evaluate the selected model; explicit fallback never hides its source.
    /// # Errors
    /// Reports out-of-domain queries under the Stop policy.
    pub fn evaluate(
        &self,
        rocket: &Rocket,
        query: AeroQuery,
        cg: f64,
    ) -> Result<(AeroCoefficients, AeroSource), ValidationError> {
        match self {
            Self::Builtin => Ok((compute(rocket, query), AeroSource::Builtin)),
            Self::Dataset {
                data,
                outside_domain,
            } => match data.evaluate(query, cg) {
                Ok(coeff) => Ok((coeff, AeroSource::Dataset)),
                Err(_) if *outside_domain == DomainPolicy::BuiltinFallback => {
                    Ok((compute(rocket, query), AeroSource::BuiltinOutsideDataset))
                }
                Err(error) => Err(error),
            },
        }
    }
}

fn bracket(axis: &[f64], value: f64, name: &str) -> Result<Vec<(f64, f64)>, ValidationError> {
    let (&lo, &hi) = axis
        .first()
        .zip(axis.last())
        .ok_or_else(|| ValidationError("Empty dataset".into()))?;
    let eps = 1e-10 * (1.0 + value.abs());
    if !value.is_finite() || value < lo - eps || value > hi + eps {
        return Err(ValidationError(format!(
            "{name} {value} outside dataset [{lo}, {hi}]"
        )));
    }
    let value = value.clamp(lo, hi);
    if let Some(&exact) = axis.iter().find(|v| (**v - value).abs() <= eps) {
        return Ok(vec![(exact, 1.0)]);
    }
    let upper = axis.partition_point(|v| *v < value);
    let (a, b) = (axis[upper - 1], axis[upper]);
    let w = (value - a) / (b - a);
    Ok(vec![(a, 1.0 - w), (b, w)])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dataset() -> AeroDataset {
        let rocket = Rocket::new("Fixture");
        let mut rows = Vec::new();
        for mach in [0.1, 0.3] {
            for alpha in [-0.1, 0.1] {
                for beta in [-0.05, 0.05] {
                    rows.push(CoefficientRow {
                        mach,
                        alpha_rad: alpha,
                        beta_rad: beta,
                        ca: 1.0 + mach + alpha + beta,
                        cy: -2.0 * beta,
                        cn: 3.0 * alpha,
                        cl: 0.1 * beta,
                        cm: -0.1 * alpha + 2.0 * beta,
                        cn_yaw: 0.2 * beta,
                    });
                }
            }
        }
        AeroDataset {
            version: 1,
            metadata: DatasetMetadata {
                name: "Analytic grid fixture".into(),
                source: "Synthetic affine function, not CFD".into(),
                vehicle_sha256: vehicle_fingerprint(&rocket),
                axes: "body_x_nose_y_right_z_rh".into(),
                reference_area_m2: 0.01,
                reference_length_m: 0.025,
                moment_reference_station_m: 0.1,
                reynolds_min: 1000.0,
                reynolds_max: 1e7,
                evidence:
                    "Interpolation/moment-translation verification only; no physical validation"
                        .into(),
            },
            rows,
        }
    }
    #[test]
    fn interpolation_and_moment_translation_match_an_affine_field_and_cross_product() {
        let data = dataset();
        data.validate().expect("valid grid");
        let query = AeroQuery {
            mach: 0.2,
            alpha: 0.05,
            beta: -0.02,
            reynolds: 1e6,
            roll_rate: 0.0,
            pitch_rate: 0.0,
        };
        let c = data.evaluate(query, 0.2).expect("in domain");
        assert!((c.c_a - 1.23).abs() < 1e-12);
        assert!((c.c_n - 0.15).abs() < 1e-12);
        assert!((c.c_y - 0.04).abs() < 1e-12);
        // r_ref-cg=(+0.1,0,0) in nose-positive body axes; r cross F shifts My/Mz.
        assert!((c.c_m - (-0.045 - 4.0 * 0.15)).abs() < 1e-12);
        assert!((c.c_n_yaw - (-0.004 + 4.0 * 0.04)).abs() < 1e-12);
    }
    #[test]
    fn ambiguity_invalid_rows_and_missing_grid_points_are_rejected() {
        let original = dataset();
        let mut invalid = original.clone();
        invalid.rows.pop();
        assert!(invalid.validate().is_err());
        let mut invalid = original.clone();
        invalid.rows[0] = invalid.rows[1].clone();
        assert!(invalid.validate().is_err());
        let mut invalid = original.clone();
        invalid.metadata.axes = "wind_axes".into();
        assert!(invalid.validate().is_err());
        let mut invalid = original.clone();
        invalid.metadata.reference_area_m2 = 0.0;
        assert!(invalid.validate().is_err());
        let mut invalid = original;
        invalid.rows[0].ca = f64::NAN;
        assert!(invalid.validate().is_err());
    }
    #[test]
    fn out_of_domain_behavior_is_explicit_and_vehicle_binding_is_enforced() {
        let data = Arc::new(dataset());
        let rocket = Rocket::new("Fixture");
        let mut query = AeroQuery::at_mach(0.9);
        query.reynolds = 1e6;
        let strict = AeroModel::Dataset {
            data: data.clone(),
            outside_domain: DomainPolicy::Stop,
        };
        assert!(strict.evaluate(&rocket, query, 0.0).is_err());
        let fallback = AeroModel::Dataset {
            data,
            outside_domain: DomainPolicy::BuiltinFallback,
        };
        assert_eq!(
            fallback
                .evaluate(&rocket, query, 0.0)
                .expect("explicit fallback")
                .1,
            AeroSource::BuiltinOutsideDataset
        );
        strict.validate_vehicle(&rocket).expect("same inputs");
        let mut changed = rocket.clone();
        changed.root.motor_mount = Some(rocket_core::MotorMount {
            tube: rocket_core::ComponentId::new(1),
            inner_diameter: rocket_core::Length::millimeters(18.0),
        });
        assert!(strict.validate_vehicle(&changed).is_err());
        changed = rocket;
        changed.name = "Display name changed".into();
        strict.validate_vehicle(&changed).expect("name excluded");
    }
}
