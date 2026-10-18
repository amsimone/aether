//! # rocket-render
//!
//! 3D rendering with `wgpu` / `three-d`:
//! - Procedural meshes from the component tree (nose cones, body tubes,
//!   fins, transitions).
//! - Orbit camera (design mode), follow camera (playback), free-fly.
//! - Ray-cast picking for component selection in the 3D viewport.
//! - Flight playback FX: particle trails, Mach coloring.
//!
//! See: `rocket-simulator-plan.md` §12

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]
