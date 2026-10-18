//! Dynamics validation stubs (Phase 2+).
//!
//! These tests are `#[ignore]` until the dynamics/integrator modules
//! are implemented.

#![allow(unused_imports)]

use crate::fixtures;

#[ignore = "Phase 2: implement integrator, then test against closed-form"]
#[test]
pub fn vacuum_vertical_shot() {
    // z(t) = v0*t - 0.5*g*t² (analytical). Must match to atol=1e-7 m.
    todo!("implement RK45 integrator");
}

#[ignore = "Phase 3: implement quaternion kinematics"]
#[test]
pub fn vacuum_spinning_body() {
    // |ω| constant to machine precision; q(t) matches q(0)⊗exp(½ωt) to 1e-9.
    todo!("implement quaternion dynamics");
}

#[ignore = "Phase 3: the single most valuable test — catches sign bugs"]
#[test]
pub fn ballistic_dart_zero_drift() {
    // Symmetric fins, still air, zero initial AoA → zero lateral drift.
    todo!("implement 6-DOF dynamics");
}

#[ignore = "Phase 3: energy conservation post-burnout"]
#[test]
pub fn energy_conservation_vacuum() {
    // Total energy ½mv² + mgh monotone within rtol=1e-7.
    todo!("implement integrator");
}

#[ignore = "Phase 3: angular momentum conservation"]
#[test]
pub fn angular_momentum_conservation() {
    // L = I·ω + r×mv conserved to rtol=1e-6.
    todo!("implement 6-DOF dynamics");
}

#[ignore = "Phase 3: quaternion norm must stay 1"]
#[test]
pub fn quaternion_norm_unity() {
    // |q| = 1 at all logged times to 1e-9.
    todo!("implement quaternion dynamics");
}

#[ignore = "Phase 3: end-to-end apogee validation"]
#[test]
pub fn estes_alpha_apogee() {
    let _rocket = fixtures::load_rocket_json("estes_alpha.json");
    // Apogee within 5% of OpenRocket for Estes Alpha on C6. Expected ~110 m.
    todo!("implement full sim");
}

#[ignore = "Phase 3: end-to-end HPR flight validation"]
#[test]
pub fn hpr_dual_deploy_flight() {
    let _rocket = fixtures::load_rocket_json("hpr_dual_deploy.json");
    // Apogee within 7%, descent rate within 10%, landing within 15%.
    todo!("implement full sim + recovery");
}

#[ignore = "Phase 3: spin decay matches analytical"]
#[test]
pub fn spin_decay_analytical() {
    todo!("implement roll dynamics and verify ω decay rate");
}

#[ignore = "Phase 3: weathercocking direction"]
#[test]
pub fn weathercocking_direction() {
    // Rocket turns into wind (upwind drift initially, downwind after apogee).
    todo!("implement wind model + 6-DOF");
}
