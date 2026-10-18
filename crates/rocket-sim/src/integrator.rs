//! Fixed-step RK4 (4th-order Runge-Kutta) integrator.
//!
//! For Phase 2 we use a fixed step of 1 ms. This is simpler than the
//! adaptive Dormand-Prince RK45 planned for ADR-0003, and accurate
//! enough for the 5% apogee gate. Phase 3 will switch to adaptive
//! step size with event localization.

use crate::state::SimState;
use rocket_core::Duration;

/// One RK4 step for the 3-DOF translational dynamics.
///
/// The derivative function `f` returns `((dx, dy, dz), (dvx, dvy, dvz), dmass)`
/// — i.e. the time derivatives of position, velocity, and mass.
///
/// Mass is handled specially: after the RK4 step, the caller should
/// recompute mass from the motor's mass-at-time function (which uses
/// the impulse-proportional model). The `dmass` returned by `f` is
/// ignored by this function; it exists only to satisfy the trait.
pub fn rk4_step<F>(state: &SimState, dt: f64, f: F) -> SimState
where
    F: Fn(&SimState) -> ((f64, f64, f64), (f64, f64, f64), f64),
{
    // k1 = f(s)
    let (k1_pos, k1_vel, _k1_mass) = f(state);

    // k2 = f(s + dt/2 * k1)
    let s2 = SimState {
        time: Duration::seconds(state.time.as_seconds() + dt / 2.0),
        position: (
            state.position.0 + dt / 2.0 * k1_pos.0,
            state.position.1 + dt / 2.0 * k1_pos.1,
            state.position.2 + dt / 2.0 * k1_pos.2,
        ),
        velocity: (
            state.velocity.0 + dt / 2.0 * k1_vel.0,
            state.velocity.1 + dt / 2.0 * k1_vel.1,
            state.velocity.2 + dt / 2.0 * k1_vel.2,
        ),
        mass: state.mass,
    };
    let (k2_pos, k2_vel, _) = f(&s2);

    // k3 = f(s + dt/2 * k2)
    let s3 = SimState {
        time: Duration::seconds(state.time.as_seconds() + dt / 2.0),
        position: (
            state.position.0 + dt / 2.0 * k2_pos.0,
            state.position.1 + dt / 2.0 * k2_pos.1,
            state.position.2 + dt / 2.0 * k2_pos.2,
        ),
        velocity: (
            state.velocity.0 + dt / 2.0 * k2_vel.0,
            state.velocity.1 + dt / 2.0 * k2_vel.1,
            state.velocity.2 + dt / 2.0 * k2_vel.2,
        ),
        mass: state.mass,
    };
    let (k3_pos, k3_vel, _) = f(&s3);

    // k4 = f(s + dt * k3)
    let s4 = SimState {
        time: Duration::seconds(state.time.as_seconds() + dt),
        position: (
            state.position.0 + dt * k3_pos.0,
            state.position.1 + dt * k3_pos.1,
            state.position.2 + dt * k3_pos.2,
        ),
        velocity: (
            state.velocity.0 + dt * k3_vel.0,
            state.velocity.1 + dt * k3_vel.1,
            state.velocity.2 + dt * k3_vel.2,
        ),
        mass: state.mass,
    };
    let (k4_pos, k4_vel, _) = f(&s4);

    // Combine: s_{n+1} = s_n + dt/6 * (k1 + 2k2 + 2k3 + k4)
    let new_pos = (
        state.position.0 + dt / 6.0 * (k1_pos.0 + 2.0 * k2_pos.0 + 2.0 * k3_pos.0 + k4_pos.0),
        state.position.1 + dt / 6.0 * (k1_pos.1 + 2.0 * k2_pos.1 + 2.0 * k3_pos.1 + k4_pos.1),
        state.position.2 + dt / 6.0 * (k1_pos.2 + 2.0 * k2_pos.2 + 2.0 * k3_pos.2 + k4_pos.2),
    );
    let new_vel = (
        state.velocity.0 + dt / 6.0 * (k1_vel.0 + 2.0 * k2_vel.0 + 2.0 * k3_vel.0 + k4_vel.0),
        state.velocity.1 + dt / 6.0 * (k1_vel.1 + 2.0 * k2_vel.1 + 2.0 * k3_vel.1 + k4_vel.1),
        state.velocity.2 + dt / 6.0 * (k1_vel.2 + 2.0 * k2_vel.2 + 2.0 * k3_vel.2 + k4_vel.2),
    );

    SimState {
        time: Duration::seconds(state.time.as_seconds() + dt),
        position: new_pos,
        velocity: new_vel,
        mass: state.mass, // Mass is recomputed by the caller via update_mass.
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp, clippy::cast_precision_loss)]

    use super::*;
    use rocket_core::{Duration, Mass};

    /// A simple harmonic oscillator derivative for testing the integrator.
    /// y'' = -y, which has solution y = A*sin(t + φ).
    fn sho_deriv(s: &SimState) -> ((f64, f64, f64), (f64, f64, f64), f64) {
        // position = (y, 0, 0), velocity = (v, 0, 0)
        // dy/dt = v, dv/dt = -y
        ((s.velocity.0, 0.0, 0.0), (-s.position.0, 0.0, 0.0), 0.0)
    }

    #[test]
    fn rk4_solves_harmonic_oscillator() {
        // y(0) = 0, v(0) = 1 → y(t) = sin(t)
        let mut state = SimState {
            time: Duration::ZERO,
            position: (0.0, 0.0, 0.0),
            velocity: (1.0, 0.0, 0.0),
            mass: Mass::kilograms(1.0),
        };
        let dt = 0.001;
        for _ in 0..1000 {
            state = rk4_step(&state, dt, sho_deriv);
        }
        // After 1 second, y should be sin(1) ≈ 0.8415.
        let y = state.position.0;
        assert!(
            (y - 1.0_f64.sin()).abs() < 1e-5,
            "RK4 SHO: y(1) = {y}, expected {}",
            1.0_f64.sin()
        );
    }

    #[test]
    fn rk4_conserves_energy_in_vacuum() {
        // Free fall in vacuum: y(t) = y0 + v0*t - 0.5*g*t²
        // Energy = 0.5*v² + g*y should be constant.
        const G: f64 = 9.80665;
        let deriv = |s: &SimState| -> ((f64, f64, f64), (f64, f64, f64), f64) {
            ((0.0, 0.0, s.velocity.2), (0.0, 0.0, -G), 0.0)
        };

        let mut state = SimState {
            time: Duration::ZERO,
            position: (0.0, 0.0, 100.0),
            velocity: (0.0, 0.0, 0.0),
            mass: Mass::kilograms(1.0),
        };
        let e0 = 0.5 * state.velocity.2 * state.velocity.2 + G * state.position.2;

        let dt = 0.001;
        for _ in 0..1000 {
            state = rk4_step(&state, dt, deriv);
        }
        let e1 = 0.5 * state.velocity.2 * state.velocity.2 + G * state.position.2;
        assert!(
            (e1 - e0).abs() / e0.abs() < 1e-4,
            "energy should be conserved: E0={e0}, E1={e1}"
        );
    }

    #[test]
    fn rk4_free_fall_matches_closed_form() {
        // z(t) = z0 - 0.5*g*t², v(t) = -g*t
        const G: f64 = 9.80665;
        let deriv = |s: &SimState| -> ((f64, f64, f64), (f64, f64, f64), f64) {
            ((0.0, 0.0, s.velocity.2), (0.0, 0.0, -G), 0.0)
        };

        let z0 = 100.0_f64;
        let mut state = SimState {
            time: Duration::ZERO,
            position: (0.0, 0.0, z0),
            velocity: (0.0, 0.0, 0.0),
            mass: Mass::kilograms(1.0),
        };
        let dt = 0.001;
        for _ in 0..2000 {
            state = rk4_step(&state, dt, deriv);
        }
        let t = state.time.as_seconds();
        let z_expected = z0 - 0.5 * G * t * t;
        assert!(
            (state.position.2 - z_expected).abs() < 1e-4,
            "z({t}) = {}, expected {z_expected}",
            state.position.2
        );
        let v_expected = -G * t;
        assert!(
            (state.velocity.2 - v_expected).abs() < 1e-4,
            "v({t}) = {}, expected {v_expected}",
            state.velocity.2
        );
    }
}
