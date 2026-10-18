//! # rocket-physics
//!
//! Atmosphere models (ISA, US-1976, NRLMSISE-00), rigid-body dynamics
//! (6-DOF with quaternions, Euler equations with variable inertia),
//! propulsion (thrust curves, mass flow, CG/inertia vs time), and
//! gravity (constant, g(h), J2 oblateness).
//!
//! Depends on: `rocket-core`.
//! Must NOT depend on: any UI or networking crate (ADR-0001).
//!
//! See:
//! - `docs/adr/0001-layered-rust-workspace-architecture.md`
//! - `docs/adr/0003-rk45-dopri5-integrator.md`
//! - `rocket-simulator-aero-dynamics.md` Part II (6-DOF dynamics spec)

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]
