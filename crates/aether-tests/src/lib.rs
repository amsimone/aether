//! # Aether validation tests
//!
//! Test stubs for the validation cases defined in
//! `tests/validation/README.md`. Most are `#[ignore]` until the physics
//! code they test is implemented; they form the to-do list for Phases 2–4.
//!
//! The motor fixture tests (not ignored) run now and verify the RASP
//! parser against real `.eng` files.

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]

mod fixtures;

mod aero;
mod dynamics;
mod motors;
