//! Motor fixture tests — validate the RASP parser against real .eng files.
//!
//! These tests run now (not ignored) and verify the parser handles real
//! motor data files correctly, with computed accessors matching expected
//! impulse-class ranges.

#![allow(unused_imports)]

use crate::fixtures;
use rocket_motors::rasp;

#[test]
pub fn estes_a8_3_fixture_parses() {
    let eng = fixtures::load_motor_eng("estes_a8_3.eng");
    let motor = rasp::parse(&eng).expect("A8-3 fixture should parse");
    assert_eq!(motor.designation, "A8");
    assert!((motor.diameter.as_millimeters() - 18.0).abs() < 0.1);
    assert!((motor.length.as_millimeters() - 70.0).abs() < 0.1);
    assert!(motor.thrust_curve.len() > 10);
    let impulse = motor.total_impulse();
    assert!(impulse > 1.5, "impulse {impulse} should be > 1.5");
    assert!(
        impulse < 3.5,
        "impulse {impulse} should be < 3.5 (A class: 1.26-2.5 N·s)"
    );
}

#[test]
pub fn estes_b6_fixture_parses() {
    let eng = fixtures::load_motor_eng("estes_b6.eng");
    let motor = rasp::parse(&eng).expect("B6 fixture should parse");
    assert_eq!(motor.designation, "B6");
    let impulse = motor.total_impulse();
    assert!(impulse > 2.5, "impulse {impulse} should be > 2.5 (B class)");
    assert!(impulse < 5.0, "impulse {impulse} should be < 5.0");
}

#[test]
pub fn estes_c6_fixture_parses() {
    let eng = fixtures::load_motor_eng("estes_c6.eng");
    let motor = rasp::parse(&eng).expect("C6 fixture should parse");
    assert_eq!(motor.designation, "C6");
    let impulse = motor.total_impulse();
    assert!(impulse > 5.0, "impulse {impulse} should be > 5.0 (C class)");
    assert!(impulse < 10.0, "impulse {impulse} should be < 10.0");
}
