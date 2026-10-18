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

pub mod components;
pub mod id;
pub mod materials;
pub mod rocket;
pub mod stage;
pub mod units;

pub use components::{
    Airfoil, BodyTube, Component, Coupler, FinPlanform, FinSet, LaunchLug, MassObject,
    MassObjectKind, NoseCone, NoseShape, Pod, Transition,
};
pub use id::ComponentId;
pub use materials::{Material, MaterialId, MaterialLibrary};
pub use rocket::{MassProperties, Rocket};
pub use stage::{
    Event, EventAction, EventTrigger, MotorMount, RecoveryDevice, RecoveryKind, Stage,
};
pub use units::{Density, Duration, Force, Length, Mass, Pressure, Temperature, Velocity};
