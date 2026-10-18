//! # rocket-sim
//!
//! The flight loop: fixed-step RK4 integration with event detection
//! (apogee, ground impact), telemetry streaming, and the sim config.
//!
//! Phase 2 implements a 3-DOF translational (point-mass) model:
//! - Thrust along the launch direction during boost.
//! - Gravity (constant g).
//! - Drag (constant Cd) opposing velocity relative to wind.
//! - ISA atmosphere for density.
//!
//! Phase 3 will add 6-DOF attitude dynamics, variable Cd with Mach, and
//! recovery events.
//!
//! Depends on: `rocket-core`, `rocket-physics`, `rocket-aero`,
//! `rocket-motors`.
//!
//! See:
//! - `docs/adr/0003-rk45-dopri5-integrator.md`
//! - `rocket-simulator-aero-dynamics.md` Part II §16 (integrator spec)

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]

mod dopri5;
mod integrator;
mod result;
mod state;
mod state6dof;

pub use dopri5::{step as dopri5_step, Dopri5Config, Dopri5Step};
pub use result::{SimResult, TelemetryPoint};
pub use state::{SimConfig, SimState, Simulator, DEFAULT_TIME_STEP};
pub use state6dof::{RecoveryState, SimState6Dof, Simulator6Dof};

// Re-export the core types so consumers of rocket-sim don't need to
// depend on rocket-core directly for common types.
pub use rocket_core::Rocket;
