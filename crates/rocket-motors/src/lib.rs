//! # rocket-motors
//!
//! Motor thrust-curve management:
//! - RASP `.eng` and `RockSim` `.rse` parsers (Phase-0 Task 3).
//! - `ThrustCurve.org` API client with on-disk cache (Phase-0 Task 4,
//!   ADR-0006).
//! - Thrust-curve interpolation (PCHIP, area-preserving).
//! - Motor catalog indexing and search.
//!
//! Motor data is fetched on demand from the `ThrustCurve.org` API and
//! cached locally. No motor files are bundled with the application
//! (ADR-0006).
//!
//! See:
//! - `docs/adr/0006-thrustcurve-api-fetch-on-demand.md`
//! - `rocket-simulator-plan.md` §9 (motor library)

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]

use rocket_core::{Duration, Force, Length, Mass};

/// A parsed rocket motor with its thrust curve.
///
/// This struct is populated by the RASP/`.eng` parser (Phase-0 Task 3)
/// and the `ThrustCurve` API client (Phase-0 Task 4).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Motor {
    /// Motor designation (e.g., "A8-3", "H128W").
    pub designation: String,
    /// Manufacturer name.
    pub manufacturer: String,
    /// Comment lines from the file header.
    #[serde(default)]
    pub comment: Vec<String>,
    /// Motor casing diameter.
    pub diameter: Length,
    /// Motor casing length.
    pub length: Length,
    /// Ejection charge delays (seconds). `-1` means plugged / no delay.
    #[serde(default)]
    pub delays: Vec<f32>,
    /// Propellant mass.
    pub propellant_mass: Mass,
    /// Total motor mass (loaded).
    pub total_mass: Mass,
    /// Thrust curve as `(time, thrust)` pairs.
    pub thrust_curve: Vec<(Duration, Force)>,
}
