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

pub mod rasp;

use rocket_core::{Duration, Force, Length, Mass};

/// A parsed rocket motor with its thrust curve.
///
/// Populated by the RASP/`.eng` parser ([`rasp::parse`]) and (in Task 4)
/// the `ThrustCurve` API client.
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
    /// Ejection charge delays (seconds).
    /// `-1.0` means plugged (no ejection charge).
    /// `0.0` means no delay (ejection charge fires immediately).
    #[serde(default)]
    pub delays: Vec<f32>,
    /// Propellant mass.
    pub propellant_mass: Mass,
    /// Total motor mass (loaded).
    pub total_mass: Mass,
    /// Thrust curve as `(time, thrust)` pairs.
    pub thrust_curve: Vec<(Duration, Force)>,
}

pub use rasp::ParseError;
