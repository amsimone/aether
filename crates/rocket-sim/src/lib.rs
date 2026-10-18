//! # rocket-sim
//!
//! The flight loop: Dormand-Prince RK45 integration with adaptive step
//! size and event detection (apogee, ground, deployment, staging),
//! telemetry streaming, and the tier-suggestion function.
//!
//! Depends on: `rocket-core`, `rocket-physics`, `rocket-aero`.
//!
//! See:
//! - `docs/adr/0003-rk45-dopri5-integrator.md`
//! - `rocket-simulator-aero-dynamics.md` Part II §16 (integrator spec)

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]

// Re-export the core types so consumers of rocket-sim don't need to
// depend on rocket-core directly for common types.
pub use rocket_core::Rocket;
