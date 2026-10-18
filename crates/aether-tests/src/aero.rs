//! Aerodynamics validation stubs (Phase 3+).
//!
//! These tests are `#[ignore]` until the aero module is implemented.

#![allow(unused_imports, dead_code)]

use crate::fixtures;

#[ignore = "Phase 3: implement Barrowman aero buildup, then assert against SP-7224"]
#[test]
pub fn sp7224_conical_nose_cn_alpha() {
    // A conical nose has C_Nα = 2 (slender body limit) and x_cp = 2/3 * L.
    let data = fixtures::load_reference_data_json("sp7224_worked_example.json");
    let nose = &data["components"]["nose_cone"];
    let cn_alpha = nose["cn_alpha"]
        .as_f64()
        .expect("cn_alpha should be a number");
    let length_mm = nose["length_mm"]
        .as_f64()
        .expect("length_mm should be a number");
    let x_cp_mm = nose["x_cp_mm"]
        .as_f64()
        .expect("x_cp_mm should be a number");

    assert!(
        (cn_alpha - 2.0).abs() < 0.001,
        "C_Nα should be 2.0, got {cn_alpha}"
    );
    let expected_x_cp = length_mm * 2.0 / 3.0;
    assert!(
        (x_cp_mm - expected_x_cp).abs() < 0.1,
        "x_cp should be {expected_x_cp}, got {x_cp_mm}"
    );
}

#[ignore = "Phase 3: implement fin aero, then assert against SP-7224 tables"]
#[test]
pub fn sp7224_3fin_cn_alpha_matches_table() {
    let _data = fixtures::load_reference_data_json("sp7224_worked_example.json");
    todo!("implement aero module and assert C_Nα for the 3-fin set");
}

#[ignore = "Phase 3: implement fin-count projection factor"]
#[test]
pub fn sp7224_4fin_to_3fin_ratio() {
    let _data = fixtures::load_reference_data_json("sp7224_worked_example.json");
    todo!("implement aero module and assert the 4-fin/3-fin C_Nα ratio");
}

#[ignore = "Phase 3: implement boattail transition aero — sign check"]
#[test]
pub fn boattail_moves_cp_forward() {
    let _rocket = fixtures::load_rocket_json("boattail_test.json");
    todo!("compute CP with and without boattail; assert CP moves forward");
}

#[ignore = "Phase 3: implement full Barrowman buildup"]
#[test]
pub fn estes_alpha_cp_at_mach_01() {
    let _rocket = fixtures::load_rocket_json("estes_alpha.json");
    todo!("compute CP at M=0.1; assert ≈ 2.1 calibers");
}

#[ignore = "Phase 3: implement drag buildup"]
#[test]
pub fn estes_alpha_cd_vs_mach() {
    let _rocket = fixtures::load_rocket_json("estes_alpha.json");
    todo!("compute Cd at M=0.1; assert ≈ 0.45 within 10%");
}

#[ignore = "Phase 4: implement compressible aero"]
#[test]
pub fn supersonic_dart_cp_vs_mach() {
    let _rocket = fixtures::load_rocket_json("supersonic_dart.json");
    todo!("compute CP at M=0.5, 1.0, 2.0; assert within ±0.5 cal of RASAero");
}

#[ignore = "Phase 4: implement supersonic wave drag"]
#[test]
pub fn supersonic_dart_cd_vs_mach() {
    let _rocket = fixtures::load_rocket_json("supersonic_dart.json");
    todo!("compute Cd vs Mach; assert within 10% supersonic, 20% transonic");
}

#[ignore = "Phase 3: symmetry check — zero beta → zero side force"]
#[test]
pub fn symmetry_zero_beta_zero_side_force() {
    todo!("assert C_Y, C_n_yaw, C_l are exactly zero for axisymmetric rocket at β=0");
}
