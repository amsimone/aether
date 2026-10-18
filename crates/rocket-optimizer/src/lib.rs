//! # rocket-optimizer
//!
//! Design optimization:
//! - CMA-ES (Covariance Matrix Adaptation Evolution Strategy).
//! - IPOP restart strategy for multimodal problems.
//! - Augmented Lagrangian constraint handling.
//! - Mixed-integer variables via filter-then-enumerate.
//! - Robust optimization (mini-MC per evaluation).
//! - Surrogate-assisted search (v1.x, GP + EI acquisition).
//!
//! See: `rocket-simulator-mc-optimization.md` Part II

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]
