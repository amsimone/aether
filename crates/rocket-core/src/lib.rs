//! # rocket-core
//!
//! Core data model for the Aether rocket simulator: dimensional units,
//! the rocket design tree, materials, and the component type system.
//!
//! This crate has **no dependencies on physics or UI code** (ADR-0001).
//! It is the foundation that every other crate builds on.
//!
//! See: `docs/adr/0001-layered-rust-workspace-architecture.md`

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]

pub mod rocket;
pub mod units;

pub use rocket::Rocket;
pub use units::{Duration, Force, Length, Mass, Pressure, Velocity};
