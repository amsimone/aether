//! Motor fixture tests — validate the RASP parser against real .eng files.
//!
//! These tests load the PD-licensed motor fixtures from the workspace
//! `tests/fixtures/motors/` directory and verify they parse correctly
//! with computed accessors matching expected impulse-class ranges.

use rocket_motors::rasp;
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("motors")
}

fn load_motor_eng(filename: &str) -> String {
    let path = fixtures_dir().join(filename);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read motor fixture {filename}: {e}"))
}

#[test]
fn estes_a8_3_fixture_parses() {
    let eng = load_motor_eng("estes_a8_3.eng");
    let motor = rasp::parse(&eng).expect("A8-3 fixture should parse");
    assert_eq!(motor.designation, "A8");
    assert!((motor.diameter.as_millimeters() - 18.0).abs() < 0.1);
    assert!((motor.length.as_millimeters() - 70.0).abs() < 0.1);
    assert!(motor.thrust_curve.len() > 10);
    let impulse = motor.total_impulse();
    assert!(impulse > 1.5, "impulse {impulse} should be > 1.5");
    assert!(impulse < 3.5, "impulse {impulse} should be < 3.5 (A class)");
}

#[test]
fn estes_b6_fixture_parses() {
    let eng = load_motor_eng("estes_b6.eng");
    let motor = rasp::parse(&eng).expect("B6 fixture should parse");
    assert_eq!(motor.designation, "B6");
    let impulse = motor.total_impulse();
    assert!(impulse > 2.5, "impulse {impulse} should be > 2.5 (B class)");
    assert!(impulse < 5.0, "impulse {impulse} should be < 5.0");
}

#[test]
fn estes_c6_fixture_parses() {
    let eng = load_motor_eng("estes_c6.eng");
    let motor = rasp::parse(&eng).expect("C6 fixture should parse");
    assert_eq!(motor.designation, "C6");
    let impulse = motor.total_impulse();
    assert!(impulse > 5.0, "impulse {impulse} should be > 5.0 (C class)");
    assert!(impulse < 10.0, "impulse {impulse} should be < 10.0");
}
