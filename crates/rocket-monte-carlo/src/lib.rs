//! # rocket-monte-carlo
//!
//! Uncertainty quantification via Monte Carlo simulation:
//! - Distributions (Normal, `LogNormal`, Uniform, Weibull, von Mises, …).
//! - Samplers: Latin Hypercube (LHS), Sobol quasi-random, plain MC.
//! - Gaussian copula for correlated parameters.
//! - Parallel execution via `rayon` with bit-identical reproducibility.
//! - Statistical aggregation: percentiles, landing ellipses, trajectory bands.
//! - Sobol sensitivity indices (Saltelli sampling plan).
//!
//! See: `rocket-simulator-mc-optimization.md` Part I

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]
