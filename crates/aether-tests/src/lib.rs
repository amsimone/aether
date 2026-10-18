//! # Aether validation tests
//!
//! Test stubs for the validation cases defined in
//! `tests/validation/README.md`. Most are `#[ignore]` until the physics
//! code they test is implemented; they form the to-do list for Phases 2–4.
//!
//! The motor fixture tests, the Estes Alpha integration test, and the
//! Phase 2 flight simulation tests (not ignored) run now and verify the
//! data model, mass properties, aero estimate, and flight simulator work
//! correctly together on real rocket data.

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]

mod estes_alpha;
mod fixtures;
mod flight_sim;

mod aero;
mod dynamics;
mod motors;
