//! # rocket-aero
//!
//! Aerodynamic coefficient prediction: the component buildup (extended
//! Barrowman for subsonic, Prandtl-Glauert / transonic drag rise /
//! supersonic wave drag for compressible), skin friction, base drag,
//! dynamic derivatives, and the coefficient cache.
//!
//! Kept as a separate crate from `rocket-physics` because the aero code
//! is the highest-risk, most-tested code in the project and deserves its
//! own test boundary (ADR-0001).
//!
//! Must NOT depend on: any UI or networking crate (ADR-0001).
//!
//! See: `rocket-simulator-aero-dynamics.md` Part I (aerodynamics spec)

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]

pub mod buildup;
pub mod design;
pub mod drag;

pub use buildup::{compute, AeroCoefficients, AeroQuery};
pub use design::{estimate, static_margin, DesignAero};
pub use drag::{estimate_cd, estimate_cd_breakdown, DragBreakdown};
